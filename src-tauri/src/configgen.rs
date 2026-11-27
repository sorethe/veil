use crate::paths;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub const MIXED_TAG: &str = "veil-mixed";
pub const TUN_TAG: &str = "veil-tun";
pub const TUN_INTERFACE: &str = "veil0";
pub const TUN_ADDRESS: [&str; 2] = ["172.19.0.1/30", "fdfe:dcba:9876::1/126"];
pub const LAN_EXCLUDES: [&str; 8] = [
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];

#[derive(Clone, Debug)]
pub struct GenOptions {
    pub mode: String,
    pub mixed_port: u16,
    pub api_port: u16,
    pub auto_redirect: bool,
    pub log_level: String,
}

impl Default for GenOptions {
    fn default() -> Self {
        Self {
            mode: "system".into(),
            mixed_port: 2080,
            api_port: 9090,
            auto_redirect: false,
            log_level: "info".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Generated {
    pub value: Value,
    pub json: String,
    pub secret: String,
    pub api_port: u16,
}

fn random_secret() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// Build a runtime config from a stored raw profile, mutating only the
/// known integration points and preserving every unknown field.
pub fn generate(raw: &str, opts: &GenOptions) -> Result<Generated, String> {
    let mut value: Value = serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    if !value.is_object() {
        return Err("profile must be a JSON object".into());
    }
    let outbounds_ok = value
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    if !outbounds_ok {
        return Err("profile must contain a non-empty outbounds array".into());
    }

    // log.level is UI-controlled; other log keys are preserved.
    if !value.get("log").map(Value::is_object).unwrap_or(false) {
        value["log"] = json!({ "level": opts.log_level });
    } else {
        value["log"]["level"] = json!(opts.log_level);
    }

    // experimental.clash_api: preserve cors/external_ui/etc.
    if !value
        .get("experimental")
        .map(Value::is_object)
        .unwrap_or(false)
    {
        value["experimental"] = json!({});
    }
    if !value["experimental"]
        .get("clash_api")
        .map(Value::is_object)
        .unwrap_or(false)
    {
        value["experimental"]["clash_api"] = json!({});
    }
    let secret = random_secret();
    value["experimental"]["clash_api"]["external_controller"] =
        json!(format!("127.0.0.1:{}", opts.api_port));
    value["experimental"]["clash_api"]["secret"] = json!(secret);

    if value.get("inbounds").is_none() {
        value["inbounds"] = json!([]);
    }
    if !value["inbounds"].is_array() {
        return Err("profile inbounds must be an array".into());
    }

    let tun_mode = opts.mode == "tun";
    {
        let inbounds = value["inbounds"].as_array_mut().unwrap();
        inbounds.retain(|inbound| {
            let tag = inbound.get("tag").and_then(Value::as_str).unwrap_or("");
            if tag == MIXED_TAG || tag == TUN_TAG {
                return false;
            }
            let kind = inbound.get("type").and_then(Value::as_str).unwrap_or("");
            // Reuse the port safely: a profile mixed inbound on our port would
            // otherwise make sing-box fail to bind. It is replaced by ours.
            if kind == "mixed"
                && inbound.get("listen_port").and_then(Value::as_u64)
                    == Some(opts.mixed_port as u64)
            {
                return false;
            }
            // Same for an existing TUN device with our interface name.
            if tun_mode
                && kind == "tun"
                && inbound.get("interface_name").and_then(Value::as_str) == Some(TUN_INTERFACE)
            {
                return false;
            }
            true
        });
        inbounds.push(json!({
            "type": "mixed",
            "tag": MIXED_TAG,
            "listen": "127.0.0.1",
            "listen_port": opts.mixed_port
        }));
        if tun_mode {
            let mut tun = json!({
                "type": "tun",
                "tag": TUN_TAG,
                "interface_name": TUN_INTERFACE,
                "address": TUN_ADDRESS,
                "mtu": 9000,
                "auto_route": true,
                "strict_route": true,
                "dns_mode": "hijack",
                "route_exclude_address": LAN_EXCLUDES
            });
            if opts.auto_redirect {
                tun["auto_redirect"] = json!(true);
            }
            inbounds.push(tun);
        }
    }

    let json = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    Ok(Generated {
        value,
        json,
        secret,
        api_port: opts.api_port,
    })
}

pub fn outbound_tags(value: &Value) -> Vec<String> {
    value
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|o| o.get("tag").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Write the runtime config with 0600 permissions, atomically.
pub fn write_runtime_config(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create runtime dir: {e}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    let _ = fs::remove_file(&tmp);
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("create runtime config: {e}"))?;
        file.write_all(contents.as_bytes())
            .map_err(|e| format!("write runtime config: {e}"))?;
        file.sync_all().ok();
    }
    fs::rename(&tmp, path).map_err(|e| format!("install runtime config: {e}"))?;
    Ok(())
}

pub fn ensure_runtime_dirs() -> Result<(), String> {
    paths::ensure_dirs().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_profile() -> String {
        json!({
            "log": { "level": "warn", "timestamp": true },
            "inbounds": [
                { "type": "mixed", "tag": "user-mixed", "listen": "127.0.0.1", "listen_port": 1080 }
            ],
            "outbounds": [
                { "type": "selector", "tag": "Proxy", "outbounds": ["a"] },
                { "type": "direct", "tag": "a" }
            ],
            "route": { "final": "Proxy" },
            "dns": { "servers": [{ "tag": "dns-remote", "address": "https://1.1.1.1/dns-query" }] },
            "experimental": { "cache_file": { "enabled": true }, "clash_api": { "external_ui": "ui" } },
            "custom_marker": { "kept": true }
        })
        .to_string()
    }

    #[test]
    fn preserves_unknown_fields_and_injects_clash_api() {
        let opts = GenOptions::default();
        let generated = generate(&base_profile(), &opts).unwrap();
        let v = &generated.value;
        assert_eq!(v["custom_marker"]["kept"], json!(true));
        assert_eq!(v["experimental"]["cache_file"]["enabled"], json!(true));
        assert_eq!(v["experimental"]["clash_api"]["external_ui"], json!("ui"));
        assert_eq!(
            v["experimental"]["clash_api"]["external_controller"],
            json!("127.0.0.1:9090")
        );
        assert_eq!(
            v["experimental"]["clash_api"]["secret"]
                .as_str()
                .unwrap()
                .len(),
            32
        );
        assert_eq!(v["log"]["level"], json!("info"));
        assert_eq!(v["log"]["timestamp"], json!(true));
        let tags: Vec<&str> = v["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| i["tag"].as_str())
            .collect();
        assert!(tags.contains(&"user-mixed"));
        assert!(tags.contains(&MIXED_TAG));
        assert!(!tags.contains(&TUN_TAG));
        let mixed = v["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["tag"] == MIXED_TAG)
            .unwrap();
        assert_eq!(mixed["listen"], json!("127.0.0.1"));
        assert_eq!(mixed["listen_port"], json!(2080));
        assert!(outbound_tags(v).contains(&"Proxy".to_string()));
    }

    #[test]
    fn tun_mode_adds_tun_and_respects_auto_redirect() {
        let opts = GenOptions {
            mode: "tun".into(),
            auto_redirect: false,
            ..GenOptions::default()
        };
        let generated = generate(&base_profile(), &opts).unwrap();
        let tun = generated.value["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["tag"] == TUN_TAG)
            .unwrap();
        assert_eq!(tun["interface_name"], json!(TUN_INTERFACE));
        assert_eq!(tun["address"], json!(TUN_ADDRESS));
        assert_eq!(tun["mtu"], json!(9000));
        assert_eq!(tun["auto_route"], json!(true));
        assert_eq!(tun["strict_route"], json!(true));
        assert_eq!(tun["dns_mode"], json!("hijack"));
        assert!(tun.get("auto_redirect").is_none());

        let opts = GenOptions {
            mode: "tun".into(),
            auto_redirect: true,
            ..GenOptions::default()
        };
        let generated = generate(&base_profile(), &opts).unwrap();
        let tun = generated.value["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["tag"] == TUN_TAG)
            .unwrap();
        assert_eq!(tun["auto_redirect"], json!(true));
    }

    #[test]
    fn replaces_conflicting_mixed_inbound_on_same_port() {
        let raw = json!({
            "inbounds": [
                { "type": "mixed", "tag": "conflict", "listen_port": 2080 },
                { "type": "mixed", "tag": "keep", "listen_port": 1234 }
            ],
            "outbounds": [{ "type": "direct", "tag": "direct" }]
        })
        .to_string();
        let generated = generate(&raw, &GenOptions::default()).unwrap();
        let inbounds = generated.value["inbounds"].as_array().unwrap();
        assert!(!inbounds.iter().any(|i| i["tag"] == "conflict"));
        assert!(inbounds.iter().any(|i| i["tag"] == "keep"));
        assert!(inbounds.iter().any(|i| i["tag"] == MIXED_TAG));
    }

    #[test]
    fn rejects_invalid_profiles() {
        assert!(generate("{}", &GenOptions::default()).is_err());
        assert!(generate("[]", &GenOptions::default()).is_err());
        assert!(generate("{\"outbounds\": []}", &GenOptions::default()).is_err());
        assert!(generate(
            "{\"outbounds\":[{\"type\":\"direct\",\"tag\":\"d\"}],\"inbounds\":{}}",
            &GenOptions::default()
        )
        .is_err());
    }

    /// Real-binary test, gated by VEIL_SINGBOX (run it explicitly):
    /// `VEIL_SINGBOX=/path/to/sing-box cargo test --lib tun_config_passes_real_check`.
    #[test]
    fn tun_config_passes_real_check() {
        let Ok(core) = std::env::var("VEIL_SINGBOX") else {
            return;
        };
        let raw = json!({
            "outbounds": [{ "type": "direct", "tag": "direct" }],
            "route": { "final": "direct" }
        })
        .to_string();
        for auto_redirect in [false, true] {
            let opts = GenOptions {
                mode: "tun".into(),
                auto_redirect,
                ..GenOptions::default()
            };
            let generated = generate(&raw, &opts).unwrap();
            let path = std::env::temp_dir().join(format!(
                "veil-tun-check-{}-{auto_redirect}.json",
                uuid::Uuid::new_v4()
            ));
            write_runtime_config(&path, &generated.json).unwrap();
            let result = crate::core::check(std::path::Path::new(&core), &path);
            let _ = std::fs::remove_file(&path);
            result.expect("generated TUN config must pass sing-box check");
        }
    }

    #[test]
    fn secrets_are_unique() {
        let a = generate(&base_profile(), &GenOptions::default()).unwrap();
        let b = generate(&base_profile(), &GenOptions::default()).unwrap();
        assert_ne!(a.secret, b.secret);
    }
}
