# Development guide

This guide covers day-to-day work on Veil: environment setup, the module map,
commands, debugging, test harnesses, data locations and formatting. For
user-facing build instructions see [building.md](building.md); for packaging see
[PACKAGING.md](PACKAGING.md); for failure modes see
[troubleshooting.md](troubleshooting.md). Architecture, feature status and the
security model are in [ARCHITECTURE.md](ARCHITECTURE.md),
[features.md](features.md) and [security.md](security.md).

## Environment

| Tool | Needed | Notes |
| --- | --- | --- |
| Rust | 1.80+ | `src-tauri/Cargo.toml`; the helper declares 1.77. rustup stable is recommended |
| Node.js | 22.13+ | ESLint 10 requires 22.13+; CI uses 22; development used 26 |
| pnpm | 10+ | Lockfile version 9.0; development used 12 |
| WebKitGTK 4.1, GTK 3, libsoup 3 | Build/run GUI | Distro packages below |
| `nft`, `ip` | TUN/route diagnostics | From `nftables` and `iproute2` |
| polkit, `pkexec` | TUN helper | The GUI never runs as root |
| `zbarimg` | QR import | Optional; from `zbar` |
| sing-box 1.14.5 | Runtime / real tests | Can come from `PATH`, `$VEIL_SING_BOX`, vendor, or packaging |

Arch Linux:

```bash
sudo pacman -S --needed \
  base-devel rust nodejs pnpm \
  webkit2gtk-4.1 gtk3 libsoup3 \
  nftables iproute2 polkit dbus zbar
```

Ubuntu 24.04:

```bash
sudo apt install build-essential curl file pkg-config libssl-dev \
  libdbus-1-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev librsvg2-dev patchelf
```

Install Rust with rustup, Node 22+ with your preferred version manager, and pnpm
10+. Then:

```bash
pnpm install
```

## Module map

```
src/                              Svelte 5 UI
  main.ts, App.svelte             entry and shell
  lib/api.ts                      typed invoke wrappers for docs/CONTRACT.md
  lib/state.svelte.ts             app/engine state, events, toasts
  lib/outbounds.svelte.ts         shared outbound selection state
  lib/views/                      Overview, Profiles, Outbounds, Routing,
                                  Connections, Logs, Diagnostics, Settings
  lib/components/                 reusable UI (OutboundBrowser, Drawer, ...)

src-tauri/
  src/main.rs                     thin Tauri entrypoint
  src/lib.rs                      app state, commands, setup, shutdown
  src/errors.rs                   CommandError envelope
  src/paths.rs                    XDG paths and directory permissions
  src/settings.rs                 settings.json load/patch/atomic write
  src/db.rs                       SQLite profiles, selections, backups
  src/core.rs                     core discovery, version, `sing-box check`
  src/configgen.rs                runtime config injection + 0600 write
  src/engine.rs                   lifecycle, supervision, stats, adoption
  src/clash.rs                    Clash API client and mapping
  src/sysproxy.rs                 desktop proxy apply/restore
  src/links.rs                    share-link parsers
  src/diagnostics.rs              read-only host probing
  src/logs.rs                     ring buffer, redaction, export
  src/tray.rs                     ksni StatusNotifierItem
  helper/src/main.rs              veil-helper (root TUN control)
  examples/                       real-engine harnesses

docs/                             architecture, contract, verification, packaging
packaging/                        PKGBUILD, source tarball, local install
```

The invariant to preserve: the UI does not duplicate engine logic, and the
backend does not report a state it has not verified. `engine://state` is the
canonical lifecycle event; `connected` requires a successful Clash API
`/version` check.

## Common commands

```bash
make install            # pnpm install
make build              # pnpm build + release app + release helper
make frontend           # pnpm build
make backend            # release app only
make helper             # release helper only
make test               # app + helper tests
make smoke              # engine_smoke against the vendored core
make install-local      # per-user install, no root
make uninstall-local    # remove the per-user install, keep data
make install-helper     # sudo: helper + polkit policy + desktop entry
make source-tarball     # packaging/veil-0.1.0.tar.gz
make clean              # remove dist and both Cargo target dirs
```

Development:

```bash
pnpm tauri dev          # Vite on 127.0.0.1:1420 + Tauri window
pnpm dev                # frontend only, no Tauri API
```

For a backend build that loads `devUrl` instead of embedded `dist/` assets:

```bash
cd src-tauri
cargo build --no-default-features
```

A plain `cargo build --release` enables `custom-protocol` by default and embeds
`dist/`, so run `pnpm build` first.

## Formatting and linting

`pnpm exec prettier --check .` and `pnpm exec eslint .` are the concrete
commands behind the `format:check` and `lint` package scripts.

```bash
# Main Rust crate
(cd src-tauri && cargo fmt --all -- --check)
(cd src-tauri && cargo clippy --locked --all-targets --all-features -- -D warnings)

# Helper crate (separate manifest, from the repository root)
cargo fmt --manifest-path src-tauri/helper/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/helper/Cargo.toml --all-targets -- -D warnings

# Frontend
pnpm exec prettier --check .
pnpm exec eslint .
pnpm exec svelte-check --tsconfig ./tsconfig.json
```

Omit `-- --check` from `cargo fmt` to rewrite files. Prettier and ESLint read
the repository configuration; do not add per-directory exceptions for generated
output (`dist/`, `node_modules/`, `src-tauri/target/`).

`.editorconfig` and `.gitattributes` define UTF-8, LF, final newlines and
indentation: 2 spaces for JS/TS/Svelte/CSS/JSON/YAML, 4 for Rust, tabs for the
`Makefile`.

## Test harnesses

### Unit and integration tests

```bash
(cd src-tauri && cargo test --locked)
(cd src-tauri && cargo test --locked --release)            # CI
cargo test --locked --manifest-path src-tauri/helper/Cargo.toml
```

Tests requiring the real core are skipped unless `VEIL_SINGBOX` is set:

```bash
VEIL_SINGBOX=/path/to/sing-box cargo test
```

They cover share-link generation, config injection, TUN configs, DB CRUD,
settings, redaction, Clash mapping and diagnostics against the real binary.

### Engine smoke

Starts a real sing-box with a generated config, validates it, waits for the
Clash API, reads `/version`, `/connections` and `/proxies`, captures logs and
stops with `SIGTERM`. No root, TUN or display:

```bash
cd src-tauri
VEIL_SINGBOX=/path/to/sing-box cargo run --example engine_smoke
```

### Engine lifecycle

Uses the real `Engine` and a recording event sink with isolated XDG directories.
It asserts the `starting -> connected -> stopping -> idle` sequence, graceful
cleanup, and the `error` path after `SIGKILL`. It forces the unsupported desktop
proxy path and never touches real proxy settings:

```bash
cd src-tauri
VEIL_SINGBOX=/path/to/sing-box \
  XDG_DATA_HOME=/tmp/veil-lifecycle-data \
  XDG_CONFIG_HOME=/tmp/veil-lifecycle-config \
  XDG_CURRENT_DESKTOP=Hyprland \
  cargo run --example engine_lifecycle
```

### Subscription integration

Reads a subscription URL from `$VEIL_SUB_FILE` (default `/tmp/veil-sub.url`),
keeps it out of stdout and the repository, imports it through the real
normalization path, validates with `sing-box check`, connects, measures traffic
and delay, refreshes, and checks log redaction:

```bash
cd src-tauri
VEIL_SINGBOX=/path/to/sing-box \
  VEIL_SUB_FILE=/path/to/subscription.url \
  XDG_DATA_HOME=/tmp/veil-sub-data \
  XDG_CONFIG_HOME=/tmp/veil-sub-config \
  cargo run --example subscription_integration
```

The URL file must be `0600` and outside the repository. Do not commit its
contents or any generated profile.

### Gated desktop proxy tests

These mutate the real desktop proxy settings, snapshot the exact prior values and
restore them (including on failure). Run only on a machine you control:

```bash
cd src-tauri
VEIL_TEST_GSETTINGS=1 cargo test --lib \
  sysproxy::tests::gnome_apply_restore_roundtrip_when_enabled -- --nocapture
VEIL_TEST_GSETTINGS=1 cargo test --lib \
  sysproxy::tests::unsupported_desktop_apply_is_a_noop_when_enabled -- --nocapture
```

### TUN

TUN requires root/polkit and `/dev/net/tun`. There is no passwordless sudoers
rule; do not add one to make tests pass. Check the helper without starting a
tunnel:

```bash
veil-helper check --core /path/to/sing-box
```

Do not claim TUN success unless the tunnel actually starts and routes traffic.
The verification record documents one real end-to-end PASS on 2026-11-27
(`gvisor` stack, `auto_detect_interface`, `pkexec`-approved helper). That result
is host-specific: TUN still needs `/dev/net/tun`, nftables, an installed helper
and policy, and no conflicting tunnel. See
[VERIFICATION.md](VERIFICATION.md#tun-end-to-end--pass-2026-11-27).

## Debugging

### Logs

- Application log: `~/.local/share/veil/logs/veil.log` (mode `0600`, rotated to
  `veil.log.1`). The in-memory ring holds 2000 entries.
- Exports: `~/.local/share/veil/exports/veil-logs-*.log`.
- TUN helper engine log: `/run/veil/sing-box.log` (mode `0644`, tail-able by the
  user); pid file: `/run/veil/sing-box.pid`.

Messages are redacted before they enter the ring, the file, or events:
URL userinfo, password/token/secret/key fields, UUIDs, subscription query
strings, and the exact Clash API secret. If you need more detail while
debugging, add a redacted diagnostic instead of disabling redaction.

### Engine and Clash API

`runtime/runtime.json` stores the running engine's pid, mode, profile, mixed
port, API port and secret (mode `0600`):

```bash
RT=~/.local/share/veil/runtime/runtime.json
jq '{pid,mode,profileId,mixedPort,apiPort}' "$RT"
ps -p "$(jq -r .pid "$RT")" -o pid,ppid,args
curl -s -H "Authorization: Bearer $(jq -r .secret "$RT")" \
  "http://127.0.0.1:$(jq -r .apiPort "$RT")/version"
```

The same runtime file proves adoption after a GUI restart. If the engine exits
unexpectedly, the app restores the desktop proxy, removes runtime files and
emits an `error` state with the stderr tail.

### Isolated instance

To avoid touching your normal profiles, settings or proxy state, run with
separate XDG directories:

```bash
XDG_DATA_HOME=/tmp/veil-dev-data \
XDG_CONFIG_HOME=/tmp/veil-dev-config \
./src-tauri/target/release/veil
```

The app creates `veil/` under each. Use `VEIL_SING_BOX` or the Settings core
path to select a core for that instance.

### Single-instance lock

Startup takes an exclusive non-blocking `flock` on
`<data_dir>/veil.lock` before opening SQLite or runtime state. A second launch
exits with `another Veil instance is already running (lock: ...)`. The lock is
released when the process exits, so a leftover file is harmless; its contents
are the owner pid. The lock itself is unit-tested
(`instance::tests::second_acquire_fails`); the two-launch and adopted-engine
flows are implemented-unverified end to end.

### Stale or adopted engine

`runtime/runtime.json` records the engine Veil believes it owns. On startup it
verifies the pid, `/proc/<pid>/cmdline`, the exact config path and the Clash API
secret before adopting it. Inspect before starting anything:

```bash
RT=~/.local/share/veil/runtime/runtime.json
jq '{pid,mode,profileId,mixedPort,apiPort}' "$RT"
ps -p "$(jq -r .pid "$RT")" -o pid,ppid,args
```

Never start a second TUN while one is live, and never kill processes by hand.
Stop through the app or the helper so routes and proxy restore are cleaned up.

### Desktop proxy state

Before modifying GNOME/KDE/XFCE settings, the backend writes the exact previous
values to `runtime/sysproxy.json` (mode `0600`). On disconnect, exit, or the next
launch after a crash it restores them and removes the file. If you are
debugging proxy restore, inspect that file first; do not delete it manually
while the engine is connected.

### Diagnostics view

The Diagnostics view reports only obtainable data: core version, helper/polkit
state, `/dev/net/tun`, nftables, interfaces, routes, DNS, tray availability and
recent errors. Capabilities are derived, not hardcoded. An absent tool shows as
unavailable rather than "working".

## Data locations

| Path | Contents | Mode |
| --- | --- | --- |
| `~/.config/veil/settings.json` | UI and engine settings | `0600` |
| `~/.local/share/veil/veil.db` | Profiles, selections, metadata | file |
| `~/.local/share/veil/veil.lock` | Single-instance lock owner pid | `0600` |
| `~/.local/share/veil/runtime/runtime.json` | Live engine identity and API secret | `0600` |
| `~/.local/share/veil/runtime/config.json` | Generated runtime config | `0600` |
| `~/.local/share/veil/runtime/sysproxy.json` | Desktop proxy restore state | `0600` |
| `~/.local/share/veil/logs/veil.log` | Redacted application log | `0600` |
| `~/.local/share/veil/exports/` | Log and profile backups | `0700` dir |
| `/run/veil/sing-box.pid` | Helper-managed engine pid (TUN) | `0644` |
| `/run/veil/sing-box.log` | Helper-managed engine log (TUN) | `0644` |

The database stores raw profile content and is never rewritten by config
generation. The generated runtime config is a separate file.

## Environment variables

| Variable | Used by | Purpose |
| --- | --- | --- |
| `VEIL_SING_BOX` | App and helper | Explicit core path (after `settings.corePath`) |
| `VEIL_SINGBOX` | Tests and examples | Enables real-binary test paths and harnesses |
| `VEIL_TEST_GSETTINGS` | Tests | Set to `1` to allow real gsettings mutation |
| `VEIL_SUB_FILE` | Subscription harness | Subscription URL file (default `/tmp/veil-sub.url`) |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME` | App and harnesses | Isolate settings/data |
| `XDG_CURRENT_DESKTOP` | App | Desktop detection for system proxy |

`VEIL_SING_BOX` and `VEIL_SINGBOX` are intentionally different; do not merge
them in a change without updating the app and test docs together.

## CI

`.github/workflows/quality.yml` runs frontend checks and the Rust checks/tests on
pushes and pull requests to `master` and `dev`. `.github/workflows/build.yml`
builds the release app and helper and uploads them as artifacts. CI does not run
TUN, desktop-proxy, or AppImage tests; do not claim those from a green build.

For runtime failures, start with [troubleshooting.md](troubleshooting.md) and
the in-app Diagnostics/Logs views.
