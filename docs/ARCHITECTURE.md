# Veil architecture

## Crates and modules

```
src-tauri/
  Cargo.toml            veil (lib veil_lib + bin veil)
  src/main.rs           thin Tauri entrypoint
  src/lib.rs            app state, commands, setup, exit handling
  src/settings.rs       ~/.config/veil/settings.json (0600, atomic writes)
  src/paths.rs          config/data/runtime/exports paths, 0700 dirs
  src/db.rs             rusqlite (bundled) profiles, selections, meta, backups
  src/core.rs           core discovery, `sing-box version`, `sing-box check`
  src/configgen.rs      runtime config injection, 0600 atomic write
  src/engine.rs         lifecycle, process supervision, stats, adoption
  src/clash.rs          Clash API client and contract mapping
  src/sysproxy.rs       GNOME/KDE/XFCE apply + exact restore, platform detect
  src/links.rs          share-link parsers -> sing-box config
  src/diagnostics.rs    read-only environment probing
  src/logs.rs           2000-entry ring buffer, redaction, events, export
  src/tray.rs           ksni StatusNotifierItem tray
  helper/               separate veil-helper crate (root only)
  examples/engine_smoke.rs  real sing-box integration harness
```

The frontend only talks to the backend through the commands/events documented in
`docs/CONTRACT.md`. No UI logic is duplicated in Rust; no engine state is invented
in Rust.

## Process and privilege separation

```
Svelte UI  --invoke/events-->  veil (user, no root)
                                 |  spawn `sing-box run` (system mode, as user)
                                 |  system proxy via gsettings/kwriteconfig/xfconf
                                 |
                                 +--pkexec--> /usr/bin/veil-helper (root)
                                                | sing-box check (as root)
                                                | setsid + `sing-box run` (TUN)
                                                | writes /run/veil/{pid,log}
```

- System mode needs no privileges: the mixed inbound listens on
  `127.0.0.1:<mixedPort>` and the Clash API on `127.0.0.1:<apiPort>`.
- TUN mode needs root for `/dev/net/tun` and nftables. Only `veil-helper` is
  elevated, via the `dev.veil.helper` polkit action. The GUI refuses to start TUN
  when the helper, policy, pkexec, `/dev/net/tun` or nft are missing, with a
  specific error, and never runs the GUI as root.
- `veil-helper` uses direct argv, never a shell. It validates configs with
  `sing-box check` before spawning, detaches with `setsid`, verifies
  `/proc/<pid>/cmdline` before stopping, uses SIGTERM → SIGKILL after 8s, and
  only touches the pid/log files it was given.
- The polkit policy grants `auth_admin_keep` to active sessions; there is no
  passwordless sudoers entry.

## Engine lifecycle

State machine (exposed as `EngineState.status`):

```
idle --connect--> starting --health ok--> connected
                    |                        |
                    | health fail            | unexpected exit
                    v                        v
                  error <------------------ error
connected --disconnect--> stopping --> idle
```

Connect steps (`engine::connect`):

1. Serialize connect/disconnect with an async operation lock.
2. Resolve mode (`argument` > `settings.mode`) and profile
   (`profileId` > last used > first profile). Require a non-empty outbounds list.
3. Find the core (settings → env → PATH → app dir → resources →
   `/usr/lib/veil/sing-box` → vendor), read its real version.
4. Generate the runtime config from the stored raw content, write it 0600, and
   run the real `sing-box check` on the *generated* file.
5. Emit `starting`, spawn the engine (directly or via pkexec helper). In TUN mode
   the helper returns JSON with the detached pid.
6. Wait up to 12s for Clash API `/version`; if the child dies, fail immediately
   using the captured stderr tail. On failure kill the process, clear runtime
   state, set `error` with the last output, and emit a notice.
7. Re-apply per-profile outbound selections to the selector groups.
8. Apply the system proxy in system mode (never in TUN mode), then persist
   `runtime/runtime.json` (0600) and emit `connected`.
9. Start live tasks: `/traffic` NDJSON stream (speeds), `/connections` poll every
   2s (count + session totals), `/proxies` poll every 5s (latency history),
   1s stats emitter, Clash `/logs` stream, process monitor.

Disconnect: set `stopping`, stop the helper or SIGTERM the child (SIGKILL after
8s), restore the desktop proxy, abort tasks, remove runtime files, set `idle`
(or `error` with the still-running pid if stopping failed).

Unexpected exit: restore the system proxy, remove runtime state, set `error`
with the stderr tail, emit `engine://notice`, send a desktop notification.

## Runtime config generation

The stored profile is parsed as JSON and only these integration points are
touched:

- `experimental.clash_api.external_controller = 127.0.0.1:<apiPort>` and a fresh
  32-char secret; other Clash API keys (`external_ui`, `cors`, `default_mode`…)
  are preserved.
- `log.level` is set from settings; other log keys are preserved.
- An `inbounds` entry tagged `veil-mixed` is added (mixed HTTP+SOCKS, 127.0.0.1).
  A pre-existing mixed inbound on the same port is replaced to avoid a bind
  conflict; unrelated inbounds are kept.
- In TUN mode a `veil-tun` inbound is added: interface `veil0`, addresses
  `172.19.0.1/30` + `fdfe:dcba:9876::1/126`, MTU 9000, `auto_route`,
  `strict_route`, `dns_mode: hijack`, LAN route exclusions, and
  `auto_redirect` only when the setting is enabled. The mixed inbound stays so
  local proxies remain usable.
- Every unknown field is preserved byte-for-byte at the JSON value level. Raw
  profile content in SQLite is never modified by generated runtime config.

## Runtime persistence and adoption

`runtime/runtime.json` contains `{pid, mode, profileId, startedAt, apiPort,
secret, configPath, pidFile, logFile}` (0600). On startup the backend:

1. Reads the file. Verifies the pid is alive and `/proc/<pid>/cmdline` contains
   `sing-box` and the exact config path.
2. Verifies the Clash API answers with the stored secret.
3. If valid, emits `connected`, reattaches stats/monitor/log-tail tasks (log
   files are tailed from the end to avoid replay). This is how a restarted GUI
   adopts a still-running engine.
4. If invalid, removes the runtime file, stops only processes proven to be our
   sing-box, restores any leftover system proxy, and stays idle.
5. If no runtime file exists but a proxy restore state file does (crash), it
   restores the previous desktop proxy values.

## System proxy restore

Before modifying anything, the backend snapshots the exact previous values
(`gsettings get` raw output, KDE ini values, or xfconf properties) into
`runtime/sysproxy.json` (0600). On any failure during apply it restores and
removes the state file. Restore runs on disconnect, unexpected exit, app exit,
and at the next startup if the state file survives a crash. Unsupported desktops
are never modified; the returned `ProxyInfo` explains the manual env fallback.

## Stats and logs

Stats come only from sing-box: `/traffic` (bytes/sec), `/connections`
(`uploadTotal`, `downloadTotal`, active count) and `/proxies` history (last
delay). No polling runs while idle. The 1s emitter emits `engine://stats`.

Logs are a 2000-entry ring with monotonic sequence numbers. Engine stdout/stderr
is read line-by-line; TUN mode tails the helper log file (0644) written by the
root sing-box process; Clash `/logs` is a second source (`source: "clash"`).
Messages are redacted for URL userinfo, password/token/secret/key fields,
UUIDs, and subscription query strings before entering the ring, file or event.
The API secret is additionally registered for exact replacement.

## Failure handling

- Child death during startup or while connected is detected by a monitor poll
  (1s) using `try_wait` (system mode) or `kill(pid, 0)` (helper mode/adopted).
- Missing core, invalid config, helper/auth failure, missing TUN device, nft
  absence, and Clash API timeouts produce specific errors, never a fake
  `connected`.
- Clash API operations surface HTTP status and body on failure (`PATCH /configs`
  errors included).
- The tray and notifications are best-effort; failures do not change engine
  state.
