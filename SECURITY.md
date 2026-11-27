# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Yes (current pre-release line) |
| Older / unversioned builds | No |

Veil is pre-release software. No version has been tagged or published yet, and
there is no long-term support commitment.

## Reporting a vulnerability

Report security issues **privately** through GitHub Security Advisories:

<https://github.com/sorethe/veil/security/advisories/new>

The repository is currently private, so this form is available to people with
access to it. Do not open a public issue, discussion, or pull request for a
vulnerability. Do not include working credentials, real subscription URLs, or
other users' data in a report; redact them while keeping the reproduction
faithful.

There is no dedicated security email address. Please use the advisory form so
the report stays private and can be discussed with a fix attached.

## What to report

Veil has an unusually sensitive surface for a desktop app:

- the privileged `veil-helper` (`/usr/bin/veil-helper`) and the
  `dev.veil.helper` polkit action, including command-line handling, pid-file
  validation, config validation, and process cleanup;
- privilege separation between the GUI (never root) and the helper;
- the Tauri command boundary and capability configuration;
- profile import and parsing: sing-box JSON, subscriptions, share links, QR
  images, backups;
- runtime config generation and file permissions;
- log redaction and export (credentials, URLs, API secrets, UUIDs);
- system-proxy snapshot, apply and restore, including crash recovery;
- core discovery and execution paths (`corePath`, `$VEIL_SING_BOX`, `PATH`,
  resource directory, `/usr/lib/veil/sing-box`);
- dependency and packaging issues that affect the shipped binaries.

## Sensitive local data

Veil intentionally stores profile material, subscription URLs, proxy
credentials, settings, runtime state and logs on the local machine:

- `~/.config/veil/settings.json` (mode `0600`)
- `~/.local/share/veil/veil.db` (profiles and selections)
- `~/.local/share/veil/runtime/` (generated config, API secret, proxy restore
  state; `0600`)
- `~/.local/share/veil/logs/veil.log` (redacted, but still local activity data)
- `/run/veil/` (helper pid and engine log; readable so the user can tail it)

Backups and exported logs are written under `~/.local/share/veil/exports/`.
Treat all of these as private. A report that demonstrates a credential leak
through logs, exports, error messages, notifications, or the UI is in scope.

## Scope and non-guarantees

In scope: code in this repository and the packaging it produces.

Out of scope: vulnerabilities in upstream `sing-box`, WebKitGTK, Tauri, or
distribution packages, except where Veil's integration makes them worse
(e.g. an unsafe invocation or missing validation). Report upstream issues to the
upstream project.

This project makes no guarantee of a response time, fix time, or backport. It is
maintained on a best-effort basis. If a report is accepted, the fix and
disclosure will be coordinated through the advisory. Please allow time for a fix
before public disclosure.

## Verifying the security posture yourself

- `docs/ARCHITECTURE.md` describes the privilege separation and helper
  invariants.
- `docs/VERIFICATION.md` records what was and was not exercised, including the
  explicitly unverified TUN path.
- The helper's source is small and self-contained in `src-tauri/helper/`.
- The polkit policy is `src-tauri/resources/dev.veil.helper.policy`.
