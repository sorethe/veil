use crate::configgen::{self, GenOptions};
use serde::Serialize;
use serde_json::Value;
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CoreInfo {
    pub path: Option<String>,
    pub version: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
}

fn is_executable(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    for dir in env::split_paths(&paths) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn vendor_core() -> Option<PathBuf> {
    // Dev convenience: <project>/vendor/sing-box-*/sing-box relative to the
    // executable (target/debug/veil) or the current directory.
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = env::current_exe() {
        for ancestor in exe.ancestors().take(5) {
            roots.push(ancestor.to_path_buf());
        }
    }
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let vendor = root.join("vendor");
        if let Ok(entries) = fs::read_dir(&vendor) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("sing-box-") {
                    let candidate = entry.path().join("sing-box");
                    if is_executable(&candidate) {
                        found.push(candidate);
                    }
                }
            }
        }
    }
    found.sort();
    found.pop()
}

/// Search order required by the contract:
/// settings.core_path, $VEIL_SING_BOX, `which sing-box`, beside the app
/// executable, resource dir, /usr/lib/veil/sing-box, dev vendor build.
pub fn find_core(explicit: Option<&str>, resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = explicit.map(str::trim).filter(|p| !p.is_empty()) {
        candidates.push(crate::paths::expand_tilde(path));
    }
    if let Ok(path) = env::var("VEIL_SING_BOX") {
        if !path.trim().is_empty() {
            candidates.push(crate::paths::expand_tilde(path.trim()));
        }
    }
    if let Some(path) = which("sing-box") {
        candidates.push(path);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("sing-box"));
            candidates.push(dir.join("resources").join("sing-box"));
            // e.g. src-tauri/target/release/veil -> project root
            for ancestor in dir.ancestors().take(4) {
                candidates.push(ancestor.join("resources").join("sing-box"));
                candidates.push(ancestor.join("src-tauri").join("resources").join("sing-box"));
                candidates.push(ancestor.join("sing-box"));
            }
        }
    }
    if let Some(dir) = resource_dir {
        candidates.push(dir.join("sing-box"));
        candidates.push(dir.join("resources").join("sing-box"));
    }
    candidates.push(PathBuf::from("/usr/lib/veil/sing-box"));
    if let Some(vendor) = vendor_core() {
        candidates.push(vendor);
    }
    candidates.dedup();
    candidates.into_iter().find(|p| is_executable(p))
}

pub fn version(core: &Path) -> Result<String, String> {
    let output = Command::new(core)
        .arg("version")
        .output()
        .map_err(|e| format!("failed to run {}: {e}", core.display()))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("sing-box version failed: {}", err.trim()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("sing-box version") {
            let v = rest.trim().split_whitespace().next().unwrap_or("").to_string();
            if !v.is_empty() {
                return Ok(v);
            }
        }
    }
    Err("could not parse sing-box version output".into())
}

pub fn info(explicit: Option<&str>, resource_dir: Option<&Path>) -> CoreInfo {
    let Some(path) = find_core(explicit, resource_dir) else {
        return CoreInfo {
            path: None,
            version: None,
            ok: false,
            error: Some("sing-box binary not found (set corePath in settings or $VEIL_SING_BOX)".into()),
        };
    };
    match version(&path) {
        Ok(v) => CoreInfo {
            path: Some(path.to_string_lossy().into_owned()),
            version: Some(v),
            ok: true,
            error: None,
        },
        Err(e) => CoreInfo {
            path: Some(path.to_string_lossy().into_owned()),
            version: None,
            ok: false,
            error: Some(e),
        },
    }
}

fn trim_output(output: &std::process::Output) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    let text = text.trim();
    if text.is_empty() {
        format!("sing-box check exited with {}", output.status)
    } else {
        // Keep the tail; FATAL lines are usually last.
        let chars: Vec<char> = text.chars().collect();
        let start = chars.len().saturating_sub(3000);
        chars[start..].iter().collect()
    }
}

pub fn check(core: &Path, config: &Path) -> Result<(), String> {
    let output = Command::new(core)
        .arg("check")
        .arg("-c")
        .arg(config)
        .arg("--disable-color")
        .output()
        .map_err(|e| format!("failed to run {} check: {e}", core.display()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(trim_output(&output))
    }
}

/// Validate raw profile JSON through a generated runtime config, using the
/// real `sing-box check` binary. Returns outbound tags.
pub fn validate_content(
    core: &Path,
    raw: &str,
    mode: &str,
    mixed_port: u16,
    api_port: u16,
    auto_redirect: bool,
    log_level: &str,
) -> Result<Vec<String>, String> {
    let opts = GenOptions {
        mode: mode.to_string(),
        mixed_port,
        api_port,
        auto_redirect,
        log_level: log_level.to_string(),
    };
    let generated = configgen::generate(raw, &opts)?;
    let tags = configgen::outbound_tags(&generated.value);
    let path = env::temp_dir().join(format!("veil-check-{}.json", uuid::Uuid::new_v4()));
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("create temp config: {e}"))?;
        file.write_all(generated.json.as_bytes())
            .map_err(|e| format!("write temp config: {e}"))?;
    }
    let result = check(core, &path);
    let _ = fs::remove_file(&path);
    result?;
    Ok(tags)
}

pub fn parse_outbounds(raw: &str) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let outbounds = value
        .get("outbounds")
        .and_then(Value::as_array)
        .ok_or_else(|| "profile must have an outbounds array".to_string())?;
    Ok(outbounds
        .iter()
        .filter_map(|o| o.get("tag").and_then(Value::as_str).map(str::to_string))
        .collect())
}

pub fn is_executable_public(path: &Path) -> bool {
    is_executable(path)
}

pub fn file_uid(path: &Path) -> Option<u32> {
    fs::metadata(path).ok().map(|m| m.uid())
}
