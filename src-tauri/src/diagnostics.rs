use crate::core::{CoreInfo, EngineCapabilities};
use crate::logs::LogStore;
use crate::sysproxy;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub user: String,
    pub helper: bool,
    pub polkit: bool,
    pub tun_device: bool,
    pub nft: bool,
    pub root: bool,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceInfo {
    pub name: String,
    pub addresses: Vec<String>,
    pub up: bool,
    pub default: bool,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TunInfo {
    pub present: bool,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DnsInfo {
    pub mode: String,
    pub servers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TrayInfo {
    pub available: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HelperInfo {
    pub installed: bool,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub core: CoreInfo,
    pub capabilities: EngineCapabilities,
    pub permissions: Permissions,
    pub interfaces: Vec<InterfaceInfo>,
    pub tun: TunInfo,
    pub dns: DnsInfo,
    pub errors: Vec<String>,
    pub tray: TrayInfo,
    pub helper: HelperInfo,
}

fn command_exists(name: &str) -> bool {
    let Ok(paths) = env::var("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|dir| {
        let path = dir.join(name);
        fs::metadata(&path)
            .map(|m| {
                use std::os::unix::fs::PermissionsExt;
                m.is_file() && m.permissions().mode() & 0o111 != 0
            })
            .unwrap_or(false)
    })
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = env::var("PATH").ok()?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Locate an executable on PATH (used for pkexec preflight).
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    which(name)
}

pub fn find_helper(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/bin/veil-helper"),
        PathBuf::from("/usr/local/bin/veil-helper"),
    ];
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("veil-helper"));
            for ancestor in dir.ancestors().take(5) {
                candidates.push(ancestor.join("veil-helper"));
                candidates.push(ancestor.join("helper").join("target").join("release").join("veil-helper"));
                candidates.push(ancestor.join("helper").join("target").join("debug").join("veil-helper"));
                candidates.push(ancestor.join("src-tauri").join("helper").join("target").join("release").join("veil-helper"));
            }
        }
    }
    if let Some(dir) = resource_dir {
        candidates.push(dir.join("veil-helper"));
    }
    if let Some(path) = which("veil-helper") {
        candidates.push(path);
    }
    candidates.into_iter().find(|p| {
        fs::metadata(p)
            .map(|m| {
                use std::os::unix::fs::PermissionsExt;
                m.is_file() && m.permissions().mode() & 0o111 != 0
            })
            .unwrap_or(false)
    })
}

fn polkit_policy_installed() -> bool {
    Path::new("/usr/share/polkit-1/actions/dev.veil.helper.policy").exists()
        || Path::new("/usr/share/polkit-1/actions/veil-helper.policy").exists()
}

fn run_ip(args: &[&str]) -> Option<Value> {
    let output = Command::new("ip").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

fn interfaces() -> Vec<InterfaceInfo> {
    let default_devs: BTreeSet<String> = run_ip(&["-j", "route", "show", "default"])
        .and_then(|v| {
            v.as_array().map(|routes| {
                routes
                    .iter()
                    .filter_map(|r| r.get("dev").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default();
    let Some(value) = run_ip(&["-j", "addr"]) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    if let Some(list) = value.as_array() {
        for iface in list {
            let name = iface
                .get("ifname")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let up = iface
                .get("flags")
                .and_then(Value::as_array)
                .map(|flags| flags.iter().any(|f| f.as_str() == Some("UP")))
                .unwrap_or_else(|| iface.get("operstate").and_then(Value::as_str) == Some("UP"));
            let addresses = iface
                .get("addr_info")
                .and_then(Value::as_array)
                .map(|addrs| {
                    addrs
                        .iter()
                        .filter_map(|a| {
                            let local = a.get("local").and_then(Value::as_str)?;
                            let prefix = a.get("prefixlen").and_then(Value::as_u64).unwrap_or(0);
                            Some(format!("{local}/{prefix}"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            result.push(InterfaceInfo {
                default: default_devs.contains(&name),
                name,
                addresses,
                up,
            });
        }
    }
    result
}

fn tun_present() -> TunInfo {
    let output = Command::new("ip").args(["link", "show", "veil0"]).output();
    match output {
        Ok(out) if out.status.success() => TunInfo {
            present: true,
            name: Some("veil0".into()),
        },
        _ => TunInfo {
            present: false,
            name: None,
        },
    }
}

fn config_dns(active_content: Option<&str>) -> (bool, Vec<String>) {
    let Some(raw) = active_content else {
        return (false, Vec::new());
    };
    let Ok(profile) = serde_json::from_str::<Value>(raw) else {
        return (false, Vec::new());
    };
    let Some(dns) = profile.get("dns") else {
        return (false, Vec::new());
    };
    let mut servers = Vec::new();
    if let Some(list) = dns.get("servers").and_then(Value::as_array) {
        for server in list {
            match server {
                Value::String(s) => servers.push(s.clone()),
                Value::Object(map) => {
                    if let Some(address) = map.get("address").and_then(Value::as_str) {
                        servers.push(address.to_string());
                    } else if let Some(server) = map.get("server").and_then(Value::as_str) {
                        servers.push(server.to_string());
                    } else if let Some(tag) = map.get("tag").and_then(Value::as_str) {
                        servers.push(tag.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    (true, servers)
}

fn system_dns() -> Vec<String> {
    let mut servers = Vec::new();
    if let Ok(output) = Command::new("resolvectl").arg("dns").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for token in text.split_whitespace() {
                let token = token.trim_end_matches(':');
                if token.is_empty() || token.ends_with(':') {
                    continue;
                }
                if token.parse::<IpAddr>().is_ok() {
                    servers.push(token.to_string());
                }
            }
        }
    }
    if servers.is_empty() {
        if let Ok(raw) = fs::read_to_string("/etc/resolv.conf") {
            for line in raw.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("nameserver") {
                    if let Some(addr) = rest.split_whitespace().next() {
                        if addr.parse::<IpAddr>().is_ok() {
                            servers.push(addr.to_string());
                        }
                    }
                }
            }
        }
    }
    servers.sort();
    servers.dedup();
    servers
}

pub fn run(
    core: CoreInfo,
    active_content: Option<&str>,
    logs: &LogStore,
    resource_dir: Option<&Path>,
) -> Diagnostics {
    let helper_path = find_helper(resource_dir);
    let helper = HelperInfo {
        installed: helper_path.is_some(),
        path: helper_path.map(|p| p.to_string_lossy().into_owned()),
    };
    let policy = polkit_policy_installed();
    let polkit = policy && command_exists("pkexec");
    let policy_shipped = resource_dir
        .map(|dir| {
            dir.join("dev.veil.helper.policy").exists() || dir.join("veil-helper.policy").exists()
        })
        .unwrap_or(false);
    let tun_device = Path::new("/dev/net/tun").exists();
    let nft = command_exists("nft");
    let root = unsafe { libc::geteuid() } == 0;
    let user = env::var("USER")
        .or_else(|_| env::var("LOGNAME"))
        .unwrap_or_else(|_| format!("uid {}", unsafe { libc::geteuid() }));

    let (has_profile_dns, profile_servers) = config_dns(active_content);
    let system_servers = system_dns();
    let has_system_dns = !system_servers.is_empty();
    let mode = match (has_profile_dns, has_system_dns) {
        (true, true) => "profile+system".to_string(),
        (true, false) => "profile".to_string(),
        (false, true) => "system".to_string(),
        (false, false) => "unknown".to_string(),
    };
    let mut servers = profile_servers;
    for server in system_servers {
        if !servers.contains(&server) {
            servers.push(server);
        }
    }

    let capabilities = crate::core::capabilities_for(core.path.as_deref().map(Path::new));
    let (tray_available, tray_detail) = sysproxy::tray_available();

    let mut errors: Vec<String> = logs.tail_lines(5, Some("error"));
    if let Some(error) = &core.error {
        if !errors.iter().any(|e| e == error) {
            errors.push(error.clone());
        }
    }
    if !helper.installed {
        errors.push(
            "veil-helper is not installed; TUN mode will fail. Install it with the helper install script."
                .into(),
        );
    }
    if !policy {
        let detail = if policy_shipped {
            "polkit policy dev.veil.helper is shipped in resources/ but not installed in /usr/share/polkit-1/actions; run ./install-helper.sh"
        } else {
            "polkit policy dev.veil.helper is not installed; TUN mode needs ./install-helper.sh"
        };
        errors.push(detail.into());
    }
    if !tun_device {
        errors.push("/dev/net/tun is missing; TUN mode is unavailable.".into());
    }
    if !nft {
        errors.push("nft is not installed; auto_redirect and TUN routing diagnostics are limited.".into());
    }
    errors.truncate(12);

    Diagnostics {
        core,
        capabilities,
        permissions: Permissions {
            user,
            helper: helper.installed,
            polkit,
            tun_device,
            nft,
            root,
        },
        interfaces: interfaces(),
        tun: tun_present(),
        dns: DnsInfo { mode, servers },
        errors,
        tray: TrayInfo {
            available: tray_available,
            detail: tray_detail,
        },
        helper,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_dns_from_profile() {
        let raw = r#"{"dns":{"servers":[{"tag":"remote","address":"https://1.1.1.1/dns-query"},"tls://8.8.8.8"]}}"#;
        let (has, servers) = config_dns(Some(raw));
        assert!(has);
        assert!(servers.contains(&"https://1.1.1.1/dns-query".to_string()));
        assert!(servers.contains(&"tls://8.8.8.8".to_string()));
        assert!(!config_dns(None).0);
    }

    #[test]
    fn command_detection_is_real() {
        assert!(command_exists("sh"));
        assert!(!command_exists("veil-definitely-missing"));
    }

    #[test]
    fn finds_installed_helper_or_reports_absence() {
        // Never fabricate: the result must match the filesystem.
        let found = find_helper(None);
        if let Some(path) = &found {
            assert!(path.exists());
        }
    }

    #[test]
    fn diagnostics_serializes_contract_fields() {
        let diag = run(
            CoreInfo::default(),
            None,
            &LogStore::new(),
            None,
        );
        let json = serde_json::to_value(&diag).unwrap();
        for key in [
            "core",
            "capabilities",
            "permissions",
            "interfaces",
            "tun",
            "dns",
            "errors",
            "tray",
            "helper",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert!(json["permissions"].get("tunDevice").is_some());
    }
}
