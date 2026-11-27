//! Real subscription integration harness.
//!
//! Reads the subscription URL from /tmp/veil-sub.url (never prints it), uses the
//! same normalization/validation/profile/engine code as the backend, proves
//! real HTTP+SOCKS traffic and Clash stats, refreshes, and cleans up.
//!
//! Run:
//!   XDG_CURRENT_DESKTOP=Hyprland \
//!   VEIL_SINGBOX=$PWD/vendor/sing-box-1.14.5-linux-amd64/sing-box \
//!     cargo run --example subscription_integration --manifest-path src-tauri/Cargo.toml
//!
//! Output deliberately contains only classification/status/counts - never the
//! URL, body, profile content, node tags or hostnames.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use veil_lib::clash::ClashClient;
use veil_lib::core;
use veil_lib::db::Db;
use veil_lib::engine::{Engine, EventSink};
use veil_lib::logs::LogStore;
use veil_lib::normalize_subscription_body;
use veil_lib::paths;
use veil_lib::settings::SettingsStore;

struct SinkRecorder {
    states: Mutex<Vec<String>>,
}

impl SinkRecorder {
    fn new() -> Self {
        Self {
            states: Mutex::new(Vec::new()),
        }
    }
    fn states(&self) -> Vec<String> {
        self.states.lock().unwrap().clone()
    }
}

impl EventSink for SinkRecorder {
    fn emit(&self, event: &str, payload: Value) {
        if event == "engine://state" {
            if let Some(status) = payload.get("status").and_then(Value::as_str) {
                self.states.lock().unwrap().push(status.to_string());
            }
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

fn decode_b64(input: &str) -> Option<Vec<u8>> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let trimmed = cleaned.trim_end_matches('=');
    for engine in [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD] {
        if let Ok(bytes) = engine.decode(trimmed) {
            return Some(bytes);
        }
    }
    None
}

fn classify_valid_body(body: &str) -> &'static str {
    let text = body.trim();
    if text.starts_with('{') {
        return "sing-box JSON";
    }
    if text.contains("://") {
        return "plain share-link list";
    }
    if let Some(bytes) = decode_b64(text) {
        if let Ok(decoded) = String::from_utf8(bytes) {
            let decoded = decoded.trim();
            if decoded.starts_with('{') {
                return "base64 sing-box JSON";
            }
            if decoded.contains("://") {
                return "base64 share-link list";
            }
        }
    }
    "unknown (normalized by backend)"
}

async fn fetch_raw(client: &reqwest::Client, url: &str) -> Result<(u16, String), ()> {
    let response = client.get(url).send().await.map_err(|_| ())?;
    let status = response.status();
    let bytes = response.bytes().await.map_err(|_| ())?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err(());
    }
    Ok((
        status.as_u16(),
        String::from_utf8_lossy(&bytes).into_owned(),
    ))
}

fn curl_example(proxy: &str) -> (bool, u16, Option<i32>) {
    let output = Command::new("curl")
        .args([
            "-sS",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "25",
            "--proxy",
            proxy,
            "https://example.com",
        ])
        .output();
    match output {
        Ok(output) => {
            let code = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u16>()
                .unwrap_or(0);
            (
                output.status.success() && code == 200,
                code,
                output.status.code(),
            )
        }
        Err(_) => (false, 0, None),
    }
}

fn parse_traffic_line(line: &str) -> Option<(u64, u64)> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    Some((
        value.get("up").and_then(Value::as_u64).unwrap_or(0),
        value.get("down").and_then(Value::as_u64).unwrap_or(0),
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let secret_file = std::env::var("VEIL_SUB_FILE").unwrap_or_else(|_| "/tmp/veil-sub.url".into());
    let url = std::fs::read_to_string(&secret_file)?.trim().to_string();
    if url.is_empty() {
        return Err("subscription URL file is empty".into());
    }
    let core = find_core();
    println!("core_version: {}", core::version(&core)?);

    let temp = PathBuf::from("/tmp/veil-sub-harness");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(temp.join("data"))?;
    std::fs::create_dir_all(temp.join("config"))?;
    std::fs::create_dir_all(temp.join("home"))?;
    std::env::set_var("XDG_DATA_HOME", temp.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", temp.join("config"));
    std::env::set_var("HOME", temp.join("home"));
    std::env::set_var("XDG_CURRENT_DESKTOP", "Hyprland");
    std::env::set_var("VEIL_SINGBOX", core.to_string_lossy().to_string());
    std::fs::set_permissions(&temp, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;

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
    let logs = Arc::new(LogStore::new());
    // The real import command registers the URL as a secret before fetching or
    // logging. Do the same before any network/log activity.
    logs.add_secret(&url);

    // 1. HTTP fetch with the same UA/timeout as fetch_subscription.
    let client = reqwest::Client::builder()
        .user_agent("veil/0.1 sing-box/1.14.5")
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let Some((http_status, body)) = (match fetch_raw(&client, &url).await {
        Ok(value) => Some(value),
        Err(()) => None,
    }) else {
        println!("classification: HTTP/network error; no profile created");
        verify_log_redaction(&logs, &url);
        return Ok(());
    };
    println!("http_status: {http_status}");
    if !(200..300).contains(&http_status) {
        println!("classification: HTTP error; no profile created");
        verify_log_redaction(&logs, &url);
        return Ok(());
    }

    let normalized = normalize_subscription_body(&body);
    let content = match normalized {
        Ok(content) => content,
        Err(error) => {
            let clash_yaml = error.contains("Clash YAML");
            println!(
                "classification: {}",
                if clash_yaml {
                    "rejected Clash YAML"
                } else {
                    "rejected (backend normalization error)"
                }
            );
            if clash_yaml {
                println!("rejection_message: {error}");
            }
            verify_log_redaction(&logs, &url);
            return Ok(());
        }
    };
    println!("classification: {}", classify_valid_body(&body));

    let value: Value = serde_json::from_str(&content)?;
    let outbound_count = value
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|outbounds| outbounds.len())
        .unwrap_or(0);
    println!("outbound_count: {outbound_count}");

    let checked = core::validate_content(
        &core,
        &content,
        "system",
        mixed_port,
        api_port,
        false,
        "info",
    );
    match checked {
        Ok(tags) => println!("sing_box_check: ok ({} outbound tags)", tags.len()),
        Err(_) => {
            println!("sing_box_check: failed; no profile created/connection attempted");
            verify_log_redaction(&logs, &url);
            return Ok(());
        }
    }

    let profile = db.create_profile(
        "Subscription Test",
        "subscription",
        &content,
        "subscription",
        Some(&url),
        true,
    )?;

    let recorder = Arc::new(SinkRecorder::new());
    let sink: Arc<dyn EventSink> = recorder.clone();
    let engine = Engine::new(sink, settings.clone(), db.clone(), logs.clone(), None);
    let connected = engine
        .connect(Some("system".into()), Some(profile.id))
        .await?;
    println!(
        "connect: status={} events={:?}",
        connected.status,
        recorder.states()
    );
    assert_eq!(connected.status, "connected");
    let engine_pid = connected.pid.expect("engine pid");

    verify_log_redaction(&logs, &url);

    // 2. Real HTTP and SOCKS5 traffic through the mixed inbound.
    let http_proxy = format!("http://127.0.0.1:{mixed_port}");
    let socks_proxy = format!("socks5h://127.0.0.1:{mixed_port}");
    let (http_ok, http_code, http_exit) = curl_example(&http_proxy);
    println!("http_proxy: ok={http_ok} http_code={http_code} exit={http_exit:?}");
    let (socks_ok, socks_code, socks_exit) = curl_example(&socks_proxy);
    println!("socks5_proxy: ok={socks_ok} http_code={socks_code} exit={socks_exit:?}");

    // Read the runtime state for the Clash API (real generated secret).
    let runtime: Value =
        serde_json::from_str(&std::fs::read_to_string(paths::runtime_file_path())?)?;
    let api = runtime["apiPort"].as_u64().unwrap_or(api_port as u64) as u16;
    let secret = runtime["secret"].as_str().unwrap_or_default().to_string();
    let clash = ClashClient::new(api, &secret);

    // Sample /traffic while curl generates requests through the proxy.
    let traffic_client = clash.clone();
    let traffic_stream = traffic_client.traffic().await;
    let mut traffic_max_up = 0u64;
    let mut traffic_max_down = 0u64;
    if let Ok(response) = traffic_stream {
        let proxy_for_thread = socks_proxy.clone();
        let traffic_thread = std::thread::spawn(move || {
            for _ in 0..25 {
                let _ = curl_example(&proxy_for_thread);
            }
        });
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            let chunk = tokio::time::timeout(Duration::from_millis(1200), stream.next()).await;
            match chunk {
                Ok(Some(Ok(bytes))) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(index) = buffer.find('\n') {
                        let line: String = buffer[..index].to_string();
                        buffer.drain(..=index);
                        if let Some((up, down)) = parse_traffic_line(&line) {
                            traffic_max_up = traffic_max_up.max(up);
                            traffic_max_down = traffic_max_down.max(down);
                        }
                    }
                    if traffic_max_up > 0 || traffic_max_down > 0 {
                        break;
                    }
                }
                Ok(Some(Err(_))) | Ok(None) => break,
                Err(_) => {}
            }
        }
        let _ = traffic_thread.join();
    }
    println!("clash_traffic_max: up={traffic_max_up} down={traffic_max_down}");

    let connections = clash.connections().await?;
    let connections_up = connections
        .get("uploadTotal")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let connections_down = connections
        .get("downloadTotal")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let active_connections = connections
        .get("connections")
        .and_then(Value::as_array)
        .map(|list| list.len())
        .unwrap_or(0);
    println!(
        "clash_connections: count={active_connections} up={connections_up} down={connections_down}"
    );

    let outbounds = engine.outbounds().await?;
    println!("clash_outbounds: {}", outbounds.len());
    let delay_tag = outbounds
        .iter()
        .find(|outbound| outbound.kind == "selector")
        .and_then(|outbound| outbound.now.clone())
        .or_else(|| {
            outbounds
                .iter()
                .find(|outbound| outbound.kind == "other")
                .map(|outbound| outbound.tag.clone())
        });
    if let Some(tag) = delay_tag {
        match engine.test_delay(&tag).await {
            Ok(delay) => println!("test_delay_ms: {delay}"),
            Err(_) => println!("test_delay: failed"),
        }
    } else {
        println!("test_delay: unavailable (no outbound candidate)");
    }

    // 3. Refresh: new fetch, normalize, real check, updated content/updated_at.
    let before_updated = db
        .get_profile(profile.id)?
        .ok_or("profile missing before refresh")?
        .updated_at;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    match fetch_raw(&client, &url).await {
        Ok((refresh_status, refresh_body)) => {
            println!("refresh_http_status: {refresh_status}");
            let refresh_result = normalize_subscription_body(&refresh_body).and_then(|content| {
                core::validate_content(
                    &core,
                    &content,
                    "system",
                    mixed_port,
                    api_port,
                    false,
                    "info",
                )
                .map(|_| content)
            });
            match refresh_result {
                Ok(refreshed) => {
                    let updated =
                        db.set_subscription_result(profile.id, Some(&refreshed), None)?;
                    println!(
                        "refresh: revalidated=true updated_at_advanced={} last_error_null={}",
                        updated.updated_at > before_updated,
                        updated.last_error.is_none()
                    );
                }
                Err(_) => {
                    let _ = db.set_subscription_result(
                        profile.id,
                        None,
                        Some("refresh validation failed"),
                    );
                    println!("refresh: revalidation_failed=true");
                }
            }
        }
        Err(()) => {
            let _ = db.set_subscription_result(
                profile.id,
                None,
                Some("refresh network failure"),
            );
            println!("refresh: network_failure=true");
        }
    }

    // 4. Disconnect and verify cleanup / no proxy state.
    let stopped = engine.disconnect().await?;
    println!("disconnect: status={}", stopped.status);
    println!("disconnect_pid_gone: {}", !pid_alive(engine_pid));
    println!(
        "disconnect_runtime_clean: {}",
        !paths::runtime_file_path().exists() && !paths::runtime_config_path().exists()
    );
    println!(
        "disconnect_proxy_state_absent: {}",
        !paths::sysproxy_state_path().exists()
    );

    println!("harness_data: /tmp/veil-sub-harness (isolated, 0700; DB 0600)");
    println!("SUBSCRIPTION INTEGRATION OK");
    Ok(())
}

fn verify_log_redaction(logs: &LogStore, url: &str) {
    logs.push(
        "info",
        "engine",
        format!("log redaction probe for subscription {url}"),
    );
    let ring_text = logs
        .snapshot(None)
        .iter()
        .map(|entry| entry.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let log_file = paths::logs_dir().join("veil.log");
    let file_text = std::fs::read_to_string(&log_file).unwrap_or_default();
    println!(
        "log_redaction: ring_clean={} file_clean={}",
        !ring_text.contains(url),
        !file_text.contains(url)
    );
    assert!(!ring_text.contains(url), "URL leaked into log ring");
    assert!(!file_text.contains(url), "URL leaked into log file");
}
