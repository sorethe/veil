# Packaging Veil

Target: Arch Linux, x86_64. Veil is a Tauri 2 app (Svelte frontend, Rust backend) plus a
privileged helper, `veil-helper`, that is started through `pkexec` for TUN mode.

## Files

| Path | Purpose |
| --- | --- |
| `packaging/PKGBUILD` | Arch package recipe (`veil`). |
| `packaging/make-source-tarball.sh` | Creates `packaging/veil-<pkgver>.tar.gz` from the working tree. |
| `packaging/.SRCINFO` | Generated from the PKGBUILD; regenerate after any PKGBUILD edit. |
| `install-helper.sh` | Development install of `/usr/bin/veil-helper`, polkit policy, desktop entry, icon. Not for pacman-managed systems. |
| `packaging/install-local.sh` | User-local app, core, icon, and desktop entry; no sudo. |
| `packaging/uninstall-local.sh` | Removes the user-local app, core, icon, and desktop entry; keeps profile data/settings. |
| `Makefile` | `make build`, `make test`, `make install-helper`, `make install-local`, `make uninstall-local`, `make source-tarball`, etc. |
| `src-tauri/resources/dev.veil.helper.policy` | Polkit action `dev.veil.helper`. |
| `src-tauri/resources/veil.desktop` | Desktop entry (`Exec=veil`, `Icon=veil`, `StartupWMClass=veil`). |
| `src-tauri/icons/icon.png` | 512x512 RGBA PNG installed as `hicolor/512x512/apps/veil.png`. |

## Build from source

Prerequisites (Arch): `rust` (provides cargo), `nodejs`, `pnpm`, `webkit2gtk-4.1`, `gtk3`,
`libsoup3`, `iproute2`, `nftables`, `polkit`. Optional at runtime: `zbar` (QR import via `zbarimg`),
`sing-box` (system core). The tray uses the pure-Rust `ksni` crate over D-Bus, so no appindicator
library is needed.

```sh
cd /home/deus/veil
make install                # pnpm install
make build                  # pnpm build; cargo build --release (app); cargo build --release (helper)
make test                   # cargo test (app and helper)
```

Outputs:

- `src-tauri/target/release/veil`
- `src-tauri/helper/target/release/veil-helper`

The release app embeds the built frontend (`custom-protocol` is a default cargo feature).

## User-local launcher (no sudo)

Build the release first, then create a per-user launcher. This installs only into the
current user's home and does not use root, modify a package database, or copy profiles.

```sh
make install-local
# equivalent:
./packaging/install-local.sh
```

Installed paths:

| Path | Contents |
| --- | --- |
| `~/.local/opt/veil/veil` | Release app binary. |
| `~/.local/opt/veil/sing-box` | Official sing-box 1.14.5 core beside the app. The exe-dir core search finds it without setting `corePath`. |
| `~/.local/share/icons/hicolor/512x512/apps/veil.png` | 512x512 RGBA application icon. |
| `~/.local/share/applications/veil.desktop` | Application launcher entry. |

The launcher entry contains:

```ini
[Desktop Entry]
Type=Application
Version=1.0
Name=Veil
Comment=Linux desktop proxy client powered by sing-box
Exec=/home/deus/.local/opt/veil/veil
TryExec=/home/deus/.local/opt/veil/veil
Icon=veil
Terminal=false
Categories=Network;Utility;Security;
StartupNotify=true
StartupWMClass=veil
```

Validate and refresh the desktop database:

```sh
update-desktop-database ~/.local/share/applications
desktop-file-validate ~/.local/share/applications/veil.desktop
gio info ~/.local/share/applications/veil.desktop
~/.local/opt/veil/sing-box version
```

`desktop-file-validate` is expected to pass with hints for the intentionally multi-category
`Categories` value. Launch Veil from the desktop application launcher, with `gtk-launch veil`,
or directly with `~/.local/opt/veil/veil`.

Normal launches use the user's default XDG locations, `~/.local/share/veil` (database/logs)
and `~/.config/veil` (settings). They do **not** read the isolated `/tmp/veil-app-data` or
`/tmp/veil-app-config` directories. The launcher install **preserves existing profile data**:
it never moves, deletes, copies, or resets `~/.local/share/veil/veil.db` or
`~/.config/veil/settings.json`, and no subscription is copied from test data. Existing profiles
remain visible after installing. Overview shows **First run / Import profile** only when the
default database genuinely has no profiles. Close any existing Veil/test instance before
launching the installed entry.

Uninstall the launcher and binaries without deleting user data:

```sh
make uninstall-local
# equivalent:
./packaging/uninstall-local.sh
```

This removes `~/.local/opt/veil/veil`, `~/.local/opt/veil/sing-box`, the icon, and
`veil.desktop`. It leaves `~/.local/share/veil` (profiles and logs) and `~/.config/veil`
(settings) intact.

## Arch package (PKGBUILD)

The PKGBUILD takes two sources:

1. `veil-<pkgver>.tar.gz`, a local tarball placed next to the PKGBUILD. Its checksum is
   `SKIP` because it is built from the working tree and changes on every edit. Pin it once a
   tagged release is published.
2. The official sing-box v1.14.5 `linux-amd64` release tarball from GitHub, pinned with
   `sha256 ca57e405be6b7dc79a0efa91823a0cc4d81138cbe4a9aacbfd749c65e6fca3b2` (matches the GitHub
   release asset digest and `vendor/sb.tar.gz`).

Steps (use a scratch directory, because makepkg writes `src/` and `pkg/` beside the PKGBUILD):

```sh
cd /home/deus/veil
make source-tarball                   # writes packaging/veil-0.1.0.tar.gz and prints its sha256
mkdir -p /tmp/veil-pkg
cp packaging/PKGBUILD packaging/veil-0.1.0.tar.gz /tmp/veil-pkg/
cd /tmp/veil-pkg
makepkg --printsrcinfo > .SRCINFO     # no network needed
makepkg -s                            # builds the package; -s installs makedepends via sudo
```

`makepkg -s` installs missing build dependencies with pacman and therefore needs an interactive
sudo password. Use `makepkg` without `-s` if the dependencies are already installed.
`makepkg -i` (installing the built package) is the step that needs root; it is not
verified here.

`prepare()` runs `pnpm install --frozen-lockfile` and `cargo fetch --locked` (network). `build()`
and `package()` run offline after that.

Installed paths:

| Path | Source |
| --- | --- |
| `/usr/bin/veil` | `src-tauri/target/release/veil` |
| `/usr/bin/veil-helper` | `src-tauri/helper/target/release/veil-helper` |
| `/usr/lib/veil/sing-box`, `libcronet.so` | official sing-box 1.14.5 tarball |
| `/usr/share/polkit-1/actions/dev.veil.helper.policy` | `src-tauri/resources/dev.veil.helper.policy` |
| `/usr/share/applications/veil.desktop` | `src-tauri/resources/veil.desktop` |
| `/usr/share/icons/hicolor/512x512/apps/veil.png` | `src-tauri/icons/icon.png` |
| `/usr/share/licenses/veil/sing-box-LICENSE` | sing-box tarball `LICENSE` |

The app looks for the core in this order: settings, `$VEIL_SING_BOX`, `which sing-box`, beside the
executable, resource dir, `/usr/lib/veil/sing-box`, dev vendor build. A `sing-box` found on `$PATH`
is therefore preferred over the bundled `/usr/lib/veil/sing-box`; the bundled core is used when no
other core is found or configured.

## Helper and polkit

- The app runs `pkexec /usr/bin/veil-helper start|stop ...` (`src-tauri/src/engine.rs`). The GUI
  never runs as root.
- The polkit action id is `dev.veil.helper`, and its `org.freedesktop.policykit.exec.path`
  annotation is `/usr/bin/veil-helper`. This matches the path the app checks in `diagnostics.rs`.
- `allow_active` is `auth_admin_keep`: an active-session user must authenticate as admin, and
  that authorization is kept briefly. `allow_any` and `allow_inactive` are `no`.
- The helper has no shell code path; it runs with explicit argv.
- `install-helper.sh` installs the same files for development outside pacman. Do not run it on a
  system where the `veil` package is installed (the paths conflict and nothing owns them).
- Check the policy with `pkaction --verbose --action-id dev.veil.helper`.

## AppImage

Status: **not built and not verified.**

Tauri's AppImage bundler is available (`@tauri-apps/cli` 2.12.6 is installed through pnpm). The
expected command is:

```sh
pnpm tauri build --bundles appimage
```

Likely prerequisites:

- Tauri downloads `linuxdeploy` (and its GTK plugin) from GitHub on first use. Network access is
  required, and the download is not pinned by this repository.
- `appimagetool` may be needed. Neither `linuxdeploy` nor `appimagetool` was found on `PATH`.
- A full release build (`pnpm tauri build`) of the app; this was not run.

The bundled core for an AppImage is not produced by this repository workflow: the local
`src-tauri/resources/sing-box` is git-ignored and excluded from the source tarball, and the
AppImage builder was not run. An AppImage build would need an explicit step to stage the
official core. The helper and polkit policy are not part of the AppImage; TUN mode still needs
the installed helper.

## Limitations

- **TUN mode and root are unverified in this packaging pass.** The helper install path, polkit
  action and `pkexec` call were checked by reading code only. No `pkexec` authentication, no
  `/dev/net/tun` access and no nftables rules were exercised here.
- `src-tauri/resources/sing-box` is **not tracked** and is ignored by `.gitignore`; a local copy
  may remain for development. The PKGBUILD and `make-source-tarball.sh` use the official pinned
  upstream core instead. The current repository history does not contain the local core binary;
  only `/usr/lib/veil/sing-box` from the package is a supported bundled-core location.
- `vendor/`, `src-tauri/target/`, `src-tauri/helper/target/`, `node_modules/`, `dist/`, and
  `packaging/{src,pkg,*.tar.gz}` are git-ignored. The source tarball is built from the working
  tree with those excluded.
- `LICENSE` is the standard MIT text (2026 Veil contributors), matching `license=('MIT')`; the
  PKGBUILD installs it to `/usr/share/licenses/veil/LICENSE`.
- `url="https://example.invalid/veil"` is a deliberate placeholder (reserved `.invalid` domain),
  not a verified project URL. The polkit `vendor_url="https://veil.dev"` is likewise UNVERIFIED.
- `StartupWMClass=veil` matches the window class observed for the running app (`hyprctl clients`
  reports class `veil`).
- `desktop-file-validate` passes with hints only: the `Categories` value lists more than one main
  category.
- `src-tauri/resources/veil-helper.policy` duplicates `dev.veil.helper.policy`. It is
  intentionally left in place: `diagnostics.rs` checks the legacy resource name when deciding
  whether a shipped policy exists, and also accepts the legacy installed filename. Packaging
  installs only `dev.veil.helper.policy`. Removing the duplicate would require changing that
  diagnostics fallback first.
- No AppImage, no `namcap` lint, and no `shellcheck` were run (not installed).
- `makepkg`/PKGBUILD was **syntax-checked only** (`bash -n`, `makepkg --printsrcinfo`); the
  package was **not built** and not installed. The official sing-box tarball checksum was
  verified separately (`sha256 ca57e405...`). AppImage was **not built**.

## Repository hygiene

- `src-tauri/resources/sing-box` is not tracked and is ignored by `.gitignore`; the current
  history does not contain the binary. The local file is retained only for development builds.
- `packaging/src/`, `packaging/pkg/`, and `packaging/*.tar.gz` are ignored makepkg scratch/output.
- The only external core source in packaging is the official sing-box v1.14.5 release archive at
  `https://github.com/SagerNet/sing-box/releases/download/v1.14.5/sing-box-1.14.5-linux-amd64.tar.gz`
  with the pinned SHA-256 above.

For the full release flow (version updates, tagging, GitHub release assets, PKGBUILD and
checksums), see [RELEASING.md](RELEASING.md).
