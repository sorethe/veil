# Veil features and limits

This page states what is actually implemented and what has actually been
exercised. It does not describe a roadmap or advertise planned work as done.

## Status vocabulary

The repository uses these terms; they are not interchangeable.

| Term | Meaning |
| --- | --- |
| **Verified / integration-tested** | Exercised against the real `sing-box` binary, real desktop tooling, or the running release app, and recorded in [VERIFICATION.md](VERIFICATION.md) or a test harness. |
| **Tested** | Covered by the automated test suite. May or may not use the real binary; real-binary paths are gated by `VEIL_SINGBOX`. |
| **Implemented** | Code exists and is wired into the app, and the project builds. |
| **Implemented-unverified** | Code exists and builds, but there is no end-to-end proof in this repository. |
| **Best-effort** | Failure is tolerated by design (tray, notifications, autostart) and must not be reported as guaranteed. |
| **Unsupported** | Veil deliberately refuses or reports the limitation instead of guessing. |
| **Absent / not built** | Not present, not produced, or not exercised. Do not claim it from a green build. |

## Core behavior

### Real engine, no reimplementation — verified

Veil does not implement a proxy protocol or engine. Profiles are sing-box JSON.
Veil generates a runtime config, validates it with the real `sing-box check`,
spawns `sing-box run`, and reads live state from the Clash API. Every speed,
delay, route and connection value shown in the UI comes from that process.

- Supported profile content: sing-box JSON.
- Runtime identity: real `sing-box version`; sing-box 1.14.5 is the validated
  core.
- No invented state: `connected` requires a successful Clash API `/version`
  health check.
- Raw profile content in SQLite is never rewritten by config generation.

### Engine lifecycle — verified

`idle → starting → connected → stopping → idle`, with `error` on failure.
Recorded through `engine_smoke` and `engine_lifecycle` against the real binary.

- Real config check before spawn.
- 12 s Clash API health deadline; a dying child fails immediately with its
  stderr tail.
- Live `/traffic`, `/connections` and `/proxies` polling; 1 s stats emitter.
- Graceful `SIGTERM`, then `SIGKILL` after 8 s.
- Adopts a still-running engine after a GUI restart by verifying the pid,
  `/proc/<pid>/cmdline`, the exact config path and the Clash API secret.
- Unexpected exit restores the desktop proxy, clears runtime files and reports
  `error`.
- TUN helper start/stop calls time out after 120 s and map to a permission
  error.

### Profiles — tested

- SQLite store with create, edit, duplicate and delete.
- Raw sing-box JSON stored verbatim.
- Per-profile outbound selections.
- Backup export and import.
- Subscription metadata and `last_error`; automatic refresh on the configured
  interval (24 h default, maximum 30 days).

### Imports — integration-tested

- sing-box JSON.
- Subscriptions returning JSON, base64 JSON, a base64 share-link list, or a
  plain share-link list.
- Share links: `vless`, `vmess`, `trojan`, `ss`, `hysteria2`, `tuic`, `socks`,
  `http`/`https`.
- QR images through the external `zbarimg` tool.
- Clash YAML is **rejected** with an explicit error. There is no converter.

A real subscription was imported and exercised: 31 outbounds, real
`sing-box check`, HTTP and SOCKS5h requests, traffic, delay, refresh, and log
redaction.

### Validation — integration-tested

Every profile and every generated runtime config is checked with the actual
`sing-box check`. Malformed JSON, missing or empty outbounds, and Clash YAML
produce specific validation errors. The UI can validate content before saving.

## Connectivity modes

### System mode — verified

The mixed HTTP+SOCKS inbound listens on `127.0.0.1:<mixedPort>`, and the Clash
API listens on `127.0.0.1:<apiPort>`. No privileges are needed. A live manual
session was exercised with real HTTP and SOCKS5h requests through the mixed
port.

### System proxy — GNOME verified, KDE/XFCE implemented-unverified

| Desktop | Backend | Status |
| --- | --- | --- |
| GNOME family | `gsettings` (`org.gnome.system.proxy`) | Verified with real gsettings: apply, exact restore, no leftover state file |
| KDE | `kwriteconfig6`/`kwriteconfig5` | Implemented-unverified on this host |
| XFCE | `xfconf-query` | Implemented-unverified; the tool was absent on the verification host |
| Hyprland / sway / other | none | **Unsupported**: reported as `unsupported`, nothing written |

Before modifying anything, Veil snapshots the exact previous values into
`runtime/sysproxy.json` (mode `0600`). Restore runs on disconnect, unexpected
exit, app exit, and at the next startup if the state file survived a crash. If
apply fails partway, Veil restores and removes the state file.

On desktops without a desktop-wide proxy API, the UI shows `LIVE · MANUAL` with
the live mixed port and HTTP/SOCKS environment-variable instructions. This is
presentation only and does not claim automatic coverage for other apps.

### TUN mode — verified end-to-end on the verification host

Verified on 2026-11-27 with a real `pkexec` approval and the
`dev.veil.helper` polkit action:

- `veil0` up with the configured IPv4/IPv6 addresses.
- DNS, TCP and UDP through the tunnel; real HTTP and SOCKS5h requests.
- Generated route sets `auto_detect_interface: true` so outbound dials bind to
  the physical interface.
- Generated TUN inbound uses `stack: "gvisor"`.
- Disconnect removes `veil0`, policy rules and table entries, closes listeners,
  and leaves the runtime directory empty.

Requirements and boundaries:

- `/dev/net/tun`, nftables (`nft`), `ip`, `pkexec`, an installed
  `/usr/bin/veil-helper`, and the `dev.veil.helper` polkit action.
- The GUI never runs as root; only the helper is elevated.
- Veil refuses TUN before starting when another active TUN/VPN interface
  (`ARPHRD_NONE`, not down, excluding `veil0` and `lo`) or an overlapping
  route/address exists. The preflight is read-only and names the conflict.
- `auto_redirect` in the generated config is test-covered, but actual nftables
  redirect behavior has **not** been end-to-end verified.
- Verification was host-specific. Other distributions, kernels, network
  managers and hardware remain untested.

### First-use TUN setup dialog — implemented-unverified

`get_tun_setup` probes the helper, polkit policy, `pkexec`, `/dev/net/tun`,
nftables, configured addresses and conflicts. `install_tun_helper` installs the
embedded helper and policy through `pkexec`. The recorded TUN PASS proves the
`pkexec`-approved start/stop path, not that this GUI install button was
exercised.

## Observability and desktop integration

### Diagnostics — implemented

The Diagnostics view reports only obtainable data: core path/version,
helper/polkit/permission state, `/dev/net/tun`, nftables, interfaces, routes,
DNS, tray availability and recent errors. Capabilities are derived from the
binary, build tags and host tools; an absent tool shows as unavailable rather
than "working".

### Logs — implemented, redaction tested

- 2000-entry in-memory ring with monotonic sequence numbers.
- File log at `~/.local/share/veil/logs/veil.log` (mode `0600`), rotated to
  `veil.log.1` after 8 MiB when reopened.
- Sources: engine stdout/stderr, the TUN helper log, and Clash `/logs`.
- Redaction: URL userinfo, password/token/secret/key fields, UUIDs,
  subscription query strings, and registered secrets (the Clash API secret and
  subscription URLs).
- Export to `~/.local/share/veil/exports/` (directory `0700`).

Redaction reduces leaks; it does not make the database, exports or settings
safe to share. See [security.md](security.md).

### Tray, notifications, autostart — implemented, best-effort

- Tray: `ksni` StatusNotifierItem over D-Bus. It requires a StatusNotifier
  host; without one, the tray is absent while the window behaves normally.
  Full tray interaction is not part of the verified matrix.
- Notifications: best-effort; failures do not change engine state.
- Autostart: writes a desktop autostart entry; covered by the settings/UI path
  but not part of the verified interaction matrix.
- Global shortcuts: not registered. Wayland does not provide them; nothing is
  faked.

### Single-instance guard — implemented, partially verified

An exclusive non-blocking `flock` on `<data_dir>/veil.lock` makes a second GUI
exit before opening SQLite or runtime state. The lock is held for the process
lifetime and released on exit. The lock behavior is unit-tested; two-launch and
adopted-engine behavior are implemented-unverified end to end.

### Typed contract — implemented and documented

Every command rejects with `CommandError { code, message, detail }` using the
codes `not-ready`, `validation`, `permission`, `engine`, `network`, `io`,
`unsupported`, `unknown`. Live engine truth is the `engine://state` event; the
other events are `engine://stats`, `engine://log`, `engine://notice` and
`profiles://changed`. See [CONTRACT.md](CONTRACT.md).

## Not implemented, not advertised

These are deliberately absent. Do not describe them as features:

- No proxy engine, protocol implementation, rule engine or traffic attribution
  of its own.
- No Clash YAML conversion.
- No desktop-wide proxy on Hyprland or sway.
- No AppImage, deb, rpm or verified Arch package.
- No AppImage bundling of the core or helper.
- No global shortcuts, especially on Wayland.
- No background service or root GUI.
- No built-in subscription sharing, account system or telemetry.
- No promise that stats distinguish "zero traffic" from "stale poll": stats
  carry no freshness timestamp.
- No deduplication of log events across engine, helper and Clash sources.

## Platform summary

| Platform | Status |
| --- | --- |
| Linux x86_64 | Target |
| Arch Linux + Hyprland (Wayland) | Verification host |
| GNOME proxy | Verified |
| KDE / XFCE proxy | Implemented-unverified |
| X11 | Untested in this matrix |
| Other distributions/architectures | Best-effort, untested |
| macOS / Windows | Not supported |

For build steps see [building.md](building.md); for failure modes see
[troubleshooting.md](troubleshooting.md).