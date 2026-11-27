use reqwest::{Method, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct ClashClient {
    pub base: String,
    pub secret: String,
    http: reqwest::Client,
    /// Streaming client without a total-request timeout: `/traffic` and
    /// `/logs` are long-lived NDJSON responses.
    stream_http: reqwest::Client,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outbound {
    pub tag: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub now: Option<String>,
    pub options: Vec<String>,
    pub delay: Option<u64>,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Conn {
    pub id: String,
    pub host: String,
    pub destination: String,
    pub network: String,
    pub protocol: String,
    pub process: String,
    pub outbound: String,
    pub rule: String,
    pub upload: u64,
    pub download: u64,
    pub start: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    pub index: usize,
    pub action: String,
    pub outbound: Option<String>,
    #[serde(rename = "match")]
    pub match_: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteInfo {
    pub rules: Vec<RouteRule>,
    #[serde(rename = "final")]
    pub final_outbound: Option<String>,
    pub mode: Option<String>,
    pub modes: Vec<String>,
    pub dns_rules: Vec<RouteRule>,
    pub dns_final: Option<String>,
}

impl ClashClient {
    pub fn new(port: u16, secret: &str) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        let stream_http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_default();
        Self {
            base: format!("http://127.0.0.1:{port}"),
            secret: secret.to_string(),
            http,
            stream_http,
        }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let req = self
            .http
            .request(method, format!("{}{}", self.base, path))
            .header("Accept", "application/json");
        if self.secret.is_empty() {
            req
        } else {
            req.bearer_auth(&self.secret)
        }
    }

    async fn json(&self, method: Method, path: &str) -> Result<Value, String> {
        let resp = self
            .request(method, path)
            .send()
            .await
            .map_err(|e| format!("clash api {path}: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "clash api {path}: HTTP {} {}",
                status.as_u16(),
                body.trim()
            ));
        }
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|e| format!("clash api {path}: invalid JSON: {e}"))
    }

    pub async fn version(&self) -> Result<Value, String> {
        self.json(Method::GET, "/version").await
    }

    pub async fn configs(&self) -> Result<Value, String> {
        self.json(Method::GET, "/configs").await
    }

    pub async fn proxies(&self) -> Result<Value, String> {
        self.json(Method::GET, "/proxies").await
    }

    pub async fn connections(&self) -> Result<Value, String> {
        self.json(Method::GET, "/connections").await
    }

    pub async fn close_connection(&self, id: &str) -> Result<(), String> {
        self.json(Method::DELETE, &format!("/connections/{}", urlencode(id)))
            .await
            .map(|_| ())
    }

    pub async fn select(&self, group: &str, tag: &str) -> Result<(), String> {
        let resp = self
            .request(Method::PUT, &format!("/proxies/{}", urlencode(group)))
            .json(&serde_json::json!({ "name": tag }))
            .send()
            .await
            .map_err(|e| format!("select outbound: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "select outbound: HTTP {} {}",
                status.as_u16(),
                body.trim()
            ));
        }
        Ok(())
    }

    pub async fn delay(&self, tag: &str, timeout_ms: u64) -> Result<u64, String> {
        let path = format!(
            "/proxies/{}/delay?url={}&timeout={}",
            urlencode(tag),
            urlencode("https://www.gstatic.com/generate_204"),
            timeout_ms
        );
        let value = self.json(Method::GET, &path).await?;
        value
            .get("delay")
            .and_then(Value::as_u64)
            .ok_or_else(|| "delay test returned no delay (proxy unreachable?)".to_string())
    }

    pub async fn set_mode(&self, mode: &str) -> Result<(), String> {
        let resp = self
            .request(Method::PATCH, "/configs")
            .json(&serde_json::json!({ "mode": mode }))
            .send()
            .await
            .map_err(|e| format!("set clash mode: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "set clash mode: HTTP {} {}",
                status.as_u16(),
                body.trim()
            ));
        }
        Ok(())
    }

    pub async fn traffic(&self) -> Result<reqwest::Response, String> {
        let resp = self
            .stream_http
            .request(Method::GET, format!("{}/traffic", self.base))
            .bearer_auth(&self.secret)
            .send()
            .await
            .map_err(|e| format!("traffic stream: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("traffic stream: HTTP {}", resp.status().as_u16()));
        }
        Ok(resp)
    }

    pub async fn logs(&self, level: &str) -> Result<reqwest::Response, String> {
        let path = format!("/logs?level={}", urlencode(level));
        let resp = self
            .stream_http
            .request(Method::GET, format!("{}{}", self.base, path))
            .bearer_auth(&self.secret)
            .send()
            .await
            .map_err(|e| format!("log stream: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("log stream: HTTP {}", resp.status().as_u16()));
        }
        Ok(resp)
    }

    /// Poll /version until it answers or the deadline passes.
    pub async fn wait_ready(&self, timeout: Duration) -> Result<Value, String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let last_err = match self.version().await {
                Ok(value) => return Ok(value),
                Err(e) => e,
            };
            if tokio::time::Instant::now() >= deadline {
                return Err(last_err);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

fn urlencode(input: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(input, NON_ALPHANUMERIC).to_string()
}

pub fn map_outbounds(value: &Value) -> Vec<Outbound> {
    let Some(proxies) = value.get("proxies").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (tag, proxy) in proxies {
        if proxy.get("hidden").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let kind_name = proxy
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let kind = match kind_name.to_ascii_lowercase().as_str() {
            "selector" => "selector",
            "urltest" => "urltest",
            _ => "other",
        }
        .to_string();
        let now = proxy
            .get("now")
            .and_then(Value::as_str)
            .map(str::to_string);
        let options = proxy
            .get("all")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let delay = proxy
            .get("history")
            .and_then(Value::as_array)
            .and_then(|h| h.last())
            .and_then(|h| h.get("delay"))
            .and_then(Value::as_u64)
            .filter(|d| *d > 0);
        out.push(Outbound {
            tag: tag.clone(),
            r#type: kind_name,
            now,
            options,
            delay,
            kind,
        });
    }
    out.sort_by(|a, b| a.tag.cmp(&b.tag));
    out
}

pub fn map_connections(value: &Value) -> Vec<Conn> {
    let Some(list) = value.get("connections").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .map(|c| {
            let meta = c.get("metadata").cloned().unwrap_or(Value::Null);
            let host = meta
                .get("host")
                .and_then(Value::as_str)
                .filter(|h| !h.is_empty())
                .unwrap_or_else(|| {
                    meta.get("destinationIP")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                })
                .to_string();
            let ip = meta
                .get("destinationIP")
                .and_then(Value::as_str)
                .unwrap_or("");
            let port = meta
                .get("destinationPort")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let host_for_dest = if ip.is_empty() { host.as_str() } else { ip };
            let destination = if host_for_dest.contains(':') && !host_for_dest.starts_with('[') {
                format!("[{host_for_dest}]:{port}")
            } else if port.is_empty() {
                host_for_dest.to_string()
            } else {
                format!("{host_for_dest}:{port}")
            };
            let chains = c
                .get("chains")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                .unwrap_or_default();
            let outbound = chains
                .last()
                .copied()
                .or_else(|| chains.first().copied())
                .unwrap_or("")
                .to_string();
            let rule = match c.get("rulePayload").and_then(Value::as_str) {
                Some(p) if !p.is_empty() => {
                    format!("{} ({})", c.get("rule").and_then(Value::as_str).unwrap_or(""), p)
                }
                _ => c
                    .get("rule")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            };
            let start = c
                .get("start")
                .and_then(Value::as_str)
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.timestamp())
                .unwrap_or(0);
            Conn {
                id: c
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                host,
                destination,
                network: meta
                    .get("network")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                protocol: meta
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                process: meta
                    .get("process")
                    .and_then(Value::as_str)
                    .filter(|p| !p.is_empty())
                    .or_else(|| meta.get("processPath").and_then(Value::as_str))
                    .unwrap_or("")
                    .to_string(),
                outbound,
                rule,
                upload: c.get("upload").and_then(Value::as_u64).unwrap_or(0),
                download: c.get("download").and_then(Value::as_u64).unwrap_or(0),
                start,
            }
        })
        .collect()
}

fn summarize(obj: &serde_json::Map<String, Value>, skip: &[&str]) -> String {
    let mut parts = Vec::new();
    for (key, value) in obj {
        if skip.contains(&key.as_str()) {
            continue;
        }
        let rendered = match value {
            Value::String(s) => s.clone(),
            Value::Array(a) => a
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" "),
            Value::Null => continue,
            other => other.to_string(),
        };
        parts.push(format!("{key}={rendered}"));
    }
    parts.join(", ")
}

pub fn route_info(profile_content: Option<&str>, clash_mode: Option<String>) -> RouteInfo {
    let mut info = RouteInfo {
        modes: vec!["rule".into(), "global".into(), "direct".into()],
        mode: clash_mode,
        ..RouteInfo::default()
    };
    let Some(raw) = profile_content else {
        return info;
    };
    let Ok(profile) = serde_json::from_str::<Value>(raw) else {
        return info;
    };
    if let Some(route) = profile.get("route") {
        if info.mode.is_none() {
            info.mode = profile
                .get("experimental")
                .and_then(|e| e.get("clash_api"))
                .and_then(|c| c.get("default_mode"))
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        info.final_outbound = route
            .get("final")
            .and_then(Value::as_str)
            .map(str::to_string);
        let skip = ["action", "outbound", "type"];
        if let Some(rules) = route.get("rules").and_then(Value::as_array) {
            for (index, rule) in rules.iter().enumerate() {
                let Some(obj) = rule.as_object() else { continue };
                let action = obj
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or("route")
                    .to_string();
                info.rules.push(RouteRule {
                    index,
                    action,
                    outbound: obj.get("outbound").and_then(Value::as_str).map(str::to_string),
                    match_: summarize(obj, &skip),
                });
            }
        }
    }
    if let Some(dns) = profile.get("dns") {
        info.dns_final = dns.get("final").and_then(Value::as_str).map(str::to_string);
        if let Some(rules) = dns.get("rules").and_then(Value::as_array) {
            for (index, rule) in rules.iter().enumerate() {
                let Some(obj) = rule.as_object() else { continue };
                let action = obj
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or("route")
                    .to_string();
                let outbound = obj
                    .get("server")
                    .and_then(Value::as_str)
                    .or_else(|| obj.get("outbound").and_then(Value::as_str))
                    .map(str::to_string);
                info.dns_rules.push(RouteRule {
                    index,
                    action,
                    outbound,
                    match_: summarize(obj, &["action", "server", "outbound"]),
                });
            }
        }
    }
    if info.mode.is_none() {
        info.mode = profile
            .get("experimental")
            .and_then(|e| e.get("clash_api"))
            .and_then(|c| c.get("default_mode"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some("rule".into()));
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_route_and_dns_rules() {
        let raw = json!({
            "route": {
                "final": "Proxy",
                "rules": [
                    { "action": "reject", "protocol": "dns" },
                    { "outbound": "Proxy", "domain_suffix": [".example.com"] }
                ]
            },
            "dns": {
                "final": "dns-remote",
                "rules": [
                    { "action": "route", "server": "dns-local", "domain_suffix": [".lan"] }
                ]
            }
        })
        .to_string();
        let info = route_info(Some(&raw), None);
        assert_eq!(info.final_outbound.as_deref(), Some("Proxy"));
        assert_eq!(info.rules.len(), 2);
        assert_eq!(info.rules[0].action, "reject");
        assert_eq!(info.rules[1].outbound.as_deref(), Some("Proxy"));
        assert!(info.rules[1].match_.contains("domain_suffix"));
        assert_eq!(info.dns_final.as_deref(), Some("dns-remote"));
        assert_eq!(info.dns_rules[0].outbound.as_deref(), Some("dns-local"));
    }

    #[test]
    fn maps_clash_connections() {
        let value = json!({
            "connections": [{
                "id": "abc",
                "metadata": {
                    "network": "tcp", "type": "HTTPS", "host": "example.com",
                    "destinationIP": "93.184.216.34", "destinationPort": "443",
                    "process": "curl"
                },
                "upload": 10, "download": 20,
                "start": "2026-01-01T00:00:00Z",
                "chains": ["Proxy", "node-a"],
                "rule": "Match"
            }]
        });
        let conns = map_connections(&value);
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].host, "example.com");
        assert_eq!(conns[0].destination, "93.184.216.34:443");
        assert_eq!(conns[0].outbound, "node-a");
        assert_eq!(conns[0].process, "curl");
        assert!(conns[0].start > 0);
    }
}
