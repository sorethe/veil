use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".").join(".config"))
        .join("veil")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from(".").join(".local/share"))
        .join("veil")
}

pub fn runtime_dir() -> PathBuf {
    data_dir().join("runtime")
}

pub fn exports_dir() -> PathBuf {
    data_dir().join("exports")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn db_path() -> PathBuf {
    data_dir().join("veil.db")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn runtime_file_path() -> PathBuf {
    runtime_dir().join("runtime.json")
}

pub fn runtime_config_path() -> PathBuf {
    runtime_dir().join("config.json")
}

pub fn sysproxy_state_path() -> PathBuf {
    runtime_dir().join("sysproxy.json")
}

pub fn helper_pid_path() -> PathBuf {
    runtime_dir().join("helper.pid")
}

/// Runtime helper files are written by veil-helper under /run/veil.
pub fn helper_config_path() -> PathBuf {
    PathBuf::from("/run/veil").join("config.json")
}

pub fn helper_log_path() -> PathBuf {
    PathBuf::from("/run/veil").join("sing-box.log")
}

pub fn helper_pid_file() -> PathBuf {
    PathBuf::from("/run/veil").join("sing-box.pid")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    for dir in [config_dir(), data_dir(), runtime_dir(), exports_dir(), logs_dir()] {
        fs::create_dir_all(&dir)?;
        // Private user data: owner-only.
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

pub fn expand_tilde(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(input)
}
