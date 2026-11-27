//! Real engine smoke test.
//!
//! Starts the actual sing-box binary with a generated runtime config (direct
//! outbound, free mixed port, Clash API), waits for the Clash API /version
//! health check, reads a real endpoint, then stops the process with SIGTERM.
//!
//! This exercises the same primitives as the app's engine without a display,
//! polkit or root: config generation, real `sing-box check`, process spawn,
//! log capture, Clash API health, /version and /connections, graceful stop.
//!
//! Run:
//!   VEIL_SINGBOX=$PWD/vendor/sing-box-1.14.5-linux-amd64/sing-box \
//!     cargo run --example engine_smoke --manifest-path src-tauri/Cargo.toml

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use futures_util::StreamExt;
use veil_lib::clash::ClashClient;
use veil_lib::configgen::{self, GenOptions};
use veil_lib::core;

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .expect("bind a free local port")
}

fn find_core() -> PathBuf {
    if let Ok(path) = std::env::var("VEIL_SINGBOX") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("sing-box");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri parent")
        .to_path_buf();
    let vendor = project.join("vendor");
    if let Ok(entries) = std::fs::read_dir(&vendor) {
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("sing-box-"))
            .map(|e| e.path().join("sing-box"))
            .filter(|p| p.is_file())
            .collect();
        found.sort();
        if let Some(path) = found.pop() {
            return path;
        }
    }
    panic!("sing-box not found; set VEIL_SINGBOX=/path/to/sing-box");
}

fn collect(
    stream: impl std::io::Read + Send + 'static,
    sink: Arc<Mutex<Vec<String>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            println!("  sing-box: {line}");
            sink.lock().unwrap().push(line);
        }
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let core_path = find_core();
    println!("core: {}", core_path.display());
    let version = core::version(&core_path)?;
    println!("version: {version}");
    assert!(!version.is_empty(), "empty sing-box version");

    let api_port = free_port();
    let mixed_port = free_port();
    let opts = GenOptions {
        mode: "system".into(),
        mixed_port,
        api_port,
        auto_redirect: false,
        log_level: "info".into(),
    };
    let raw_profile = r#"{
        "outbounds": [{ "type": "direct", "tag": "direct" }],
        "route": { "final": "direct" }
    }"#;
    let generated = configgen::generate(raw_profile, &opts)?;
    assert!(generated.json.contains("veil-mixed"));
    assert!(generated.json.contains(&api_port.to_string()));

    let dir = std::env::temp_dir().join(format!("veil-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let config_path = dir.join("config.json");
    configgen::write_runtime_config(&config_path, &generated.json)?;
    core::check(&core_path, &config_path)?;
    println!("config check: ok ({mixed_port} mixed, clash api {api_port})");

    let mut child = Command::new(&core_path)
        .arg("run")
        .arg("-c")
        .arg(&config_path)
        .arg("--disable-color")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pid = child.id();
    struct KillOnDrop {
        pid: u32,
        armed: bool,
    }
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            if self.armed {
                unsafe { libc::kill(self.pid as i32, libc::SIGTERM) };
            }
        }
    }
    let mut guard = KillOnDrop { pid, armed: true };
    let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let stdout_reader = child.stdout.take().map(|s| collect(s, logs.clone()));
    let stderr_reader = child.stderr.take().map(|s| collect(s, logs.clone()));

    let client = ClashClient::new(api_port, &generated.secret);
    let health = client.wait_ready(Duration::from_secs(12)).await;
    let health = match health {
        Ok(value) => value,
        Err(error) => {
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            let _ = child.wait();
            return Err(format!("health check failed: {error}").into());
        }
    };
    println!("clash /version: {health}");
    let api_version = health
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_start_matches("sing-box ")
        .to_string();
    assert_eq!(
        api_version, version,
        "Clash API version must match the binary"
    );

    let connections = client.connections().await?;
    let count = connections
        .get("connections")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    println!("clash /connections: {count} active");

    let proxies = client.proxies().await?;
    assert!(
        proxies.get("proxies").is_some(),
        "Clash /proxies must be available"
    );

    // Prove the live stats path: /traffic is a long-lived NDJSON stream.
    let traffic = client.traffic().await?;
    let mut stream = traffic.bytes_stream();
    let mut buffer = String::new();
    let first = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));
            if let Some(idx) = buffer.find('\n') {
                let line = buffer[..idx].to_string();
                return Ok::<String, Box<dyn std::error::Error>>(line);
            }
        }
        Err("traffic stream ended".into())
    })
    .await;
    let line = match first {
        Ok(Ok(line)) => line,
        Ok(Err(e)) => return Err(format!("traffic stream: {e}").into()),
        Err(_) => return Err("no /traffic line within 5s".into()),
    };
    let sample: serde_json::Value = serde_json::from_str(line.trim())?;
    assert!(sample.get("up").is_some() && sample.get("down").is_some());
    println!("clash /traffic sample: {sample}");

    // Stop exactly like the app: SIGTERM, then wait up to 8s.
    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut exited = false;
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            exited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
        return Err("sing-box did not stop within 8s after SIGTERM".into());
    }
    std::thread::sleep(Duration::from_millis(300));
    let lines = logs.lock().unwrap().clone();
    if let Some(reader) = stdout_reader {
        let _ = reader.join();
    }
    if let Some(reader) = stderr_reader {
        let _ = reader.join();
    }
    let line_count = lines.len();
    assert!(line_count > 0, "expected captured sing-box log lines");
    println!("captured logs: {line_count} lines");
    println!("sample: {:?}", &lines[..lines.len().min(3)]);

    guard.armed = false;
    let _ = std::fs::remove_dir_all(&dir);
    println!("ENGINE SMOKE OK");
    Ok(())
}
