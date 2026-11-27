# Building Veil

This guide covers getting Veil from a source checkout to a runnable binary, with
the same commands the repository and CI use. For packaging and release
publishing, see [PACKAGING.md](PACKAGING.md) and [RELEASING.md](RELEASING.md).

Veil is pre-release (`0.1.0`). There is no published binary, AppImage, deb or
rpm. Everything below builds from source.

## Toolchain and system requirements

| Tool | Minimum | Notes |
| --- | --- | --- |
| Rust | 1.80 | `src-tauri/Cargo.toml`; the helper declares 1.77. rustup stable recommended |
| Node.js | 22.13 | ESLint 10 needs 22.13+; CI uses 22, local development used 26 |
| pnpm | 10 | Lockfile version 9.0; CI pins 12.22.0 |
| sing-box | 1.14.5 | Validated core. Other versions are unverified |
| WebKitGTK 4.1, GTK 3, libsoup 3 | Build/run GUI | Names differ by distribution |
| `nft`, `ip` | TUN and route diagnostics | `nftables`, `iproute2` |
| polkit / `pkexec` | TUN helper | The GUI itself never runs as root |
| `zbarimg` | QR import | Optional, from `zbar` |

### Arch Linux

```bash
sudo pacman -S --needed \
  base-devel rust nodejs pnpm \
  webkit2gtk-4.1 gtk3 libsoup3 \
  nftables iproute2 polkit dbus \
  zbar
# Optional: use a system sing-box instead of a vendored/downloaded core
sudo pacman -S sing-box
```

### Ubuntu 24.04 (CI parity)

```bash
sudo apt update
sudo apt install build-essential curl file pkg-config libssl-dev \
  libdbus-1-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev librsvg2-dev patchelf
```

Install Rust with [rustup](https://rustup.rs/), Node.js 22.13+ with your
preferred version manager, and pnpm 10+. `libayatana-appindicator` is **not**
required: the tray uses `ksni` over D-Bus StatusNotifierItem.

## Get the source and frontend dependencies

```bash
git clone https://github.com/sorethe/veil.git
cd veil
pnpm install
```

`pnpm install` resolves the Svelte/Vite frontend toolchain. Rust dependencies
are fetched by Cargo during the first build.

## Core discovery

Veil never bundles a core in the repository. At connect time it resolves
`sing-box` in this order:

1. `settings.corePath` (Settings → Core path)
2. `$VEIL_SING_BOX`
3. `sing-box` on `PATH`
4. beside the app executable
5. the Tauri resource directory
6. `/usr/lib/veil/sing-box`
7. a development `vendor/sing-box-*` build

The first existing file wins; a configured path is not silently skipped if the
binary is broken. `sing-box version` is read to report the real version, and
every profile is validated with `sing-box check`.

A development checkout may keep a core at
`vendor/sing-box-1.14.5-linux-amd64/sing-box` (git-ignored). The official
pinned 1.14.5 release is what packaging fetches.

## Development

```bash
pnpm tauri dev
```

This starts Vite on `127.0.0.1:1420` and opens the Tauri dev window. The window
talks to the dev server, so frontend edits reload without rebuilding the Rust
side.

For backend-only work there is no dev server:

```bash
cd src-tauri
cargo build --no-default-features
```

That binary loads `devUrl` instead of embedded assets. A plain release build
enables `custom-protocol` by default and embeds `dist/`, so run `pnpm build`
first.

## Release build

```bash
make build
```

Equivalent steps:

```bash
pnpm build
(cd src-tauri && cargo build --release)
cargo build --release --manifest-path src-tauri/helper/Cargo.toml
```

Outputs:

- `src-tauri/target/release/veil` — the desktop application, with `dist/`
  embedded.
- `src-tauri/helper/target/release/veil-helper` — the root-only TUN helper.

Run the app from a graphical session:

```bash
./src-tauri/target/release/veil
```

The app reads normal XDG locations (`~/.config/veil`, `~/.local/share/veil`).
Use [isolated XDG directories](#isolated-test-instance) when you do not want to
touch your real profiles or proxy state.

A locked build, matching CI:

```bash
pnpm install --frozen-lockfile
pnpm build
(cd src-tauri && cargo build --locked --release)
cargo build --locked --release --manifest-path src-tauri/helper/Cargo.toml
```

## Privileged helper

TUN mode needs a root-owned helper and polkit action. Build it separately:

```bash
cargo build --release --manifest-path src-tauri/helper/Cargo.toml
```

Install the helper and policy for development:

```bash
make install-helper
```

This runs `sudo install` for:

- `/usr/bin/veil-helper`
- `/usr/share/polkit-1/actions/dev.veil.helper.policy`
- `/usr/share/applications/veil.desktop`
- `/usr/share/icons/hicolor/512x512/apps/veil.png`

It is for development only. Do not run it on a system where the `veil` package
owns those paths. The GUI never runs as root and never calls `sudo`; TUN
start/stop uses `pkexec /usr/bin/veil-helper ...`. Verify the policy with:

```bash
pkaction --verbose --action-id dev.veil.helper
veil-helper check --core /path/to/sing-box
```

The in-app first-use dialog offers the same installation through `pkexec` with
admin approval. It is implemented; only the `pkexec`-approved start/stop path
is recorded as verified in [VERIFICATION.md](VERIFICATION.md).

## User-local install

```bash
make install-local
```

`packaging/install-local.sh` installs, for the current user only:

| Path | Contents |
| --- | --- |
| `~/.local/opt/veil/veil` | Release app binary |
| `~/.local/opt/veil/sing-box` | Core placed beside the executable |
| `~/.local/share/icons/hicolor/512x512/apps/veil.png` | Application icon |
| `~/.local/share/applications/veil.desktop` | Launcher entry |

It needs an already-built release app, a core at
`vendor/sing-box-1.14.5-linux-amd64/sing-box` (or `VEIL_CORE_BIN`) and
`src-tauri/icons/icon.png`. It does not use root, does not touch the package
database, and never moves, copies, deletes or rewrites existing profile data.
Uninstall with `make uninstall-local`; profiles and settings are kept.

## Packaging

The Arch `PKGBUILD`, source tarball and their exact commands are documented in
[PACKAGING.md](PACKAGING.md). Summary:

```bash
make source-tarball
cp packaging/PKGBUILD packaging/veil-0.1.0.tar.gz /tmp/veil-pkg/
cd /tmp/veil-pkg
makepkg -s
```

The PKGBUILD is syntax-checked but has not been built or installed in the
verified matrix. AppImage/deb/rpm are **not built**. Do not describe packaging
output as tested until it is.

## Tests and checks

```bash
make test

# What CI runs
(cd src-tauri && cargo test --locked --release)
cargo test --locked --release --manifest-path src-tauri/helper/Cargo.toml

# Formatting, lint, types
(cd src-tauri && cargo fmt --all -- --check)
(cd src-tauri && cargo clippy --locked --all-targets --all-features -- -D warnings)
cargo fmt --manifest-path src-tauri/helper/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/helper/Cargo.toml --all-targets -- -D warnings
pnpm exec prettier --check .
pnpm exec eslint .
pnpm exec svelte-check --tsconfig ./tsconfig.json
```

Tests that need the real binary are skipped unless `VEIL_SINGBOX` points at
one:

```bash
(cd src-tauri && VEIL_SINGBOX=/path/to/sing-box cargo test --locked)
```

The app resolves a core from `$VEIL_SING_BOX`; tests and examples opt in with
`$VEIL_SINGBOX`. The names are intentionally different.

Real-engine harnesses need no root, TUN or display:

```bash
cd src-tauri
VEIL_SINGBOX=/path/to/sing-box cargo run --example engine_smoke

VEIL_SINGBOX=/path/to/sing-box \
  XDG_DATA_HOME=/tmp/veil-lifecycle-data \
  XDG_CONFIG_HOME=/tmp/veil-lifecycle-config \
  XDG_CURRENT_DESKTOP=Hyprland \
  cargo run --example engine_lifecycle
```

`engine_lifecycle` forces the unsupported desktop-proxy path so it never
mutates real desktop settings. The gated GNOME test does mutate them briefly and
restores the exact prior values; run it only on a machine you control:

```bash
cd src-tauri
VEIL_TEST_GSETTINGS=1 cargo test --lib \
  sysproxy::tests::gnome_apply_restore_roundtrip_when_enabled -- --nocapture
```

See [DEVELOPMENT.md](DEVELOPMENT.md) for the module map, debugging commands and
the subscription harness.

## Isolated test instance

To avoid touching normal profiles, settings or proxy state:

```bash
XDG_DATA_HOME=/tmp/veil-dev-data \
XDG_CONFIG_HOME=/tmp/veil-dev-config \
./src-tauri/target/release/veil
```

Each XDG root gets its own `veil/` directory. Point that instance at a core with
`VEIL_SING_BOX` or the Settings core path. Never start a second TUN while
another Veil engine or any other TUN/VPN is active; inspect
`runtime/runtime.json` and `pgrep` first.

## CI

`.github/workflows/quality.yml` runs the frontend and Rust checks/tests on
pushes and pull requests. `.github/workflows/build.yml` builds the release app
and helper and uploads them as workflow artifacts. CI does not run TUN, mutate a
real desktop proxy, or build packages; a green build is not evidence for those
paths.

If a build or run fails, check [troubleshooting.md](troubleshooting.md) before
changing code.