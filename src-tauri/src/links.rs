use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use percent_encoding::percent_decode_str;
use serde_json::{json, Value};
use std::collections::HashMap;
use url::Url;

#[derive(Clone, Debug)]
pub struct ParsedOutbound {
    pub tag: String,
    pub outbound: Value,
}

#[derive(Clone, Debug)]
pub struct LinkImport {
    pub outbounds: Vec<ParsedOutbound>,
    pub config: Value,
}

const SHADOWSOCKS_METHODS: [&str; 8] = [
    "none",
    "aes-128-gcm",
    "aes-192-gcm",
    "aes-256-gcm",
    "chacha20-ietf-poly1305",
    "xchacha20-ietf-poly1305",
    "2022-blake3-aes-128-gcm",
    "2022-blake3-aes-256-gcm",
];

fn decode_percent(input: &str) -> String {
    percent_decode_str(input).decode_utf8_lossy().into_owned()
}

fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let trimmed = cleaned.trim_end_matches('=');
    for engine in [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD] {
        if let Ok(bytes) = engine.decode(trimmed) {
            return Ok(bytes);
        }
    }
    Err("invalid base64 payload".into())
}

fn b64_string(input: &str) -> Result<String, String> {
    let bytes = b64_decode(input)?;
    String::from_utf8(bytes).map_err(|_| "base64 payload is not UTF-8".into())
}

fn split_host_port(raw: &str) -> Result<(String, u16), String> {
    let s = raw.trim().trim_end_matches('/');
    if let Some(rest) = s.strip_prefix('[') {
        let end = rest
            .find(']')
            .ok_or_else(|| format!("invalid IPv6 address in '{raw}'"))?;
        let host = &rest[..end];
        let after = &rest[end + 1..];
        let port = after
            .strip_prefix(':')
            .ok_or_else(|| format!("missing port in '{raw}'"))?;
        return Ok((
            host.to_string(),
            port.parse::<u16>()
                .map_err(|_| format!("invalid port in '{raw}'"))?,
        ));
    }
    let (host, port) = s
        .rsplit_once(':')
        .ok_or_else(|| format!("missing port in '{raw}'"))?;
    if host.is_empty() {
        return Err(format!("missing host in '{raw}'"));
    }
    Ok((
        host.to_string(),
        port.parse::<u16>()
            .map_err(|_| format!("invalid port in '{raw}'"))?,
    ))
}

fn query_map(url: &Url) -> HashMap<String, String> {
    url.query_pairs()
        .into_owned()
        .map(|(k, v)| (k, v))
        .collect()
}

fn q_bool(q: &HashMap<String, String>, keys: &[&str]) -> bool {
    for key in keys {
        if let Some(v) = q.get(*key) {
            if matches!(v.as_str(), "1" | "true" | "True" | "TRUE" | "yes") {
                return true;
            }
        }
    }
    false
}

fn q_alpn(q: &HashMap<String, String>) -> Vec<String> {
    q.get("alpn")
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn tls_json(
    server_name: &str,
    insecure: bool,
    alpn: Vec<String>,
    fingerprint: Option<&str>,
) -> Value {
    let mut tls = json!({ "enabled": true, "server_name": server_name });
    if insecure {
        tls["insecure"] = json!(true);
    }
    if !alpn.is_empty() {
        tls["alpn"] = json!(alpn);
    }
    if let Some(fp) = fingerprint.filter(|f| !f.is_empty()) {
        tls["utls"] = json!({ "enabled": true, "fingerprint": fp });
    }
    tls
}

fn transport_for(
    network: &str,
    path: Option<&str>,
    host: Option<&str>,
    service: Option<&str>,
    header_type: Option<&str>,
    scheme: &str,
) -> Result<Option<Value>, String> {
    let network = if network.is_empty() { "tcp" } else { network };
    let path = path.filter(|p| !p.is_empty()).unwrap_or("/");
    let result = match network {
        "tcp" => {
            if header_type == Some("http") {
                let mut t = json!({ "type": "http", "path": path });
                if let Some(host) = host.filter(|h| !h.is_empty()) {
                    t["host"] = json!([host]);
                }
                Some(t)
            } else {
                None
            }
        }
        "ws" => {
            let mut t = json!({ "type": "ws", "path": path });
            if let Some(host) = host.filter(|h| !h.is_empty()) {
                t["headers"] = json!({ "Host": host });
            }
            Some(t)
        }
        "http" | "h2" => {
            let mut t = json!({ "type": "http", "path": path });
            if let Some(host) = host.filter(|h| !h.is_empty()) {
                t["host"] = json!([host]);
            }
            Some(t)
        }
        "httpupgrade" => {
            let mut t = json!({ "type": "httpupgrade", "path": path });
            if let Some(host) = host.filter(|h| !h.is_empty()) {
                t["host"] = json!(host);
            }
            Some(t)
        }
        "grpc" => Some(json!({ "type": "grpc", "service_name": service.unwrap_or("") })),
        other => {
            return Err(format!(
                "unsupported {scheme} transport '{other}' (supported: tcp, ws, http, httpupgrade, grpc)"
            ))
        }
    };
    Ok(result)
}

fn unique_tag(base: &str, used: &mut Vec<String>) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "Node" } else { base };
    let mut candidate = base.to_string();
    let mut n = 2;
    while used.contains(&candidate) {
        candidate = format!("{base} {n}");
        n += 1;
    }
    used.push(candidate.clone());
    candidate
}

fn name_from_url(url: &Url, fallback: &str) -> String {
    url.fragment()
        .map(decode_percent)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn parse_vless(link: &str) -> Result<(String, Value), String> {
    let url = Url::parse(link).map_err(|e| format!("invalid vless URL: {e}"))?;
    let uuid = decode_percent(url.username());
    if uuid.is_empty() {
        return Err("vless link is missing a UUID".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "vless link is missing a host".to_string())?;
    let port = url.port().unwrap_or(443);
    let q = query_map(&url);
    let encryption = q.get("encryption").map(String::as_str).unwrap_or("none");
    if !matches!(encryption, "none" | "") {
        return Err(format!(
            "unsupported vless encryption '{encryption}' (only none is valid for sing-box)"
        ));
    }
    let mut out = json!({
        "type": "vless",
        "server": host,
        "server_port": port,
        "uuid": uuid
    });
    if let Some(flow) = q.get("flow").filter(|f| !f.is_empty()) {
        out["flow"] = json!(flow);
    }
    let security = q.get("security").map(String::as_str).unwrap_or("none");
    let sni = q
        .get("sni")
        .or_else(|| q.get("peer"))
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| host.to_string());
    match security {
        "tls" | "reality" => {
            let mut tls = tls_json(
                &sni,
                q_bool(&q, &["allowInsecure", "insecure"]),
                q_alpn(&q),
                q.get("fp").map(String::as_str),
            );
            if security == "reality" {
                let pbk = q
                    .get("pbk")
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| "vless reality link is missing pbk (public key)".to_string())?;
                let mut reality = json!({ "enabled": true, "public_key": pbk });
                if let Some(sid) = q.get("sid").filter(|s| !s.is_empty()) {
                    reality["short_id"] = json!(sid);
                }
                tls["reality"] = reality;
            }
            out["tls"] = tls;
        }
        "none" | "" => {}
        other => return Err(format!("unsupported vless security '{other}'")),
    }
    let network = q.get("type").map(String::as_str).unwrap_or("tcp");
    let path = q.get("path").map(String::as_str);
    let host_header = q.get("host").map(String::as_str).or_else(|| q.get("sni").map(String::as_str));
    let service = q.get("serviceName").map(String::as_str);
    if let Some(transport) = transport_for(
        network,
        path,
        host_header,
        service,
        q.get("headerType").map(String::as_str),
        "vless",
    )? {
        out["transport"] = transport;
    }
    let name = name_from_url(&url, &format!("{host}:{port}"));
    Ok((name, out))
}

fn parse_vmess(link: &str) -> Result<(String, Value), String> {
    let payload = link
        .strip_prefix("vmess://")
        .ok_or_else(|| "not a vmess link".to_string())?;
    let payload = payload.split('#').next().unwrap_or(payload);
    let decoded = b64_string(payload)
        .map_err(|e| format!("invalid vmess payload (expected base64 JSON): {e}"))?;
    let v: Value = serde_json::from_str(&decoded)
        .map_err(|e| format!("invalid vmess JSON: {e}"))?;
    let server = v
        .get("add")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "vmess link is missing 'add'".to_string())?;
    let port = match v.get("port") {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(443) as u16,
        Some(Value::String(s)) => s.parse().map_err(|_| "invalid vmess port".to_string())?,
        _ => 443,
    };
    let uuid = v
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "vmess link is missing 'id'".to_string())?;
    let security = v
        .get("scy")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("auto");
    let alter_id = match v.get("aid") {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    };
    let mut out = json!({
        "type": "vmess",
        "server": server,
        "server_port": port,
        "uuid": uuid,
        "security": security,
        "alter_id": alter_id
    });
    let tls_enabled = v.get("tls").and_then(Value::as_str) == Some("tls");
    let sni = v
        .get("sni")
        .and_then(Value::as_str)
        .or_else(|| v.get("host").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .unwrap_or(server);
    if tls_enabled {
        let alpn = v
            .get("alpn")
            .and_then(Value::as_str)
            .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_default();
        let insecure = matches!(
            v.get("allowInsecure").and_then(Value::as_str),
            Some("1") | Some("true")
        );
        out["tls"] = tls_json(
            sni,
            insecure,
            alpn,
            v.get("fp").and_then(Value::as_str),
        );
    }
    let network = v.get("net").and_then(Value::as_str).unwrap_or("tcp");
    let path = v.get("path").and_then(Value::as_str);
    let host = v.get("host").and_then(Value::as_str);
    let header_type = v.get("type").and_then(Value::as_str);
    if let Some(transport) = transport_for(network, path, host, path, header_type, "vmess")? {
        out["transport"] = transport;
    }
    let name = v
        .get("ps")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{server}:{port}"));
    Ok((name, out))
}

fn parse_trojan(link: &str) -> Result<(String, Value), String> {
    let url = Url::parse(link).map_err(|e| format!("invalid trojan URL: {e}"))?;
    let password = decode_percent(url.username());
    if password.is_empty() {
        return Err("trojan link is missing a password".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "trojan link is missing a host".to_string())?;
    let port = url.port().unwrap_or(443);
    let q = query_map(&url);
    let mut out = json!({
        "type": "trojan",
        "server": host,
        "server_port": port,
        "password": password
    });
    let security = q.get("security").map(String::as_str).unwrap_or("tls");
    if security != "none" {
        let sni = q
            .get("sni")
            .or_else(|| q.get("peer"))
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| host.to_string());
        out["tls"] = tls_json(
            &sni,
            q_bool(&q, &["allowInsecure", "insecure"]),
            q_alpn(&q),
            q.get("fp").map(String::as_str),
        );
    }
    let network = q.get("type").map(String::as_str).unwrap_or("tcp");
    if let Some(transport) = transport_for(
        network,
        q.get("path").map(String::as_str),
        q.get("host").map(String::as_str),
        q.get("serviceName").map(String::as_str),
        q.get("headerType").map(String::as_str),
        "trojan",
    )? {
        out["transport"] = transport;
    }
    let name = name_from_url(&url, &format!("{host}:{port}"));
    Ok((name, out))
}

fn decode_ss_userinfo(raw: &str) -> Result<(String, String), String> {
    let decoded = decode_percent(raw);
    let plain = if decoded.contains(':') {
        decoded
    } else {
        b64_string(&decoded).map_err(|e| format!("invalid Shadowsocks userinfo: {e}"))?
    };
    let (method, password) = plain
        .split_once(':')
        .ok_or_else(|| "Shadowsocks userinfo must be method:password".to_string())?;
    if method.is_empty() || password.is_empty() {
        return Err("Shadowsocks method or password is empty".into());
    }
    Ok((method.to_string(), password.to_string()))
}

fn parse_ss(link: &str) -> Result<(String, Value), String> {
    let rest = link
        .strip_prefix("ss://")
        .ok_or_else(|| "not a Shadowsocks link".to_string())?;
    let (rest, fragment) = match rest.split_once('#') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('/') {
        Some((a, b)) if b.starts_with('?') || b.is_empty() => (a, Some(b.trim_start_matches('?'))),
        _ => (rest, None),
    };
    let (rest, inline_query) = match rest.split_once('?') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let query = inline_query.or(query).unwrap_or("").to_string();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key.eq_ignore_ascii_case("plugin") && !value.is_empty() && value != "none" {
            return Err(format!(
                "Shadowsocks plugin '{value}' is not supported by sing-box; remove the plugin or use a different node"
            ));
        }
    }
    let mut name: Option<String> = fragment.map(decode_percent);
    let (method, password, host, port) = if let Some((userinfo, hostport)) = rest.rsplit_once('@') {
        let (method, password) = decode_ss_userinfo(userinfo)?;
        let (host, port) = split_host_port(hostport)?;
        (method, password, host, port)
    } else {
        let decoded = b64_string(rest)
            .map_err(|e| format!("invalid legacy Shadowsocks link: {e}"))?;
        let (userinfo, hostport) = decoded
            .rsplit_once('@')
            .ok_or_else(|| "invalid legacy Shadowsocks payload".to_string())?;
        let (method, password) = userinfo
            .split_once(':')
            .ok_or_else(|| "legacy Shadowsocks payload must be method:password@host:port".to_string())?;
        let (host, port) = split_host_port(hostport)?;
        (method.to_string(), password.to_string(), host, port)
    };
    if !SHADOWSOCKS_METHODS.contains(&method.as_str()) {
        return Err(format!(
            "unsupported Shadowsocks method '{method}' (sing-box supports: {})",
            SHADOWSOCKS_METHODS.join(", ")
        ));
    }
    if name.as_deref().map(str::trim).unwrap_or("").is_empty() {
        name = Some(format!("{host}:{port}"));
    }
    let out = json!({
        "type": "shadowsocks",
        "server": host,
        "server_port": port,
        "method": method,
        "password": password
    });
    Ok((name.unwrap(), out))
}

fn parse_hysteria2(link: &str) -> Result<(String, Value), String> {
    let url = Url::parse(link).map_err(|e| format!("invalid hysteria2 URL: {e}"))?;
    let mut password = decode_percent(url.username());
    if let Some(pw) = url.password() {
        password = format!("{password}:{}", decode_percent(pw));
    }
    if password.is_empty() {
        return Err("hysteria2 link is missing a password".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "hysteria2 link is missing a host".to_string())?;
    let port = url.port().unwrap_or(443);
    let q = query_map(&url);
    if q.get("pinSHA256").map(|p| !p.is_empty()).unwrap_or(false) {
        return Err(
            "hysteria2 certificate pinning (pinSHA256) is not supported by sing-box".into(),
        );
    }
    let sni = q
        .get("sni")
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| host.to_string());
    let mut out = json!({
        "type": "hysteria2",
        "server": host,
        "server_port": port,
        "password": password,
        "tls": tls_json(&sni, q_bool(&q, &["insecure", "allowInsecure"]), q_alpn(&q), None)
    });
    if let Some(obfs) = q.get("obfs").filter(|s| !s.is_empty()) {
        if obfs != "salamander" {
            return Err(format!("unsupported hysteria2 obfs '{obfs}'"));
        }
        out["obfs"] = json!({
            "type": "salamander",
            "password": q.get("obfs-password").cloned().unwrap_or_default()
        });
    }
    let name = name_from_url(&url, &format!("{host}:{port}"));
    Ok((name, out))
}

fn parse_tuic(link: &str) -> Result<(String, Value), String> {
    let url = Url::parse(link).map_err(|e| format!("invalid tuic URL: {e}"))?;
    let uuid = decode_percent(url.username());
    let password = url.password().map(decode_percent).unwrap_or_default();
    if uuid.is_empty() || password.is_empty() {
        return Err("tuic link requires uuid:password".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "tuic link is missing a host".to_string())?;
    let port = url.port().unwrap_or(443);
    let q = query_map(&url);
    let sni = q
        .get("sni")
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| host.to_string());
    let mut out = json!({
        "type": "tuic",
        "server": host,
        "server_port": port,
        "uuid": uuid,
        "password": password,
        "tls": tls_json(&sni, q_bool(&q, &["insecure", "allow_insecure", "allowInsecure"]), q_alpn(&q), None)
    });
    if let Some(cc) = q.get("congestion_control").filter(|s| !s.is_empty()) {
        out["congestion_control"] = json!(cc);
    }
    if let Some(mode) = q.get("udp_relay_mode").filter(|s| !s.is_empty()) {
        out["udp_relay_mode"] = json!(mode);
    }
    let name = name_from_url(&url, &format!("{host}:{port}"));
    Ok((name, out))
}

fn parse_socks(link: &str) -> Result<(String, Value), String> {
    let url = Url::parse(link).map_err(|e| format!("invalid socks URL: {e}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| "socks link is missing a host".to_string())?;
    let port = url.port().unwrap_or(1080);
    let mut out = json!({
        "type": "socks",
        "server": host,
        "server_port": port,
        "version": "5"
    });
    if !url.username().is_empty() {
        out["username"] = json!(decode_percent(url.username()));
    }
    if let Some(password) = url.password() {
        out["password"] = json!(decode_percent(password));
    }
    let name = name_from_url(&url, &format!("{host}:{port}"));
    Ok((name, out))
}

fn parse_http(link: &str) -> Result<(String, Value), String> {
    let url = Url::parse(link).map_err(|e| format!("invalid HTTP proxy URL: {e}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| "HTTP proxy link is missing a host".to_string())?;
    let tls = url.scheme() == "https";
    let port = url
        .port()
        .unwrap_or(if tls { 443 } else { 8080 });
    let mut out = json!({
        "type": "http",
        "server": host,
        "server_port": port
    });
    if !url.username().is_empty() {
        out["username"] = json!(decode_percent(url.username()));
    }
    if let Some(password) = url.password() {
        out["password"] = json!(decode_percent(password));
    }
    if tls {
        out["tls"] = json!({ "enabled": true, "server_name": host });
    }
    let name = name_from_url(&url, &format!("{host}:{port}"));
    Ok((name, out))
}

fn parse_one(link: &str) -> Result<(String, Value), String> {
    let scheme = link
        .split_once("://")
        .map(|(s, _)| s.to_ascii_lowercase())
        .ok_or_else(|| "line is not a URI (missing ://)".to_string())?;
    match scheme.as_str() {
        "vless" => parse_vless(link),
        "vmess" => parse_vmess(link),
        "trojan" => parse_trojan(link),
        "ss" => parse_ss(link),
        "hysteria2" | "hy2" => parse_hysteria2(link),
        "tuic" => parse_tuic(link),
        "socks" | "socks5" | "socks5h" => parse_socks(link),
        "http" | "https" => parse_http(link),
        "socks4" | "socks4a" => Err("SOCKS4 is not supported; use SOCKS5".into()),
        other => Err(format!("unsupported link scheme '{other}'")),
    }
}

/// Parse a newline-separated list of share links into a sing-box config with a
/// "Proxy" selector and route.final = "Proxy".
pub fn parse_links(text: &str) -> Result<LinkImport, String> {
    let mut outbounds: Vec<ParsedOutbound> = Vec::new();
    let mut used = vec!["Proxy".to_string(), "direct".to_string(), "DIRECT".to_string()];
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim().trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let (name, outbound) = parse_one(line)
            .map_err(|e| format!("line {}: {e}", index + 1))?;
        let tag = unique_tag(&name, &mut used);
        outbounds.push(ParsedOutbound { tag, outbound });
    }
    if outbounds.is_empty() {
        return Err("no proxy links found".into());
    }
    let tags: Vec<String> = outbounds.iter().map(|o| o.tag.clone()).collect();
    let mut config_outbounds: Vec<Value> = Vec::new();
    config_outbounds.push(json!({
        "type": "selector",
        "tag": "Proxy",
        "outbounds": tags,
        "default": outbounds[0].tag
    }));
    for parsed in &outbounds {
        let mut outbound = parsed.outbound.clone();
        outbound["tag"] = json!(parsed.tag);
        config_outbounds.push(outbound);
    }
    config_outbounds.push(json!({ "type": "direct", "tag": "direct" }));
    let config = json!({
        "log": { "level": "info" },
        "outbounds": config_outbounds,
        "route": { "final": "Proxy" }
    });
    Ok(LinkImport { outbounds, config })
}

pub fn links_to_config_json(text: &str) -> Result<String, String> {
    let import = parse_links(text)?;
    serde_json::to_string_pretty(&import.config).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outbound_for(link: &str) -> Value {
        let (_, out) = parse_one(link).unwrap();
        out
    }

    #[test]
    fn parses_vless_ws_tls() {
        let out = outbound_for("vless://11111111-2222-3333-4444-555555555555@example.com:443?encryption=none&security=tls&sni=cdn.example.com&type=ws&path=%2Fws&host=cdn.example.com&fp=chrome#My%20Node");
        assert_eq!(out["type"], "vless");
        assert_eq!(out["uuid"], "11111111-2222-3333-4444-555555555555");
        assert_eq!(out["server"], "example.com");
        assert_eq!(out["server_port"], 443);
        assert_eq!(out["tls"]["server_name"], "cdn.example.com");
        assert_eq!(out["tls"]["utls"]["fingerprint"], "chrome");
        assert_eq!(out["transport"]["type"], "ws");
        assert_eq!(out["transport"]["path"], "/ws");
        assert_eq!(out["transport"]["headers"]["Host"], "cdn.example.com");
    }

    #[test]
    fn parses_vless_reality_grpc() {
        let out = outbound_for("vless://aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee@1.2.3.4:443?security=reality&sni=www.example.com&pbk=PUBKEY&sid=ab12&fp=firefox&type=grpc&serviceName=svc");
        assert_eq!(out["tls"]["reality"]["enabled"], true);
        assert_eq!(out["tls"]["reality"]["public_key"], "PUBKEY");
        assert_eq!(out["tls"]["reality"]["short_id"], "ab12");
        assert_eq!(out["transport"]["type"], "grpc");
        assert_eq!(out["transport"]["service_name"], "svc");
    }

    #[test]
    fn parses_vmess_base64() {
        let payload = json!({
            "v": "2", "ps": "VM Node", "add": "vm.example.com", "port": "8443",
            "id": "11111111-2222-3333-4444-555555555555", "aid": "0", "scy": "auto",
            "net": "ws", "path": "/ray", "host": "vm.example.com", "tls": "tls",
            "sni": "vm.example.com"
        });
        let encoded = STANDARD.encode(serde_json::to_vec(&payload).unwrap());
        let out = outbound_for(&format!("vmess://{encoded}"));
        assert_eq!(out["type"], "vmess");
        assert_eq!(out["server"], "vm.example.com");
        assert_eq!(out["server_port"], 8443);
        assert_eq!(out["security"], "auto");
        assert_eq!(out["tls"]["server_name"], "vm.example.com");
        assert_eq!(out["transport"]["path"], "/ray");
        let (name, _) = parse_one(&format!("vmess://{encoded}")).unwrap();
        assert_eq!(name, "VM Node");
    }

    #[test]
    fn parses_trojan_ws() {
        let out = outbound_for("trojan://secret%20pass@tr.example.com:443?security=tls&sni=tr.example.com&type=ws&path=/tj");
        assert_eq!(out["type"], "trojan");
        assert_eq!(out["password"], "secret pass");
        assert_eq!(out["tls"]["server_name"], "tr.example.com");
        assert_eq!(out["transport"]["type"], "ws");
    }

    #[test]
    fn parses_shadowsocks_sip002_and_legacy() {
        let userinfo = STANDARD.encode("aes-256-gcm:p@ss:word");
        let out = outbound_for(&format!("ss://{userinfo}@ss.example.com:8388#SS%20Node"));
        assert_eq!(out["type"], "shadowsocks");
        assert_eq!(out["method"], "aes-256-gcm");
        assert_eq!(out["password"], "p@ss:word");
        assert_eq!(out["server_port"], 8388);

        let legacy = STANDARD.encode("chacha20-ietf-poly1305:pw@1.1.1.1:443");
        let out = outbound_for(&format!("ss://{legacy}"));
        assert_eq!(out["server"], "1.1.1.1");
        assert_eq!(out["method"], "chacha20-ietf-poly1305");
    }

    #[test]
    fn rejects_ss_plugin_and_unknown_method() {
        let userinfo = STANDARD.encode("aes-256-gcm:pw");
        let link = format!("ss://{userinfo}@example.com:8388?plugin=obfs-local%3Bobfs%3Dhttp");
        assert!(parse_one(&link).unwrap_err().contains("plugin"));
        let userinfo = STANDARD.encode("rc4-md5:pw");
        let link = format!("ss://{userinfo}@example.com:8388");
        assert!(parse_one(&link).unwrap_err().contains("unsupported Shadowsocks method"));
    }

    #[test]
    fn parses_hysteria2_with_obfs() {
        let out = outbound_for("hysteria2://letmein@hy.example.com:8443?sni=hy.example.com&insecure=1&obfs=salamander&obfs-password=xyz");
        assert_eq!(out["type"], "hysteria2");
        assert_eq!(out["password"], "letmein");
        assert_eq!(out["tls"]["insecure"], true);
        assert_eq!(out["obfs"]["type"], "salamander");
        assert_eq!(out["obfs"]["password"], "xyz");
    }

    #[test]
    fn parses_tuic() {
        let out = outbound_for("tuic://11111111-2222-3333-4444-555555555555:pw@tu.example.com:443?sni=tu.example.com&congestion_control=bbr&alpn=h3");
        assert_eq!(out["type"], "tuic");
        assert_eq!(out["uuid"], "11111111-2222-3333-4444-555555555555");
        assert_eq!(out["password"], "pw");
        assert_eq!(out["congestion_control"], "bbr");
        assert_eq!(out["tls"]["alpn"][0], "h3");
    }

    #[test]
    fn parses_socks_and_http() {
        let out = outbound_for("socks5://user:pw@socks.example.com:1080#Socks");
        assert_eq!(out["type"], "socks");
        assert_eq!(out["version"], "5");
        assert_eq!(out["username"], "user");
        assert_eq!(out["password"], "pw");
        let out = outbound_for("https://user:pw@proxy.example.com:3128");
        assert_eq!(out["type"], "http");
        assert_eq!(out["tls"]["enabled"], true);
        assert_eq!(out["server_port"], 3128);
    }

    #[test]
    fn builds_selector_config_with_unique_tags() {
        let text = "\
vless://11111111-2222-3333-4444-555555555555@a.example.com:443?encryption=none&security=tls#Node
ss://YWVzLTI1Ni1nY206cHc=@b.example.com:8388#Node
http://c.example.com:8080
";
        let import = parse_links(text).unwrap();
        assert_eq!(import.outbounds.len(), 3);
        assert_eq!(import.outbounds[0].tag, "Node");
        assert_eq!(import.outbounds[1].tag, "Node 2");
        let outbounds = import.config["outbounds"].as_array().unwrap();
        assert_eq!(outbounds[0]["type"], "selector");
        assert_eq!(outbounds[0]["tag"], "Proxy");
        assert_eq!(outbounds[0]["outbounds"].as_array().unwrap().len(), 3);
        assert_eq!(import.config["route"]["final"], "Proxy");
        assert!(outbounds.iter().any(|o| o["tag"] == "direct"));
    }

    /// Real-binary validation, gated by VEIL_SINGBOX:
    /// `VEIL_SINGBOX=/path/to/sing-box cargo test --lib generated_link_config_passes_real_check`.
    #[test]
    fn generated_link_config_passes_real_check() {
        let Ok(core) = std::env::var("VEIL_SINGBOX") else {
            return;
        };
        let vmess = STANDARD.encode(
            serde_json::to_vec(&json!({
                "v": "2", "ps": "vm", "add": "vm.example.com", "port": "443",
                "id": "11111111-2222-3333-4444-555555555555", "aid": "0",
                "scy": "auto", "net": "ws", "path": "/ray", "host": "vm.example.com",
                "tls": "tls"
            }))
            .unwrap(),
        );
        let text = format!(
            "vless://11111111-2222-3333-4444-555555555555@example.com:443?encryption=none&security=tls&type=ws&path=%2Fws#VLESS\n\
             vmess://{vmess}\n\
             trojan://pw@tr.example.com:443?sni=tr.example.com\n\
             ss://YWVzLTI1Ni1nY206cHc=@ss.example.com:8388#SS\n\
             hysteria2://pw@hy.example.com:8443?sni=hy.example.com\n\
             tuic://11111111-2222-3333-4444-555555555555:pw@tu.example.com:443?sni=tu.example.com\n\
             socks5://user:pw@socks.example.com:1080\n\
             http://proxy.example.com:8080"
        );
        let content = links_to_config_json(&text).unwrap();
        let path = std::env::temp_dir().join(format!(
            "veil-links-check-{}.json",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, content).unwrap();
        let result = crate::core::check(std::path::Path::new(&core), &path);
        let _ = std::fs::remove_file(&path);
        result.expect("generated share-link config must pass sing-box check");
    }

    #[test]
    fn reports_line_numbers_for_bad_links() {
        let err = parse_links("http://ok.example.com:8080\nftp://bad.example.com:21").unwrap_err();
        assert!(err.contains("line 2"), "{err}");
        assert!(err.contains("unsupported link scheme"));
        assert!(parse_links("# only comments\n\n").is_err());
    }
}
