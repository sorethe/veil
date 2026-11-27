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
                candidates.push(
                    ancestor
                        .join("src-tauri")
                        .join("resources")
                        .join("sing-box"),
                );
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

/// Real binary probe: version string plus build tags (used for capability
/// detection such as `with_clash_api`). Never hardcodes features.
pub fn probe(core: &Path) -> Result<(String, Vec<String>), String> {
    let output = Command::new(core)
        .arg("version")
        .output()
        .map_err(|e| format!("failed to run {}: {e}", core.display()))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("sing-box version failed: {}", err.trim()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut version = None;
    let mut tags = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("sing-box version") {
            let v = rest.split_whitespace().next().unwrap_or("").to_string();
            if !v.is_empty() {
                version = Some(v);
            }
        } else if let Some(rest) = trimmed.strip_prefix("Tags:") {
            tags = rest
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    match version {
        Some(version) => Ok((version, tags)),
        None => Err("could not parse sing-box version output".into()),
    }
}

pub fn version(core: &Path) -> Result<String, String> {
    probe(core).map(|(version, _)| version)
}

#[derive(Clone, Debug, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapabilities {
    pub clash_api: bool,
    pub tun: bool,
    pub auto_route: bool,
    pub auto_redirect: bool,
    pub nftables: bool,
    pub ipv6: bool,
    pub dns_hijack: bool,
    pub system_proxy_backends: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub id: String,
    pub version: Option<String>,
    pub path: Option<String>,
    pub api: String,
    pub capabilities: EngineCapabilities,
}

impl Default for EngineInfo {
    fn default() -> Self {
        Self {
            id: "sing-box".into(),
            version: None,
            path: None,
            api: "clash".into(),
            capabilities: EngineCapabilities::default(),
        }
    }
}

fn command_exists(name: &str) -> bool {
    which(name).is_some()
}

fn ipv6_enabled() -> bool {
    let disabled = fs::read_to_string("/proc/sys/net/ipv6/conf/all/disable_ipv6")
        .map(|value| value.trim() != "0")
        .unwrap_or(false);
    if disabled {
        return false;
    }
    Path::new("/proc/net/if_inet6").exists()
}

fn version_at_least(version: &str, minimum: &str) -> bool {
    fn parts(value: &str) -> Vec<u64> {
        value
            .split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<u64>().ok())
            .collect()
    }
    let current = parts(version);
    let minimum = parts(minimum);
    for index in 0..minimum.len().max(current.len()) {
        let a = current.get(index).copied().unwrap_or(0);
        let b = minimum.get(index).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    true
}

/// Capabilities derived from the real binary tags, the presence of
/// /dev/net/tun, nft/ip tooling, IPv6 support and available desktop proxy CLIs.
pub fn capabilities_for(core: Option<&Path>) -> EngineCapabilities {
    let mut capabilities = EngineCapabilities {
        nftables: command_exists("nft"),
        ipv6: ipv6_enabled(),
        system_proxy_backends: crate::sysproxy::available_backends(),
        ..EngineCapabilities::default()
    };
    let Some(core) = core else {
        return capabilities;
    };
    let (version, tags) = probe(core).unwrap_or_default();
    let tun_device = Path::new("/dev/net/tun").exists();
    capabilities.tun = cfg!(target_os = "linux") && tun_device;
    capabilities.clash_api = tags.iter().any(|tag| tag == "with_clash_api");
    capabilities.auto_route = capabilities.tun && command_exists("ip");
    capabilities.auto_redirect = capabilities.tun && capabilities.nftables;
    capabilities.dns_hijack =
        capabilities.tun && version_at_least(&version, "1.10") && !version.is_empty();
    capabilities
}

pub fn engine_info(explicit: Option<&str>, resource_dir: Option<&Path>) -> EngineInfo {
    let core_path = find_core(explicit, resource_dir);
    let version = core_path
        .as_deref()
        .and_then(|path| probe(path).ok())
        .map(|(v, _)| v);
    let capabilities = capabilities_for(core_path.as_deref());
    EngineInfo {
        id: "sing-box".into(),
        version,
        path: core_path.map(|path| path.to_string_lossy().into_owned()),
        api: "clash".into(),
        capabilities,
    }
}

pub fn info(explicit: Option<&str>, resource_dir: Option<&Path>) -> CoreInfo {
    let Some(path) = find_core(explicit, resource_dir) else {
        return CoreInfo {
            path: None,
            version: None,
            ok: false,
            error: Some(
                "sing-box binary not found (set corePath in settings or $VEIL_SING_BOX)".into(),
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_info_serializes_contract_shape() {
        let info = engine_info(None, None);
        let value = serde_json::to_value(&info).unwrap();
        assert_eq!(value["id"], "sing-box");
        assert_eq!(value["api"], "clash");
        for key in [
            "clashApi",
            "tun",
            "autoRoute",
            "autoRedirect",
            "nftables",
            "ipv6",
            "dnsHijack",
            "systemProxyBackends",
        ] {
            assert!(
                value["capabilities"].get(key).is_some(),
                "missing capability {key}"
            );
        }
        assert!(value["capabilities"]["systemProxyBackends"].is_array());
        // If a real core was found, version/path come from the binary and
        // clashApi is derived from its build tags rather than hardcoded.
        if let Some(path) = &info.path {
            assert!(Path::new(path).is_file());
            if let Ok((_, tags)) = probe(Path::new(path)) {
                assert_eq!(
                    info.capabilities.clash_api,
                    tags.iter().any(|tag| tag == "with_clash_api")
                );
            }
        }
    }

    #[test]
    fn capabilities_are_derived_from_the_binary() {
        let Ok(path) = std::env::var("VEIL_SINGBOX") else {
            return;
        };
        let capabilities = capabilities_for(Some(Path::new(&path)));
        assert!(
            capabilities.clash_api,
            "vendored sing-box has with_clash_api"
        );
        assert!(capabilities.tun, "/dev/net/tun exists on this host");
        assert_eq!(
            capabilities.auto_redirect,
            capabilities.tun && capabilities.nftables
        );
    }

    #[test]
    fn validates_real_sing_box_configs() {
        let Ok(core) = std::env::var("VEIL_SINGBOX") else {
            return;
        };
        let core = Path::new(&core);
        let valid = r#"{"outbounds":[{"type":"direct","tag":"direct"}]}"#;
        assert!(validate_content(core, valid, "system", 2080, 9090, false, "info").is_ok());
        let subscription_style = r#"{
            "outbounds": [
                {"type":"selector","tag":"Proxy","outbounds":["node-a","node-b"]},
                {"type":"direct","tag":"node-a"},
                {"type":"direct","tag":"node-b"}
            ],
            "route": {"final": "Proxy"}
        }"#;
        let tags = validate_content(
            core,
            subscription_style,
            "system",
            2080,
            9090,
            false,
            "info",
        )
        .expect("subscription-style sing-box config must pass real check");
        assert!(tags.contains(&"Proxy".to_string()));
        assert!(validate_content(
            core,
            "{ this is not json",
            "system",
            2080,
            9090,
            false,
            "info"
        )
        .is_err());
        assert!(validate_content(
            core,
            r#"{"log":{"level":"info"}}"#,
            "system",
            2080,
            9090,
            false,
            "info"
        )
        .is_err());
        assert!(validate_content(
            core,
            r#"{"outbounds":[]}"#,
            "system",
            2080,
            9090,
            false,
            "info"
        )
        .is_err());
    }

    #[test]
    fn version_compare_handles_numeric_segments() {
        assert!(version_at_least("1.14.5", "1.10"));
        assert!(version_at_least("2.0.0", "1.99.99"));
        assert!(!version_at_least("1.9.9", "1.10"));
        assert!(version_at_least("1.10.0", "1.10"));
    }
}
