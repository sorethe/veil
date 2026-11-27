# Troubleshooting Veil

Start with the **Diagnostics** and **Logs** views in the application. They show
the resolved core and its real version, helper/polkit state, `/dev/net/tun`,
nftables, interfaces, routes, DNS, tray availability and recent errors. Nothing
on that page is hardcoded: an absent tool shows as unavailable.

Command-line fallbacks used below:

```bash
# Live engine identity (pid, mode, ports, secret)
jq '{pid,mode,profileId,mixedPort,apiPort}' \
  ~/.local/share/veil/runtime/runtime.json

# Application log
tail -50 ~/.local/share/veil/logs/veil.log

# TUN helper log and pid (when TUN is running)
tail -50 /run/veil/sing-box.log
```

Do not delete routes, rules or firewall state by hand, and do not
`pkill sing-box`. Stop the engine through the application or the helper so
cleanup and proxy restore run. If another TUN/VPN is active, leave it alone;
resolve the conflict on your side first.

## The core is not found

**Symptom:** connect or validate fails with `sing-box binary not found`
(`set corePath in settings or $VEIL_SING_BOX`), or Diagnostics shows the core as
unavailable.

**Cause:** Veil resolves the core in this order: `settings.corePath`,
`$VEIL_SING_BOX`, `sing-box` on `PATH`, beside the app executable, the Tauri
resource directory, `/usr/lib/veil/sing-box`, then a development
`vendor/sing-box-*` build. A configured path that does not exist is not skipped
silently.

**Fix:**

```bash
command -v sing-box && sing-box version
# or point Veil at a specific binary
VEIL_SING_BOX=/path/to/sing-box ./src-tauri/target/release/veil
```

You can also set the core path in Settings. The validated core is sing-box
1.14.5; other versions are unverified.

## The core exists but validation fails

**Symptom:** profile save/import or connect reports a validation error; the
generated runtime config is rejected.

**Cause:** Veil runs the real `sing-box check`. Malformed JSON, missing or empty
`outbounds`, invalid fields and Clash YAML all fail here.

**Fix:** run the same check yourself:

```bash
sing-box check -c /path/to/profile.json
```

- A subscription returning Clash YAML is rejected on purpose. Veil does not
  convert Clash/Clash.Meta configs; export a sing-box profile or use share
  links.
- A subscription with no outbounds is rejected rather than stored empty.
- The Logs view carries the real error message and the offending source.

## Clash API does not become ready

**Symptom:** the session ends in `error` with
`Clash API did not become ready within 12s: ...` and possibly a stderr tail.

**Cause:** the engine started but its Clash API on `127.0.0.1:<apiPort>` did not
answer within the 12 s startup deadline, or the child died. Typical reasons: a
port conflict, an invalid config that passed `check` but failed at runtime, a
sandbox/firewall blocking loopback, or a proxy environment variable redirecting
the API dial.

**Fix:**

1. Read the Logs view for the stderr tail and the failing config path.
2. Check that nothing else owns the API port:
   ```bash
   ss -ltnp | grep -E ':(2080|9090)\b'
   ```
   Replace the ports with your configured `mixedPort` and `apiPort`.
3. Change ports in Settings while disconnected, then reconnect.
4. Confirm the core was built with Clash API support:
   `sing-box version` should list `with_clash_api`.

## Port conflicts / address already in use

**Symptom:** startup fails, or the engine logs a bind/listen failure such as
`address already in use`.

**Cause:** `mixedPort` (default `2080`) or `apiPort` (default `9090`) is taken,
or both were set to the same value (Settings rejects that explicitly).

**Fix:** disconnect, change one of the ports in Settings, reconnect. The engine
must be stopped before changing the live port; the UI locks mode/port edits
during a session. If an old engine is still running, the application may adopt
it; inspect `runtime.json` first rather than starting a second instance.

## System proxy is reported as unsupported

**Symptom:** the session is healthy but shows `LIVE · MANUAL`, or Diagnostics
reports `systemProxyKind: "unsupported"`.

**Cause:** Hyprland, sway and similar compositors have no desktop-wide proxy API
that Veil can set. KDE needs `kwriteconfig6`/`kwriteconfig5`; XFCE needs
`xfconf-query`; GNOME needs the `org.gnome.system.proxy` gsettings schema.

**Fix:** for the manual mode, export the proxy for the applications you launch:

```bash
export HTTP_PROXY=http://127.0.0.1:<mixedPort>
export HTTPS_PROXY=http://127.0.0.1:<mixedPort>
export ALL_PROXY=socks5://127.0.0.1:<mixedPort>
```

Use TUN mode if you need all traffic routed. TUN has its own requirements (see
below).

## The proxy apply or restore failed

**Symptom:** the proxy apply fails, or settings look wrong after a crash.

Before writing anything, Veil snapshots the exact previous values to
`~/.local/share/veil/runtime/sysproxy.json` (mode `0600`). On a partial failure
it restores and removes that file. Restore also runs on disconnect, unexpected
exit, app exit and the next launch after a crash.

- Inspect `sysproxy.json` before acting, and do not delete it while the engine
  is connected.
- If Veil is not running at all, start it once so the startup restore path can
  run.
- `ProxyInfo.detail` names the backend and the manual fallback.

## TUN setup fails

TUN needs **all** of the following. The first-use dialog lists each one and
blocks connect until they are ready.

| Requirement | Check | Fix |
| --- | --- | --- |
| Helper binary `/usr/bin/veil-helper` | Diagnostics → Helper installed | `make install-helper` or use the in-app installer |
| Polkit action `dev.veil.helper` | `pkaction --verbose --action-id dev.veil.helper` | Install the policy with the helper |
| `pkexec` | `command -v pkexec` | Install polkit |
| `/dev/net/tun` | `ls -l /dev/net/tun` | Load the `tun` module / use a kernel with TUN |
| nftables | `command -v nft` | Install `nftables` |
| `ip` | `command -v ip` | Install `iproute2` |

Specific messages you may see:

- `veil-helper is not installed. TUN mode needs /usr/bin/veil-helper and the
  polkit policy.`
- `pkexec is not installed; TUN mode needs polkit to start veil-helper`
- `/dev/net/tun is missing; TUN mode is unavailable on this system`
- `nft is not installed; TUN mode needs nftables`
- `pkexec authentication timed out after 120s; administrative permission was
  not granted` — the polkit prompt was cancelled or dismissed. Retry and
  authenticate as an admin user. There is no passwordless sudoers fallback.

Do not weaken the polkit policy or run the GUI as root. Check the helper
without starting a tunnel:

```bash
veil-helper check --core /path/to/sing-box
```

## TUN is refused because of a conflict

**Symptom:** TUN is blocked and the dialog or error names another interface,
route or address.

Veil detects active TUN/VPN interfaces (`ARPHRD_NONE`, not down, excluding
`veil0` and `lo`) and compares existing routes/addresses with the configured
`tunAddress` / `tunAddress6`. It refuses rather than sharing routes or DNS with
another tunnel. The preflight is read-only.

**Fix:** stop the other tunnel, or change Veil's TUN addresses in Settings so
they do not overlap. Do not delete the other tool's routes or firewall state.
Two tunnels cannot share interface routes and DNS. The default addresses are
`172.19.0.1/30` and `fdfe:dcba:9876::1/126`.

## Subscription refresh fails

**Symptom:** the profile shows a `last_error`, or refresh returns a
`network`/`validation` error.

Real causes and messages:

- `subscription URL must start with http:// or https://`
- `subscription request failed: HTTP <status>` — the provider returned a non-2xx
  response (expired token, redirect loop, rate limit).
- `subscription request failed: <transport error>` — DNS, TLS or timeout
  (20 s request timeout, at most 5 redirects).
- `subscription is larger than 20 MB`
- `unrecognized subscription payload: expected sing-box JSON,
  base64-encoded JSON, or share links`
- `Clash YAML is not sing-box JSON. Veil does not convert...`

A failed refresh keeps the previous profile content and records the error;
it does not write a partial profile. The URL is registered as a secret before
fetching, so it is redacted from logs.

## Tray icon is missing

**Symptom:** no tray icon, but the window works.

**Cause:** the tray is a StatusNotifierItem and needs a StatusNotifier host on
the session bus. Diagnostics reports the exact probe result:

- `gdbus is not installed; cannot query StatusNotifierWatcher`
- `no StatusNotifierWatcher host on the session bus`
- `StatusNotifierWatcher is registered on the session bus`

**Fix:** run a host that implements `org.kde.StatusNotifierWatcher` (most
desktop panels do; a bare compositor may not). Tray failures never change engine
state. Use the window or the tray menu when a host is present.

## Notifications or autostart do not work

Both are best-effort. Notifications need a working notification daemon;
autostart writes a desktop entry under `~/.config/autostart/`. If the daemon or
desktop environment ignores them, Veil does not fake success or change engine
state.

## Logs look incomplete or duplicated

Known behavior:

- The in-memory ring holds 2000 entries; the Logs view renders at most 1500.
- The file rotates to `veil.log.1` only when reopened after exceeding 8 MiB.
- TUN mode has three possible sources (engine stdout/stderr, the helper log
  file, and Clash `/logs`), so the same event can appear twice with different
  sources. Deduplication is by sequence number, not message.
- Stats carry no freshness timestamp; cached values may continue once per
  second if a poll fails. "Zero traffic" and "stale stats" are not
  distinguishable in the current UI.

For a sensitive reproduction, export logs from the Logs view. Exports go to
`~/.local/share/veil/exports/` (directory `0700`).

## Redaction is not a guarantee

Logs are redacted before they enter the ring, the file or events: URL userinfo,
password/token/secret/key fields, UUIDs, subscription query strings and
registered secrets. This is pattern-based and cannot catch every custom
credential format. Profiles, the SQLite database, runtime files, settings and
exports still contain sensitive material. Do not paste them into a public issue.
See [security.md](security.md).

## A second launch exits immediately

**Symptom:** starting Veil again prints
`another Veil instance is already running (lock: .../veil.lock)` and exits.

This is the single-instance guard. An exclusive non-blocking `flock` on
`<data_dir>/veil.lock` prevents two GUIs from opening the same database and
runtime state. Use the running window (or its tray) instead. The lock is
released when the process exits; a leftover lock file is harmless. The
two-launch and adopted-engine flows are implemented but not yet verified
end-to-end.

## A stale or adopted engine is still running

`runtime.json` describes the engine Veil believes it owns. On startup Veil
verifies the pid, `/proc/<pid>/cmdline`, the exact config path and the Clash API
secret before adopting it. If verification fails, it removes the runtime file,
stops only processes proven to be its own `sing-box`, restores any leftover
system proxy, and stays idle.

To inspect without changing anything:

```bash
RT=~/.local/share/veil/runtime/runtime.json
jq '{pid,mode,profileId,mixedPort,apiPort}' "$RT"
ps -p "$(jq -r .pid "$RT")" -o pid,ppid,args
```

Never start a second TUN while one is live. Stop it through the application or
helper, then reconnect.

## Nothing above helps

Collect, without secrets:

1. The Diagnostics view contents (core path/version, permissions, capabilities,
   interfaces, routes, DNS, tray).
2. The last relevant Logs entries and any exported log file.
3. The exact command or action, the error `code`/`message`/`detail`, and the
   desktop/session type.
4. `sing-box version` and the distribution/kernel.

Then open a bug report. For vulnerabilities, follow
[SECURITY.md](../SECURITY.md) instead of a public issue.