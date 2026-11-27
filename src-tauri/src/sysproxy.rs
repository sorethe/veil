use crate::paths;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyInfo {
    pub kind: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrevEntry {
    pub scope: String,
    pub key: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SysProxyState {
    pub kind: String,
    pub applied_at: i64,
    pub entries: Vec<PrevEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub desktop: String,
    pub session: String,
    pub system_proxy_supported: bool,
    pub system_proxy_kind: String,
    pub tray_available: bool,
}

fn env_first(keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Ok(value) = env::var(key) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

pub fn desktop_raw() -> String {
    env_first(&["XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP", "DESKTOP_SESSION"])
        .unwrap_or_else(|| "unknown".into())
}

pub fn session_type() -> String {
    if let Some(session) = env_first(&["XDG_SESSION_TYPE"]) {
        let lower = session.to_ascii_lowercase();
        if lower == "wayland" || lower == "x11" {
            return lower;
        }
    }
    if env_first(&["WAYLAND_DISPLAY"]).is_some() {
        "wayland".into()
    } else if env_first(&["DISPLAY"]).is_some() {
        "x11".into()
    } else {
        "unknown".into()
    }
}

fn command_exists(name: &str) -> bool {
    let Ok(paths) = env::var("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|dir| {
        let path = dir.join(name);
        fs::metadata(&path)
            .map(|m| m.is_file() && (m.permissions().mode() & 0o111 != 0))
            .unwrap_or(false)
    })
}


pub fn proxy_kind_from_desktop(desktop: &str) -> String {
    let lower = desktop.to_ascii_lowercase();
    if [
        "gnome", "ubuntu", "unity", "pop", "cinnamon", "budgie", "pantheon", "elementary",
        "cosmic",
    ]
    .iter()
    .any(|token| lower.contains(token))
    {
        return "gnome".into();
    }
    if lower.contains("kde") || lower.contains("plasma") {
        return "kde".into();
    }
    if lower.contains("xfce") {
        return "xfce".into();
    }
    "unsupported".into()
}

/// Desktop proxy mechanisms whose CLI tooling is actually present. This is
/// tool availability, not a claim that the active desktop applies them.
pub fn available_backends() -> Vec<String> {
    let mut backends = Vec::new();
    if command_exists("gsettings") {
        backends.push("gnome".to_string());
    }
    if command_exists("kwriteconfig6") || command_exists("kwriteconfig5") {
        backends.push("kde".to_string());
    }
    if command_exists("xfconf-query") {
        backends.push("xfce".to_string());
    }
    backends
}

pub fn system_proxy_kind() -> String {
    let kind = proxy_kind_from_desktop(&desktop_raw());
    match kind.as_str() {
        "gnome" if command_exists("gsettings") => kind,
        "kde" if command_exists("kwriteconfig6") || command_exists("kwriteconfig5") => kind,
        "xfce" if command_exists("xfconf-query") => kind,
        _ => "unsupported".into(),
    }
}

pub fn platform_info() -> PlatformInfo {
    let desktop = desktop_raw();
    let kind = system_proxy_kind();
    PlatformInfo {
        desktop,
        session: session_type(),
        system_proxy_supported: kind != "unsupported",
        system_proxy_kind: kind,
        tray_available: tray_available().0,
    }
}

pub fn tray_available() -> (bool, String) {
    if !command_exists("gdbus") {
        return (false, "gdbus is not installed; cannot query StatusNotifierWatcher".into());
    }
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.NameHasOwner",
            "org.kde.StatusNotifierWatcher",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.contains("true") {
                (true, "StatusNotifierWatcher is registered on the session bus".into())
            } else {
                (false, "no StatusNotifierWatcher host on the session bus".into())
            }
        }
        Ok(out) => (
            false,
            format!(
                "StatusNotifierWatcher query failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ),
        Err(e) => (false, format!("StatusNotifierWatcher query failed: {e}")),
    }
}

pub fn proxy_instructions(port: u16) -> String {
    let desktop = desktop_raw();
    format!(
        "{desktop} does not expose a desktop-wide proxy API that Veil can set. \
         Export HTTP_PROXY=http://127.0.0.1:{port}, HTTPS_PROXY=http://127.0.0.1:{port} and \
         ALL_PROXY=socks5://127.0.0.1:{port} for the applications you launch, or use TUN mode \
         to route all traffic."
    )
}

fn unsupported_info(port: u16) -> ProxyInfo {
    ProxyInfo {
        kind: "unsupported".into(),
        detail: proxy_instructions(port),
    }
}

fn state_path() -> PathBuf {
    paths::sysproxy_state_path()
}

fn save_state(state: &SysProxyState) -> Result<(), String> {
    if let Some(parent) = state_path().parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    let path = state_path();
    let tmp = path.with_extension("json.tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("save proxy state: {e}"))?;
    file.write_all(raw.as_bytes())
        .map_err(|e| format!("save proxy state: {e}"))?;
    file.sync_all().ok();
    fs::rename(&tmp, &path).map_err(|e| format!("save proxy state: {e}"))?;
    Ok(())
}

fn read_state() -> Option<SysProxyState> {
    let raw = fs::read_to_string(state_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn remove_state() {
    let _ = fs::remove_file(state_path());
}

fn run(cmd: &str, args: &[String]) -> Result<String, String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("run {cmd}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "{cmd} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn gsettings_get(schema: &str, key: &str) -> Result<Option<String>, String> {
    let out = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .map_err(|e| format!("gsettings get: {e}"))?;
    if out.status.success() {
        let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(Some(value))
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("No such key") || err.contains("not found") {
            Ok(None)
        } else {
            Err(format!("gsettings get {schema} {key}: {}", err.trim()))
        }
    }
}

fn gsettings_set(schema: &str, key: &str, value: &str) -> Result<(), String> {
    run(
        "gsettings",
        &["set".to_string(), schema.into(), key.into(), value.into()],
    )
    .map(|_| ())
}

const GNOME_SCHEMA: &str = "org.gnome.system.proxy";

fn gnome_snapshot() -> Result<Vec<PrevEntry>, String> {
    let mut entries = Vec::new();
    for (schema, key) in [
        (GNOME_SCHEMA, "mode"),
        ("org.gnome.system.proxy.http", "host"),
        ("org.gnome.system.proxy.http", "port"),
        ("org.gnome.system.proxy.https", "host"),
        ("org.gnome.system.proxy.https", "port"),
        ("org.gnome.system.proxy.socks", "host"),
        ("org.gnome.system.proxy.socks", "port"),
    ] {
        entries.push(PrevEntry {
            scope: schema.into(),
            key: key.into(),
            value: gsettings_get(schema, key)?,
        });
    }
    Ok(entries)
}

fn gnome_restore(state: &SysProxyState) -> Result<(), String> {
    for entry in &state.entries {
        if let Some(value) = &entry.value {
            gsettings_set(&entry.scope, &entry.key, value)?;
        }
    }
    Ok(())
}

fn apply_gnome(port: u16) -> Result<ProxyInfo, String> {
    let entries = gnome_snapshot()?;
    let state = SysProxyState {
        kind: "gnome".into(),
        applied_at: chrono::Utc::now().timestamp(),
        entries,
    };
    save_state(&state)?;
    let port_s = port.to_string();
    let result = (|| -> Result<(), String> {
        gsettings_set(GNOME_SCHEMA, "mode", "'manual'")?;
        for schema in [
            "org.gnome.system.proxy.http",
            "org.gnome.system.proxy.https",
            "org.gnome.system.proxy.socks",
        ] {
            gsettings_set(schema, "host", "'127.0.0.1'")?;
            gsettings_set(schema, "port", &port_s)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        let restore = gnome_restore(&state);
        if restore.is_ok() {
            remove_state();
        }
        return Err(format!("failed to set GNOME proxy: {e}"));
    }
    Ok(ProxyInfo {
        kind: "gnome".into(),
        detail: format!("GNOME system proxy set to 127.0.0.1:{port} (http/https/socks)"),
    })
}

fn kioslaverc_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".").join(".config"))
        .join("kioslaverc")
}

fn parse_ini_value(raw: &str, group: &str, key: &str) -> Option<String> {
    let mut current = String::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].to_string();
            continue;
        }
        if current == group {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

fn kde_snapshot() -> Vec<PrevEntry> {
    let raw = fs::read_to_string(kioslaverc_path()).unwrap_or_default();
    ["ProxyType", "httpProxy", "httpsProxy", "socksProxy"]
        .iter()
        .map(|key| PrevEntry {
            scope: "Proxy Settings".into(),
            key: key.to_string(),
            value: parse_ini_value(&raw, "Proxy Settings", key),
        })
        .collect()
}

fn kde_writer() -> Option<&'static str> {
    if command_exists("kwriteconfig6") {
        Some("kwriteconfig6")
    } else if command_exists("kwriteconfig5") {
        Some("kwriteconfig5")
    } else {
        None
    }
}

fn kwrite(delete: bool, key: &str, value: Option<&str>, typed_int: bool) -> Result<(), String> {
    let writer = kde_writer().ok_or_else(|| "kwriteconfig6/5 not found".to_string())?;
    let file = kioslaverc_path();
    let file = file.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec![
        "--file".into(),
        file,
        "--group".into(),
        "Proxy Settings".into(),
        "--key".into(),
        key.into(),
    ];
    if delete {
        args.push("--delete".into());
    } else {
        if typed_int {
            args.push("--type".into());
            args.push("int".into());
        }
        args.push(value.unwrap_or_default().into());
    }
    let output = Command::new(writer)
        .args(&args)
        .output()
        .map_err(|e| format!("run {writer}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{writer} {key}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn kde_restore(state: &SysProxyState) -> Result<(), String> {
    for entry in &state.entries {
        let typed_int = entry.key == "ProxyType";
        match &entry.value {
            Some(value) => kwrite(false, &entry.key, Some(value), typed_int)?,
            None => kwrite(true, &entry.key, None, false)?,
        }
    }
    Ok(())
}

fn apply_kde(port: u16) -> Result<ProxyInfo, String> {
    if kde_writer().is_none() {
        return Err("kwriteconfig6/5 not found; cannot set the KDE system proxy".into());
    }
    let state = SysProxyState {
        kind: "kde".into(),
        applied_at: chrono::Utc::now().timestamp(),
        entries: kde_snapshot(),
    };
    save_state(&state)?;
    let result = (|| -> Result<(), String> {
        kwrite(false, "ProxyType", Some("1"), true)?;
        kwrite(
            false,
            "httpProxy",
            Some(&format!("http://127.0.0.1:{port}")),
            false,
        )?;
        kwrite(
            false,
            "httpsProxy",
            Some(&format!("http://127.0.0.1:{port}")),
            false,
        )?;
        kwrite(
            false,
            "socksProxy",
            Some(&format!("socks://127.0.0.1:{port}")),
            false,
        )?;
        Ok(())
    })();
    if let Err(e) = result {
        if kde_restore(&state).is_ok() {
            remove_state();
        }
        return Err(format!("failed to set KDE proxy: {e}"));
    }
    Ok(ProxyInfo {
        kind: "kde".into(),
        detail: format!("KDE proxy settings set to 127.0.0.1:{port} (http/https/socks)"),
    })
}

const XFCE_CHANNEL: &str = "xfce4-session";

fn xfconf_get(property: &str) -> Result<Option<String>, String> {
    let output = Command::new("xfconf-query")
        .args(["-c", XFCE_CHANNEL, "-p", property])
        .output()
        .map_err(|e| format!("xfconf-query: {e}"))?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).trim().into()))
    } else {
        Ok(None)
    }
}

fn xfconf_set(property: &str, value: &str, is_int: bool) -> Result<(), String> {
    let mut args = vec!["-c", XFCE_CHANNEL, "-p", property, "-s", value];
    if is_int {
        args.extend(["-t", "int"]);
    }
    run("xfconf-query", &args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).map(|_| ())
}

fn apply_xfce(port: u16) -> Result<ProxyInfo, String> {
    if !command_exists("xfconf-query") {
        return Err("xfconf-query not found; cannot set the XFCE system proxy".into());
    }
    let props = [
        "/proxy/mode",
        "/proxy/http/host",
        "/proxy/http/port",
        "/proxy/https/host",
        "/proxy/https/port",
        "/proxy/socks/host",
        "/proxy/socks/port",
    ];
    let mut entries = Vec::new();
    for prop in props {
        entries.push(PrevEntry {
            scope: XFCE_CHANNEL.into(),
            key: prop.into(),
            value: xfconf_get(prop)?,
        });
    }
    let state = SysProxyState {
        kind: "xfce".into(),
        applied_at: chrono::Utc::now().timestamp(),
        entries,
    };
    save_state(&state)?;
    let port_s = port.to_string();
    let result = (|| -> Result<(), String> {
        xfconf_set("/proxy/mode", "manual", false)?;
        for prefix in ["http", "https", "socks"] {
            xfconf_set(&format!("/proxy/{prefix}/host"), "127.0.0.1", false)?;
            xfconf_set(&format!("/proxy/{prefix}/port"), &port_s, true)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        for entry in &state.entries {
            if let Some(value) = &entry.value {
                let _ = xfconf_set(&entry.key, value, entry.key.ends_with("/port"));
            }
        }
        remove_state();
        return Err(format!("failed to set XFCE proxy: {e}"));
    }
    Ok(ProxyInfo {
        kind: "xfce".into(),
        detail: format!("XFCE system proxy set to 127.0.0.1:{port} (http/https/socks)"),
    })
}

/// Apply the desktop proxy for system mode. Unsupported desktops are reported,
/// never guessed at.
pub fn apply(port: u16) -> Result<ProxyInfo, String> {
    // Never stack a second layer of settings on top of a crashed session.
    restore()?;
    match system_proxy_kind().as_str() {
        "gnome" => apply_gnome(port),
        "kde" => apply_kde(port),
        "xfce" => apply_xfce(port),
        _ => Ok(unsupported_info(port)),
    }
}

/// Restore the exact previous desktop proxy values recorded before applying.
pub fn restore() -> Result<(), String> {
    let Some(state) = read_state() else {
        return Ok(());
    };
    match state.kind.as_str() {
        "gnome" => gnome_restore(&state)?,
        "kde" => kde_restore(&state)?,
        "xfce" => {
            for entry in &state.entries {
                if let Some(value) = &entry.value {
                    xfconf_set(&entry.key, value, entry.key.ends_with("/port"))?;
                }
            }
        }
        _ => {}
    }
    remove_state();
    Ok(())
}

/// If a state file survives from a previous session, the desktop proxy is
/// still applied even though this process did not set it.
pub fn active_proxy_info() -> Option<ProxyInfo> {
    read_state().map(|state| ProxyInfo {
        kind: state.kind,
        detail: "system proxy still applied from a previous Veil session".into(),
    })
}

pub fn tun_info() -> ProxyInfo {
    ProxyInfo {
        kind: "tun".into(),
        detail: "TUN mode routes all traffic; the system proxy setting is not modified".into(),
    }
}

/// Proxy-related environment variables we saw, for honest diagnostics text.
pub fn proxy_env() -> Vec<String> {
    let mut values = Vec::new();
    for key in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"] {
        if let Ok(v) = env::var(key) {
            if !v.trim().is_empty() {
                values.push(format!("{key}={v}"));
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrips_exactly() {
        let state = SysProxyState {
            kind: "gnome".into(),
            applied_at: 1234,
            entries: vec![
                PrevEntry {
                    scope: "org.gnome.system.proxy".into(),
                    key: "mode".into(),
                    value: Some("'manual'".into()),
                },
                PrevEntry {
                    scope: "org.gnome.system.proxy.socks".into(),
                    key: "host".into(),
                    value: None,
                },
            ],
        };
        let raw = serde_json::to_string(&state).unwrap();
        let back: SysProxyState = serde_json::from_str(&raw).unwrap();
        assert_eq!(state, back);
        assert!(raw.contains("\"appliedAt\":1234"));
    }

    #[test]
    fn parses_kde_kioslaverc_values() {
        let raw = "[Proxy Settings]\nProxyType=1\nhttpProxy=http://127.0.0.1:2080\n\n[General]\nFoo=bar\n";
        assert_eq!(
            parse_ini_value(raw, "Proxy Settings", "ProxyType"),
            Some("1".into())
        );
        assert_eq!(
            parse_ini_value(raw, "Proxy Settings", "httpProxy"),
            Some("http://127.0.0.1:2080".into())
        );
        assert_eq!(parse_ini_value(raw, "Proxy Settings", "socksProxy"), None);
        assert_eq!(parse_ini_value(raw, "General", "Foo"), Some("bar".into()));
    }

    #[test]
    fn classifies_desktops_conservatively() {
        assert_eq!(proxy_kind_from_desktop("GNOME"), "gnome");
        assert_eq!(proxy_kind_from_desktop("ubuntu:GNOME"), "gnome");
        assert_eq!(proxy_kind_from_desktop("KDE"), "kde");
        assert_eq!(proxy_kind_from_desktop("XFCE"), "xfce");
        assert_eq!(proxy_kind_from_desktop("Hyprland"), "unsupported");
        assert_eq!(proxy_kind_from_desktop("sway"), "unsupported");
        assert_eq!(proxy_kind_from_desktop("unknown"), "unsupported");
    }

    #[test]
    fn unsupported_text_is_honest() {
        let info = unsupported_info(2080);
        assert_eq!(info.kind, "unsupported");
        assert!(info.detail.contains("127.0.0.1:2080"));
    }

    #[test]
    fn command_lookup_uses_path() {
        assert!(command_exists("sh"));
        assert!(!command_exists("definitely-not-a-veil-command"));
    }

    #[test]
    fn collects_proxy_env_without_inventing() {
        let values = proxy_env();
        for v in values {
            assert!(v.contains('='));
        }
    }
}
