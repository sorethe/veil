pub mod clash;
pub mod configgen;
pub mod core;
pub mod db;
pub mod diagnostics;
pub mod errors;
pub mod engine;
pub mod links;
pub mod logs;
pub mod paths;
pub mod settings;
pub mod sysproxy;
pub mod tray;

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use clash::{Conn, Outbound, RouteInfo};
use db::{Db, Profile};
use engine::{Engine, EngineState, Stats};
use errors::CommandError;
use logs::{LogEntry, LogStore};
use serde_json::{json, Value};
use settings::{Settings, SettingsStore};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub engine: Arc<Engine>,
    pub settings: Arc<SettingsStore>,
    pub db: Arc<Db>,
    pub resource_dir: Option<PathBuf>,
}

const CLASH_YAML_ERROR: &str =
    "Clash YAML is not sing-box JSON. Veil does not convert Clash/Clash.Meta configs; \
     export a sing-box profile or use share links instead.";

fn validate_profile_content(
    settings: &Settings,
    resource_dir: Option<&Path>,
    content: &str,
) -> Result<Vec<String>, String> {
    let core_path = core::find_core(settings.core_path.as_deref(), resource_dir).ok_or_else(|| {
        "sing-box binary not found; cannot validate the profile. Set corePath in settings."
            .to_string()
    })?;
    core::validate_content(
        &core_path,
        content,
        &settings.mode,
        settings.mixed_port,
        settings.api_port,
        settings.auto_redirect,
        &settings.log_level,
    )
}

fn emit_profiles(app: &AppHandle, db: &Db) {
    if let Ok(profiles) = db.list_profiles() {
        let _ = app.emit("profiles://changed", &profiles);
    }
}

fn autostart_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".").join(".config"))
        .join("autostart")
        .join("veil.desktop")
}

fn write_autostart(enabled: bool) -> Result<bool, String> {
    let path = autostart_path();
    if !enabled {
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("remove autostart file: {e}"))?;
        }
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create autostart dir: {e}"))?;
    }
    let exe = std::env::current_exe().map_err(|e| format!("resolve executable path: {e}"))?;
    let exe_str = exe.to_string_lossy();
    let exec = if exe_str.contains(' ') {
        format!("\"{exe_str}\"")
    } else {
        exe_str.into_owned()
    };
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Veil\n\
         Comment=Veil proxy client\n\
         Exec={exec}\n\
         Icon=veil\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n\
         X-GNOME-Autostart-Delay=3\n\
         Categories=Network;\n"
    );
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(&path)
        .map_err(|e| format!("write autostart file: {e}"))?;
    file.write_all(entry.as_bytes())
        .map_err(|e| format!("write autostart file: {e}"))?;
    Ok(path.exists())
}

fn looks_like_clash_yaml(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return false;
    }
    let mut saw_proxies = false;
    for line in trimmed.lines().take(120) {
        let line = line.trim_start();
        if line.starts_with("proxies:") {
            saw_proxies = true;
        }
        if line.starts_with("proxy-groups:")
            || line.starts_with("mixed-port:")
            || line.starts_with("rules:")
            || (saw_proxies && line.starts_with("port:"))
        {
            return true;
        }
    }
    saw_proxies
}

fn decode_base64_any(input: &str) -> Option<Vec<u8>> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let trimmed = cleaned.trim_end_matches('=');
    for engine in [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD] {
        if let Ok(bytes) = engine.decode(trimmed) {
            return Some(bytes);
        }
    }
    None
}

fn validate_json_subscription(text: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("subscription returned invalid JSON: {e}"))?;
    if !value.is_object() {
        return Err("subscription JSON must be an object".into());
    }
    let outbounds = value.get("outbounds").and_then(Value::as_array);
    if outbounds.map(|a| a.is_empty()).unwrap_or(true) {
        return Err("subscription JSON has no outbounds array".into());
    }
    Ok(text.to_string())
}

/// Turn a subscription body into sing-box JSON. Supports JSON, base64
/// (JSON or share-link lists), and plain share-link lists. Clash YAML is
/// rejected explicitly instead of being mis-converted.
pub fn normalize_subscription_body(body: &str) -> Result<String, String> {
    let text = body.trim();
    if text.is_empty() {
        return Err("subscription response is empty".into());
    }
    if text.starts_with('{') {
        return validate_json_subscription(text);
    }
    if looks_like_clash_yaml(text) {
        return Err(CLASH_YAML_ERROR.into());
    }
    if text.contains("://") {
        return links::links_to_config_json(text);
    }
    if let Some(bytes) = decode_base64_any(text) {
        if let Ok(decoded) = String::from_utf8(bytes) {
            let decoded = decoded.trim();
            if decoded.starts_with('{') {
                return validate_json_subscription(decoded);
            }
            if looks_like_clash_yaml(decoded) {
                return Err(CLASH_YAML_ERROR.into());
            }
            if decoded.contains("://") {
                return links::links_to_config_json(decoded);
            }
        }
    }
    Err("unrecognized subscription payload: expected sing-box JSON, base64-encoded JSON, or share links".into())
}

async fn fetch_subscription(url: &str) -> Result<String, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("subscription URL must start with http:// or https://".into());
    }
    let client = reqwest::Client::builder()
        .user_agent("veil/0.1 sing-box/1.14.5")
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("create HTTP client: {e}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("subscription request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "subscription request failed: HTTP {}",
            status.as_u16()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("read subscription response: {e}"))?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("subscription is larger than 20 MB".into());
    }
    normalize_subscription_body(&String::from_utf8_lossy(&bytes))
}

fn derive_subscription_name(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "Subscription".into())
}

fn import_links_profile(
    app: &AppHandle,
    state: &AppState,
    name: &str,
    text: &str,
    source: &str,
) -> Result<Profile, CommandError> {
    let content = links::links_to_config_json(text).map_err(CommandError::from)?;
    validate_profile_content(&state.settings.get(), state.resource_dir.as_deref(), &content)
        .map_err(CommandError::validation)?;
    let profile = state
        .db
        .create_profile(name, "links", &content, source, None, false)
        .map_err(CommandError::io)?;
    emit_profiles(app, &state.db);
    Ok(profile)
}

#[tauri::command]
async fn bootstrap(state: State<'_, AppState>) -> Result<Value, CommandError> {
    let settings = state.settings.get();
    let profiles = state
        .db
        .list_profiles()
        .map_err(CommandError::io)?;
    let engine = state.engine.snapshot();
    let core_info = core::info(settings.core_path.as_deref(), state.resource_dir.as_deref());
    let engine_info = core::engine_info(settings.core_path.as_deref(), state.resource_dir.as_deref());
    let platform = sysproxy::platform_info();
    Ok(json!({
        "settings": settings,
        "profiles": profiles,
        "engine": engine,
        "core": core_info,
        "engineInfo": engine_info,
        "platform": platform,
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[tauri::command]
async fn get_state(state: State<'_, AppState>) -> Result<EngineState, CommandError> {
    Ok(state.engine.snapshot())
}

#[tauri::command]
async fn connect(
    state: State<'_, AppState>,
    mode: Option<String>,
    profile_id: Option<i64>,
) -> Result<EngineState, CommandError> {
    state.engine.connect(mode, profile_id).await.map_err(CommandError::from_engine)
}

#[tauri::command]
async fn disconnect(state: State<'_, AppState>) -> Result<EngineState, CommandError> {
    state.engine.disconnect().await.map_err(CommandError::from_engine)
}

#[tauri::command]
async fn get_stats(state: State<'_, AppState>) -> Result<Stats, CommandError> {
    Ok(state.engine.stats())
}

#[tauri::command]
async fn get_connections(state: State<'_, AppState>) -> Result<Vec<Conn>, CommandError> {
    state.engine.connections().await.map_err(CommandError::from_engine)
}

#[tauri::command]
async fn close_connection(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state.engine.close_connection(&id).await.map_err(CommandError::from_engine)
}

#[tauri::command]
async fn get_logs(
    state: State<'_, AppState>,
    since_seq: Option<u64>,
) -> Result<Vec<LogEntry>, CommandError> {
    Ok(state.engine.logs.snapshot(since_seq))
}

#[tauri::command]
async fn clear_logs(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.engine.logs.clear();
    Ok(())
}

#[tauri::command]
async fn export_logs(state: State<'_, AppState>) -> Result<String, CommandError> {
    state.engine.logs.export().map_err(CommandError::from)
}

#[tauri::command]
async fn get_route(state: State<'_, AppState>) -> Result<RouteInfo, CommandError> {
    state.engine.route().await.map_err(CommandError::from_engine)
}

#[tauri::command]
async fn get_outbounds(state: State<'_, AppState>) -> Result<Vec<Outbound>, CommandError> {
    state.engine.outbounds().await.map_err(CommandError::from_engine)
}

#[tauri::command]
async fn select_outbound(
    state: State<'_, AppState>,
    group: String,
    tag: String,
) -> Result<(), CommandError> {
    state.engine.select_outbound(&group, &tag).await.map_err(CommandError::from_engine)
}

#[tauri::command]
async fn test_delay(state: State<'_, AppState>, tag: String) -> Result<u64, CommandError> {
    state.engine.test_delay(&tag).await.map_err(CommandError::from_engine)
}

#[tauri::command]
async fn set_clash_mode(state: State<'_, AppState>, mode: String) -> Result<(), CommandError> {
    if !matches!(mode.as_str(), "rule" | "global" | "direct") {
        return Err(CommandError::validation(format!(
            "unknown clash mode '{mode}' (expected rule, global or direct)"
        )));
    }
    state
        .engine
        .set_clash_mode(&mode)
        .await
        .map_err(CommandError::from_engine)
}

#[tauri::command]
async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<Profile>, CommandError> {
    state.db.list_profiles().map_err(CommandError::from)
}

#[tauri::command]
async fn create_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    content: String,
) -> Result<Profile, CommandError> {
    validate_profile_content(&state.settings.get(), state.resource_dir.as_deref(), &content)
        .map_err(CommandError::validation)?;
    let profile = state
        .db
        .create_profile(&name, "local", &content, "manual", None, false)
        .map_err(CommandError::io)?;
    emit_profiles(&app, &state.db);
    Ok(profile)
}

#[tauri::command]
async fn update_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    name: String,
    content: Option<String>,
) -> Result<Profile, CommandError> {
    if let Some(content) = &content {
        validate_profile_content(&state.settings.get(), state.resource_dir.as_deref(), content)
            .map_err(CommandError::validation)?;
    }
    let profile = state
        .db
        .update_profile(id, &name, content.as_deref())
        .map_err(CommandError::from)?;
    emit_profiles(&app, &state.db);
    Ok(profile)
}

#[tauri::command]
async fn set_profile_auto_update(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<Profile, CommandError> {
    let profile = state
        .db
        .set_profile_auto_update(id, enabled)
        .map_err(CommandError::from)?;
    emit_profiles(&app, &state.db);
    Ok(profile)
}

#[tauri::command]
async fn delete_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    state.db.delete_profile(id).map_err(CommandError::from)?;
    emit_profiles(&app, &state.db);
    Ok(())
}

#[tauri::command]
async fn duplicate_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<Profile, CommandError> {
    let profile = state
        .db
        .duplicate_profile(id, &name)
        .map_err(CommandError::from)?;
    emit_profiles(&app, &state.db);
    Ok(profile)
}

#[tauri::command]
async fn import_json(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    content: String,
    source: Option<String>,
) -> Result<Profile, CommandError> {
    validate_profile_content(&state.settings.get(), state.resource_dir.as_deref(), &content)
        .map_err(CommandError::validation)?;
    let source = source.unwrap_or_else(|| "inline".into());
    let profile = state
        .db
        .create_profile(&name, "local", &content, &source, None, false)
        .map_err(CommandError::io)?;
    emit_profiles(&app, &state.db);
    Ok(profile)
}

#[tauri::command]
async fn import_subscription(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    name: Option<String>,
) -> Result<Profile, CommandError> {
    state.engine.logs.add_secret(&url);
    let content = fetch_subscription(&url)
        .await
        .map_err(CommandError::network)?;
    validate_profile_content(&state.settings.get(), state.resource_dir.as_deref(), &content)
        .map_err(CommandError::validation)?;
    let name = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| derive_subscription_name(&url));
    let profile = state
        .db
        .create_profile(&name, "subscription", &content, &url, Some(&url), true)
        .map_err(CommandError::io)?;
    emit_profiles(&app, &state.db);
    Ok(profile)
}

#[tauri::command]
async fn refresh_subscription(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<Profile, CommandError> {
    let profile = state
        .db
        .get_profile(id)
        .map_err(CommandError::io)?
        .ok_or_else(|| CommandError::validation(format!("profile {id} not found")))?;
    let url = profile.subscription_url.clone().ok_or_else(|| {
        CommandError::validation(format!(
            "profile '{}' has no subscription URL",
            profile.name
        ))
    })?;
    state.engine.logs.add_secret(&url);
    let result = async {
        let content = fetch_subscription(&url).await?;
        validate_profile_content(&state.settings.get(), state.resource_dir.as_deref(), &content)?;
        Ok::<String, String>(content)
    }
    .await;
    match result {
        Ok(content) => {
            let updated = state.db.set_subscription_result(id, Some(&content), None)?;
            emit_profiles(&app, &state.db);
            Ok(updated)
        }
        Err(error) => {
            let _ = state.db.set_subscription_result(id, None, Some(&error));
            emit_profiles(&app, &state.db);
            Err(CommandError::from(error))
        }
    }
}

#[tauri::command]
async fn import_share_links(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    text: String,
) -> Result<Profile, CommandError> {
    import_links_profile(&app, &state, &name, &text, "share-links")
}

#[tauri::command]
async fn import_qr(
    app: AppHandle,
    state: State<'_, AppState>,
    image_base64: String,
    name: Option<String>,
) -> Result<Profile, CommandError> {
    let zbar = diagnostics::find_in_path("zbarimg").ok_or_else(|| {
        CommandError::unsupported("zbarimg is not installed; QR import is unavailable")
    })?;
    let payload = image_base64
        .split_once(',')
        .map(|(_, rest)| rest)
        .unwrap_or(&image_base64);
    let bytes = decode_base64_any(payload)
        .ok_or_else(|| "invalid base64 image for QR import".to_string())?;
    let path = std::env::temp_dir().join(format!("veil-qr-{}.png", uuid::Uuid::new_v4()));
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("write temporary QR image: {e}"))?;
        file.write_all(&bytes)
            .map_err(|e| format!("write temporary QR image: {e}"))?;
    }
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new(zbar)
            .arg("-q")
            .arg("--raw")
            .arg(&path)
            .output(),
    )
    .await;
    let _ = fs::remove_file(&path);
    let output = match output {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(CommandError::io(format!("run zbarimg: {e}"))),
        Err(_) => return Err(CommandError::io("zbarimg timed out after 10s")),
    };
    if !output.status.success() {
        return Err(CommandError::validation(format!(
            "QR decode failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(CommandError::validation("no QR content detected in the image"));
    }
    let name = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "QR import".into());
    import_links_profile(&app, &state, &name, &text, "qr")
}

#[tauri::command]
async fn validate_content(state: State<'_, AppState>, content: String) -> Result<Value, CommandError> {
    match validate_profile_content(
        &state.settings.get(),
        state.resource_dir.as_deref(),
        &content,
    ) {
        Ok(outbounds) => Ok(json!({ "ok": true, "error": Value::Null, "outbounds": outbounds })),
        Err(error) => Ok(json!({ "ok": false, "error": error, "outbounds": [] })),
    }
}

#[tauri::command]
async fn export_profiles(state: State<'_, AppState>) -> Result<String, CommandError> {
    paths::ensure_dirs().map_err(|e| CommandError::io(e.to_string()))?;
    let backup = state.db.export_backup().map_err(CommandError::io)?;
    let raw = serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?;
    let path = paths::exports_dir().join(format!(
        "veil-profiles-{}-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        &uuid::Uuid::new_v4().to_string()[..8]
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    file.write_all(raw.as_bytes())
        .map_err(|e| format!("write backup: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
async fn import_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    content: String,
) -> Result<usize, CommandError> {
    let count = state
        .db
        .import_backup(&content)
        .map_err(CommandError::from)?;
    emit_profiles(&app, &state.db);
    Ok(count)
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings, CommandError> {
    Ok(state.settings.get())
}

#[tauri::command]
async fn update_settings(
    state: State<'_, AppState>,
    patch: Value,
) -> Result<Settings, CommandError> {
    let before = state.settings.get();
    let after = state.settings.update(&patch).map_err(CommandError::validation)?;
    if before.autostart != after.autostart {
        write_autostart(after.autostart).map_err(CommandError::io)?;
    }
    Ok(after)
}

#[tauri::command]
async fn run_diagnostics(state: State<'_, AppState>) -> Result<diagnostics::Diagnostics, CommandError> {
    let settings = state.settings.get();
    let core_info = core::info(settings.core_path.as_deref(), state.resource_dir.as_deref());
    let active = state.engine.active_profile_content();
    Ok(diagnostics::run(
        core_info,
        active.as_deref(),
        &state.engine.logs,
        state.resource_dir.as_deref(),
    ))
}

#[tauri::command]
async fn read_clipboard() -> Result<String, CommandError> {
    if let Ok(Ok(output)) = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::process::Command::new("wl-paste")
            .arg("--no-newline")
            .output(),
    )
    .await
    {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
    }
    if let Ok(Ok(output)) = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::process::Command::new("xclip")
            .args(["-o", "-selection", "clipboard"])
            .output(),
    )
    .await
    {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
    }
    Err(CommandError::unsupported("could not read the clipboard (tried wl-paste and xclip)"))
}

#[tauri::command]
async fn open_config_dir() -> Result<(), CommandError> {
    paths::ensure_dirs().map_err(|e| CommandError::io(e.to_string()))?;
    let dir = paths::config_dir();
    let dir_str = dir.to_string_lossy().into_owned();
    if std::process::Command::new("xdg-open")
        .arg(&dir_str)
        .spawn()
        .is_ok()
    {
        return Ok(());
    }
    if std::process::Command::new("gio")
        .args(["open", &dir_str])
        .spawn()
        .is_ok()
    {
        return Ok(());
    }
    Err(CommandError::unsupported("no file manager opener found (tried xdg-open and gio)"))
}

#[tauri::command]
async fn set_autostart(state: State<'_, AppState>, enabled: bool) -> Result<bool, CommandError> {
    let result = write_autostart(enabled).map_err(CommandError::io)?;
    if let Err(e) = state.settings.update(&json!({ "autostart": enabled })) {
        return Err(CommandError::io(format!("autostart file written but settings update failed: {e}")));
    }
    Ok(result)
}

#[tauri::command]
async fn quit(app: AppHandle, state: State<'_, AppState>) -> Result<(), CommandError> {
    state.engine.begin_shutdown_and_exit(app);
    Ok(())
}

fn spawn_subscription_updater(
    app: AppHandle,
    settings: Arc<SettingsStore>,
    db: Arc<Db>,
    logs: Arc<LogStore>,
    resource_dir: Option<PathBuf>,
) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            let interval = settings.get().subscription_interval_hours;
            if interval > 0 {
                if let Ok(due) = db.due_subscriptions(interval) {
                    let mut changed = false;
                    for profile in due {
                        let Some(url) = profile.subscription_url.clone() else {
                            continue;
                        };
                        logs.add_secret(&url);
                        let content = match fetch_subscription(&url).await {
                            Ok(content) => content,
                            Err(error) => {
                                logs.push(
                                    "warn",
                                    "engine",
                                    format!(
                                        "refresh of '{}' failed: {error}",
                                        profile.name
                                    ),
                                );
                                let _ = db.set_subscription_result(
                                    profile.id,
                                    None,
                                    Some(&error),
                                );
                                changed = true;
                                continue;
                            }
                        };
                        match validate_profile_content(
                            &settings.get(),
                            resource_dir.as_deref(),
                            &content,
                        ) {
                            Ok(_) => {
                                let _ =
                                    db.set_subscription_result(profile.id, Some(&content), None);
                                logs.push(
                                    "info",
                                    "engine",
                                    format!("subscription '{}' updated", profile.name),
                                );
                            }
                            Err(error) => {
                                logs.push(
                                    "warn",
                                    "engine",
                                    format!(
                                        "subscription '{}' is invalid: {error}",
                                        profile.name
                                    ),
                                );
                                let _ = db.set_subscription_result(
                                    profile.id,
                                    None,
                                    Some(&error),
                                );
                            }
                        }
                        changed = true;
                    }
                    if changed {
                        emit_profiles(&app, &db);
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    });
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            paths::ensure_dirs()?;
            let settings = Arc::new(SettingsStore::load());
            let db = Arc::new(Db::open(&paths::db_path())?);
            let logs = Arc::new(LogStore::new());
            logs.attach(handle.clone());
            let resource_dir = app.path().resource_dir().ok();
            let engine = Engine::new(
                Arc::new(handle.clone()),
                settings.clone(),
                db.clone(),
                logs.clone(),
                resource_dir.clone(),
            );
            logs.push(
                "info",
                "engine",
                format!(
                    "Veil {} starting (settings {}, data {})",
                    env!("CARGO_PKG_VERSION"),
                    paths::settings_path().display(),
                    paths::data_dir().display()
                ),
            );
            let tray_available = tray::spawn(handle.clone(), engine.clone());
            logs.push(
                "info",
                "engine",
                if tray_available {
                    "tray icon registered via StatusNotifierItem"
                } else {
                    "no StatusNotifier host; tray is disabled and the window stays normal"
                },
            );

            app.manage(AppState {
                engine: engine.clone(),
                settings: settings.clone(),
                db: db.clone(),
                resource_dir: resource_dir.clone(),
            });

            if let Some(window) = app.get_webview_window("main") {
                let window_handle = window.clone();
                let close_settings = settings.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if tray_available && close_settings.get().minimize_to_tray {
                            api.prevent_close();
                            let _ = window_handle.hide();
                        }
                    }
                });
            }

            engine.spawn_startup();
            engine.install_signal_handlers(handle.clone());
            spawn_subscription_updater(
                handle.clone(),
                settings.clone(),
                db.clone(),
                logs.clone(),
                resource_dir,
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            get_state,
            connect,
            disconnect,
            get_stats,
            get_connections,
            close_connection,
            get_logs,
            clear_logs,
            export_logs,
            get_route,
            get_outbounds,
            select_outbound,
            test_delay,
            set_clash_mode,
            list_profiles,
            create_profile,
            update_profile,
            set_profile_auto_update,
            delete_profile,
            duplicate_profile,
            import_json,
            import_subscription,
            refresh_subscription,
            import_share_links,
            import_qr,
            validate_content,
            export_profiles,
            import_backup,
            get_settings,
            update_settings,
            run_diagnostics,
            read_clipboard,
            open_config_dir,
            set_autostart,
            quit
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the Veil application")
        .run(|app_handle, event| match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                let state = app_handle.state::<AppState>();
                if !state.engine.is_shutting_down() && state.engine.has_runtime() {
                    api.prevent_exit();
                    state.engine.begin_shutdown_and_exit(app_handle.clone());
                }
            }
            tauri::RunEvent::Exit => {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.engine.shutdown_sync();
                }
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_subscription() -> String {
        serde_json::json!({
            "outbounds": [
                { "type": "selector", "tag": "Proxy", "outbounds": ["direct"] },
                { "type": "direct", "tag": "direct" }
            ],
            "route": { "final": "Proxy" }
        })
        .to_string()
    }

    #[test]
    fn subscription_rejects_clash_yaml() {
        let error = normalize_subscription_body("mixed-port: 7890\nproxies:\n  - name: node\n")
            .unwrap_err();
        assert!(error.contains("Clash YAML"), "{error}");
    }

    #[test]
    fn subscription_accepts_singbox_json_and_rejects_missing_outbounds() {
        let normalized = normalize_subscription_body(&json_subscription()).unwrap();
        assert!(normalized.contains("outbounds"));
        let error = normalize_subscription_body(r#"{"log":{"level":"info"}}"#).unwrap_err();
        assert!(error.contains("no outbounds"), "{error}");
        let error = normalize_subscription_body("{not json").unwrap_err();
        assert!(error.contains("unrecognized") || error.contains("invalid"), "{error}");
    }

    #[test]
    fn subscription_accepts_base64_json_and_links() {
        let encoded = STANDARD.encode(json_subscription());
        assert!(normalize_subscription_body(&encoded).is_ok());
        let links = "vless://11111111-2222-3333-4444-555555555555@example.com:443?encryption=none&security=tls#Node";
        let encoded = STANDARD.encode(links);
        let normalized = normalize_subscription_body(&encoded).unwrap();
        let value: Value = serde_json::from_str(&normalized).unwrap();
        assert_eq!(value["route"]["final"], "Proxy");
        assert!(value["outbounds"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn subscription_accepts_plain_link_list() {
        let normalized = normalize_subscription_body(
            "ss://YWVzLTI1Ni1nY206cHc=@example.com:8388#SS\nhttp://proxy.example.com:8080",
        )
        .unwrap();
        let value: Value = serde_json::from_str(&normalized).unwrap();
        assert_eq!(value["route"]["final"], "Proxy");
    }
}
