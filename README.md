# Veil

A Linux desktop proxy client for Tauri 2 + Svelte 5, powered by the official
sing-box 1.14.5 binary. Veil does not implement a proxy engine of its own: every
connection, statistic and route shown in the UI comes from a real sing-box
process through its Clash API.

- UI: `src/` (Svelte 5, Vite, TypeScript) — owned by the frontend.
- Backend: `src-tauri/` (Rust) — profiles, engine lifecycle, Clash API, system
  proxy, diagnostics, tray, logging.
- Privileged helper: `src-tauri/helper/` (`veil-helper`) — the only component
  that ever runs as root. The GUI never runs as root.

## Status / honest scope

Implemented and exercised in this environment:

- Real sing-box lifecycle in **system mode**: config generation, `sing-box
  check`, spawn, Clash API `/version` health check, stats, logs, graceful
  SIGTERM/SIGKILL, runtime adoption after a GUI restart.
- Profile CRUD in SQLite, real validation through `sing-box check`, JSON and
  subscription import (JSON / base64 / share-link lists), share-link parsers
  (vless, vmess, trojan, ss, hysteria2, tuic, socks, http/https), refresh with
  `last_error`, backup export/import.
- Clash API: `/version`, `/traffic` (live speed NDJSON), `/connections`,
  `/proxies`, `/proxies/{name}/delay`, `PUT /proxies/{name}`,
  `PATCH /configs` mode, `/logs` stream, connection close.
- System proxy apply/restore for GNOME (`gsettings`), KDE
  (`kwriteconfig6`), XFCE (`xfconf-query`) with exact previous values saved to a
  state file so a crashed session is restored on the next start. On
  Hyprland/sway/unknown desktops the backend reports `unsupported` and does not
  modify anything.
- Desktop integration: autostart `.desktop`, notifications (notify-rust), tray
  via `ksni` (pure Rust, StatusNotifierItem) when a watcher host exists,
  minimize-to-tray only with a working tray.
- Diagnostics built only from obtainable data: core, helper, polkit, TUN device,
  nft, root, `ip -j addr`, default routes, `ip link show veil0`, profile DNS +
  `resolvectl`/`resolv.conf`, recent errors, tray host detection.

Not verified end-to-end here (documented, not faked):

- **TUN end-to-end.** `/dev/net/tun` exists, nft 1.1.7 exists, the helper builds,
  `veil-helper check` runs, and the generated TUN config passes the real
  `sing-box check`. Actually starting TUN requires root/polkit authentication,
  which this build environment cannot supply non-interactively. The app reports
  missing helper/policy/device honestly and surfaces the helper's real error.
- GNOME/KDE/XFCE system proxy was implemented against their documented CLIs;
  the test host is Hyprland, where Veil deliberately reports `unsupported`.
  `gsettings` and `kwriteconfig6` are present and their command paths are
  exercised, but Hyprland does not apply them globally, so Veil does not claim
  system-proxy coverage there.
- XFCE requires `xfconf-query`, which is absent on this host; Veil reports
  `unsupported` rather than pretending.
- Global shortcuts are not available on Wayland. Veil does not register or fake
  them.

## Requirements (Arch Linux)

```bash
sudo pacman -S --needed \
  rust nodejs pnpm \
  webkit2gtk-4.1 gtk3 libsoup3 \
  nftables iproute2 polkit \
  zbar \
  sing-box   # optional: a bundled core is used otherwise
```

`libayatana-appindicator` is **not** required: Tauri's tray feature is disabled
and the tray uses `ksni` over D-Bus StatusNotifierItem.

Rust is expected via rustup (`~/.cargo/bin` on `PATH`). Node 26 + pnpm 12 were
used here.

## Build

```bash
pnpm install            # frontend deps (lockfile committed)
pnpm build              # builds dist/
cd src-tauri
cargo build --release   # builds target/release/veil
cargo build --release --manifest-path helper/Cargo.toml  # veil-helper
```

`src-tauri/Cargo.toml` enables Tauri's `custom-protocol` feature by default so a
plain `cargo build --release` embeds `dist/` and the release binary is
standalone. For a Tauri dev-server build that loads `devUrl` instead, disable
default features (`cargo build --no-default-features`); normal UI work can also
use Vite directly. A GUI session is required to show the window. The backend
itself has no display dependency and is covered by unit tests and the
real-engine smoke/lifecycle harnesses.

## sing-box core discovery

The first executable found in this order is used:

1. `settings.corePath`
2. `$VEIL_SING_BOX`
3. `sing-box` on `PATH` (`which sing-box`)
4. `sing-box` beside the app executable
5. Tauri resource dir (`resources/sing-box`)
6. `/usr/lib/veil/sing-box`
7. development vendor build (`vendor/sing-box-*/sing-box`)

A copy of the vendored 1.14.5 binary is bundled at
`src-tauri/resources/sing-box` (also installed to `/usr/lib/veil/sing-box` by the
PKGBUILD). Change the core path in Settings to use a system sing-box instead.

## TUN helper install

```bash
./install-helper.sh
```

This builds `veil-helper`, installs it to `/usr/bin/veil-helper`, installs
`dev.veil.helper.policy` into `/usr/share/polkit-1/actions/`, and installs the
desktop entry/icon. It uses interactive `sudo` only for installation. There is no
passwordless sudoers rule; TUN start/stop goes through:

```bash
pkexec /usr/bin/veil-helper start --config PATH --core PATH --pid-file PATH --log-file PATH
pkexec /usr/bin/veil-helper stop --pid-file PATH
pkexec /usr/bin/veil-helper check
```

`veil-helper` runs direct argv (no shell), validates the config with
`sing-box check` as root, detaches with `setsid`, stores the pid + config path in
the pid file, refuses to overwrite a live unrelated process, and on stop verifies
`/proc/<pid>/cmdline` belongs to sing-box with that config before SIGTERM, then
SIGKILL after 8s. It leaves unrelated nftables rules untouched.

## System proxy behaviour

| Desktop | Mechanism | Notes |
|---|---|---|
| GNOME family | `gsettings org.gnome.system.proxy.*` | http/https/socks host+port, mode manual |
| KDE Plasma | `kwriteconfig6/5` on `kioslaverc` | ProxyType/httpProxy/httpsProxy/socksProxy |
| XFCE | `xfconf-query` | only when the tool exists |
| Hyprland / sway / other | none | reports `unsupported`; UI shows manual env instructions |

Applied **only in system mode**, after the engine passes its health check. The
exact previous values are saved in `runtime/sysproxy.json` (0600) *before*
modifying anything and restored on disconnect, unexpected engine exit, app exit,
and on the next launch after a crash. TUN mode never touches the system proxy and
reports `TUN routes all traffic`.

## Data locations

- Settings: `~/.config/veil/settings.json` (0600)
- Database: `~/.local/share/veil/veil.db` (profiles, selections, meta)
- Runtime: `~/.local/share/veil/runtime/` (config 0600, runtime state 0600,
  proxy restore state)
- Logs: `~/.local/share/veil/logs/veil.log` (0600, ring buffer 2000 entries in
  memory; exports under `~/.local/share/veil/exports/`)
- Helper files: `/run/veil/` (0755 dir, pid 0644, log 0644 so the user can tail
  the real engine output)

Raw profile content is never rewritten. The generated runtime config is a
separate file and injects only `experimental.clash_api` and the `veil-mixed`
(and, in TUN mode, `veil-tun`) inbounds, preserving all other fields.

## Tests

```bash
cd src-tauri && cargo test          # link parsers, config gen, DB, proxy state,
                                   # Clash mapping, logs, settings, diagnostics
cargo test --manifest-path helper/Cargo.toml
```

Real engine smoke test (no root, no TUN, no display):

```bash
cd src-tauri
VEIL_SINGBOX=$PWD/../vendor/sing-box-1.14.5-linux-amd64/sing-box \
  cargo run --example engine_smoke
```

It generates a config with a free mixed port and Clash API, validates it with the
real binary, starts sing-box, waits for the Clash API, asserts `/version` matches,
reads `/connections` and `/proxies`, captures real log lines, and stops the
process with SIGTERM.

## Package build

`packaging/PKGBUILD` builds the frontend, the Tauri app and the helper, and
installs the helper to `/usr/bin/veil-helper`, the polkit action and the bundled
core to `/usr/lib/veil/sing-box`.

## Known limitations

- TUN end-to-end was not run here (requires polkit/root authentication and a
  display for the agent). The helper and generated TUN config are validated.
- System proxy support is limited to the desktop APIs above. Hyprland has no
  desktop-wide proxy API Veil can set; the backend says so instead of writing
  GNOME/KDE settings that would not apply.
- Notifications are best-effort (D-Bus); failures are ignored.
- Tray requires a StatusNotifier host; without one, window behavior is normal.
- `PATCH /configs` (Clash mode) is forwarded to sing-box; if sing-box rejects a
  mode, the error is surfaced verbatim.
- QR import needs `zbarimg`; no decoding is simulated.
