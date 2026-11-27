# Changelog

All notable changes to Veil are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Veil is pre-release. `0.1.0` describes the current repository state and has not
been tagged or published as a binary.

## [Unreleased]

### Added

- First-use TUN setup: Overview, Settings and connect now probe the privileged
  helper, `dev.veil.helper` polkit policy, `pkexec`, `/dev/net/tun` and nftables.
  A setup dialog shows each prerequisite and installs the embedded helper and
  policy through `pkexec` with admin approval; the GUI itself never runs as
  root. TUN connect stays gated until the real setup is ready.
- Detect other active TUN interfaces (`ARPHRD_NONE`, not `down`, excluding
  `veil0`) and refuse TUN before starting: two TUN tunnels cannot share
  interface routes or DNS. The setup dialog names the conflicting interfaces
  and offers retry/Diagnostics instead of installation; System proxy remains
  available.
- Configurable `tunAddress` / `tunAddress6` settings (IPv4 and IPv6 CIDRs,
  defaults `172.19.0.1/30` and `fdfe:dcba:9876::1/126`) feed TUN config
  generation and are validated as real CIDRs before they are saved.
- Read-only TUN overlap preflight: `get_tun_setup` and `connect` parse
  `ip route`/`ip -j addr` output, compare existing route prefixes and assigned
  addresses with the proposed TUN CIDRs, and refuse TUN with the conflicting
  routes/interfaces named. No route, address, DNS, or firewall state is
  modified, and the setup dialog states that TUN remains blocked and Throne
  must stay untouched.

### Changed

- Nothing yet.

### Fixed

- Nothing yet.

## [0.1.0] - 2026-11-27

Initial repository state. No git tag exists for this entry.

### Added

- Tauri 2 + Svelte 5 desktop app with a Rust backend, TypeScript frontend and
  Vite build; release binaries embed the frontend via Tauri's
  `custom-protocol` feature.
- sing-box **1.14.5** core discovery: `settings.corePath`, `$VEIL_SING_BOX`,
  `PATH`, beside the executable, Tauri resources, `/usr/lib/veil/sing-box`, and
  development vendor builds.
- Engine lifecycle with real `sing-box check`, process spawn, Clash API
  `/version` health check, live traffic/connections/proxies/logs, graceful
  stop, and adoption of a running engine after a GUI restart.
- SQLite profile store with create, edit, duplicate, delete, raw content
  preservation, selections, backup export and import.
- Imports for sing-box JSON, subscriptions (JSON, base64 JSON, base64
  share-link lists), share links (`vless`, `vmess`, `trojan`, `ss`,
  `hysteria2`, `tuic`, `socks`, `http`/`https`), and QR images via `zbarimg`.
  Subscription refresh with `last_error` reporting.
- Typed `CommandError { code, message, detail }` envelope for every command and
  typed `EngineInfo`/`EngineCapabilities` derived from the real binary and host
  environment.
- System-proxy apply/restore for GNOME (`gsettings`), KDE (`kwriteconfig`) and
  XFCE (`xfconf-query`), with exact previous values snapshotted to a `0600`
  state file and restored on disconnect, exit, and crash recovery. Unsupported
  desktops report `unsupported` without modifying anything.
- Privileged `veil-helper` (separate crate) invoked through `pkexec` and the
  `dev.veil.helper` polkit action. It validates configs as root, detaches with
  `setsid`, verifies `/proc/<pid>/cmdline` before stopping, and never runs a
  shell.
- Diagnostics for core, helper/polkit, TUN device, nftables, interfaces, routes,
  DNS, tray and recent errors, all derived from obtainable host data.
- Desktop integration: autostart entry, best-effort notifications, and a
  StatusNotifierItem tray implemented with `ksni`.
- Log ring buffer with redaction (URL userinfo, password/token/secret/key
  fields, UUIDs, subscription query strings, the Clash API secret) plus log
  export.
- Packaging assets: MIT license, Arch `PKGBUILD` fetching the pinned official
  sing-box 1.14.5 release, `.SRCINFO`, source-tarball script, user-local
  install/uninstall scripts, polkit policy and desktop entry.
- Real-engine harnesses: `engine_smoke`, `engine_lifecycle` and
  `subscription_integration`, plus gated system-proxy tests.
- README, architecture, contract, verification and packaging documentation.
- Quality and release-build CI workflows.

### Changed

- Overview layout reworked to a 25/75 split with a shared outbound browser and
  outbound store; sidebar, profiles, settings and styles updated.
- Added a persisted `developerMode` setting (default `false`) and its contract
  field.
- TUN diagnostics require a usable helper and installed polkit action rather
  than merely finding a development helper binary.
- Packaged core source changed to the official pinned sing-box release;
  `src-tauri/resources/sing-box` is no longer tracked.
- Removed unused runtime dependencies (`librsvg`, `libayatana-appindicator`);
  QR support (`zbar`) is an optional dependency.

### Fixed

- `SIGTERM`/`SIGINT` now route through the same graceful disconnect path as
  window close, tray quit and the `quit` command; shutdown is idempotent.
- Stale runtime files are cleaned up when disconnect finds the engine already
  idle.
- Selector/URLTest outbound groups are no longer mislabeled as fixed nodes in
  the Outbounds view.

### Security

- The GUI never runs as root. Only `veil-helper` is elevated, through a polkit
  action with `auth_admin_keep` for active sessions; there is no passwordless
  sudoers rule.
- Runtime configs, settings, proxy-restore state and runtime metadata are
  written with owner-only permissions.

### Known limitations

- TUN end-to-end is **unverified**. The helper and generated TUN configs are
  validated, but a concurrent TUN owned the required route during verification;
  no TUN success is claimed.
- Hyprland and sway have no supported system-proxy API; Veil reports
  `unsupported`.
- KDE and XFCE proxy backends are implemented but unverified on the
  verification host.
- AppImage, deb and rpm bundles are not built. The Arch package is
  syntax-checked but not built or installed in the verified matrix.
- Clash YAML profiles are rejected rather than converted.
- Wayland global shortcuts are unavailable.
- Notifications and the tray are best-effort; the tray requires a
  StatusNotifier host.
