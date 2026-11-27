# Releasing Veil

Veil is Linux x86_64 software with a privileged helper and a bundled/fetched
sing-box core. Releases are cut from `master` with an annotated `v<version>`
tag. The tag triggers `.github/workflows/release.yml`, which validates the
version, runs the checks, builds the raw release binaries, uploads the binaries
and configuration files, and creates a GitHub Release.

The workflow does **not** build Tauri bundles (deb/AppImage/rpm), publish to a
package manager, sign binaries, or produce macOS/Windows artifacts. Those paths
are not built or verified; see [PACKAGING.md](PACKAGING.md) and the limitations
below.

## 1. Prepare the release

Confirm the tree is clean and `master` is the intended commit:

```bash
git switch master
git pull --ff-only
git status
git log --oneline -5
```

Use [Semantic Versioning](https://semver.org/). While the project is `0.x`,
breaking changes may appear in minor versions; document them in the changelog.

## 2. Bump the version everywhere

Keep these in sync. The release workflow validates `package.json`,
`src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and
`src-tauri/helper/Cargo.toml` against the tag. The PKGBUILD and `.SRCINFO` are
checked manually.

| File | Field |
| --- | --- |
| `package.json` | `"version"` |
| `src-tauri/Cargo.toml` | `[package] version` |
| `src-tauri/helper/Cargo.toml` | `[package] version` |
| `src-tauri/tauri.conf.json` | `"version"` |
| `packaging/PKGBUILD` | `pkgver`, and reset `pkgrel=1` |
| `packaging/.SRCINFO` | `pkgver`, `pkgrel` (regenerate in step 7) |
| `CHANGELOG.md` | Move `Unreleased` entries under the new version and date |

Refresh the lockfiles after changing local package versions, then verify they
are stable under `--locked`:

```bash
cd src-tauri
cargo metadata --format-version 1 > /dev/null
cd ..
cargo metadata --format-version 1 --manifest-path src-tauri/helper/Cargo.toml > /dev/null
git diff -- src-tauri/Cargo.lock src-tauri/helper/Cargo.lock
```

`Cargo.lock` entries for `veil` and `veil-helper` must show the new version.
`pnpm-lock.yaml` does not carry the root package version, but keep it committed
and unchanged unless dependencies actually changed.

Do not edit dependency versions opportunistically in a release commit.

## 3. Update the changelog

In `CHANGELOG.md`:

1. Move the user-visible items from `## [Unreleased]` into a new
   `## [<version>] - YYYY-MM-DD` section.
2. Leave `Unreleased` in place with empty subsections for future work.
3. Keep the known limitations honest. Do not turn unverified behavior into a
   feature claim for the release.

## 4. Run the release checks locally

These are the same checks the release workflow runs. Do not tag a release if
they fail.

```bash
pnpm install --frozen-lockfile
pnpm check
pnpm lint
pnpm format:check
pnpm build

(cd src-tauri && cargo fmt --all -- --check)
(cd src-tauri && cargo clippy --locked --all-targets --all-features -- -D warnings)
(cd src-tauri && cargo test --locked --release)

cargo fmt --manifest-path src-tauri/helper/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/helper/Cargo.toml --all-targets -- -D warnings
cargo test --locked --release --manifest-path src-tauri/helper/Cargo.toml
```

If a real sing-box 1.14.5 binary is available, also run the gated tests and the
real-engine harnesses:

```bash
cd src-tauri
VEIL_SINGBOX=/path/to/sing-box cargo test --release
VEIL_SINGBOX=/path/to/sing-box cargo run --example engine_smoke
```

Do not claim TUN, desktop proxy on unlisted desktops, or AppImage/deb/rpm
packaging without evidence. Record what was not run.

## 5. Commit and tag

```bash
git add package.json pnpm-lock.yaml \
  src-tauri/Cargo.toml src-tauri/Cargo.lock \
  src-tauri/helper/Cargo.toml src-tauri/helper/Cargo.lock \
  src-tauri/tauri.conf.json packaging/PKGBUILD packaging/.SRCINFO \
  CHANGELOG.md
git commit -m "chore(release): v<version>"
git push origin master
git tag -a "v<version>" -m "Veil <version>"
git push origin "v<version>"
```

Use an annotated tag. If a release is wrong, issue a new patch version; do not
move or delete a published tag or rewrite history.

## 6. Automated release workflow

The pushed tag triggers `.github/workflows/release.yml`. It:

1. Validates that `package.json`, `src-tauri/tauri.conf.json`,
   `src-tauri/Cargo.toml`, and `src-tauri/helper/Cargo.toml` all report the
   tag's version. A mismatch fails the run.
2. Installs the Tauri system dependencies and runs the checks from step 4.
3. Builds the frontend and both release binaries with the committed lockfiles.
4. Packages the raw binaries and configuration files:
   - `veil-<version>-linux-x86_64`
   - `veil-helper-<version>-linux-x86_64`
   - `veil-<version>-linux-x86_64.tar.gz` (binaries, polkit policy, desktop
     entry, icon, and `LICENSE`)
   - `dev.veil.helper.policy`
   - `veil.desktop`
   - `veil.png`
   - `LICENSE`
   - `SHA256SUMS`
5. Uploads the files as a workflow artifact and creates a GitHub Release from
   the tag. It uses only `GITHUB_TOKEN`; no signing credentials are configured.

The release is created from the tag. It is not a package-manager publication.
The repository is private; a GitHub Release is visible only to people with
access. Do not describe it as a public download.

Binaries produced by `.github/workflows/build.yml` are CI artifacts, not a
release. Download and re-verify them before using them as release assets:

```bash
sha256sum veil veil-helper
./veil-helper check --core /path/to/sing-box
```

## 7. Manual fallback

If the workflow is unavailable, run the same checks and build the same assets
by hand:

```bash
pnpm install --frozen-lockfile
pnpm check
pnpm lint
pnpm format:check
pnpm build

(cd src-tauri && cargo build --locked --release)
cargo build --locked --release --manifest-path src-tauri/helper/Cargo.toml

mkdir -p /tmp/veil-release
cp src-tauri/target/release/veil \
  "/tmp/veil-release/veil-<version>-linux-x86_64"
cp src-tauri/helper/target/release/veil-helper \
  "/tmp/veil-release/veil-helper-<version>-linux-x86_64"
cp src-tauri/resources/dev.veil.helper.policy \
  src-tauri/resources/veil.desktop \
  src-tauri/icons/icon.png \
  LICENSE /tmp/veil-release/
cd /tmp/veil-release
mv icon.png veil.png
tar -czf "veil-<version>-linux-x86_64.tar.gz" \
  "veil-<version>-linux-x86_64" \
  "veil-helper-<version>-linux-x86_64" \
  dev.veil.helper.policy veil.desktop veil.png LICENSE
sha256sum \
  "veil-<version>-linux-x86_64" \
  "veil-helper-<version>-linux-x86_64" \
  "veil-<version>-linux-x86_64.tar.gz" \
  dev.veil.helper.policy veil.desktop veil.png LICENSE > SHA256SUMS
```

Create the release from the pushed tag:

```bash
gh release create "v<version>" \
  --title "Veil <version>" \
  --notes-file /path/to/release-notes.md \
  /tmp/veil-release/*
```

Do not attach the local `src-tauri/resources/sing-box` development copy. The
core is fetched from the pinned official release, or installed separately by the
user. Do not attach bundler output unless a real bundle was produced and checked.

## 8. Arch package

The PKGBUILD is in `packaging/`. It builds the frontend, the app, the helper,
installs the polkit policy/desktop entry/icon, and fetches the pinned official
sing-box 1.14.5 release with a fixed SHA-256. The release workflow does not build
the Arch package; that remains a manual maintainer step.

For a release:

1. Set `pkgver` to the release version and `pkgrel=1`.
2. Regenerate the metadata:

   ```bash
   cd packaging
   makepkg --printsrcinfo > .SRCINFO
   ```

3. Create the source tarball from the release working tree:

   ```bash
   ./packaging/make-source-tarball.sh
   ```

   The script packages tracked and untracked non-ignored files from the working
   tree, so run it after committing the release. It writes
   `packaging/veil-<version>.tar.gz` and prints its SHA-256. The PKGBUILD
   currently uses `SKIP` for this local tarball because it changes with the
   working tree. Once the tarball is final, pin its checksum by replacing
   `SKIP` with the printed value and rebuilding metadata. A tagged GitHub
   archive URL is not wired into the PKGBUILD yet; do not claim one.

4. Build in a scratch directory (makepkg writes `src/` and `pkg/` beside the
   PKGBUILD):

   ```bash
   mkdir -p /tmp/veil-pkg
   cp packaging/PKGBUILD packaging/veil-<version>.tar.gz /tmp/veil-pkg/
   cd /tmp/veil-pkg
   makepkg --printsrcinfo > .SRCINFO
   makepkg -s
   ```

   `makepkg -s` installs missing build dependencies through sudo. Install the
   built package only if you intend to test it (`makepkg -i`); the packaging
   pass has not verified an install.

5. Verify the installed file list and paths:

   ```bash
   pacman -Qlp veil-<version>-1-x86_64.pkg.tar.zst
   pkaction --verbose --action-id dev.veil.helper
   sing-box version   # or /usr/lib/veil/sing-box version
   ```

The package installs the helper to `/usr/bin/veil-helper`, matching the polkit
action's `org.freedesktop.policykit.exec.path`. Do not change one without the
other.

## 9. Helper and polkit packaging checks

Regardless of distribution method:

- `veil-helper` is the only root component. The GUI must never be launched with
  root.
- The polkit action id is `dev.veil.helper`; `allow_active` is
  `auth_admin_keep`, `allow_any`/`allow_inactive` are `no`.
- There is no passwordless sudoers rule. Do not add one for packaging
  convenience.
- The helper runs direct argv (no shell), validates configs with
  `sing-box check` before spawning, and only writes the pid/log files it was
  given.
- `install-helper.sh` is a development installer. Do not run it on a system
  where the `veil` package is installed; the paths conflict and no package owns
  the files.

## 10. Bundles and limitations

- **No deb, AppImage, or rpm is built or verified.** The release workflow uses
  raw binaries and configuration files. `docs/PACKAGING.md` records the expected
  Tauri bundle commands and their open questions. Do not list these formats in
  release notes until an actual build has been produced, launched and checked.
- The helper and polkit policy cannot be delivered by an AppImage; TUN still
  requires the installed helper.
- **No code signing.** The release assets are unsigned.
- **No package-manager publishing.** The workflow does not publish to AUR,
  Flathub, or a distribution repository.
- **No Windows or macOS artifacts.** Linux x86_64 is the only supported release
  target.
- **TUN end-to-end is verified on the maintainer's Arch/Hyprland host
  (2026-11-27)** with real `pkexec` approval, `veil0`, `gvisor`,
  `auto_detect_interface`, and DNS/TCP/UDP. The helper, polkit policy and
  generated TUN configs are validated. Users still need `/dev/net/tun`,
  nftables, an installed helper and the `dev.veil.helper` polkit action; other
  hosts are untested. `auto_redirect` runtime behavior is not end-to-end
  verified. This is not a guarantee for every host.

## 11. Post-release

- Verify the tag, release assets and checksums from a different checkout.
- Record the exact verification performed (and what was not) in
  `docs/VERIFICATION.md`.
- Keep `Unreleased` open in the changelog for the next cycle.
