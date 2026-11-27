# Third-party notices

Veil itself is distributed under the MIT license; see [LICENSE](LICENSE). This
file summarizes the direct dependencies declared in the repository and the
license strings reported by their package metadata. It is not a complete
transitive license list. Generate a full list from the committed lockfiles when
you need one, rather than copying a hand-written inventory.

## Rust crates (direct dependencies)

### `src-tauri/Cargo.toml`

| Crate                            | License           |
| -------------------------------- | ----------------- |
| `tauri`                          | Apache-2.0 OR MIT |
| `tauri-build` (build dependency) | Apache-2.0 OR MIT |
| `serde`                          | MIT OR Apache-2.0 |
| `serde_json`                     | MIT OR Apache-2.0 |
| `tokio`                          | MIT               |
| `rusqlite`                       | MIT               |
| `reqwest`                        | MIT OR Apache-2.0 |
| `uuid`                           | Apache-2.0 OR MIT |
| `base64`                         | MIT OR Apache-2.0 |
| `url`                            | MIT OR Apache-2.0 |
| `rand`                           | MIT OR Apache-2.0 |
| `chrono`                         | MIT OR Apache-2.0 |
| `dirs`                           | MIT OR Apache-2.0 |
| `thiserror`                      | MIT OR Apache-2.0 |
| `regex`                          | MIT OR Apache-2.0 |
| `libc`                           | MIT OR Apache-2.0 |
| `notify-rust`                    | MIT OR Apache-2.0 |
| `ksni`                           | Unlicense         |
| `futures-util`                   | MIT OR Apache-2.0 |
| `percent-encoding`               | MIT OR Apache-2.0 |

### `src-tauri/helper/Cargo.toml`

| Crate        | License           |
| ------------ | ----------------- |
| `serde`      | MIT OR Apache-2.0 |
| `serde_json` | MIT OR Apache-2.0 |
| `libc`       | MIT OR Apache-2.0 |

## npm packages (direct dependencies)

### Runtime dependencies

| Package                     | License           |
| --------------------------- | ----------------- |
| `@fontsource/ibm-plex-mono` | OFL-1.1           |
| `@fontsource/inter`         | OFL-1.1           |
| `@fontsource/space-grotesk` | OFL-1.1           |
| `@tauri-apps/api`           | Apache-2.0 OR MIT |

### Development dependencies

| Package                        | License           |
| ------------------------------ | ----------------- |
| `@eslint/js`                   | MIT               |
| `@sveltejs/vite-plugin-svelte` | MIT               |
| `@tauri-apps/cli`              | Apache-2.0 OR MIT |
| `eslint`                       | MIT               |
| `eslint-plugin-svelte`         | MIT               |
| `globals`                      | MIT               |
| `prettier`                     | MIT               |
| `prettier-plugin-svelte`       | MIT               |
| `svelte`                       | MIT               |
| `svelte-check`                 | MIT               |
| `svelte-eslint-parser`         | MIT               |
| `typescript`                   | Apache-2.0        |
| `typescript-eslint`            | MIT               |
| `vite`                         | MIT               |

## Bundled assets and core

- **sing-box core:** the packaged core is the official upstream sing-box 1.14.5
  Linux amd64 release. sing-box is distributed under the GPL-3.0-or-later. The
  Arch PKGBUILD installs the upstream `LICENSE` as
  `/usr/share/licenses/veil/sing-box-LICENSE`. The core is not stored in this
  repository; packaging fetches the pinned official release.
- **Fonts:** the `@fontsource` packages ship IBM Plex Mono, Inter, and Space
  Grotesk font files under the SIL Open Font License 1.1 (OFL-1.1).
- **Application icons:** the icons under `src-tauri/icons/` are project assets
  distributed under the repository's MIT license unless a file states otherwise.
- **Tauri runtime:** Tauri's Rust and npm components are direct dependencies and
  are covered by the licenses above. Tauri's transitive dependencies are not
  listed here.

## Transitive dependencies

This summary covers direct dependencies only. It does not enumerate transitive
crates or npm packages. Generate a complete license report from the committed
`src-tauri/Cargo.lock`, `src-tauri/helper/Cargo.lock`, and `pnpm-lock.yaml` using
a license tool (for example `cargo-about`, `cargo-license`, or a pnpm license
report) rather than extending this table by hand.

## Verification note

The license strings above were read from the installed package metadata and the
resolved crate metadata when this file was written. Dependency updates can change
licenses; re-check the relevant metadata when dependencies change.
