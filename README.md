# Veil

**A Linux desktop proxy client powered by [sing-box](https://github.com/SagerNet/sing-box).**

Veil is a Tauri 2 desktop application with a Svelte 5 frontend and a Rust
backend. It does not implement a proxy engine of its own: every connection,
speed, latency value and route shown in the UI comes from a real `sing-box`
process through its Clash API. Veil manages profiles, generates the runtime
config, supervises the engine and restores desktop state when it exits.

The repository is currently private and pre-release. There is **no published
binary or package** yet; build from source. Version `0.1.0` has not been tagged.

## Features

- **Profiles in SQLite** — create, edit, duplicate and delete profiles; raw
  sing-box JSON is stored verbatim and never rewritten in place.
- **Real validation** — every profile and generated runtime config is checked
  with the actual `sing-box check` before it is used.
- **Imports** — sing-box JSON, subscriptions (JSON, base64 JSON, or base64
  share-link lists), share links (`vless`, `vmess`, `trojan`, `ss`,
  `hysteria2`, `tuic`, `socks`, `http`/`https`), and QR images through
  `zbarimg`. Clash YAML is rejected with an explicit error rather than
  silently converted.
- **Engine lifecycle** — `sing-box check`, spawn, Clash API `/version` health
  check, live stats, connection list, selector switching, delay tests, Clash
  logs, graceful `SIGTERM`/`SIGKILL`, and adoption of a still-running engine
  after the GUI restarts.
- **System proxy** — apply and restore on GNOME (`gsettings`), KDE
  (`kwriteconfig`), and XFCE (`xfconf-query`). Exact previous values are saved
  before applying and restored on disconnect, crash recovery, and app exit.
  Desktops without a supported API report `unsupported` instead of writing
  settings that would not apply.
- **TUN mode through a privileged helper** — `veil-helper` is the only
  component that runs as root, invoked through `pkexec` and polkit. The GUI
  never runs as root. TUN startup itself is unverified end-to-end here; see
  [Known limitations](#known-limitations).
- **Diagnostics built from real data** — core version, helper/polkit state, TUN
  device, nftables, routes, interfaces, DNS and recent errors.
- **Desktop integration** — autostart entry, best-effort notifications, and a
  StatusNotifierItem tray (`ksni`, no appindicator dependency).
- **Typed frontend/backend contract** — every command rejects with
  `CommandError { code, message, detail }`; `engine://state` is the canonical
  lifecycle stream.

## Screenshots

Screenshots will be added with the first tagged release. The verification run
captured screenshots of the running release app; their paths and observations
are recorded in [docs/VERIFICATION.md](docs/VERIFICATION.md), but the image
files are not committed to this repository.

## Architecture

```
Svelte 5 UI (src/)
    |  invoke() commands + engine:// events (typed contract)
    v
Rust / Tauri backend (src-tauri/)
    |-- profiles, settings, logs, diagnostics (SQLite + local files)
    |-- engine supervisor -> `sing-box run` (system mode, as the user)
    |   `-- Clash API client -> 127.0.0.1:<apiPort>
    `-- pkexec -> veil-helper (root, TUN mode only)
```

The frontend talks to the backend only through the commands and events in
[docs/CONTRACT.md](docs/CONTRACT.md). The backend never invents engine state:
`connected` is emitted only after a real Clash API health check.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the module map, lifecycle
state machine, config generation rules, privilege separation, and restoration
logic.

## Supported platform

| Item | Status |
| --- | --- |
| Linux x86_64 | Target platform |
| Arch Linux + Hyprland 0.57 (Wayland) | Tested verification environment |
| GNOME-family system proxy | Implemented and exercised against real `gsettings` |
| KDE / XFCE system proxy | Implemented; **unverified** on the verification host |
| X11 | Not part of the verified matrix; expected to work via Tauri/WebKitGTK |
| Other Linux distributions and architectures | Best-effort; untested |

sing-box **1.14.5** (official upstream build) is the validated core. Veil looks
for it in this order: `settings.corePath`, `$VEIL_SING_BOX`, `sing-box` on
`PATH`, beside the app executable, the Tauri resource directory,
`/usr/lib/veil/sing-box`, then a development `vendor/sing-box-*` build.

### Known limitations

- **TUN end-to-end is unverified.** The helper, polkit policy and generated TUN
  configs are validated, but starting TUN needs root/polkit authentication and
  `/dev/net/tun`; a concurrent TUN owned the required route during the
  verification run, so no TUN success is claimed.
- **Hyprland/sway system proxy is unsupported.** There is no desktop-wide proxy
  API Veil can set; the backend reports `unsupported` and the UI shows manual
  environment-variable instructions.
- **KDE and XFCE proxy backends are unverified** on this host (XFCE also needs
  `xfconf-query`).
- **No AppImage, deb or rpm is built or verified.** `docs/PACKAGING.md`
  documents the expected AppImage command and its open questions. The Arch
  `PKGBUILD` is syntax-checked but has not been built or installed as part of
  the verified matrix.
- **Global shortcuts are unavailable on Wayland** and are not registered or
  faked.
- **Notifications and the tray are best-effort.** The tray needs a
  StatusNotifier host; without one the window behaves normally.
- **Clash YAML profiles are not converted.** Veil reports the limitation
  instead of guessing.
- **Logs and profiles contain sensitive local data.** Logs are redacted for
  common credential fields and the API secret, but settings, the database and
  runtime files still contain profile material.

## Install and build

### Prerequisites

Arch Linux:

```bash
sudo pacman -S --needed \
  base-devel rust nodejs pnpm \
  webkit2gtk-4.1 gtk3 libsoup3 \
  nftables iproute2 polkit dbus \
  zbar
# optional: an existing system core
sudo pacman -S sing-box
```

Ubuntu 24.04 equivalents (for CI parity):

```bash
sudo apt install build-essential curl file pkg-config libssl-dev \
  libdbus-1-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev librsvg2-dev patchelf
```

Rust 1.80 or newer (rustup stable recommended), Node.js 22.13 or newer, and
pnpm 10 or newer. The local development host used Node 26 and pnpm 12. The Tauri
tray uses `ksni`, so `libayatana-appindicator` is not required.

### Release build

```bash
git clone https://github.com/sorethe/veil.git
cd veil
pnpm install
make build
```

`make build` runs `pnpm build` and then builds both release binaries:

- `src-tauri/target/release/veil`
- `src-tauri/helper/target/release/veil-helper`

The app embeds `dist/` via Tauri's `custom-protocol` default feature, so the
release binary runs without a dev server.

Run the app from a graphical session:

```bash
./src-tauri/target/release/veil
```

### User-local install (no root)

```bash
make install-local
```

This installs the app, a bundled core and a desktop entry under the current
user's home without touching the package database or existing profiles. See
[docs/PACKAGING.md](docs/PACKAGING.md) for the exact paths and uninstall steps.

### Development

```bash
pnpm install
pnpm tauri dev          # Vite on 127.0.0.1:1420 + Tauri dev window
```

For backend-only work, `cargo build --no-default-features` in `src-tauri/`
builds a binary that loads `devUrl` instead of embedded assets. Backend tests
and the real-engine examples do not need a display.

### Tests

```bash
make test               # app tests + helper tests

cd src-tauri
VEIL_SINGBOX=/path/to/sing-box cargo test   # enables real-binary validations
```

Real-engine harnesses (no root, no TUN, no display):

```bash
cd src-tauri
VEIL_SINGBOX=/path/to/sing-box cargo run --example engine_smoke
VEIL_SINGBOX=/path/to/sing-box \
  XDG_DATA_HOME=/tmp/veil-lifecycle-data \
  XDG_CONFIG_HOME=/tmp/veil-lifecycle-config \
  cargo run --example engine_lifecycle
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for toolchain versions, formatting and
lint commands, test gates, and commit conventions. See
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for debugging, data locations and
the module map.

### TUN helper

```bash
make install-helper     # builds and installs veil-helper + polkit policy (sudo)
```

This installs `/usr/bin/veil-helper`, the `dev.veil.helper` polkit action, the
desktop entry and the icon. TUN start/stop goes through
`pkexec /usr/bin/veil-helper ...`; there is no passwordless sudoers rule. Do not
run this on a system where the `veil` package is installed (the paths conflict).

## Repository layout

```
src/                      Svelte 5 UI (views, components, stores, API client)
src-tauri/
  src/                    Rust backend (engine, config, DB, Clash, proxy, logs)
  helper/                 separate veil-helper crate (root-only TUN control)
  examples/               real sing-box smoke, lifecycle and subscription harnesses
  resources/              polkit policy, desktop entry, local core (ignored)
docs/                     architecture, contract, verification, packaging
packaging/                PKGBUILD, source tarball and local install scripts
.github/workflows/        quality and release-build CI
Makefile                  build/test/install shortcuts
```

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Backend/frontend contract](docs/CONTRACT.md)
- [Development guide](docs/DEVELOPMENT.md)
- [Verification record](docs/VERIFICATION.md)
- [Packaging](docs/PACKAGING.md)
- [Releasing](docs/RELEASING.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
- [Changelog](CHANGELOG.md)

## Security

Report vulnerabilities privately through GitHub Security Advisories as
described in [SECURITY.md](SECURITY.md). Do not open a public issue for a
security problem.

## License

MIT. See [LICENSE](LICENSE). Packaging fetches the official sing-box binary;
sing-box is distributed under its own license (GPL-3.0-or-later), which the
packaging installs alongside the core.
