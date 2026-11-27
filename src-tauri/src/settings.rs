use crate::paths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Mutex;

fn default_theme() -> String {
    "dark".into()
}
fn default_mode() -> String {
    "system".into()
}
fn default_mixed_port() -> u16 {
    2080
}
fn default_api_port() -> u16 {
    9090
}
fn default_log_level() -> String {
    "info".into()
}
fn default_sub_interval() -> u32 {
    24
}
fn default_language() -> String {
    "en".into()
}
fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    pub mode: String,
    pub mixed_port: u16,
    pub api_port: u16,
    pub core_path: Option<String>,
    pub auto_redirect: bool,
    pub autostart: bool,
    pub connect_on_launch: bool,
    pub notifications: bool,
    pub minimize_to_tray: bool,
    pub log_level: String,
    pub subscription_interval_hours: u32,
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            mode: default_mode(),
            mixed_port: default_mixed_port(),
            api_port: default_api_port(),
            core_path: None,
            auto_redirect: false,
            autostart: false,
            connect_on_launch: false,
            notifications: default_true(),
            minimize_to_tray: false,
            log_level: default_log_level(),
            subscription_interval_hours: default_sub_interval(),
            language: default_language(),
        }
    }
}

impl Settings {
    fn validate(&mut self) -> Result<(), String> {
        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            return Err("theme must be dark, light or system".into());
        }
        if !matches!(self.mode.as_str(), "system" | "tun") {
            return Err("mode must be system or tun".into());
        }
        if self.mixed_port == 0 || self.api_port == 0 {
            return Err("ports must be between 1 and 65535".into());
        }
        if self.mixed_port == self.api_port {
            return Err("mixedPort and apiPort must differ".into());
        }
        if !matches!(
            self.log_level.as_str(),
            "trace" | "debug" | "info" | "warn" | "error" | "fatal" | "panic"
        ) {
            return Err("logLevel must be one of trace, debug, info, warn, error".into());
        }
        if self.subscription_interval_hours > 24 * 30 {
            self.subscription_interval_hours = 24 * 30;
        }
        Ok(())
    }
}

pub struct SettingsStore {
    path: PathBuf,
    inner: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load() -> Self {
        let path = paths::settings_path();
        let settings = match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Settings>(&raw) {
                Ok(mut s) => {
                    let _ = s.validate();
                    s
                }
                Err(_) => Settings::default(),
            },
            Err(_) => Settings::default(),
        };
        Self {
            path,
            inner: Mutex::new(settings),
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn get(&self) -> Settings {
        self.inner.lock().unwrap().clone()
    }

    /// Apply a partial JSON object patch, validate, then persist atomically.
    pub fn update(&self, patch: &Value) -> Result<Settings, String> {
        let obj = patch
            .as_object()
            .ok_or_else(|| "settings patch must be a JSON object".to_string())?;
        let mut guard = self.inner.lock().unwrap();
        let mut current = serde_json::to_value(&*guard).map_err(|e| e.to_string())?;
        let cur_obj = current
            .as_object_mut()
            .ok_or_else(|| "settings serialization failed".to_string())?;
        for (key, value) in obj {
            if cur_obj.contains_key(key) {
                cur_obj.insert(key.clone(), value.clone());
            }
        }
        let mut next: Settings =
            serde_json::from_value(current).map_err(|e| format!("invalid settings: {e}"))?;
        next.validate()?;
        self.persist(&next)?;
        *guard = next.clone();
        Ok(next)
    }

    pub fn save(&self, next: Settings) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap();
        let mut next = next;
        next.validate()?;
        self.persist(&next)?;
        *guard = next;
        Ok(())
    }

    fn persist(&self, settings: &Settings) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
        }
        let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("write settings: {e}"))?;
        file.write_all(raw.as_bytes())
            .map_err(|e| format!("write settings: {e}"))?;
        file.sync_all().ok();
        fs::rename(&tmp, &self.path).map_err(|e| format!("save settings: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_patch_preserves_other_fields() {
        let dir = std::env::temp_dir().join(format!("veil-settings-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore {
            path: dir.join("settings.json"),
            inner: Mutex::new(Settings::default()),
        };
        let updated = store
            .update(&serde_json::json!({"mixedPort": 2100, "theme": "light"}))
            .unwrap();
        assert_eq!(updated.mixed_port, 2100);
        assert_eq!(updated.theme, "light");
        assert_eq!(updated.api_port, 9090);
        let raw = fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(raw.contains("\"mixedPort\": 2100"));
        assert!(store.update(&serde_json::json!({"mixedPort": 0})).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
