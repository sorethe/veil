use crate::clash::{self, ClashClient, Conn, Outbound, RouteInfo};
use crate::configgen::{self, GenOptions};
use crate::core;
use crate::db::Db;
use crate::diagnostics;
use crate::logs::{parse_engine_line, LogStore};
use crate::paths;
use crate::settings::SettingsStore;
use crate::sysproxy::{self, ProxyInfo};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeekExt, BufReader};
use tokio::process::Child;
use tokio::signal::unix::SignalKind;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub status: String,
    pub mode: Option<String>,
    /// Port for this session's mixed inbound, set when a session starts and
    /// kept while connected. None when idle or in error. Old serialized states
    /// without the field also deserialize.
    #[serde(default)]
    pub mixed_port: Option<u16>,
    pub profile_id: Option<i64>,
    pub profile_name: Option<String>,
    pub started_at: Option<i64>,
    pub pid: Option<u32>,
    pub error: Option<String>,
    pub proxy: Option<ProxyInfo>,
    pub core_version: Option<String>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            status: "idle".into(),
            mode: None,
            mixed_port: None,
            profile_id: None,
            profile_name: None,
            started_at: None,
            pid: None,
            error: None,
            proxy: None,
            core_version: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub up: u64,
    pub down: u64,
    pub up_speed: f64,
    pub down_speed: f64,
    pub connections: u64,
    pub duration: i64,
    pub latency: Option<u64>,
}

/// Event boundary for the engine. The app uses the Tauri AppHandle; the
/// lifecycle harness uses a recorder. This keeps engine state observable
/// without simulating any engine behavior.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: Value);
}

impl EventSink for AppHandle {
    fn emit(&self, event: &str, payload: Value) {
        let _ = tauri::Emitter::emit(self, event, payload);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeFile {
    pub pid: u32,
    pub mode: String,
    pub profile_id: i64,
    pub started_at: i64,
    /// Old runtime files predate this field; 0 is treated as unknown.
    #[serde(default)]
    pub mixed_port: u16,
    pub api_port: u16,
    pub secret: String,
    pub config_path: String,
    pub pid_file: Option<String>,
    pub log_file: Option<String>,
}

pub struct Engine {
    events: Arc<dyn EventSink>,
    settings: Arc<SettingsStore>,
    db: Arc<Db>,
    pub logs: Arc<LogStore>,
    resource_dir: Option<PathBuf>,
    state: Mutex<EngineState>,
    child: Mutex<Option<Child>>,
    runtime: Mutex<Option<RuntimeFile>>,
    clash: Mutex<Option<ClashClient>>,
    stats: Mutex<Stats>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    last_stderr: Mutex<VecDeque<String>>,
    op_lock: AsyncMutex<()>,
    shutting_down: AtomicBool,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let rc = unsafe { libc::kill(pid as i32, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn pid_cmdline(pid: u32) -> Option<String> {
    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let parts: Vec<String> = raw
        .split(|b| *b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    Some(parts.join(" "))
}

fn pid_matches_engine(pid: u32, config: &str) -> bool {
    pid_cmdline(pid)
        .map(|cmdline| cmdline.contains("sing-box") && cmdline.contains(config))
        .unwrap_or(false)
}

fn send_signal(pid: u32, signal: i32) -> bool {
    unsafe { libc::kill(pid as i32, signal) == 0 }
}

struct HelperOutput {
    value: Value,
}

fn parse_helper_response(
    output: &std::process::Output,
    action: &str,
) -> Result<HelperOutput, String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    for line in combined.lines().rev() {
        let line = line.trim();
        if line.starts_with('{') {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                if value.get("ok").and_then(Value::as_bool) == Some(true) {
                    return Ok(HelperOutput { value });
                }
                let message = value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("helper reported failure");
                return Err(format!("veil-helper {action}: {message}"));
            }
        }
    }
    if combined.to_ascii_lowercase().contains("dismissed")
        || combined.to_ascii_lowercase().contains("not authorized")
        || combined.to_ascii_lowercase().contains("authentication")
    {
        return Err(format!(
            "veil-helper {action}: polkit authentication was cancelled or failed. Install the polkit policy and try again. ({})",
            combined.trim()
        ));
    }
    Err(format!(
        "veil-helper {action} failed ({}): {}",
        output.status,
        combined.trim()
    ))
}

impl Engine {
    pub fn new(
        events: Arc<dyn EventSink>,
        settings: Arc<SettingsStore>,
        db: Arc<Db>,
        logs: Arc<LogStore>,
        resource_dir: Option<PathBuf>,
    ) -> Arc<Self> {
        Arc::new(Self {
            events,
            settings,
            db,
            logs,
            resource_dir,
            state: Mutex::new(EngineState::default()),
            child: Mutex::new(None),
            runtime: Mutex::new(None),
            clash: Mutex::new(None),
            stats: Mutex::new(Stats::default()),
            tasks: Mutex::new(Vec::new()),
            last_stderr: Mutex::new(VecDeque::new()),
            op_lock: AsyncMutex::new(()),
            shutting_down: AtomicBool::new(false),
        })
    }

    pub fn snapshot(&self) -> EngineState {
        self.state.lock().unwrap().clone()
    }

    fn update_state<F: FnOnce(&mut EngineState)>(&self, mutate: F) -> EngineState {
        let state = {
            let mut guard = self.state.lock().unwrap();
            mutate(&mut guard);
            guard.clone()
        };
        self.events.emit(
            "engine://state",
            serde_json::to_value(&state).unwrap_or(Value::Null),
        );
        state
    }

    fn set_state(&self, next: EngineState) -> EngineState {
        {
            let mut guard = self.state.lock().unwrap();
            *guard = next.clone();
        }
        self.events.emit(
            "engine://state",
            serde_json::to_value(&next).unwrap_or(Value::Null),
        );
        next
    }

    fn notice(&self, level: &str, message: &str) {
        self.events.emit(
            "engine://notice",
            json!({ "level": level, "message": message }),
        );
    }

    fn notify_desktop(&self, title: &str, body: &str) {
        if !self.settings.get().notifications {
            return;
        }
        let title = title.to_string();
        let body = body.to_string();
        std::thread::spawn(move || {
            let _ = notify_rust::Notification::new()
                .appname("Veil")
                .summary(&title)
                .body(&body)
                .show();
        });
    }

    fn stderr_tail(&self, count: usize) -> String {
        let tail = self.last_stderr.lock().unwrap();
        tail.iter()
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn ingest_line(&self, line: &str) {
        let (level, message) = parse_engine_line(line);
        self.logs.push(level, "engine", message);
        let mut tail = self.last_stderr.lock().unwrap();
        tail.push_back(line.to_string());
        while tail.len() > 40 {
            tail.pop_front();
        }
    }

    fn spawn_line_reader(
        self: &Arc<Self>,
        reader: impl AsyncRead + Unpin + Send + 'static,
    ) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                engine.ingest_line(&line);
            }
        })
    }

    fn spawn_log_tailer(self: &Arc<Self>, path: PathBuf, from_end: bool) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut offset: u64 = 0;
            let mut initialized = false;
            let mut buffer = String::new();
            loop {
                let status = engine.snapshot().status;
                if !matches!(status.as_str(), "starting" | "connected" | "stopping") {
                    break;
                }
                if !initialized {
                    if let Ok(meta) = tokio::fs::metadata(&path).await {
                        offset = if from_end { meta.len() } else { 0 };
                        initialized = true;
                    }
                }
                if initialized {
                    if let Ok(mut file) = tokio::fs::File::open(&path).await {
                        let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
                        if len < offset {
                            offset = 0;
                        }
                        if len > offset && file.seek(std::io::SeekFrom::Start(offset)).await.is_ok()
                        {
                            let mut bytes = Vec::new();
                            if file.read_to_end(&mut bytes).await.is_ok() {
                                offset += bytes.len() as u64;
                                buffer.push_str(&String::from_utf8_lossy(&bytes));
                                while let Some(idx) = buffer.find('\n') {
                                    let line: String = buffer[..idx].to_string();
                                    buffer.drain(..=idx);
                                    engine.ingest_line(&line);
                                }
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        })
    }

    fn abort_tasks(&self) {
        let tasks: Vec<JoinHandle<()>> = {
            let mut guard = self.tasks.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        for task in tasks {
            task.abort();
        }
    }

    fn register_task(&self, task: JoinHandle<()>) {
        self.tasks.lock().unwrap().push(task);
    }

    fn clash(&self) -> Result<ClashClient, String> {
        self.clash
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "engine is not connected".to_string())
    }

    fn write_runtime_file(&self, runtime: &RuntimeFile) -> Result<(), String> {
        paths::ensure_dirs().map_err(|e| e.to_string())?;
        let path = paths::runtime_file_path();
        let tmp = path.with_extension("json.tmp");
        let raw = serde_json::to_string_pretty(runtime).map_err(|e| e.to_string())?;
        {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|e| format!("write runtime state: {e}"))?;
            file.write_all(raw.as_bytes())
                .map_err(|e| format!("write runtime state: {e}"))?;
            file.sync_all().ok();
        }
        fs::rename(&tmp, &path).map_err(|e| format!("write runtime state: {e}"))?;
        *self.runtime.lock().unwrap() = Some(runtime.clone());
        Ok(())
    }

    fn read_runtime_file(&self) -> Option<RuntimeFile> {
        let raw = fs::read_to_string(paths::runtime_file_path()).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Old runtime files have no mixedPort. Recover it from the persisted
    /// generated config when possible; otherwise adoption reports unavailable.
    fn mixed_port_from_config(path: &Path) -> Option<u16> {
        let raw = fs::read_to_string(path).ok()?;
        let value: Value = serde_json::from_str(&raw).ok()?;
        value
            .get("inbounds")?
            .as_array()?
            .iter()
            .find(|inbound| {
                inbound.get("tag").and_then(Value::as_str) == Some(crate::configgen::MIXED_TAG)
            })
            .and_then(|inbound| inbound.get("listen_port"))
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
    }

    fn remove_runtime_file(&self) {
        let _ = fs::remove_file(paths::runtime_file_path());
        let _ = fs::remove_file(paths::runtime_config_path());
        *self.runtime.lock().unwrap() = None;
    }

    fn spawn_system(self: &Arc<Self>, core_path: &Path, config_path: &Path) -> Result<u32, String> {
        let mut command = tokio::process::Command::new(core_path);
        command
            .arg("run")
            .arg("-c")
            .arg(config_path)
            .arg("--disable-color")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| format!("spawn sing-box: {e}"))?;
        let pid = child
            .id()
            .ok_or_else(|| "sing-box did not return a pid".to_string())?;
        if let Some(stdout) = child.stdout.take() {
            self.register_task(self.spawn_line_reader(stdout));
        }
        if let Some(stderr) = child.stderr.take() {
            self.register_task(self.spawn_line_reader(stderr));
        }
        *self.child.lock().unwrap() = Some(child);
        Ok(pid)
    }

    async fn helper_start(
        self: &Arc<Self>,
        core_path: &Path,
        config_path: &Path,
    ) -> Result<u32, String> {
        let helper = diagnostics::find_helper(self.resource_dir.as_deref()).ok_or_else(|| {
            "veil-helper is not installed. TUN mode needs /usr/bin/veil-helper and the polkit policy."
                .to_string()
        })?;
        if !Path::new("/dev/net/tun").exists() {
            return Err("/dev/net/tun is missing; TUN mode is unavailable on this system".into());
        }
        if crate::diagnostics::find_in_path("nft").is_none() {
            return Err("nft is not installed; TUN mode needs nftables".into());
        }
        let pkexec = crate::diagnostics::find_in_path("pkexec").ok_or_else(|| {
            "pkexec is not installed; TUN mode needs polkit to start veil-helper".to_string()
        })?;
        let pid_file = paths::helper_pid_file();
        let log_file = paths::helper_log_path();
        let mut command = tokio::process::Command::new(pkexec);
        command
            .arg(&helper)
            .arg("start")
            .arg("--config")
            .arg(config_path)
            .arg("--core")
            .arg(core_path)
            .arg("--pid-file")
            .arg(&pid_file)
            .arg("--log-file")
            .arg(&log_file);
        command.kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(120), command.output())
            .await
            .map_err(|_| {
                "pkexec authentication timed out after 120s; administrative permission was not granted"
                    .to_string()
            })?
            .map_err(|e| format!("run pkexec: {e}"))?;
        let response = parse_helper_response(&output, "start")?;
        let pid = response
            .value
            .get("pid")
            .and_then(Value::as_u64)
            .ok_or_else(|| "veil-helper start returned no pid".to_string())?
            as u32;
        self.register_task(self.spawn_log_tailer(log_file, false));
        Ok(pid)
    }

    async fn helper_stop(&self) -> Result<(), String> {
        let helper = diagnostics::find_helper(self.resource_dir.as_deref()).ok_or_else(|| {
            "veil-helper is not installed; cannot stop the TUN engine".to_string()
        })?;
        let pkexec = diagnostics::find_in_path("pkexec")
            .ok_or_else(|| "pkexec is not installed; cannot stop the TUN engine".to_string())?;
        if !Path::new(&paths::helper_pid_file()).exists() {
            return Ok(());
        }
        let mut command = tokio::process::Command::new(pkexec);
        command
            .arg(&helper)
            .arg("stop")
            .arg("--pid-file")
            .arg(paths::helper_pid_file());
        command.kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(120), command.output())
            .await
            .map_err(|_| {
                "pkexec authentication timed out after 120s; administrative permission was not granted"
                    .to_string()
            })?
            .map_err(|e| format!("run pkexec: {e}"))?;
        parse_helper_response(&output, "stop")?;
        Ok(())
    }

    async fn stop_system_child(&self) -> Result<(), String> {
        let Some(mut child) = self.child.lock().unwrap().take() else {
            return Ok(());
        };
        if let Some(pid) = child.id() {
            send_signal(pid, libc::SIGTERM);
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => {}
                Err(e) => return Err(format!("wait for sing-box: {e}")),
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let _ = child.start_kill();
        match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(format!("wait for killed sing-box: {e}")),
            Err(_) => Err("sing-box did not exit after SIGKILL".into()),
        }
    }

    async fn stop_current(self: &Arc<Self>) -> Result<(), String> {
        let runtime = self.runtime.lock().unwrap().clone();
        let state = self.snapshot();
        let mode = runtime
            .as_ref()
            .map(|r| r.mode.clone())
            .or_else(|| state.mode.clone())
            .unwrap_or_else(|| "system".into());
        let pid = runtime.as_ref().map(|r| r.pid).or(state.pid);
        if mode == "tun" {
            self.helper_stop().await?;
            if let Some(pid) = pid {
                if pid_alive(pid) {
                    return Err(format!(
                        "TUN engine pid {pid} is still running after veil-helper stop"
                    ));
                }
            }
            return Ok(());
        }
        if self.child.lock().unwrap().is_some() {
            return self.stop_system_child().await;
        }
        if let Some(pid) = pid {
            if pid_alive(pid) {
                send_signal(pid, libc::SIGTERM);
                let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
                while pid_alive(pid) && tokio::time::Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                if pid_alive(pid) {
                    send_signal(pid, libc::SIGKILL);
                    let kill_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
                    while pid_alive(pid) && tokio::time::Instant::now() < kill_deadline {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
                if pid_alive(pid) {
                    return Err(format!(
                        "sing-box pid {pid} is still running after SIGTERM/SIGKILL"
                    ));
                }
            }
        }
        Ok(())
    }

    fn spawn_monitor(self: &Arc<Self>, pid: u32) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let status = engine.snapshot().status;
                if !matches!(status.as_str(), "starting" | "connected") {
                    break;
                }
                let mut exited: Option<String> = None;
                {
                    let mut guard = engine.child.lock().unwrap();
                    if let Some(child) = guard.as_mut() {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                exited = Some(format!("{status}"));
                                *guard = None;
                            }
                            Ok(None) => {}
                            Err(e) => exited = Some(e.to_string()),
                        }
                    } else if !pid_alive(pid) {
                        exited = Some("process exited".into());
                    }
                }
                if let Some(detail) = exited {
                    engine.handle_unexpected_exit(&detail).await;
                    break;
                }
            }
        })
    }

    async fn handle_unexpected_exit(self: &Arc<Self>, detail: &str) {
        let status = self.snapshot().status;
        if status == "stopping" || status == "idle" || status == "error" {
            return;
        }
        let tail = self.stderr_tail(12);
        let message = if tail.is_empty() {
            format!("sing-box exited unexpectedly ({detail})")
        } else {
            format!("sing-box exited unexpectedly ({detail}). Last output:\n{tail}")
        };
        self.logs.push("error", "engine", message.clone());
        if let Err(e) = sysproxy::restore() {
            self.logs
                .push("warn", "engine", format!("restore system proxy: {e}"));
        }
        self.remove_runtime_file();
        *self.clash.lock().unwrap() = None;
        self.update_state(|state| {
            state.status = "error".into();
            state.pid = None;
            state.started_at = None;
            state.mixed_port = None;
            state.error = Some(message.clone());
            state.proxy = None;
        });
        self.notice("error", &message);
        self.notify_desktop("Veil engine stopped", &message);
        self.reset_stats();
    }

    fn reset_stats(&self) {
        *self.stats.lock().unwrap() = Stats::default();
    }

    fn spawn_traffic(self: &Arc<Self>, client: ClashClient) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                if engine.snapshot().status != "connected" {
                    break;
                }
                if let Ok(response) = client.traffic().await {
                    let mut stream = response.bytes_stream();
                    let mut buffer = String::new();
                    while let Some(chunk) = stream.next().await {
                        if engine.snapshot().status != "connected" {
                            return;
                        }
                        let Ok(bytes) = chunk else { break };
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(idx) = buffer.find('\n') {
                            let line: String = buffer[..idx].trim().to_string();
                            buffer.drain(..=idx);
                            if line.is_empty() {
                                continue;
                            }
                            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                                let mut stats = engine.stats.lock().unwrap();
                                stats.up_speed =
                                    value.get("up").and_then(Value::as_f64).unwrap_or(0.0);
                                stats.down_speed =
                                    value.get("down").and_then(Value::as_f64).unwrap_or(0.0);
                            }
                        }
                    }
                }
                if engine.snapshot().status != "connected" {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
    }

    fn spawn_connection_poll(self: &Arc<Self>, client: ClashClient) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                if engine.snapshot().status != "connected" {
                    break;
                }
                if let Ok(value) = client.connections().await {
                    let mut stats = engine.stats.lock().unwrap();
                    if let Some(list) = value.get("connections").and_then(Value::as_array) {
                        stats.connections = list.len() as u64;
                    }
                    if let Some(up) = value.get("uploadTotal").and_then(Value::as_u64) {
                        stats.up = up;
                    }
                    if let Some(down) = value.get("downloadTotal").and_then(Value::as_u64) {
                        stats.down = down;
                    }
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        })
    }

    fn spawn_latency_poll(self: &Arc<Self>, client: ClashClient) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                if engine.snapshot().status != "connected" {
                    break;
                }
                if let Ok(value) = client.proxies().await {
                    let outbounds = clash::map_outbounds(&value);
                    let latency = outbounds
                        .iter()
                        .find(|o| o.tag == "Proxy")
                        .and_then(|o| o.now.as_deref())
                        .and_then(|active| outbounds.iter().find(|o| o.tag == active))
                        .and_then(|o| o.delay)
                        .or_else(|| outbounds.iter().filter_map(|o| o.delay).next());
                    engine.stats.lock().unwrap().latency = latency;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        })
    }

    fn spawn_stats_emitter(self: &Arc<Self>, started_at: i64) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                if engine.snapshot().status != "connected" {
                    break;
                }
                let stats = {
                    let mut guard = engine.stats.lock().unwrap();
                    guard.duration = (now() - started_at).max(0);
                    guard.clone()
                };
                engine.events.emit(
                    "engine://stats",
                    serde_json::to_value(&stats).unwrap_or(Value::Null),
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
    }

    fn spawn_clash_logs(self: &Arc<Self>, client: ClashClient) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                if engine.snapshot().status != "connected" {
                    break;
                }
                let response = match client.logs("info").await {
                    Ok(response) => response,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                let mut stream = response.bytes_stream();
                let mut buffer = String::new();
                while let Some(chunk) = stream.next().await {
                    if engine.snapshot().status != "connected" {
                        return;
                    }
                    let Ok(bytes) = chunk else { break };
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(idx) = buffer.find('\n') {
                        let line: String = buffer[..idx].trim().to_string();
                        buffer.drain(..=idx);
                        if line.is_empty() {
                            continue;
                        }
                        if let Ok(value) = serde_json::from_str::<Value>(&line) {
                            let level = value.get("type").and_then(Value::as_str).unwrap_or("info");
                            let payload =
                                value.get("payload").and_then(Value::as_str).unwrap_or("");
                            if !payload.is_empty() {
                                engine.logs.push(level, "clash", payload);
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
    }

    fn start_live_tasks(self: &Arc<Self>, client: ClashClient, started_at: i64) {
        // Note: do not abort here - engine readers/log tailers registered during
        // startup must keep running. Disconnect/failure clears all tasks.
        let tasks = vec![
            self.spawn_traffic(client.clone()),
            self.spawn_connection_poll(client.clone()),
            self.spawn_latency_poll(client.clone()),
            self.spawn_stats_emitter(started_at),
            self.spawn_clash_logs(client),
        ];
        self.tasks.lock().unwrap().extend(tasks);
    }

    async fn wait_health(&self, client: &ClashClient, timeout: Duration) -> Result<Value, String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let exited = {
                let mut guard = self.child.lock().unwrap();
                if let Some(child) = guard.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            *guard = None;
                            Some(format!("exited ({status})"))
                        }
                        Ok(None) => None,
                        Err(e) => Some(format!("wait error: {e}")),
                    }
                } else {
                    match self.snapshot().pid {
                        Some(pid) if !pid_alive(pid) => Some("process exited".into()),
                        _ => None,
                    }
                }
            };
            if let Some(detail) = exited {
                return Err(format!(
                    "sing-box {detail} during startup. {}",
                    self.stderr_tail(12)
                ));
            }
            let last = match client.version().await {
                Ok(value) => return Ok(value),
                Err(e) => e,
            };
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "Clash API did not become ready within {}s: {last}",
                    timeout.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn clear_after_failed_start(self: &Arc<Self>) -> Result<(), String> {
        let stop = self.stop_current().await;
        *self.clash.lock().unwrap() = None;
        self.abort_tasks();
        match &stop {
            Ok(()) => self.remove_runtime_file(),
            Err(e) => self.logs.push(
                "error",
                "engine",
                format!("could not stop the failed engine; it may still be running: {e}"),
            ),
        }
        stop
    }

    async fn connect_profile(
        self: &Arc<Self>,
        mode_arg: Option<String>,
        profile_id_arg: Option<i64>,
    ) -> Result<EngineState, String> {
        let current = self.snapshot();
        if matches!(
            current.status.as_str(),
            "connected" | "starting" | "stopping"
        ) {
            return Ok(current);
        }
        if current.status == "error" && self.has_runtime() {
            return Err(
                "the previous engine is still running; disconnect it before connecting again"
                    .into(),
            );
        }
        // Clear any stale readers/pollers from a previous session before we
        // register new ones.
        self.abort_tasks();
        let settings = self.settings.get();
        let mode = mode_arg.unwrap_or_else(|| settings.mode.clone());
        if !matches!(mode.as_str(), "system" | "tun") {
            return Err(format!("unknown mode '{mode}' (expected system or tun)"));
        }
        if mode == "tun" {
            let setup = diagnostics::tun_setup(
                self.resource_dir.as_deref(),
                &settings.tun_address,
                &settings.tun_address6,
            );
            if !setup.ready {
                let detail = if setup.missing.is_empty() {
                    setup.detail
                } else {
                    setup.missing.join("; ")
                };
                return Err(format!("TUN setup is required: {detail}"));
            }
        }
        let profile = match profile_id_arg.or_else(|| self.db.last_profile_id()) {
            Some(id) => self
                .db
                .get_profile(id)?
                .ok_or_else(|| format!("profile {id} not found"))?,
            None => self
                .db
                .list_profiles()?
                .into_iter()
                .next()
                .ok_or_else(|| "no profiles yet; import or create one first".to_string())?,
        };

        let core_path =
            core::find_core(settings.core_path.as_deref(), self.resource_dir.as_deref())
                .ok_or_else(|| {
                    "sing-box binary not found. Set corePath in settings or install sing-box."
                        .to_string()
                })?;
        self.reset_stats();
        let core_version = core::version(&core_path).ok();

        let mut opts = GenOptions::from_settings(&settings);
        opts.mode = mode.clone();
        let generated = configgen::generate(&profile.content, &opts)?;
        let config_path = paths::runtime_config_path();
        configgen::write_runtime_config(&config_path, &generated.json)?;
        if let Err(e) = core::check(&core_path, &config_path) {
            let _ = fs::remove_file(&config_path);
            return Err(format!(
                "sing-box config check failed for '{}': {e}",
                profile.name
            ));
        }

        let started_at = now();
        self.logs.add_secret(&generated.secret);
        self.set_state(EngineState {
            status: "starting".into(),
            mode: Some(mode.clone()),
            mixed_port: Some(settings.mixed_port),
            profile_id: Some(profile.id),
            profile_name: Some(profile.name.clone()),
            started_at: Some(started_at),
            pid: None,
            error: None,
            proxy: None,
            core_version: core_version.clone(),
        });
        self.logs.push(
            "info",
            "engine",
            format!(
                "starting sing-box {} in {} mode for profile '{}'",
                core_version.clone().unwrap_or_else(|| "?".into()),
                mode,
                profile.name
            ),
        );

        let spawn_result = if mode == "tun" {
            self.helper_start(&core_path, &config_path).await
        } else {
            self.spawn_system(&core_path, &config_path)
        };
        let pid = match spawn_result {
            Ok(pid) => pid,
            Err(error) => {
                self.logs.push("error", "engine", error.clone());
                self.set_state(EngineState {
                    status: "error".into(),
                    mode: Some(mode.clone()),
                    mixed_port: None,
                    profile_id: Some(profile.id),
                    profile_name: Some(profile.name.clone()),
                    started_at: None,
                    pid: None,
                    error: Some(error.clone()),
                    proxy: None,
                    core_version,
                });
                self.notice("error", &error);
                self.remove_runtime_file();
                return Err(error);
            }
        };
        self.update_state(|state| state.pid = Some(pid));
        let client = ClashClient::new(generated.api_port, &generated.secret);
        *self.clash.lock().unwrap() = Some(client.clone());

        if let Err(error) = self.wait_health(&client, Duration::from_secs(12)).await {
            let tail = self.stderr_tail(12);
            let base = if tail.is_empty() {
                error
            } else {
                format!("{error}. Last sing-box output:\n{tail}")
            };
            let cleanup = self.clear_after_failed_start().await;
            let (message, still_running) = match cleanup {
                Ok(()) => (base, false),
                Err(stop_error) => (
                    format!(
                        "{base}\nWARNING: the process could not be stopped and may still be running: {stop_error}"
                    ),
                    true,
                ),
            };
            self.logs.push("error", "engine", message.clone());
            self.set_state(EngineState {
                status: "error".into(),
                mode: Some(mode),
                mixed_port: None,
                profile_id: Some(profile.id),
                profile_name: Some(profile.name.clone()),
                started_at: if still_running {
                    Some(started_at)
                } else {
                    None
                },
                pid: if still_running { Some(pid) } else { None },
                error: Some(message.clone()),
                proxy: None,
                core_version,
            });
            self.notice("error", &message);
            self.notify_desktop("Veil failed to connect", &message);
            return Err(message);
        }

        // Re-apply previously chosen outbound selections for this profile.
        if let Ok(selections) = self.db.selections(profile.id) {
            for (group, tag) in selections {
                if let Err(e) = client.select(&group, &tag).await {
                    self.logs.push(
                        "warn",
                        "engine",
                        format!("could not restore selection {group} -> {tag}: {e}"),
                    );
                }
            }
        }

        let proxy = if mode == "tun" {
            Some(sysproxy::tun_info())
        } else {
            match sysproxy::apply(settings.mixed_port) {
                Ok(info) => Some(info),
                Err(e) => {
                    self.logs
                        .push("warn", "engine", format!("system proxy not applied: {e}"));
                    Some(ProxyInfo {
                        kind: "error".into(),
                        detail: e,
                    })
                }
            }
        };

        let runtime = RuntimeFile {
            pid,
            mode: mode.clone(),
            profile_id: profile.id,
            started_at,
            mixed_port: settings.mixed_port,
            api_port: generated.api_port,
            secret: generated.secret.clone(),
            config_path: config_path.to_string_lossy().into_owned(),
            pid_file: if mode == "tun" {
                Some(paths::helper_pid_file().to_string_lossy().into_owned())
            } else {
                None
            },
            log_file: if mode == "tun" {
                Some(paths::helper_log_path().to_string_lossy().into_owned())
            } else {
                None
            },
        };
        if let Err(e) = self.write_runtime_file(&runtime) {
            self.logs.push(
                "warn",
                "engine",
                format!("could not persist runtime state: {e}"),
            );
        }
        self.db.set_last_profile_id(profile.id);

        let state = self.set_state(EngineState {
            status: "connected".into(),
            mode: Some(mode.clone()),
            mixed_port: Some(settings.mixed_port),
            profile_id: Some(profile.id),
            profile_name: Some(profile.name.clone()),
            started_at: Some(started_at),
            pid: Some(pid),
            error: None,
            proxy,
            core_version,
        });
        self.logs.push(
            "info",
            "engine",
            format!(
                "connected: pid {pid}, profile '{}', Clash API 127.0.0.1:{}",
                profile.name, generated.api_port
            ),
        );
        self.notice("info", "Connected");
        self.notify_desktop(
            "Veil connected",
            &format!("{} via {} mode", profile.name, mode),
        );
        self.start_live_tasks(client, started_at);
        self.register_task(self.spawn_monitor(pid));
        Ok(state)
    }

    pub async fn connect(
        self: &Arc<Self>,
        mode: Option<String>,
        profile_id: Option<i64>,
    ) -> Result<EngineState, String> {
        let _guard = self.op_lock.lock().await;
        self.connect_profile(mode, profile_id).await
    }

    pub async fn disconnect(self: &Arc<Self>) -> Result<EngineState, String> {
        let guard = self.op_lock.lock().await;
        let current = self.snapshot();
        if current.status == "idle" {
            // Idle with leftover runtime state means a previous teardown was
            // interrupted; finish it so signal-driven exit is still clean.
            self.remove_runtime_file();
            *self.clash.lock().unwrap() = None;
            if let Err(e) = sysproxy::restore() {
                self.logs
                    .push("warn", "engine", format!("restore system proxy: {e}"));
            }
            return Ok(current);
        }
        self.update_state(|state| {
            state.status = "stopping".into();
            state.mixed_port = None;
            state.error = None;
        });
        self.logs.push("info", "engine", "stopping sing-box");
        let stop_result = self.stop_current().await;
        let restore_result = sysproxy::restore();
        drop(guard);

        match stop_result {
            Ok(()) => {
                self.abort_tasks();
                *self.clash.lock().unwrap() = None;
                self.remove_runtime_file();
                self.last_stderr.lock().unwrap().clear();
                if let Err(e) = restore_result {
                    self.logs
                        .push("warn", "engine", format!("restore system proxy: {e}"));
                }
                let state = self.update_state(|state| {
                    state.status = "idle".into();
                    state.pid = None;
                    state.started_at = None;
                    state.mixed_port = None;
                    state.error = None;
                    state.proxy = None;
                });
                self.reset_stats();
                self.logs.push("info", "engine", "engine stopped");
                self.notice("info", "Disconnected");
                self.notify_desktop("Veil disconnected", "The proxy engine was stopped.");
                Ok(state)
            }
            Err(error) => {
                let message = format!("engine could not be stopped: {error}");
                self.logs.push("error", "engine", message.clone());
                self.update_state(|state| {
                    state.status = "error".into();
                    state.mixed_port = None;
                    state.error = Some(message.clone());
                    state.proxy = None;
                });
                self.notice("error", &message);
                self.notify_desktop("Veil disconnect failed", &message);
                Err(message)
            }
        }
    }

    pub fn stats(&self) -> Stats {
        let mut stats = self.stats.lock().unwrap().clone();
        stats.duration = self
            .snapshot()
            .started_at
            .map(|start| (now() - start).max(0))
            .unwrap_or(0);
        stats
    }

    pub async fn connections(&self) -> Result<Vec<Conn>, String> {
        match self.clash() {
            Ok(client) => Ok(clash::map_connections(&client.connections().await?)),
            Err(_) => Ok(Vec::new()),
        }
    }

    pub async fn close_connection(&self, id: &str) -> Result<(), String> {
        self.clash()?.close_connection(id).await
    }

    pub async fn outbounds(&self) -> Result<Vec<Outbound>, String> {
        match self.clash() {
            Ok(client) => Ok(clash::map_outbounds(&client.proxies().await?)),
            Err(_) => Ok(Vec::new()),
        }
    }

    pub async fn select_outbound(&self, group: &str, tag: &str) -> Result<(), String> {
        let client = self.clash()?;
        client.select(group, tag).await?;
        if let Some(profile_id) = self
            .runtime
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.profile_id)
            .or_else(|| self.db.last_profile_id())
        {
            if let Err(e) = self.db.set_selection(profile_id, group, tag) {
                self.logs
                    .push("warn", "engine", format!("save selection: {e}"));
            }
        }
        Ok(())
    }

    pub async fn test_delay(&self, tag: &str) -> Result<u64, String> {
        self.clash()?.delay(tag, 5000).await
    }

    pub async fn set_clash_mode(&self, mode: &str) -> Result<(), String> {
        if !matches!(mode, "rule" | "global" | "direct") {
            return Err(format!("unknown clash mode '{mode}'"));
        }
        self.clash()?.set_mode(mode).await
    }

    pub async fn route(&self) -> Result<RouteInfo, String> {
        let profile_id = self
            .runtime
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.profile_id)
            .or_else(|| self.db.last_profile_id());
        let content = profile_id
            .and_then(|id| self.db.get_profile(id).ok().flatten())
            .map(|p| p.content);
        let mode = match self.clash() {
            Ok(client) => client
                .configs()
                .await
                .ok()
                .and_then(|v| v.get("mode").and_then(Value::as_str).map(str::to_string)),
            Err(_) => None,
        };
        Ok(clash::route_info(content.as_deref(), mode))
    }

    pub fn active_profile_content(&self) -> Option<String> {
        let profile_id = self
            .runtime
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.profile_id)
            .or_else(|| self.db.last_profile_id());
        profile_id.and_then(|id| self.db.get_profile(id).ok().flatten().map(|p| p.content))
    }

    pub fn has_runtime(&self) -> bool {
        self.runtime.lock().unwrap().is_some()
            || matches!(
                self.snapshot().status.as_str(),
                "starting" | "connected" | "stopping"
            )
            || self.snapshot().pid.map(pid_alive).unwrap_or(false)
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn begin_shutdown_and_exit(self: &Arc<Self>, app: AppHandle) {
        if self.shutting_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let engine = self.clone();
        tauri::async_runtime::spawn(async move {
            let _ = engine.disconnect().await;
            app.exit(0);
        });
    }

    /// Route SIGTERM/SIGINT through the same graceful teardown as the
    /// window-close, tray-quit and `quit` command paths. Installing a Tokio
    /// signal listener also replaces the default disposition, so a signal
    /// arriving mid-cleanup cannot kill the process before the engine stops.
    pub fn install_signal_handlers(self: &Arc<Self>, app: AppHandle) {
        self.spawn_signal_listener(app.clone(), SignalKind::terminate(), "SIGTERM");
        self.spawn_signal_listener(app, SignalKind::interrupt(), "SIGINT");
    }

    fn spawn_signal_listener(
        self: &Arc<Self>,
        app: AppHandle,
        kind: SignalKind,
        name: &'static str,
    ) {
        let engine = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut stream = match tokio::signal::unix::signal(kind) {
                Ok(stream) => stream,
                Err(e) => {
                    engine
                        .logs
                        .push("warn", "engine", format!("cannot listen for {name}: {e}"));
                    return;
                }
            };
            if stream.recv().await.is_none() {
                return;
            }
            engine
                .logs
                .push("info", "engine", format!("received {name}; shutting down"));
            engine.begin_shutdown_and_exit(app);
        });
    }

    /// Last-resort cleanup on process exit: restore the desktop proxy and send
    /// a SIGTERM to a system-mode engine. TUN cleanup goes through the helper.
    pub fn shutdown_sync(&self) {
        if let Err(e) = sysproxy::restore() {
            self.logs
                .push("warn", "engine", format!("restore system proxy: {e}"));
        }
        if let Some(runtime) = self.runtime.lock().unwrap().clone() {
            if runtime.mode == "tun" {
                if let Some(helper) = diagnostics::find_helper(self.resource_dir.as_deref()) {
                    let pid_file = paths::helper_pid_file();
                    std::thread::spawn(move || {
                        let _ = std::process::Command::new("pkexec")
                            .arg(helper)
                            .arg("stop")
                            .arg("--pid-file")
                            .arg(pid_file)
                            .status();
                    });
                }
            } else {
                send_signal(runtime.pid, libc::SIGTERM);
            }
        }
    }

    pub fn spawn_startup(self: &Arc<Self>) {
        let engine = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            engine.adopt().await;
            let settings = engine.settings.get();
            if settings.connect_on_launch && engine.snapshot().status == "idle" {
                if let Ok(profiles) = engine.db.list_profiles() {
                    if !profiles.is_empty() {
                        let id = engine
                            .db
                            .last_profile_id()
                            .or_else(|| profiles.first().map(|p| p.id));
                        if let Some(id) = id {
                            if let Err(e) = engine.connect(None, Some(id)).await {
                                engine.logs.push(
                                    "warn",
                                    "engine",
                                    format!("connectOnLaunch failed: {e}"),
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    async fn adopt(self: &Arc<Self>) {
        let _guard = self.op_lock.lock().await;
        let Some(runtime) = self.read_runtime_file() else {
            let _ = fs::remove_file(paths::runtime_config_path());
            // Crash recovery: a state file means the desktop proxy is still set.
            if let Some(info) = sysproxy::active_proxy_info() {
                self.logs.push(
                    "warn",
                    "engine",
                    format!(
                        "found leftover {} system proxy state; restoring previous values",
                        info.kind
                    ),
                );
                if let Err(e) = sysproxy::restore() {
                    self.logs
                        .push("error", "engine", format!("restore system proxy: {e}"));
                }
            }
            self.reset_stats();
            return;
        };
        let config_path = runtime.config_path.clone();
        if !pid_alive(runtime.pid) || !pid_matches_engine(runtime.pid, &config_path) {
            self.logs.push(
                "warn",
                "engine",
                "stale runtime file (process not running or not our sing-box); cleaning up",
            );
            self.remove_runtime_file();
            let _ = fs::remove_file(&config_path);
            let _ = sysproxy::restore();
            return;
        }
        self.logs.add_secret(&runtime.secret);
        let client = ClashClient::new(runtime.api_port, &runtime.secret);
        *self.clash.lock().unwrap() = Some(client.clone());
        *self.runtime.lock().unwrap() = Some(runtime.clone());
        if let Err(e) = client.wait_ready(Duration::from_secs(3)).await {
            self.logs.push(
                "warn",
                "engine",
                format!("could not adopt running engine: {e}; stopping it"),
            );
            let mut probe = EngineState {
                status: "connected".into(),
                mode: Some(runtime.mode.clone()),
                ..EngineState::default()
            };
            probe.pid = Some(runtime.pid);
            self.state.lock().unwrap().clone_from(&probe);
            if runtime.mode == "tun" {
                let _ = self.helper_stop().await;
            } else {
                send_signal(runtime.pid, libc::SIGTERM);
            }
            *self.clash.lock().unwrap() = None;
            self.remove_runtime_file();
            let _ = sysproxy::restore();
            self.state
                .lock()
                .unwrap()
                .clone_from(&EngineState::default());
            return;
        }
        let profile = self.db.get_profile(runtime.profile_id).ok().flatten();
        let proxy = if runtime.mode == "tun" {
            Some(sysproxy::tun_info())
        } else {
            sysproxy::active_proxy_info()
        };
        let core_version = core::info(
            self.settings.get().core_path.as_deref(),
            self.resource_dir.as_deref(),
        )
        .version;
        let mixed_port = if runtime.mixed_port != 0 {
            Some(runtime.mixed_port)
        } else {
            Self::mixed_port_from_config(Path::new(&runtime.config_path))
        };
        let state = self.set_state(EngineState {
            status: "connected".into(),
            mode: Some(runtime.mode.clone()),
            mixed_port,
            profile_id: Some(runtime.profile_id),
            profile_name: profile.map(|p| p.name),
            started_at: Some(runtime.started_at),
            pid: Some(runtime.pid),
            error: None,
            proxy,
            core_version,
        });
        self.logs.push(
            "info",
            "engine",
            format!(
                "adopted running sing-box pid {} ({}) from a previous session",
                runtime.pid, runtime.mode
            ),
        );
        if runtime.mode == "tun" {
            if let Some(log_file) = &runtime.log_file {
                self.register_task(self.spawn_log_tailer(PathBuf::from(log_file), true));
            }
        }
        self.start_live_tasks(client, runtime.started_at);
        self.register_task(self.spawn_monitor(runtime.pid));
        let _ = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_runtime_file_without_mixed_port_is_accepted() {
        let raw = r#"{
            "pid": 123,
            "mode": "system",
            "profileId": 2,
            "startedAt": 100,
            "apiPort": 19090,
            "secret": "redacted",
            "configPath": "/tmp/config.json",
            "pidFile": null,
            "logFile": null
        }"#;
        let runtime: RuntimeFile = serde_json::from_str(raw).unwrap();
        assert_eq!(runtime.mixed_port, 0);
        assert_eq!(runtime.api_port, 19090);
    }

    #[test]
    fn adoption_recovers_mixed_port_from_runtime_config() {
        let dir = std::env::temp_dir().join(format!("veil-runtime-port-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(
            &path,
            r#"{"inbounds":[{"type":"mixed","tag":"veil-mixed","listen":"127.0.0.1","listen_port":17890}]}"#,
        )
        .unwrap();
        assert_eq!(Engine::mixed_port_from_config(&path), Some(17890));

        fs::write(&path, r#"{"inbounds":[]}"#).unwrap();
        assert_eq!(Engine::mixed_port_from_config(&path), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_engine_state_without_mixed_port_deserializes_to_none() {
        let raw = r#"{
            "status": "connected",
            "mode": "system",
            "profileId": 2,
            "profileName": "test",
            "startedAt": 100,
            "pid": 123,
            "error": null,
            "proxy": null,
            "coreVersion": "1.14.5"
        }"#;
        let state: EngineState = serde_json::from_str(raw).unwrap();
        assert_eq!(state.mixed_port, None);
    }
}
