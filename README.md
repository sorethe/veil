<p align="center">
  <img src="static/icon.svg" width="96" height="96" alt="Veil application icon">
</p>

<h1 align="center">Veil</h1>

<p align="center"><em>where every connection begins.</em></p>

<p align="center">
  <a href="https://github.com/sorethe/veil/actions/workflows/quality.yml"><img alt="Quality workflow" src="https://github.com/sorethe/veil/actions/workflows/quality.yml/badge.svg?branch=dev"></a>
  <img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg">
  <img alt="Rust 1.80+" src="https://img.shields.io/badge/rust-1.80%2B-orange.svg">
  <img alt="Node.js 22.13+" src="https://img.shields.io/badge/node-22.13%2B-339933.svg">
  <img alt="pnpm 10+" src="https://img.shields.io/badge/pnpm-10%2B-f69220.svg">
</p>

**A Linux desktop proxy client powered by [sing-box](https://github.com/SagerNet/sing-box).**

Veil is a Tauri 2 application with a Svelte 5 frontend and a Rust backend. It does
not implement a proxy engine of its own: every connection, speed, latency value
and route shown in the UI comes from a real `sing-box` process through its Clash
API. Veil stores profiles, generates the runtime config, validates it with the
real core, supervises the engine, and restores desktop state when it exits.

> **Pre-release.** Version `0.1.0` has not been tagged or published. There is no
> binary or package download yet; build from source. The repository is currently
> private.

## Features

| Area | Status |
| --- | --- |
| **Real engine, no reimplementation** — profiles are sing-box JSON; Veil only injects the documented integration points into a generated runtime copy. Stored profiles are never rewritten. | Implemented, integration-tested |
| **Engine lifecycle** — `sing-box check`, spawn, Clash API `/version` health check, live stats, connections, selector switching, delay tests, Clash logs, graceful `SIGTERM`/`SIGKILL`, and adoption of a still-running engine after a GUI restart. `connected` is never emitted without a successful health check. | Implemented, integration-tested |
| **Profiles in SQLite** — create, edit, duplicate and delete profiles; raw content preserved verbatim; per-profile outbound selections and backup export/import. | Implemented, tested |
| **Imports** — sing-box JSON, subscriptions (JSON, base64 JSON, or base64 share-link lists), share links (`vless`, `vmess`, `trojan`, `ss`, `hysteria2`, `tuic`, `socks`, `http`/`https`), and QR images through `zbarimg`. Clash YAML is rejected explicitly rather than guessed at. | Implemented, integration-tested |
| **Validation** — every profile and generated runtime config is checked with the actual `sing-box check` before use. | Implemented, integration-tested |
| **System proxy** — apply and restore on GNOME (`gsettings`), KDE (`kwriteconfig`) and XFCE (`xfconf-query`). Exact previous values are snapshotted and restored on disconnect, crash recovery and exit. Unsupported desktops report `unsupported` instead of writing settings that would not apply. | GNOME verified; KDE/XFCE implemented-unverified |
| **TUN mode through a privileged helper** — `veil-helper` is the only component that runs as root, invoked through `pkexec` and polkit. The GUI never runs as root. TUN uses the `gvisor` stack and `auto_detect_interface`, and refuses to start when another tunnel or overlapping route/address would conflict. | Verified end-to-end on 2026-11-27 (see notes) |
| **Diagnostics built from real data** — core version, helper/polkit state, TUN device, nftables, routes, interfaces, DNS, tray and recent errors, all derived from obtainable host data. | Implemented |
| **Desktop integration** — autostart entry, best-effort notifications, and a StatusNotifierItem tray (`ksni`, no appindicator dependency). | Implemented, best-effort |
| **Logs** — 2000-entry ring with redaction for URL userinfo, credential fields, UUIDs, subscription query strings and the Clash API secret, plus export. | Implemented, tested |
| **Typed frontend/backend contract** — 38 commands rejecting with `CommandError { code, message, detail }`; `engine://state` is the canonical lifecycle stream. | Implemented, documented |
| **Single-instance guard** — an exclusive `flock` on `<data_dir>/veil.lock` stops a second GUI from opening the same database and runtime state. | Implemented; lock unit-tested, two-launch flow unverified |

Status words follow [docs/features.md](docs/features.md) and
[docs/VERIFICATION.md](docs/VERIFICATION.md): **verified/integration-tested**
means exercised against the real binary, tool or desktop; **implemented-unverified**
means the code is wired and builds but was not proven on the verification host.

### TUN status

TUN start/stop, DNS/UDP/TCP through the tunnel, route setup and teardown were
verified on 2026-11-27 on the maintainer's Arch Linux / Hyprland host with
sing-box 1.14.5, using a real `pkexec` approval and the `dev.veil.helper` polkit
action. That result is host-specific: it needs `/dev/net/tun`, nftables, an
installed helper/policy and an active polkit session. Veil refuses TUN when
another TUN/VPN interface or an overlapping route/address exists; it does not
modify the other tool's state. See
[TUN end-to-end — PASS](docs/VERIFICATION.md#tun-end-to-end--pass-2026-11-27).

## Platform support

| Item | Status |
| --- | --- |
| Linux x86_64 | Target platform |
| Arch Linux + Hyprland 0.57 (Wayland) | Verification host |
| GNOME-family system proxy | Implemented and exercised against real `gsettings` |
| KDE / XFCE system proxy | Implemented; **unverified** on the verification host |
| TUN mode | Verified on the verification host; Linux kernel, `/dev/net/tun`, nftables and polkit required |
| X11 | Outside the verified matrix; expected to work via Tauri/WebKitGTK |
| Other distributions and architectures | Best-effort; untested |
| AppImage / deb / rpm / Arch package | Not built or installed as part of the verified matrix |

sing-box **1.14.5** (official upstream build) is the validated core. Veil looks
for it in this order: `settings.corePath`, `$VEIL_SING_BOX`, `sing-box` on
`PATH`, beside the app executable, the Tauri resource directory,
`/usr/lib/veil/sing-box`, then a development `vendor/sing-box-*` build.

## Install and build

Veil is built from source in this pre-release state. The short version:

```bash
git clone https://github.com/sorethe/veil.git
cd veil
pnpm install
make build
./src-tauri/target/release/veil
```

`make build` runs `pnpm build` and produces both release binaries:
`src-tauri/target/release/veil` and `src-tauri/helper/target/release/veil-helper`.
The app embeds `dist/` via Tauri's `custom-protocol` feature, so it runs without
a dev server.

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

Ubuntu 24.04 (CI parity):

```bash
sudo apt install build-essential curl file pkg-config libssl-dev \
  libdbus-1-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev librsvg2-dev patchelf
```

Rust 1.80 or newer (rustup stable recommended), Node.js 22.13 or newer, and
pnpm 10 or newer. The tray uses `ksni`, so `libayatana-appindicator` is not
required. `zbarimg` is optional and only enables QR imports.

### Development

```bash
pnpm install
pnpm tauri dev          # Vite on 127.0.0.1:1420 + Tauri dev window
```

### User-local install (no root)

```bash
make install-local
```

This installs the app, a bundled core and a desktop entry under the current
user's home without touching the package database or existing profiles. See
[docs/PACKAGING.md](docs/PACKAGING.md) for paths and uninstall steps.

### Privileged helper (TUN)

```bash
make install-helper     # builds and installs veil-helper + polkit policy (sudo)
```

This installs `/usr/bin/veil-helper` and the `dev.veil.helper` polkit action.
TUN start/stop goes through `pkexec`; there is no passwordless sudoers rule. Do
not run it on a system where the `veil` package is installed (the paths
conflict). Selecting TUN for the first time also opens a setup dialog that
probes the prerequisites and can install the helper with admin approval.

For exact prerequisites by distribution, the release/helper build steps, core
discovery, packaging and test harnesses, read
**[docs/building.md](docs/building.md)**.

## Documentation

- [Building from source](docs/building.md) — prerequisites, dev, release, helper, packaging, core discovery
- [Features and limits](docs/features.md) — what is verified, implemented-unverified, or absent
- [Troubleshooting](docs/troubleshooting.md) — core, Clash API, proxy, TUN, ports, subscriptions, tray, logs, single-instance
- [Security model](docs/security.md) — privilege separation, permissions, secret handling
- [Architecture](docs/ARCHITECTURE.md) — module map, lifecycle, config generation, restore logic
- [Backend/frontend contract](docs/CONTRACT.md) — commands, events, data shapes
- [Development guide](docs/DEVELOPMENT.md) — debugging, data locations, test harnesses
- [Verification record](docs/VERIFICATION.md) — what was actually exercised
- [Packaging](docs/PACKAGING.md) and [Releasing](docs/RELEASING.md)
- [Contributing](CONTRIBUTING.md) and [Security policy](SECURITY.md)
- [Changelog](CHANGELOG.md)

## Repository layout

```
src/                      Svelte 5 UI (views, components, stores, API client)
src-tauri/
  src/                    Rust backend (engine, config, DB, Clash, proxy, logs)
  helper/                 separate veil-helper crate (root-only TUN control)
  examples/               real sing-box smoke, lifecycle and subscription harnesses
  resources/              polkit policy, desktop entry, local core (ignored)
docs/                     architecture, contract, verification, packaging, guides
packaging/                PKGBUILD, source tarball and local install scripts
.github/workflows/        quality and release-build CI
Makefile                  build/test/install shortcuts
static/                   repository and application marks
```

## Known limitations

- **Hyprland/sway system proxy is unsupported.** There is no desktop-wide proxy
  API Veil can set; the backend reports `unsupported` and the UI shows a
  `LIVE · MANUAL` session with environment-variable instructions.
- **KDE and XFCE proxy backends are unverified** on the verification host
  (XFCE also needs `xfconf-query` installed).
- **TUN is verified only on the maintainer's host.** Other distributions,
  kernels and network managers remain untested, and starting TUN always requires
  the privileged helper, polkit authentication and no conflicting tunnel.
- **No AppImage, deb or rpm is built or verified.** The Arch `PKGBUILD` is
  syntax-checked but has not been built or installed as part of the verified
  matrix.
- **The tray, notifications and autostart are best-effort.** The tray needs a
  StatusNotifier host; without one the window behaves normally.
- **Global shortcuts are unavailable on Wayland** and are not registered or
  faked.
- **Clash YAML profiles are not converted.** Veil reports the limitation
  instead of guessing.
- **`auto_redirect` is config-tested, not end-to-end verified**; actual nftables
  redirect behavior has not been exercised.
- **Logs and profiles contain sensitive local data.** Logs are redacted for
  common credential fields and the API secret, but the database, runtime files
  and exports still contain profile material.
- **Tests are not a release claim.** CI does not run TUN, real desktop-proxy
  mutation or packaging; a green build does not prove those paths.

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) first: the
project values honest verification over optimistic claims, and any change that
cannot be tested should say so. The frontend talks to the backend only through
the commands and events in [docs/CONTRACT.md](docs/CONTRACT.md).

## Security

Report vulnerabilities privately through GitHub Security Advisories as
described in [SECURITY.md](SECURITY.md). Do not open a public issue for a
security problem. The security model is summarized in
[docs/security.md](docs/security.md).

## License

MIT. See [LICENSE](LICENSE). Veil is a separate application; sing-box is
distributed under its own license (GPL-3.0-or-later), which packaging installs
alongside the core.