# Releasing Veil

This document describes a manual release. Veil currently has **no release
automation**: CI builds binaries and uploads them as workflow artifacts, but it
does not create tags, GitHub releases, packages, or AppImages. Every publishing
step below is performed by a maintainer and verified by hand.

Veil is Linux x86_64 software with a privileged helper and a bundled/fetched
sing-box core. A release is not just an app binary; it also includes the helper,
polkit policy, desktop entry, icon and the pinned core source.

## 1. Decide the version

Use [Semantic Versioning](https://semver.org/). While the project is `0.x`,
breaking changes may appear in minor versions; document them in the changelog.

Before bumping, confirm the tree is clean and `master` is the intended commit:

```bash
git status
git log --oneline -5
```

## 2. Bump the version everywhere

Keep these in sync. A missed location produces a binary, package or window that
reports the wrong version.

| File | Field |
| --- | --- |
| `package.json` | `"version"` |
| `src-tauri/Cargo.toml` | `[package] version` |
| `src-tauri/helper/Cargo.toml` | `[package] version` |
| `src-tauri/tauri.conf.json` | `"version"` |
| `packaging/PKGBUILD` | `pkgver`, and reset `pkgrel=1` |
| `packaging/.SRCINFO` | `pkgver`, `pkgrel` (regenerate in step 6) |
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

## 4. Clean build and test

Build from a clean tree, using locked dependencies:

```bash
rm -rf dist src-tauri/target src-tauri/helper/target

pnpm install --frozen-lockfile
pnpm build

cd src-tauri
cargo build --release --locked
cargo test --release --locked
cd ..

cargo build --release --locked --manifest-path src-tauri/helper/Cargo.toml
cargo test --release --locked --manifest-path src-tauri/helper/Cargo.toml
```

Run the static checks from CI:

```bash
cd src-tauri
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cd ..

cargo fmt --manifest-path src-tauri/helper/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/helper/Cargo.toml --all-targets -- -D warnings

pnpm exec prettier --check .
pnpm exec eslint .
pnpm exec svelte-check --tsconfig ./tsconfig.json
```

If a real sing-box 1.14.5 binary is available, also run the gated tests and the
real-engine harnesses:

```bash
cd src-tauri
VEIL_SINGBOX=/path/to/sing-box cargo test --release
VEIL_SINGBOX=/path/to/sing-box cargo run --example engine_smoke
```

Do not tag a release if the build or tests fail. Do not claim TUN, desktop
proxy on unlisted desktops, or AppImage packaging without evidence; record what
was not run.

Create the release binaries and checksums:

```bash
mkdir -p /tmp/veil-release
cp src-tauri/target/release/veil \
  "/tmp/veil-release/veil-<version>-linux-x86_64"
cp src-tauri/helper/target/release/veil-helper \
  "/tmp/veil-release/veil-helper-<version>-linux-x86_64"
cd /tmp/veil-release
sha256sum veil-<version>-linux-x86_64 veil-helper-<version>-linux-x86_64 \
  > SHA256SUMS
```

Keep the checksums with the release assets; users and packages can verify them
with `sha256sum -c SHA256SUMS`.

## 5. Commit and tag

```bash
git add package.json pnpm-lock.yaml \
  src-tauri/Cargo.toml src-tauri/Cargo.lock \
  src-tauri/helper/Cargo.toml src-tauri/helper/Cargo.lock \
  src-tauri/tauri.conf.json packaging/PKGBUILD packaging/.SRCINFO \
  CHANGELOG.md
git commit -m "chore(release): v<version>"
git tag -a "v<version>" -m "Veil <version>"
git push origin master
git push origin "v<version>"
```

Use an annotated tag. If a release is wrong, issue a new patch version; do not
move or delete a published tag or rewrite history.

## 6. GitHub release

Create the release from the pushed tag:

- Title: `Veil <version>`.
- Notes: copy the matching changelog section, including known limitations.
- Assets: `veil-<version>-linux-x86_64`,
  `veil-helper-<version>-linux-x86_64`, `SHA256SUMS`, and the Arch source
  tarball if you are distributing it.
- Do not attach the local `src-tauri/resources/sing-box` development copy. The
  core is fetched from the pinned official release, or installed separately by
  the user.

The repository is currently private; a GitHub release is only visible to people
with access. Do not describe it as a public download.

Binaries produced by `.github/workflows/build.yml` are CI artifacts, not a
release. Download and re-verify them before using them as release assets:

```bash
sha256sum veil veil-helper
./veil-helper check --core /path/to/sing-box
```

## 7. Arch package

The PKGBUILD is in `packaging/`. It builds the frontend, the app, the helper,
installs the polkit policy/desktop entry/icon, and fetches the pinned official
sing-box 1.14.5 release with a fixed SHA-256.

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

## 8. Helper and polkit packaging checks

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

## 9. AppImage, deb, rpm

Not built and not verified. `docs/PACKAGING.md` records the expected Tauri
AppImage command, its network prerequisites (`linuxdeploy`, potentially
`appimagetool`), and the missing core-staging step. Do not list these formats in
release notes until an actual build has been produced, launched and checked.
The helper and polkit policy cannot be delivered by an AppImage; TUN still
requires the installed helper.

## 10. Post-release

- Verify the tag, release assets and checksums from a different checkout.
- Record the exact verification performed (and what was not) in
  `docs/VERIFICATION.md`.
- Keep `Unreleased` open in the changelog for the next cycle.
