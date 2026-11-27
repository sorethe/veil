use crate::paths;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::{LazyLock, Mutex};
use tauri::{AppHandle, Emitter};

pub const LOG_CAP: usize = 2000;
const MAX_LOG_FILE: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub seq: u64,
    pub time: i64,
    pub level: String,
    pub source: String,
    pub message: String,
}

struct Inner {
    entries: VecDeque<LogEntry>,
    seq: u64,
}

pub struct LogStore {
    inner: Mutex<Inner>,
    app: Mutex<Option<AppHandle>>,
    secrets: Mutex<Vec<String>>,
    file: Mutex<Option<File>>,
}

static USERINFO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<scheme>[a-zA-Z][a-zA-Z0-9+.\-]*://)(?P<user>[^/\s:@]+):(?P<pass>[^/\s@]+)@")
        .unwrap()
});

static KV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b(password|passwd|pass|token|secret|api[_-]?key|auth|uuid|psk|private[_-]?key|obfs-password)\b(\s*[=:]\s*)[^\s,&"'}]+"#,
    )
    .unwrap()
});

static QUERY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?P<base>https?://[^\s?#]+)\?[^\s]+").unwrap());

fn normalize_level(level: &str) -> String {
    match level.to_ascii_lowercase().as_str() {
        "fatal" | "panic" => "error".into(),
        "warning" => "warn".into(),
        "trace" | "debug" | "info" | "warn" | "error" => level.to_ascii_lowercase(),
        _ => "info".into(),
    }
}

fn redact(message: &str, secrets: &[String]) -> String {
    let mut out = message.to_string();
    for secret in secrets {
        if secret.len() >= 8 {
            out = out.replace(secret.as_str(), "<redacted>");
        }
    }
    out = USERINFO_RE.replace_all(&out, "$scheme***:***@").into_owned();
    out = KV_RE.replace_all(&out, "$1$2***").into_owned();
    out = QUERY_RE.replace_all(&out, "$base?<redacted>").into_owned();
    out
}

impl LogStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: VecDeque::with_capacity(LOG_CAP),
                seq: 0,
            }),
            app: Mutex::new(None),
            secrets: Mutex::new(Vec::new()),
            file: Mutex::new(None),
        }
    }

    pub fn attach(&self, app: AppHandle) {
        *self.app.lock().unwrap() = Some(app);
    }

    pub fn add_secret(&self, secret: &str) {
        if secret.is_empty() {
            return;
        }
        let mut secrets = self.secrets.lock().unwrap();
        if !secrets.iter().any(|s| s == secret) {
            secrets.push(secret.to_string());
            if secrets.len() > 16 {
                secrets.remove(0);
            }
        }
    }

    pub fn push(&self, level: &str, source: &str, message: impl Into<String>) {
        let level = normalize_level(level);
        let secrets = self.secrets.lock().unwrap().clone();
        let message = redact(&message.into(), &secrets);
        let time = chrono::Utc::now().timestamp_millis();
        let (entry, app) = {
            let mut inner = self.inner.lock().unwrap();
            inner.seq += 1;
            let entry = LogEntry {
                seq: inner.seq,
                time,
                level,
                source: source.to_string(),
                message,
            };
            inner.entries.push_back(entry.clone());
            while inner.entries.len() > LOG_CAP {
                inner.entries.pop_front();
            }
            (entry, self.app.lock().unwrap().clone())
        };
        self.append_file(&entry);
        if let Some(app) = app {
            let _ = app.emit("engine://log", &entry);
        }
    }

    fn append_file(&self, entry: &LogEntry) {
        let _ = paths::ensure_dirs();
        let mut guard = self.file.lock().unwrap();
        if guard.is_none() {
            let path = paths::logs_dir().join("veil.log");
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.len() > MAX_LOG_FILE {
                    let _ = std::fs::rename(&path, paths::logs_dir().join("veil.log.1"));
                }
            }
            if let Ok(file) = OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(&path)
            {
                *guard = Some(file);
            }
        }
        if let Some(file) = guard.as_mut() {
            let _ = writeln!(
                file,
                "{} [{}] {} {}",
                entry.time, entry.source, entry.level, entry.message
            );
        }
    }

    pub fn snapshot(&self, since_seq: Option<u64>) -> Vec<LogEntry> {
        let inner = self.inner.lock().unwrap();
        inner
            .entries
            .iter()
            .filter(|e| since_seq.map(|s| e.seq > s).unwrap_or(true))
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.entries.clear();
    }

    pub fn tail_lines(&self, count: usize, min_level: Option<&str>) -> Vec<String> {
        let inner = self.inner.lock().unwrap();
        let min_rank = min_level.map(level_rank).unwrap_or(0);
        inner
            .entries
            .iter()
            .rev()
            .filter(|e| level_rank(&e.level) >= min_rank)
            .take(count)
            .map(|e| format!("[{}] {}", e.level, e.message))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn export(&self) -> Result<String, String> {
        paths::ensure_dirs().map_err(|e| e.to_string())?;
        let path = paths::exports_dir().join(format!(
            "veil-logs-{}-{}.log",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            &uuid::Uuid::new_v4().to_string()[..8]
        ));
        let entries = self.snapshot(None);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("create {}: {e}", path.display()))?;
        for e in entries {
            writeln!(
                file,
                "{} seq={} [{}] {} {}",
                chrono::DateTime::from_timestamp_millis(e.time)
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default(),
                e.seq,
                e.source,
                e.level,
                e.message
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(path.to_string_lossy().into_owned())
    }
}

fn level_rank(level: &str) -> u8 {
    match level {
        "trace" => 1,
        "debug" => 2,
        "info" => 3,
        "warn" => 4,
        "error" => 5,
        _ => 3,
    }
}

/// Parse a sing-box log line into (level, message).
pub fn parse_engine_line(line: &str) -> (&'static str, String) {
    let trimmed = line.trim_end();
    for (prefix, level) in [
        ("FATAL", "error"),
        ("ERROR", "error"),
        ("WARN", "warn"),
        ("INFO", "info"),
        ("DEBUG", "debug"),
        ("TRACE", "trace"),
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let rest = rest.trim_start();
            let rest = if rest.starts_with('[') {
                rest.find(']')
                    .map(|idx| rest[idx + 1..].trim_start())
                    .unwrap_or(rest)
            } else {
                rest
            };
            return (level, rest.to_string());
        }
    }
    ("info", trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_userinfo_and_tokens() {
        let out = redact(
            "dial socks5://alice:hunter2@1.2.3.4:1080?token=abc123 and password=topsecret",
            &[],
        );
        assert!(!out.contains("hunter2"));
        assert!(!out.contains("abc123"));
        assert!(!out.contains("topsecret"));
    }

    #[test]
    fn parses_engine_levels() {
        let (level, msg) = parse_engine_line("ERROR[0001] start inbound: bind: address in use");
        assert_eq!(level, "error");
        assert_eq!(msg, "start inbound: bind: address in use");
        let (level, _) = parse_engine_line("random line");
        assert_eq!(level, "info");
    }
}
