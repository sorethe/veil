//! veil-helper: the only privileged component of Veil.
//!
//! Invoked as root through pkexec with the dev.veil.helper polkit action.
//! Never invoked by the GUI as root; never runs a shell. Commands:
//!   check  [--core PATH]
//!   start  --config PATH --core PATH --pid-file PATH [--log-file PATH]
//!   stop   --pid-file PATH
//!   status --pid-file PATH
//!   install
//! Every command prints a single JSON object on stdout.

const EMBEDDED_POLICY: &str = include_str!("../../resources/dev.veil.helper.policy");
const HELPER_INSTALL_PATH: &str = "/usr/bin/veil-helper";
const POLICY_INSTALL_PATH: &str = "/usr/share/polkit-1/actions/dev.veil.helper.policy";
const RUNTIME_DIR: &str = "/run/veil";

use serde_json::{json, Value};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn fail(message: &str) -> ! {
    println!("{}", json!({ "ok": false, "error": message }));
    std::process::exit(1);
}

fn ok(value: Value) -> ! {
    println!("{value}");
    std::process::exit(0);
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn require_flag(args: &[String], name: &str) -> String {
    flag(args, name).unwrap_or_else(|| fail(&format!("missing required flag {name}")))
}

fn require_root() {
    if unsafe { libc::geteuid() } != 0 {
        fail("veil-helper must run as root (use pkexec /usr/bin/veil-helper ...)");
    }
}

fn command_exists(name: &str) -> bool {
    let Ok(paths) = env::var("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|dir| {
        let path = dir.join(name);
        fs::metadata(&path)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = env::var("PATH").ok()?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

fn find_core(explicit: Option<String>) -> Option<PathBuf> {
    if let Some(path) = explicit.filter(|p| !p.trim().is_empty()) {
        return Some(PathBuf::from(path));
    }
    if let Ok(path) = env::var("VEIL_SING_BOX") {
        if !path.trim().is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    if let Some(path) = find_in_path("sing-box") {
        return Some(path);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("sing-box");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    Some(PathBuf::from("/usr/lib/veil/sing-box")).filter(|p| p.is_file())
}

fn core_info(core: Option<&Path>) -> Value {
    match core {
        None => json!({
            "path": Value::Null,
            "version": Value::Null,
            "ok": false,
            "error": "sing-box binary not found"
        }),
        Some(path) => {
            let output = Command::new(path).arg("version").output();
            match output {
                Ok(out) if out.status.success() => {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let version = text
                        .lines()
                        .find_map(|line| line.trim().strip_prefix("sing-box version"))
                        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_string());
                    json!({
                        "path": path.to_string_lossy(),
                        "version": version,
                        "ok": true,
                        "error": Value::Null
                    })
                }
                Ok(out) => json!({
                    "path": path.to_string_lossy(),
                    "version": Value::Null,
                    "ok": false,
                    "error": String::from_utf8_lossy(&out.stderr).trim()
                }),
                Err(e) => json!({
                    "path": path.to_string_lossy(),
                    "version": Value::Null,
                    "ok": false,
                    "error": e.to_string()
                }),
            }
        }
    }
}

fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let rc = unsafe { libc::kill(pid as i32, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn cmdline(pid: u32) -> Option<Vec<String>> {
    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let args: Vec<String> = raw
        .split(|b| *b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

fn is_sing_box_process(pid: u32, config: Option<&str>) -> bool {
    let Some(args) = cmdline(pid) else {
        return false;
    };
    let binary = args
        .first()
        .map(|a| {
            Path::new(a)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| a.clone())
        })
        .unwrap_or_default();
    let looks_sing_box = binary.contains("sing-box") || args.iter().any(|a| a.contains("sing-box"));
    if !looks_sing_box {
        return false;
    }
    match config {
        Some(config) => args.iter().any(|arg| arg == config),
        None => true,
    }
}

#[derive(Clone, Copy)]
struct PidState {
    pid: u32,
}

fn read_pid_state(path: &Path) -> Option<(PidState, Option<String>)> {
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if let Some(pid) = value.get("pid").and_then(Value::as_u64) {
                let config = value
                    .get("config")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                return Some((PidState { pid: pid as u32 }, config));
            }
        }
    }
    let pid = trimmed.lines().next()?.trim().parse::<u32>().ok()?;
    Some((PidState { pid }, None))
}

fn write_pid_file(path: &Path, pid: u32, config: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o755));
    }
    let tmp = path.with_extension("pid.tmp");
    let _ = fs::remove_file(&tmp);
    let payload = json!({
        "pid": pid,
        "config": config.to_string_lossy(),
        "startedAt": chrono_like_epoch()
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&tmp)
        .map_err(|e| format!("write pid file: {e}"))?;
    writeln!(file, "{payload}").map_err(|e| e.to_string())?;
    file.sync_all().ok();
    fs::rename(&tmp, path).map_err(|e| format!("install pid file: {e}"))?;
    Ok(())
}

fn chrono_like_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn read_log_tail(path: &Path) -> String {
    let Ok(mut file) = fs::File::open(path) else {
        return String::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(3000);
    let _ = file.seek(SeekFrom::Start(start));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).trim().to_string()
}

fn install_atomic(
    target: &Path,
    mode: u32,
    writer: impl FnOnce(&mut fs::File) -> std::io::Result<()>,
) -> Result<(), String> {
    let dir = target
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", target.display()))?;
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let tmp = dir.join(format!(".veil-helper-install-{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&tmp)
            .map_err(|e| format!("create temp file {}: {e}", tmp.display()))?;
        writer(&mut file).map_err(|e| format!("write {}: {e}", tmp.display()))?;
        file.sync_all()
            .map_err(|e| format!("sync {}: {e}", tmp.display()))?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))
            .map_err(|e| format!("chmod {}: {e}", tmp.display()))?;
        fs::rename(&tmp, target).map_err(|e| format!("install {}: {e}", target.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn cmd_install() -> ! {
    require_root();
    let exe =
        env::current_exe().unwrap_or_else(|e| fail(&format!("resolve helper executable: {e}")));
    if let Err(e) = install_atomic(Path::new(HELPER_INSTALL_PATH), 0o755, |file| {
        let mut source = fs::File::open(&exe)?;
        std::io::copy(&mut source, file).map(|_| ())
    }) {
        fail(&e);
    }
    if let Err(e) = install_atomic(Path::new(POLICY_INSTALL_PATH), 0o644, |file| {
        file.write_all(EMBEDDED_POLICY.as_bytes())
    }) {
        fail(&e);
    }
    if let Err(e) = fs::create_dir_all(RUNTIME_DIR) {
        fail(&format!("create {RUNTIME_DIR}: {e}"));
    }
    if let Err(e) = fs::set_permissions(RUNTIME_DIR, fs::Permissions::from_mode(0o755)) {
        fail(&format!("chmod {RUNTIME_DIR}: {e}"));
    }
    ok(json!({
        "ok": true,
        "helper": HELPER_INSTALL_PATH,
        "policy": POLICY_INSTALL_PATH
    }))
}

fn cmd_check(args: &[String]) -> ! {
    let core = find_core(flag(args, "--core"));
    ok(json!({
        "ok": true,
        "uid": unsafe { libc::geteuid() },
        "root": unsafe { libc::geteuid() } == 0,
        "tunDevice": Path::new("/dev/net/tun").exists(),
        "nft": command_exists("nft"),
        "ip": command_exists("ip"),
        "core": core_info(core.as_deref())
    }))
}

fn cmd_start(args: &[String]) -> ! {
    require_root();
    let config = PathBuf::from(require_flag(args, "--config"));
    let core = PathBuf::from(require_flag(args, "--core"));
    let pid_file = PathBuf::from(require_flag(args, "--pid-file"));
    let log_file = flag(args, "--log-file")
        .map(PathBuf::from)
        .unwrap_or_else(|| pid_file.with_extension("log"));

    if !config.is_file() {
        fail(&format!("config {} does not exist", config.display()));
    }
    if !core.is_file() {
        fail(&format!("core {} does not exist", core.display()));
    }
    if !Path::new("/dev/net/tun").exists() {
        fail("/dev/net/tun is missing; TUN mode is unavailable");
    }
    if !command_exists("nft") {
        fail("nft is not installed; sing-box TUN routing needs nftables");
    }

    if let Some((state, stored_config)) = read_pid_state(&pid_file) {
        if alive(state.pid) {
            if stored_config
                .as_deref()
                .map(|c| is_sing_box_process(state.pid, Some(c)))
                .unwrap_or_else(|| is_sing_box_process(state.pid, None))
            {
                fail(&format!(
                    "already running: sing-box pid {} is alive (pid file {})",
                    state.pid,
                    pid_file.display()
                ));
            }
            fail(&format!(
                "pid file {} points at live pid {} which is not this sing-box instance; refusing to overwrite",
                pid_file.display(),
                state.pid
            ));
        }
        let _ = fs::remove_file(&pid_file);
    }

    // Real validation with the real binary, as root, before starting.
    let check = Command::new(&core)
        .arg("check")
        .arg("-c")
        .arg(&config)
        .arg("--disable-color")
        .output();
    match check {
        Ok(out) if out.status.success() => {}
        Ok(out) => fail(&format!(
            "sing-box check failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => fail(&format!("failed to run sing-box check: {e}")),
    }

    if let Some(parent) = log_file.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| fail(&format!("create log dir: {e}")));
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o755));
    }
    let log = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(&log_file)
        .unwrap_or_else(|e| fail(&format!("open log file: {e}")));
    let log_err = log
        .try_clone()
        .unwrap_or_else(|e| fail(&format!("clone log file: {e}")));

    let mut command = Command::new(&core);
    command
        .arg("run")
        .arg("-c")
        .arg(&config)
        .arg("--disable-color")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    // Detach from the pkexec session so the engine survives the helper exiting.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => fail(&format!("spawn sing-box: {e}")),
    };
    let pid = child.id();
    sleep(Duration::from_millis(450));
    match child.try_wait() {
        Ok(Some(status)) => {
            let tail = read_log_tail(&log_file);
            let _ = fs::remove_file(&pid_file);
            fail(&format!(
                "sing-box exited during startup ({status}): {}",
                tail
            ));
        }
        Ok(None) => {}
        Err(e) => fail(&format!("check sing-box startup: {e}")),
    }
    if let Err(e) = write_pid_file(&pid_file, pid, &config) {
        let _ = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        fail(&format!("write pid file: {e}"));
    }
    ok(json!({
        "ok": true,
        "pid": pid,
        "config": config.to_string_lossy(),
        "logFile": log_file.to_string_lossy(),
        "pidFile": pid_file.to_string_lossy()
    }))
}

fn cmd_stop(args: &[String]) -> ! {
    require_root();
    let pid_file = PathBuf::from(require_flag(args, "--pid-file"));
    let Some((state, stored_config)) = read_pid_state(&pid_file) else {
        ok(json!({ "ok": true, "stopped": false, "detail": "no pid file" }));
    };
    if !alive(state.pid) {
        let _ = fs::remove_file(&pid_file);
        ok(json!({ "ok": true, "stopped": false, "detail": "process not running" }));
    }
    if !is_sing_box_process(state.pid, stored_config.as_deref()) {
        fail(&format!(
            "refusing to stop pid {}: it is not the sing-box process recorded in {}",
            state.pid,
            pid_file.display()
        ));
    }
    unsafe {
        libc::kill(state.pid as i32, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while alive(state.pid) && Instant::now() < deadline {
        sleep(Duration::from_millis(100));
    }
    let mut forced = false;
    if alive(state.pid) {
        unsafe {
            libc::kill(state.pid as i32, libc::SIGKILL);
        }
        forced = true;
        let kill_deadline = Instant::now() + Duration::from_secs(2);
        while alive(state.pid) && Instant::now() < kill_deadline {
            sleep(Duration::from_millis(100));
        }
    }
    if alive(state.pid) {
        fail(&format!("pid {} is still alive after SIGKILL", state.pid));
    }
    let _ = fs::remove_file(&pid_file);
    ok(json!({
        "ok": true,
        "stopped": true,
        "pid": state.pid,
        "forced": forced
    }))
}

fn cmd_status(args: &[String]) -> ! {
    let pid_file = PathBuf::from(require_flag(args, "--pid-file"));
    let Some((state, stored_config)) = read_pid_state(&pid_file) else {
        ok(json!({ "ok": true, "running": false, "pid": Value::Null }));
    };
    let running = alive(state.pid) && is_sing_box_process(state.pid, stored_config.as_deref());
    ok(json!({
        "ok": true,
        "running": running,
        "pid": if running { json!(state.pid) } else { Value::Null },
        "config": stored_config
    }))
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first() else {
        fail("usage: veil-helper <check|start|stop|status|install> [options]");
    };
    match command.as_str() {
        "check" => cmd_check(&args),
        "start" => cmd_start(&args),
        "stop" => cmd_stop(&args),
        "status" => cmd_status(&args),
        "install" => cmd_install(),
        other => fail(&format!("unknown command '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_file_json_roundtrip() {
        let dir = env::temp_dir().join(format!("veil-helper-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.pid");
        write_pid_file(&path, 4242, Path::new("/tmp/config.json")).unwrap();
        let (state, config) = read_pid_state(&path).unwrap();
        assert_eq!(state.pid, 4242);
        assert_eq!(config.as_deref(), Some("/tmp/config.json"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_pid_file_is_accepted() {
        let dir = env::temp_dir().join(format!("veil-helper-plain-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.pid");
        fs::write(&path, "1234\n").unwrap();
        let (state, config) = read_pid_state(&path).unwrap();
        assert_eq!(state.pid, 1234);
        assert!(config.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn embedded_policy_declares_veil_action() {
        assert!(EMBEDDED_POLICY.contains("<action id=\"dev.veil.helper\">"));
        assert!(EMBEDDED_POLICY.contains("/usr/bin/veil-helper"));
    }

    #[test]
    fn current_process_is_not_sing_box() {
        let pid = std::process::id();
        assert!(!is_sing_box_process(pid, None));
    }
}
