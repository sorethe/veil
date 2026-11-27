use crate::cidr::Cidr;
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

/// First-use TUN readiness. `ready` is the gate for TUN connect; `canInstall`
/// says whether `install_tun_helper` can plausibly succeed (a helper binary and
/// pkexec are available, and something is still missing).
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TunSetup {
    pub ready: bool,
    pub helper_found: bool,
    pub helper_installed: bool,
    pub helper_path: Option<String>,
    pub policy_installed: bool,
    pub pkexec_available: bool,
    pub tun_device: bool,
    pub nft: bool,
    pub can_install: bool,
    /// Configured addresses the preflight checked, copied from settings.
    pub tun_address: String,
    pub tun_address6: String,
    /// Other active TUN/VPN interfaces (ARPHRD_NONE, operstate not down) that
    /// would collide with Veil's interface routes and DNS.
    pub conflicting_interfaces: Vec<String>,
    /// Read-only route conflicts, formatted as "<cidr> dev <interface>".
    pub conflicting_routes: Vec<String>,
    /// Proposed host addresses that already belong to another interface.
    pub conflicting_addresses: Vec<String>,
    pub missing: Vec<String>,
    pub detail: String,
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

fn executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
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
            for ancestor in dir.ancestors().take(8) {
                candidates.push(ancestor.join("veil-helper"));
                candidates.push(
                    ancestor
                        .join("helper")
                        .join("target")
                        .join("release")
                        .join("veil-helper"),
                );
                candidates.push(
                    ancestor
                        .join("helper")
                        .join("target")
                        .join("debug")
                        .join("veil-helper"),
                );
                candidates.push(
                    ancestor
                        .join("src-tauri")
                        .join("helper")
                        .join("target")
                        .join("release")
                        .join("veil-helper"),
                );
            }
        }
    }
    if let Some(dir) = resource_dir {
        candidates.push(dir.join("veil-helper"));
    }
    if let Some(path) = which("veil-helper") {
        candidates.push(path);
    }
    candidates.into_iter().find(|p| executable_file(p))
}

fn polkit_policy_installed() -> bool {
    Path::new("/usr/share/polkit-1/actions/dev.veil.helper.policy").exists()
        || Path::new("/usr/share/polkit-1/actions/veil-helper.policy").exists()
}

#[derive(Default)]
struct SetupParts {
    helper_found: bool,
    helper_path: Option<String>,
    helper_installed: bool,
    policy_installed: bool,
    pkexec_available: bool,
    tun_device: bool,
    nft: bool,
    tun_address: String,
    tun_address6: String,
    conflicting_interfaces: Vec<String>,
    conflicting_routes: Vec<String>,
    conflicting_addresses: Vec<String>,
}

fn proposed_addresses(tun_address: &str, tun_address6: &str) -> (Vec<Cidr>, Vec<String>) {
    let mut proposed = Vec::new();
    let mut errors = Vec::new();
    match Cidr::parse_ipv4(tun_address) {
        Ok(cidr) => proposed.push(cidr),
        Err(error) => errors.push(format!(
            "tunAddress {tun_address:?} is not a valid IPv4 CIDR: {error}"
        )),
    }
    match Cidr::parse_ipv6(tun_address6) {
        Ok(cidr) => proposed.push(cidr),
        Err(error) => errors.push(format!(
            "tunAddress6 {tun_address6:?} is not a valid IPv6 CIDR: {error}"
        )),
    }
    (proposed, errors)
}

fn tun_setup_from_parts(parts: SetupParts) -> TunSetup {
    let SetupParts {
        helper_found,
        helper_path,
        helper_installed,
        policy_installed,
        pkexec_available,
        tun_device,
        nft,
        tun_address,
        tun_address6,
        conflicting_interfaces,
        conflicting_routes,
        conflicting_addresses,
    } = parts;

    let (_, address_errors) = proposed_addresses(&tun_address, &tun_address6);

    let mut other_missing = Vec::new();
    if !helper_installed {
        other_missing.push(if helper_found {
            "veil-helper is not installed to /usr/bin/veil-helper".to_string()
        } else {
            "veil-helper binary was not found".to_string()
        });
    }
    if !policy_installed {
        other_missing.push("polkit policy dev.veil.helper is not installed".to_string());
    }
    if !pkexec_available {
        other_missing.push("pkexec is not installed".to_string());
    }
    if !tun_device {
        other_missing.push("/dev/net/tun is missing".to_string());
    }
    if !nft {
        other_missing.push("nft is not installed".to_string());
    }

    let interface_conflict = !conflicting_interfaces.is_empty();
    let route_conflict = !conflicting_routes.is_empty();
    let address_conflict = !conflicting_addresses.is_empty();
    let address_invalid = !address_errors.is_empty();
    let conflict = interface_conflict || route_conflict || address_conflict || address_invalid;
    let mut missing = Vec::new();
    if interface_conflict {
        missing.push(format!(
            "Another TUN/VPN interface is active: {}; disable it before using Veil TUN",
            conflicting_interfaces.join(", ")
        ));
    }
    for route in &conflicting_routes {
        missing.push(format!(
            "Route {route} overlaps proposed TUN {tun_address} or {tun_address6}; Veil will not change routes"
        ));
    }
    for address in &conflicting_addresses {
        missing.push(format!(
            "Address {address} is already assigned to another interface; Veil will not reassign it"
        ));
    }
    missing.extend(address_errors.iter().cloned());
    missing.extend(other_missing.iter().cloned());

    let prerequisites_ready =
        helper_installed && policy_installed && pkexec_available && tun_device && nft;
    let ready = prerequisites_ready && !conflict;
    // Installing the helper cannot resolve route, address or DNS collisions,
    // so do not offer installation while any conflict is present.
    let can_install =
        helper_found && pkexec_available && (!helper_installed || !policy_installed) && !conflict;
    let detail = if ready {
        "TUN setup is ready.".to_string()
    } else if conflict {
        let mut detail = if interface_conflict {
            format!(
                "Another TUN/VPN interface is active ({}). Two TUN/VPN interfaces cannot share interface routes or DNS, so Veil will not start TUN until the other tunnel is disabled. Stop the other tunnel and check again, or use System proxy.",
                conflicting_interfaces.join(", ")
            )
        } else {
            "TUN setup is blocked by a network conflict.".to_string()
        };
        if route_conflict {
            detail.push_str(&format!(
                " Overlapping route(s): {}.",
                conflicting_routes.join("; ")
            ));
        }
        if address_conflict {
            detail.push_str(&format!(
                " Already assigned address(es): {}.",
                conflicting_addresses.join("; ")
            ));
        }
        if address_invalid {
            detail.push_str(&format!(
                " Invalid configured TUN address: {}.",
                address_errors.join("; ")
            ));
        }
        detail.push_str(
            " No routes were changed by this check. Veil does not modify routes, addresses, DNS, or firewall rules; Throne must remain untouched. Change the TUN addresses in Settings or use System proxy.",
        );
        if !other_missing.is_empty() {
            detail.push_str(&format!(
                " Other unmet requirements: {}.",
                other_missing.join("; ")
            ));
        }
        detail
    } else {
        format!("TUN setup is incomplete: {}", missing.join("; "))
    };
    TunSetup {
        ready,
        helper_found,
        helper_installed,
        helper_path,
        policy_installed,
        pkexec_available,
        tun_device,
        nft,
        can_install,
        tun_address,
        tun_address6,
        conflicting_interfaces,
        conflicting_routes,
        conflicting_addresses,
        missing,
        detail,
    }
}

/// Any non-down ARPHRD_NONE interface can own routes/DNS like a TUN. The
/// kernel `tun_flags` attribute is extra evidence when present, but is not
/// required: WireGuard and other VPN-style interfaces can use the same ARPHRD.
fn is_active_arp_hrd_none(path: &Path) -> bool {
    const ARPHRD_NONE: &str = "65534";
    let is_arp_hrd_none = fs::read_to_string(path.join("type"))
        .map(|value| value.trim() == ARPHRD_NONE)
        .unwrap_or(false);
    if !is_arp_hrd_none {
        return false;
    }
    let operstate = fs::read_to_string(path.join("operstate")).unwrap_or_default();
    operstate.trim() != "down"
}

/// TUN/VPN interfaces besides Veil's own that are currently not down. These
/// cannot coexist because each installs interface routes and a DNS hijack;
/// detecting the conflict up front avoids routing loops.
fn conflicting_tun_interfaces() -> Vec<String> {
    const OWN_INTERFACE: &str = "veil0";
    let mut names = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/net") else {
        return names;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == OWN_INTERFACE || name == "lo" {
            continue;
        }
        if !is_active_arp_hrd_none(&entry.path()) {
            continue;
        }
        names.push(name);
    }
    names.sort();
    names
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteSpec {
    pub destination: String,
    pub dev: Option<String>,
}

impl RouteSpec {
    pub fn from_destination(destination: &str, dev: Option<&str>) -> Option<Self> {
        let destination = normalize_destination(destination)?;
        Some(Self {
            destination,
            dev: dev.map(str::to_string),
        })
    }

    /// Parse one line of `ip route show` / `ip -6 route show` output.
    pub fn parse_line(line: &str) -> Option<Self> {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let destination = *tokens.first()?;
        let dev = tokens
            .iter()
            .position(|token| *token == "dev")
            .and_then(|index| tokens.get(index + 1))
            .copied();
        Self::from_destination(destination, dev)
    }

    pub fn description(&self) -> String {
        match &self.dev {
            Some(dev) => format!("{} dev {}", self.destination, dev),
            None => self.destination.clone(),
        }
    }
}

fn normalize_destination(value: &str) -> Option<String> {
    if value.is_empty() || value == "default" {
        return None;
    }
    let cidr = if value.contains('/') {
        Cidr::parse(value).ok()?
    } else {
        let addr: IpAddr = value.parse().ok()?;
        let prefix = if addr.is_ipv4() { 32 } else { 128 };
        Cidr { addr, prefix }
    };
    Some(cidr.to_string())
}

/// Compare route destinations with the proposed TUN CIDRs. Pure: callers can
/// feed captured `ip route` output without touching the network.
pub fn route_conflicts(routes: &[RouteSpec], proposed: &[Cidr]) -> Vec<String> {
    let mut conflicts = Vec::new();
    for route in routes {
        let Ok(destination) = Cidr::parse(&route.destination) else {
            continue;
        };
        if route.dev.as_deref() == Some("veil0") {
            continue;
        }
        if proposed
            .iter()
            .any(|address| address.overlaps(&destination))
        {
            conflicts.push(route.description());
        }
    }
    conflicts.sort();
    conflicts.dedup();
    conflicts
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddrSpec {
    pub address: String,
    pub ifname: String,
}

/// Proposed host addresses that already belong to another interface. Pure and
/// read-only; `veil0` is excluded because it is Veil's own interface.
pub fn address_conflicts(addresses: &[AddrSpec], proposed: &[Cidr]) -> Vec<String> {
    let mut conflicts = Vec::new();
    for assigned in addresses {
        if assigned.ifname == "veil0" {
            continue;
        }
        let Some(host) = assigned
            .address
            .split('/')
            .next()
            .and_then(|value| value.parse::<IpAddr>().ok())
        else {
            continue;
        };
        if proposed.iter().any(|address| address.addr == host) {
            conflicts.push(format!("{} on {}", assigned.address, assigned.ifname));
        }
    }
    conflicts.sort();
    conflicts.dedup();
    conflicts
}

fn route_from_json(value: &Value) -> Option<RouteSpec> {
    let destination = value.get("dst").and_then(Value::as_str)?;
    let dev = value.get("dev").and_then(Value::as_str);
    RouteSpec::from_destination(destination, dev)
}

fn route_specs_for_family(v6: bool) -> Vec<RouteSpec> {
    let mut json_args = vec!["-j"];
    if v6 {
        json_args.push("-6");
    }
    json_args.extend_from_slice(&["route", "show"]);
    if let Some(value) = run_ip(&json_args) {
        if let Some(routes) = value.as_array() {
            return routes.iter().filter_map(route_from_json).collect();
        }
    }

    let mut args = Vec::new();
    if v6 {
        args.push("-6");
    }
    args.extend_from_slice(&["route", "show"]);
    let Ok(output) = Command::new("ip").args(&args).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(RouteSpec::parse_line)
        .collect()
}

fn overlapping_routes(proposed: &[Cidr]) -> Vec<String> {
    let mut routes = route_specs_for_family(false);
    routes.extend(route_specs_for_family(true));
    route_conflicts(&routes, proposed)
}

fn assigned_addresses() -> Vec<AddrSpec> {
    let mut addresses = Vec::new();
    for interface in interfaces() {
        for address in interface.addresses {
            addresses.push(AddrSpec {
                address,
                ifname: interface.name.clone(),
            });
        }
    }
    addresses
}

fn overlapping_addresses(proposed: &[Cidr]) -> Vec<String> {
    address_conflicts(&assigned_addresses(), proposed)
}

/// Probe the real system for TUN prerequisites. Never reports a requirement as
/// satisfied unless the corresponding file/binary is actually present. Route
/// and address reads are read-only; nothing on the host is changed.
pub fn tun_setup(resource_dir: Option<&Path>, tun_address: &str, tun_address6: &str) -> TunSetup {
    let helper_path = find_helper(resource_dir);
    let helper_found = helper_path.is_some();
    let helper_installed = [
        Path::new("/usr/bin/veil-helper"),
        Path::new("/usr/local/bin/veil-helper"),
    ]
    .iter()
    .any(|path| executable_file(path));
    let policy_installed = polkit_policy_installed();
    let pkexec_available = find_in_path("pkexec").is_some();
    let tun_device = Path::new("/dev/net/tun").exists();
    let nft = find_in_path("nft").is_some();
    let (proposed, _) = proposed_addresses(tun_address, tun_address6);
    let (conflicting_routes, conflicting_addresses) = if proposed.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            overlapping_routes(&proposed),
            overlapping_addresses(&proposed),
        )
    };
    tun_setup_from_parts(SetupParts {
        helper_found,
        helper_path: helper_path.map(|path| path.to_string_lossy().into_owned()),
        helper_installed,
        policy_installed,
        pkexec_available,
        tun_device,
        nft,
        tun_address: tun_address.to_string(),
        tun_address6: tun_address6.to_string(),
        conflicting_interfaces: conflicting_tun_interfaces(),
        conflicting_routes,
        conflicting_addresses,
    })
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
    let helper_privileged = Path::new("/usr/bin/veil-helper").exists()
        || Path::new("/usr/local/bin/veil-helper").exists();
    // "permission" means a usable privileged helper + installed polkit action,
    // not merely a dev-build binary found beside the target directory.
    let helper_permitted = helper.installed && helper_privileged && polkit;
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
    } else if !helper_privileged {
        errors.push(
            "veil-helper is only a development build (not /usr/bin/veil-helper); TUN mode needs the installed helper."
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
        errors.push(
            "nft is not installed; auto_redirect and TUN routing diagnostics are limited.".into(),
        );
    }
    errors.truncate(12);

    Diagnostics {
        core,
        capabilities,
        permissions: Permissions {
            user,
            helper: helper_permitted,
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
    fn helper_permission_requires_privileged_helper_and_policy() {
        let diag = run(CoreInfo::default(), None, &LogStore::new(), None);
        let policy = Path::new("/usr/share/polkit-1/actions/dev.veil.helper.policy").exists()
            || Path::new("/usr/share/polkit-1/actions/veil-helper.policy").exists();
        let privileged = Path::new("/usr/bin/veil-helper").exists()
            || Path::new("/usr/local/bin/veil-helper").exists();
        let expected = diag.helper.installed && privileged && policy && command_exists("pkexec");
        eprintln!(
            "helper_installed={} helper_path={:?} polkit={} helper_permission={} privileged_helper={} policy={}",
            diag.helper.installed,
            diag.helper.path,
            diag.permissions.polkit,
            diag.permissions.helper,
            privileged,
            policy
        );
        assert_eq!(diag.permissions.helper, expected);
        if !policy {
            assert!(
                !diag.permissions.helper,
                "dev helper must not imply permission"
            );
        }
    }

    fn setup_parts(
        helper_found: bool,
        helper_installed: bool,
        policy_installed: bool,
        pkexec_available: bool,
        tun_device: bool,
        nft: bool,
    ) -> SetupParts {
        SetupParts {
            helper_found,
            helper_path: helper_found.then(|| "/usr/bin/veil-helper".to_string()),
            helper_installed,
            policy_installed,
            pkexec_available,
            tun_device,
            nft,
            tun_address: crate::configgen::DEFAULT_TUN_ADDRESS.into(),
            tun_address6: crate::configgen::DEFAULT_TUN_ADDRESS6.into(),
            ..SetupParts::default()
        }
    }

    fn setup(
        helper_found: bool,
        helper_installed: bool,
        policy_installed: bool,
        pkexec_available: bool,
        tun_device: bool,
        nft: bool,
    ) -> TunSetup {
        tun_setup_from_parts(setup_parts(
            helper_found,
            helper_installed,
            policy_installed,
            pkexec_available,
            tun_device,
            nft,
        ))
    }

    fn setup_with_conflicts(
        helper_found: bool,
        helper_installed: bool,
        policy_installed: bool,
        pkexec_available: bool,
        tun_device: bool,
        nft: bool,
        conflicting_interfaces: Vec<String>,
    ) -> TunSetup {
        let mut parts = setup_parts(
            helper_found,
            helper_installed,
            policy_installed,
            pkexec_available,
            tun_device,
            nft,
        );
        parts.conflicting_interfaces = conflicting_interfaces;
        tun_setup_from_parts(parts)
    }

    #[test]
    fn tun_setup_ready_requires_every_privileged_requirement() {
        let ready = setup(true, true, true, true, true, true);
        assert!(ready.ready);
        assert!(ready.missing.is_empty());
        assert!(!ready.can_install);
        assert_eq!(ready.detail, "TUN setup is ready.");
        for (helper_installed, policy, pkexec, tun, nft) in [
            (false, true, true, true, true),
            (true, false, true, true, true),
            (true, true, false, true, true),
            (true, true, true, false, true),
            (true, true, true, true, false),
        ] {
            let partial = setup(true, helper_installed, policy, pkexec, tun, nft);
            assert!(!partial.ready);
            assert_eq!(partial.missing.len(), 1);
        }
    }

    #[test]
    fn tun_setup_reports_each_missing_requirement_and_installability() {
        let incomplete = setup(true, false, false, true, true, true);
        assert!(!incomplete.ready);
        assert!(incomplete.can_install);
        assert!(incomplete.helper_found);
        assert!(incomplete
            .missing
            .iter()
            .any(|label| label.contains("not installed to /usr/bin/veil-helper")));
        assert!(incomplete
            .missing
            .iter()
            .any(|label| label.contains("polkit policy")));
        assert_eq!(incomplete.missing.len(), 2);

        let no_helper = setup(false, false, false, true, true, true);
        assert!(!no_helper.can_install);
        assert!(no_helper
            .missing
            .iter()
            .any(|label| label.contains("binary was not found")));

        let no_pkexec = setup(true, false, false, false, true, true);
        assert!(!no_pkexec.can_install);
    }

    #[test]
    fn tun_setup_rejects_active_conflicting_tun_interfaces() {
        let conflict = setup_with_conflicts(
            true,
            true,
            true,
            true,
            true,
            true,
            vec!["Meta".to_string(), "throne-tun".to_string()],
        );
        assert!(!conflict.ready);
        assert!(!conflict.can_install);
        assert_eq!(conflict.conflicting_interfaces, ["Meta", "throne-tun"]);
        assert_eq!(conflict.missing.len(), 1);
        assert!(
            conflict.missing[0].contains("Another TUN/VPN interface is active: Meta, throne-tun")
        );
        assert!(conflict.detail.contains("Meta, throne-tun"));
        assert!(conflict
            .detail
            .contains("cannot share interface routes or DNS"));

        // A conflict blocks readiness even when some prerequisites are also
        // missing, and the conflict label stays first for inline status.
        let mixed = setup_with_conflicts(
            true,
            false,
            false,
            true,
            true,
            true,
            vec!["Meta".to_string()],
        );
        assert!(!mixed.ready);
        assert!(!mixed.can_install);
        assert!(mixed.missing[0].contains("Meta"));
        assert!(mixed.detail.contains("Other unmet requirements"));
    }

    #[test]
    fn route_conflicts_are_parsed_from_ip_route_text() {
        let lines = [
            "172.19.0.0/24 dev throne-tun scope link",
            "10.0.0.0/8 dev eth0 scope link",
            "default via 192.168.1.1 dev eth0",
            "fdfe:dcba:9876::/64 dev throne-tun metric 256 pref medium",
            "172.19.0.128/25 dev eth0",
        ];
        let routes: Vec<RouteSpec> = lines
            .iter()
            .filter_map(|line| RouteSpec::parse_line(line))
            .collect();
        assert_eq!(routes.len(), 4);
        assert_eq!(
            RouteSpec::from_destination("172.19.0.1", None)
                .unwrap()
                .destination,
            "172.19.0.1/32"
        );
        let proposed = [
            Cidr::parse("172.19.0.1/30").unwrap(),
            Cidr::parse("fdfe:dcba:9876::1/126").unwrap(),
        ];
        assert_eq!(
            route_conflicts(&routes, &proposed),
            [
                "172.19.0.0/24 dev throne-tun",
                "fdfe:dcba:9876::/64 dev throne-tun"
            ]
        );
    }

    #[test]
    fn route_conflicts_ignore_veil_own_interface() {
        let routes = [
            RouteSpec::from_destination("172.19.0.0/30", Some("veil0")).unwrap(),
            RouteSpec::from_destination("172.19.0.0/24", Some("throne-tun")).unwrap(),
        ];
        let proposed = [Cidr::parse("172.19.0.1/30").unwrap()];
        assert_eq!(
            route_conflicts(&routes, &proposed),
            ["172.19.0.0/24 dev throne-tun"]
        );
    }

    #[test]
    fn tun_detection_accepts_arp_hrd_none_without_tun_flags() {
        let dir =
            std::env::temp_dir().join(format!("veil-tun-detect-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("type"), "65534\n").unwrap();
        fs::write(dir.join("operstate"), "unknown\n").unwrap();
        assert!(is_active_arp_hrd_none(&dir));
        assert!(!dir.join("tun_flags").exists());

        fs::write(dir.join("operstate"), "down\n").unwrap();
        assert!(!is_active_arp_hrd_none(&dir));

        if Path::new("/sys/class/net/throne-tun").exists() {
            assert!(
                conflicting_tun_interfaces()
                    .iter()
                    .any(|name| name == "throne-tun"),
                "the live Throne TUN must still be detected"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn address_conflicts_ignore_veil_and_match_proposed_hosts() {
        let assigned = [
            AddrSpec {
                address: "172.19.0.1/30".into(),
                ifname: "throne0".into(),
            },
            AddrSpec {
                address: "172.19.0.1/30".into(),
                ifname: "veil0".into(),
            },
            AddrSpec {
                address: "fdfe:dcba:9876::1/126".into(),
                ifname: "throne0".into(),
            },
            AddrSpec {
                address: "10.9.0.1/30".into(),
                ifname: "throne0".into(),
            },
        ];
        let proposed = [
            Cidr::parse("172.19.0.1/30").unwrap(),
            Cidr::parse("fdfe:dcba:9876::1/126").unwrap(),
        ];
        assert_eq!(
            address_conflicts(&assigned, &proposed),
            [
                "172.19.0.1/30 on throne0",
                "fdfe:dcba:9876::1/126 on throne0"
            ]
        );
    }

    #[test]
    fn tun_setup_rejects_overlapping_routes() {
        let mut parts = setup_parts(true, true, true, true, true, true);
        parts.conflicting_routes = vec!["172.19.0.0/24 dev throne-tun".into()];
        let setup = tun_setup_from_parts(parts);
        assert!(!setup.ready);
        assert!(!setup.can_install);
        assert_eq!(setup.conflicting_routes, ["172.19.0.0/24 dev throne-tun"]);
        assert!(setup.missing.iter().any(
            |label| label.contains("Route 172.19.0.0/24 dev throne-tun overlaps proposed TUN")
        ));
        assert!(setup.detail.contains("No routes were changed"));
        assert!(setup.detail.contains("Throne must remain untouched"));
    }

    #[test]
    fn tun_setup_rejects_assigned_proposed_addresses() {
        let mut parts = setup_parts(true, true, true, true, true, true);
        parts.conflicting_addresses = vec!["172.19.0.1/30 on Meta".into()];
        let setup = tun_setup_from_parts(parts);
        assert!(!setup.ready);
        assert!(!setup.can_install);
        assert_eq!(setup.conflicting_addresses, ["172.19.0.1/30 on Meta"]);
        assert!(setup
            .missing
            .iter()
            .any(|label| label.contains("Address 172.19.0.1/30 on Meta is already assigned")));
        assert!(setup.detail.contains("Throne must remain untouched"));
    }

    #[test]
    fn tun_setup_rejects_invalid_configured_addresses() {
        let mut parts = setup_parts(true, true, true, true, true, true);
        parts.tun_address = "172.19.0.1".into();
        parts.tun_address6 = "172.19.0.1/30".into();
        let setup = tun_setup_from_parts(parts);
        assert!(!setup.ready);
        assert!(!setup.can_install);
        assert_eq!(setup.tun_address, "172.19.0.1");
        assert!(setup
            .missing
            .iter()
            .any(|label| label.contains("tunAddress \"172.19.0.1\" is not a valid IPv4 CIDR")));
        assert!(setup
            .missing
            .iter()
            .any(|label| label.contains("tunAddress6 \"172.19.0.1/30\" is not a valid IPv6 CIDR")));
        assert!(setup.detail.contains("Invalid configured TUN address"));
    }

    #[test]
    fn tun_setup_reports_configured_addresses() {
        let mut parts = setup_parts(true, true, true, true, true, true);
        parts.tun_address = "10.9.0.1/30".into();
        parts.tun_address6 = "fdfe:2::1/126".into();
        let setup = tun_setup_from_parts(parts);
        assert!(setup.ready);
        assert_eq!(setup.tun_address, "10.9.0.1/30");
        assert_eq!(setup.tun_address6, "fdfe:2::1/126");
    }

    #[test]
    fn tun_setup_serializes_contract_fields() {
        let json = serde_json::to_value(setup(true, false, false, true, true, true)).unwrap();
        for key in [
            "ready",
            "helperFound",
            "helperInstalled",
            "helperPath",
            "policyInstalled",
            "pkexecAvailable",
            "tunDevice",
            "nft",
            "canInstall",
            "tunAddress",
            "tunAddress6",
            "conflictingInterfaces",
            "conflictingRoutes",
            "conflictingAddresses",
            "missing",
            "detail",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert_eq!(
            json["tunAddress"],
            serde_json::json!(crate::configgen::DEFAULT_TUN_ADDRESS)
        );
        assert_eq!(
            json["tunAddress6"],
            serde_json::json!(crate::configgen::DEFAULT_TUN_ADDRESS6)
        );
    }

    #[test]
    fn diagnostics_serializes_contract_fields() {
        let diag = run(CoreInfo::default(), None, &LogStore::new(), None);
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
