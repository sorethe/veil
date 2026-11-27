//! Real engine lifecycle harness.
//!
//! Exercises the actual Engine against the actual sing-box binary, without a
//! GUI: status event sequence, Clash API health, log capture, stats events,
//! graceful disconnect, and unexpected-kill error + cleanup.
//!
//! Run with isolated XDG dirs and the vendored core:
//!   XDG_DATA_HOME=/tmp/veil-lifecycle-data \
//!   XDG_CONFIG_HOME=/tmp/veil-lifecycle-config \
//!   XDG_CURRENT_DESKTOP=Hyprland \
//!   VEIL_SINGBOX=$PWD/vendor/sing-box-1.14.5-linux-amd64/sing-box \
//!     cargo run --example engine_lifecycle --manifest-path src-tauri/Cargo.toml
//!
//! The system proxy is deliberately forced to the unsupported path so the
//! harness never modifies the desktop proxy.

use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use veil_lib::db::Db;
use veil_lib::engine::{Engine, EventSink};
use veil_lib::logs::LogStore;
use veil_lib::paths;
use veil_lib::settings::SettingsStore;

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<(String, Value)>>,
}

impl EventSink for Recorder {
    fn emit(&self, event: &str, payload: Value) {
        self.events
            .lock()
            .unwrap()
            .push((event.to_string(), payload));
    }
}

impl Recorder {
    fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    fn states(&self) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == "engine://state")
            .map(|(_, payload)| payload.clone())
            .collect()
    }

    fn events_named(&self, name: &str) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == name)
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

struct KillGuard {
    pid: Option<u32>,
}

impl Default for KillGuard {
    fn default() -> Self {
        Self { pid: None }
    }
}

impl KillGuard {
    fn arm(&mut self, pid: u32) {
        self.pid = Some(pid);
    }
    fn disarm(&mut self) {
        self.pid = None;
    }
}

impl Drop for KillGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .expect("free port")
}

fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let rc = unsafe { libc::kill(pid as i32, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn find_core() -> PathBuf {
    if let Ok(path) = std::env::var("VEIL_SINGBOX") {
        return PathBuf::from(path);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("project root")
        .to_path_buf();
    let vendor = root.join("vendor");
    if let Ok(entries) = std::fs::read_dir(&vendor) {
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("sing-box-"))
            .map(|entry| entry.path().join("sing-box"))
            .filter(|path| path.is_file())
            .collect();
        found.sort();
        if let Some(path) = found.pop() {
            return path;
        }
    }
    panic!("sing-box not found; set VEIL_SINGBOX=/path/to/sing-box");
}

fn statuses(recorder: &Recorder) -> Vec<String> {
    recorder
        .states()
        .iter()
        .filter_map(|state| state["status"].as_str().map(str::to_string))
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let core = find_core();
    println!("core: {}", core.display());

    let temp = std::env::temp_dir().join(format!("veil-lifecycle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(temp.join("data"))?;
    std::fs::create_dir_all(temp.join("config"))?;
    std::fs::create_dir_all(temp.join("home"))?;
    std::env::set_var("XDG_DATA_HOME", temp.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", temp.join("config"));
    std::env::set_var("HOME", temp.join("home"));
    std::env::set_var("XDG_CURRENT_DESKTOP", "Hyprland");
    std::env::set_var("VEIL_SINGBOX", core.to_string_lossy().to_string());

    let mixed_port = free_port();
    let api_port = free_port();
    let settings = Arc::new(SettingsStore::with_path(
        temp.join("config/veil/settings.json"),
    ));
    settings.update(&json!({
        "corePath": core.to_string_lossy(),
        "mode": "system",
        "mixedPort": mixed_port,
        "apiPort": api_port,
        "notifications": false,
        "connectOnLaunch": false,
        "logLevel": "info"
    }))?;
    let db = Arc::new(Db::open(&temp.join("data/veil/veil.db"))?);
    let content = json!({
        "outbounds": [
            { "type": "direct", "tag": "direct" },
            { "type": "selector", "tag": "Proxy", "outbounds": ["direct"] }
        ],
        "route": { "final": "direct" }
    })
    .to_string();
    let profile = db.create_profile("Lifecycle Direct", "local", &content, "harness", None, false)?;
    let logs = Arc::new(LogStore::new());
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn EventSink> = recorder.clone();
    let engine = Engine::new(sink, settings.clone(), db.clone(), logs.clone(), None);
    let mut guard = KillGuard::default();

    // 1. Connect: starting -> connected, only after Clash health.
    recorder.clear();
    let connected = engine
        .connect(Some("system".into()), Some(profile.id))
        .await?;
    assert_eq!(connected.status, "connected");
    let pid = connected.pid.expect("connected pid");
    guard.arm(pid);
    let connect_states = statuses(&recorder);
    println!("connect status events: {connect_states:?}");
    assert_eq!(connect_states.first().map(String::as_str), Some("starting"));
    assert_eq!(connect_states.last().map(String::as_str), Some("connected"));
    assert!(connect_states.contains(&"starting".to_string()));
    assert!(!connect_states.contains(&"error".to_string()));

    let outbounds = engine.outbounds().await?;
    println!("clash /proxies outbounds: {}", outbounds.len());
    assert!(!outbounds.is_empty(), "Clash API must list outbounds");

    // 2. Real engine logs are captured from stdout/stderr.
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut engine_logs = Vec::new();
    while Instant::now() < deadline {
        engine_logs = logs.snapshot(None);
        if engine_logs
            .iter()
            .any(|entry| entry.source == "engine" && entry.message.contains("started"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("captured log entries: {}", engine_logs.len());
    for entry in engine_logs.iter().take(4) {
        println!("  [{}] {}", entry.level, entry.message);
    }
    assert!(
        engine_logs
            .iter()
            .any(|entry| entry.source == "engine" && entry.message.contains("started")),
        "expected real sing-box startup log lines"
    );

    // 3. Stats event comes from the Clash API /traffic + /connections tasks.
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut stats_event = None;
    while Instant::now() < deadline {
        if let Some(value) = recorder.events_named("engine://stats").last() {
            stats_event = Some(value.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    let stats_event = stats_event.expect("engine://stats event while connected");
    println!("stats event: {stats_event}");
    for key in ["up", "down", "upSpeed", "downSpeed", "connections", "duration"] {
        assert!(
            stats_event.get(key).is_some(),
            "stats event missing {key}"
        );
    }

    // 4. Graceful disconnect: stopping -> idle, process gone, runtime clean.
    recorder.clear();
    let stopped = engine.disconnect().await?;
    assert_eq!(stopped.status, "idle");
    let stop_states = statuses(&recorder);
    println!("disconnect status events: {stop_states:?}");
    assert!(stop_states.contains(&"stopping".to_string()));
    assert_eq!(stop_states.last().map(String::as_str), Some("idle"));
    assert!(!pid_alive(pid), "sing-box pid must be gone after disconnect");
    assert!(!paths::runtime_file_path().exists());
    assert!(!paths::runtime_config_path().exists());
    guard.disarm();

    // 5. Unexpected kill -> error state, proxy/runtime cleanup.
    recorder.clear();
    let connected = engine
        .connect(Some("system".into()), Some(profile.id))
        .await?;
    let pid = connected.pid.expect("second connected pid");
    guard.arm(pid);
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut error_state = None;
    while Instant::now() < deadline {
        let state = engine.snapshot();
        if state.status == "error" {
            error_state = Some(state);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let error_state = error_state.expect("unexpected kill must produce error state");
    println!(
        "unexpected-exit status={} error={:?}",
        error_state.status, error_state.error
    );
    assert!(error_state
        .error
        .as_deref()
        .unwrap_or("")
        .to_lowercase()
        .contains("exited"));
    let kill_states = statuses(&recorder);
    println!("unexpected-exit status events: {kill_states:?}");
    assert!(kill_states.contains(&"error".to_string()));
    assert!(!paths::runtime_file_path().exists());
    assert!(!paths::runtime_config_path().exists());
    guard.disarm();

    let _ = engine.disconnect().await;
    let _ = std::fs::remove_dir_all(&temp);
    println!("ENGINE LIFECYCLE OK");
    Ok(())
}
