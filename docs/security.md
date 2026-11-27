# Veil security model

This page describes how Veil separates privileges, what runs where, and what it
does and does not protect. For reporting a vulnerability, follow the root
[SECURITY.md](../SECURITY.md); do not open a public issue. That file also lists
the supported versions: only the current `0.1.x` pre-release line, with no
long-term support commitment.

## Trust boundary

```
Svelte UI (user session, no privileges)
  |  invoke() commands + engine:// events
  v
veil backend (user session, no privileges)
  |-- sing-box run (system mode, same user, loopback listeners)
  |-- desktop proxy apply/restore (same user)
  `-- pkexec --> /usr/bin/veil-helper (root, TUN only)
                    `-- sing-box run (root, TUN, detached)
```

The backend and UI are ordinary user processes. Only `veil-helper` is elevated,
and only when the user starts or stops TUN mode through the
`dev.veil.helper` polkit action.

## The GUI never runs as root

- Veil does not install a setuid GUI, does not run `sudo`, and has no shell
  invocation path.
- TUN start/stop is `pkexec /usr/bin/veil-helper start|stop ...` with direct
  argv. The GUI refuses TUN when the helper, polkit policy, `pkexec`,
  `/dev/net/tun` or nftables is missing.
- The helper runs as root only long enough to validate and spawn the detached
  engine, stop it, or install itself and its policy. It does not keep a root
  process for the GUI's lifetime.

## Helper and polkit

The helper is a small, separate crate (`src-tauri/helper/`) with three runtime
dependencies. Its invariants:

- **Direct argv only.** No shell, no string concatenation into a command line.
- **Root check on privileged actions.** `start`, `stop` and `install` verify
  `geteuid() == 0` and fail otherwise. The read-only diagnostics `check` and
  `status` run unprivileged.
- **Config validation before start.** It runs the real core as root with
  `sing-box check -c <config>` and aborts on failure.
- **Safe process identity.** `stop`/`status` read the pid file, verify the pid
  is alive, and require `/proc/<pid>/cmdline` to contain `sing-box` and, where
  recorded, the exact config path. It refuses to kill an unrelated process.
- **No pid-file clobbering.** A pid file pointing at a different live
  sing-box is refused rather than overwritten.
- **Graceful stop.** `SIGTERM`, then `SIGKILL` after 8 s, then a failure report
  if the process remains.
- **Detach with `setsid`.** The engine survives the short-lived root helper
  process.
- **Bounded writes.** The helper reads the config it was given and writes only
  the pid/log paths it was given, plus its own install targets under
  `/usr/bin` and `/usr/share/polkit-1/actions`.

The polkit action `dev.veil.helper`:

| Setting | Value |
| --- | --- |
| Vendor | Veil |
| Exec path annotation | `/usr/bin/veil-helper` |
| `allow_any` | `no` |
| `allow_inactive` | `no` |
| `allow_active` | `auth_admin_keep` |

`auth_admin_keep` requires an active-session user to authenticate as an admin
and keeps the authorization briefly. There is **no passwordless sudoers rule**
and no `allow_any` grant. The policy file is
`src-tauri/resources/dev.veil.helper.policy`; the helper embeds the same text
for the in-app installer. Verify the installed action with:

```bash
pkaction --verbose --action-id dev.veil.helper
```

Do not weaken the policy, add sudoers entries, or run the GUI as root to make
TUN work.

## Network surface

In system mode, everything Veil starts listens on loopback:

- mixed HTTP+SOCKS inbound: `127.0.0.1:<mixedPort>` (default `2080`)
- Clash API: `127.0.0.1:<apiPort>` (default `9090`), protected by a fresh
  32-character secret written into the generated config

There is no remote listener, no telemetry endpoint and no cloud service. The
only outbound network requests Veil itself makes are subscription fetches
(`http`/`https`, 20 s timeout, at most five redirects, 20 MB cap) and the
diagnostics/Clash calls to its own loopback API. sing-box carries the actual
proxied traffic according to the user's profile.

In TUN mode the engine routes traffic through `veil0`; the Clash API and mixed
inbound remain loopback-bound unless the user's own profile configures
otherwise. TUN add/remove operations require the privileged helper.

## Files and permissions

Veil creates its user directories as `0700`. Sensitive files are `0600`:

| Path | Contents | Mode |
| --- | --- | --- |
| `~/.config/veil/settings.json` | Settings, including `corePath` | `0600` |
| `~/.local/share/veil/` | Data directory | `0700` |
| `~/.local/share/veil/veil.db` | Profiles, selections, metadata; the SQLite `-wal` and `-shm` sidecars are set to the same mode when present | `0600` (db, `-wal`, `-shm`) |
| `~/.local/share/veil/runtime/runtime.json` | Live pid, ports, mode, Clash API secret | `0600` |
| `~/.local/share/veil/runtime/config.json` | Generated runtime config (contains profile material and the secret) | `0600` |
| `~/.local/share/veil/runtime/sysproxy.json` | Exact previous desktop proxy values | `0600` |
| `~/.local/share/veil/logs/veil.log` | Redacted application log | `0600` |
| `~/.local/share/veil/exports/` | Log and profile exports | `0700` directory |
| `~/.local/share/veil/veil.lock` | Single-instance lock owner pid; created `0600`, inside the `0700` directory | `0600` on creation |
| `/run/veil/sing-box.pid` | Helper-managed engine pid (TUN) | `0644` |
| `/run/veil/sing-box.log` | Helper-managed engine log (TUN) | `0644` |

`/run/veil` is intentionally readable so the user can tail the root engine log;
it contains no API secret beyond what the user's own profile writes to the
engine output. Settings and runtime metadata are written atomically with
owner-only modes.

## Config generation

Stored profiles are raw sing-box JSON and are never rewritten. Generation
operates on a parsed copy and injects only:

- `experimental.clash_api.external_controller = 127.0.0.1:<apiPort>` and a fresh
  32-character secret; other Clash API keys are preserved.
- `log.level` from settings; other log keys are preserved.
- A `veil-mixed` inbound on `127.0.0.1:<mixedPort>`; a pre-existing mixed
  inbound on that port is replaced to avoid a bind conflict.
- In TUN mode, a `veil-tun` inbound with the configured addresses, MTU,
  `auto_route`, `strict_route`, `dns_mode: hijack`, LAN exclusions, the
  `gvisor` stack, and `auto_redirect` only when enabled.
- `route.auto_detect_interface = true` in TUN mode so outbound dials bind to the
  physical interface.

Unknown fields are preserved at the JSON value level. The API secret is
regenerated for each run and is not stored in the profile.

## Secret handling and redaction

- The Clash API secret is registered for exact replacement in logs.
- Subscription URLs are registered as secrets before fetching and on profile
  load; they are redacted from log messages and events.
- Pattern redaction covers URL userinfo, password/token/secret/key fields,
  UUIDs, and subscription query strings before entries reach the ring, file or
  event stream.
- The helper and backend pass arguments as argv; no secret is interpolated into
  a shell command.

Limits:

- Redaction is pattern-based, not a guarantee. Custom credential formats,
  identifiers embedded in unusual places, or data inside profile JSON are not
  covered.
- The SQLite database, settings, generated config, runtime metadata, exports
  and `/run/veil` log contain sensitive local data in plaintext. There is no
  encryption at rest and no keyring integration.
- Do not attach profiles, real subscription URLs, `settings.json`, `veil.db`,
  runtime files or unredacted logs to a public report. Redact them while
  keeping the reproduction faithful.

## What is in scope

- The privileged helper, its argv handling, pid/config validation and process
  cleanup.
- Privilege separation between GUI and helper.
- The Tauri command boundary and capability set; the app enables a minimal
  desktop capability set (`core:default`, window controls, events) and no shell
  or filesystem plugin.
- Profile import/parsing: sing-box JSON, subscriptions, share links, QR images
  through `zbarimg`, backups.
- Runtime config generation and file permissions.
- Log redaction and export.
- System-proxy snapshot/apply/restore, including crash recovery.
- Core discovery and execution paths.
- Packaging and dependencies that affect shipped binaries.

## Known hardening gaps and non-guarantees

- Tauri's CSP is not configured (`csp: null`). The UI loads bundled local assets
  and does not load remote content, but this is a hardening gap, not an
  assurance.
- There is no sandbox around sing-box or the helper beyond Unix privileges and
  the polkit action.
- QR import shells out to the external `zbarimg` binary; its own input handling
  is outside this repository.
- Upstream vulnerabilities in sing-box, WebKitGTK, Tauri or distribution
  packages are out of scope unless Veil's integration makes them worse (for
  example, an unsafe invocation or missing validation).
- Same-user compromise is out of scope: any process running as the user can
  read the user's data and call the same commands.
- This is pre-release software with no long-term support commitment.

## Verifying the posture yourself

- [ARCHITECTURE.md](ARCHITECTURE.md) — privilege separation and helper
  invariants.
- [VERIFICATION.md](VERIFICATION.md) — what was and was not exercised.
- [troubleshooting.md](troubleshooting.md) — TUN prerequisites and failure
  modes.
- `src-tauri/helper/` — the whole privileged component.
- `src-tauri/resources/dev.veil.helper.policy` — the polkit action.
- `src-tauri/capabilities/default.json` — the Tauri capability set.

For the reporting process and supported versions, see
[SECURITY.md](../SECURITY.md).