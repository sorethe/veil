# Contributing to Veil

Veil is a Linux proxy client with a Rust/Tauri backend and a Svelte 5
frontend. Contributions are welcome, but this project values honest verification
over optimistic claims: a change that cannot be tested should say so.

## Before you start

- Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
  [docs/CONTRACT.md](docs/CONTRACT.md). The contract is the coordination
  artifact between the Rust and TypeScript sides.
- Build steps, feature status and failure modes are in
  [docs/building.md](docs/building.md), [docs/features.md](docs/features.md) and
  [docs/troubleshooting.md](docs/troubleshooting.md). The security model is in
  [docs/security.md](docs/security.md).
- The frontend talks to the backend only through the documented commands and
  events. Do not add duplicate state or engine logic in the UI, and do not make
  the backend report a state it has not observed.
- Do not commit real subscription URLs, proxy credentials, profile dumps,
  generated runtime configs, or logs. See [SECURITY.md](SECURITY.md).

## Development environment

### Arch Linux

```bash
sudo pacman -S --needed \
  base-devel rust nodejs pnpm \
  webkit2gtk-4.1 gtk3 libsoup3 \
  nftables iproute2 polkit dbus \
  zbar
# Optional: a system sing-box core
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

Install Rust through [rustup](https://rustup.rs/), Node.js 22.13 or newer, and
pnpm 10 or newer. `libayatana-appindicator` is not needed: the tray uses `ksni`
over D-Bus StatusNotifierItem.

### Versions

| Tool | Minimum | Notes |
| --- | --- | --- |
| Rust | 1.80 | `src-tauri/Cargo.toml`; helper declares 1.77 |
| Node.js | 22.13 | ESLint 10 requires 22.13+; CI uses Node 22; local development used 26 |
| pnpm | 10 | Lockfile is `lockfileVersion: 9.0`; local development used 12 |
| sing-box | 1.14.5 | Validated core. Other versions are unverified |

CI pins Ubuntu 24.04, Node 22, Rust stable (with `rustfmt` and `clippy`) and the
pnpm version in the workflow.

## Build and run

```bash
pnpm install
make build                 # frontend + release app + release helper
make test                  # backend and helper tests

pnpm tauri dev             # dev server on 127.0.0.1:1420 + Tauri window
```

Backend-only build that loads the dev server instead of embedded assets:

```bash
cd src-tauri
cargo build --no-default-features
```

Release binaries are written to `src-tauri/target/release/veil` and
`src-tauri/helper/target/release/veil-helper`.

## Formatting and linting

Run these before opening a pull request. They are the same commands CI uses.
`pnpm exec prettier --check .` and `pnpm exec eslint .` are the concrete
commands behind the `format:check` and `lint` package scripts.

```bash
# Backend (main crate)
(cd src-tauri && cargo fmt --all -- --check)
(cd src-tauri && cargo clippy --locked --all-targets --all-features -- -D warnings)

# Privileged helper (separate manifest, from the repository root)
cargo fmt --manifest-path src-tauri/helper/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/helper/Cargo.toml --all-targets -- -D warnings

# Frontend (repository root)
pnpm exec prettier --check .
pnpm exec eslint .
pnpm exec svelte-check --tsconfig ./tsconfig.json
pnpm build
```

`cargo fmt --all` edits the main crate; add `-- --check` to verify without
writing. The helper is not a member of the main workspace and needs its own
`--manifest-path` invocation. If a change is purely documentation, run at least
Prettier and the relevant Rust checks for touched code.

## Testing

### Unit and integration tests

```bash
(cd src-tauri && cargo test --locked)
(cd src-tauri && cargo test --locked --release)   # what CI runs
cargo test --locked --manifest-path src-tauri/helper/Cargo.toml
```

Tests that need a real `sing-box` binary are skipped unless
`VEIL_SINGBOX` points at one:

```bash
(cd src-tauri && VEIL_SINGBOX=/path/to/sing-box cargo test --locked)
```

Note the variable names: the **app** resolves a core from `$VEIL_SING_BOX`, while
**tests and examples** opt in with `$VEIL_SINGBOX`. A vendored development core
may exist at `vendor/sing-box-1.14.5-linux-amd64/sing-box`; it is git-ignored.

### Real-engine harnesses

These start the actual `sing-box` process and need no root, TUN or display:

```bash
cd src-tauri
VEIL_SINGBOX=/path/to/sing-box cargo run --example engine_smoke

VEIL_SINGBOX=/path/to/sing-box \
  XDG_DATA_HOME=/tmp/veil-lifecycle-data \
  XDG_CONFIG_HOME=/tmp/veil-lifecycle-config \
  XDG_CURRENT_DESKTOP=Hyprland \
  cargo run --example engine_lifecycle
```

`engine_lifecycle` deliberately forces the unsupported desktop-proxy path so it
never modifies your real proxy settings.

### Desktop proxy test (opt-in, mutates real settings briefly)

`sysproxy` tests that touch the real desktop are gated and snapshot/restore the
exact previous values, including on assertion failure. Run them only on a
machine you control:

```bash
cd src-tauri
VEIL_TEST_GSETTINGS=1 cargo test --lib \
  sysproxy::tests::gnome_apply_restore_roundtrip_when_enabled -- --nocapture
```

### Subscription harness

`examples/subscription_integration.rs` reads a subscription URL from
`$VEIL_SUB_FILE` (default `/tmp/veil-sub.url`) and never prints it. Keep that
file outside the repository, mode `0600`. Do not commit its contents or any
resulting profile.

### TUN

TUN requires root/polkit and `/dev/net/tun`; there is no passwordless sudoers
rule. Do not weaken the polkit policy to make tests pass. If you cannot test TUN,
say so in the pull request instead of claiming success. One end-to-end PASS is
recorded (2026-11-27, `gvisor` stack, `auto_detect_interface`,
`pkexec`-approved helper), but it is host-specific: do not generalize it to
other kernels, distributions or conflicting tunnels. See
[docs/VERIFICATION.md](docs/VERIFICATION.md#tun-end-to-end--pass-2026-11-27).

## Documentation

- Keep the README as the entry point and link new guides from
  [README.md](README.md#documentation).
- Use the repository's status vocabulary consistently: **verified /
  integration-tested**, **tested**, **implemented**, **implemented-unverified**,
  **best-effort**, **unsupported**, **absent**. Do not promote a planned or
  untested behavior to a finished feature.
- A change that affects engine behavior, permissions, packaging or platform
  support must update [docs/VERIFICATION.md](docs/VERIFICATION.md) or state
  explicitly what remains unverified. Feature-status changes belong in
  [docs/features.md](docs/features.md).
- Markdown under `docs/`, the root README, `CONTRIBUTING.md` and `SECURITY.md`
  is currently excluded from `prettier --check` by `.prettierignore`. Check
  links, tables and rendering manually; do not rely on the frontend format gate
  for prose.
- Do not invent screenshots, badges, download links, or release claims.

## Code conventions

- Rust: follow `rustfmt`; keep command errors in the typed `CommandError`
  envelope; preserve original error messages.
- TypeScript/Svelte: follow the Prettier and ESLint configuration in the
  repository. Svelte 5 runes are used in stores and components.
- Keep raw profile content untouched. Runtime config generation may inject only
  the documented integration points (`experimental.clash_api`, log level,
  `veil-mixed`, and `veil-tun` in TUN mode).
- Do not add fake fallbacks, invented engine state, or "best effort" success
  paths that hide failures.

## Branch and commit conventions

This repository uses trunk-based development:

- `master` is the integration branch and should stay buildable.
- Work on short-lived branches such as `feat/outbound-filter` or
  `fix/proxy-restore`. Rebase or merge `master` as needed; keep the branch
  focused.
- Open a pull request into `master`. Tags of the form `vX.Y.Z` mark releases.
- Do not rewrite published history. Existing commits use their original
  messages and are not retroactively reformatted.

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <short imperative summary>

<optional body explaining what and why>

<optional footer>
```

Common types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`. Examples drawn from this repository's style:

```
feat(ui): add shared outbound browser
fix(engine): finish runtime cleanup when disconnect finds an idle engine
docs: record TUN acceptance blocked by a concurrent TUN
chore(packaging): trim unused runtime dependencies
test(links): validate generated config with the real core
```

Keep the subject under ~72 characters and imperative. Explain verification in
the body when a change affects the engine, proxy restore, or privileged helper.

## Pull requests

A good pull request:

- solves one problem and does not mix unrelated refactors;
- includes tests for behavior changes (or explains why a test is not possible);
- updates [docs/CONTRACT.md](docs/CONTRACT.md) when commands, events, or data
  shapes change, [docs/VERIFICATION.md](docs/VERIFICATION.md)/
  [docs/features.md](docs/features.md) when behavior or status claims change,
  and [CHANGELOG.md](CHANGELOG.md) under `Unreleased` for user-visible changes;
- contains no secrets, real subscription URLs, credentials, database dumps, or
  personal logs;
- states what was actually run and what remains unverified. CI passing is not a
  substitute for runtime evidence.

CI runs the quality workflow on pushes and pull requests to `master` and `dev`,
and the build workflow produces release binaries as artifacts. TUN behavior,
desktop proxy behavior on unlisted desktops, and AppImage packaging are not
exercised by CI.
