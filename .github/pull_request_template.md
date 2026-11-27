## Summary

<!-- What changed and why? One problem per pull request. -->

## Type

- [ ] Bug fix
- [ ] Feature
- [ ] Refactor
- [ ] Documentation
- [ ] CI / build
- [ ] Other

## Verification

- [ ] `pnpm check`
- [ ] `pnpm lint`
- [ ] `pnpm format:check`
- [ ] `pnpm build`
- [ ] `cargo fmt --all -- --check` (in `src-tauri`)
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings` (in `src-tauri`)
- [ ] `cargo test --locked --release` (in `src-tauri`)
- [ ] `cargo fmt --manifest-path src-tauri/helper/Cargo.toml -- --check`
- [ ] `cargo clippy --locked --manifest-path src-tauri/helper/Cargo.toml --all-targets -- -D warnings`
- [ ] `cargo test --locked --release --manifest-path src-tauri/helper/Cargo.toml`
- [ ] Manual test: <!-- describe what you ran and what you observed -->

If a check was not run, say so here and explain why. CI passing is not a substitute for runtime evidence.

## Risk

<!-- What can break? Call out engine lifecycle, proxy restore, privileged helper, migrations, or data loss. -->

## Linked issue

<!-- Fixes #... or Relates to #... -->

## Notes

- [ ] No secrets, real subscription URLs, credentials, database dumps, or personal logs are included.
- [ ] User-visible changes are recorded under `Unreleased` in `CHANGELOG.md`.
- [ ] `docs/CONTRACT.md` is updated when commands, events, or data shapes change.
