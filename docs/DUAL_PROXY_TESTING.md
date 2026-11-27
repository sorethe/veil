# Safe dual-proxy testing: Throne plus Veil

**Status: plan and guardrails only.** This document describes a workflow for
testing Veil in `system` mode while Throne TUN stays active. The workflow in
this file has not been executed for this document, and no claim is made that
Veil TUN works, that Throne is bypassed, or that a particular egress IP belongs
to Veil.

The only supported safe path is:

- Throne stays running and is not modified.
- Veil runs in `system` mode with `autoRedirect: false`.
- The desktop proxy integration is forced to the unsupported branch, so the
  test uses an explicit manual proxy instead of changing system-wide settings.
- Veil TUN is not selected, started, installed, or tested on the host.

## Hard guardrails

Do **not** do any of the following while following this guide:

- Stop, restart, reconfigure, or remove Throne, its TUN device, routes, DNS,
  policy rules, or firewall rules.
- Run `pkexec`, `veil-helper`, `ip route ...`, `ip rule ...`, or `nft` against
  the host for this test.
- Select TUN in Veil, or change `mode` from `system` to `tun`.
- Change the proposed TUN addresses to “get past” the preflight. Throne's
  active TUN still blocks Veil TUN.
- Use the real Veil data directory or settings while another Veil instance is
  running.
- Use `pkill sing-box`, `pkill throne`, `ip route del`, `ip rule del`, or
  `nft flush` as cleanup. Those commands can affect unrelated networking and
  Throne.

If the current desktop uses a supported system-proxy backend such as GNOME,
do not use that desktop for this manual workflow. The workflow below sets
`XDG_CURRENT_DESKTOP=Hyprland` so Veil reports `systemProxyKind` as
`unsupported` and does not write desktop proxy settings.

## Known facts and fixed paths

For the installed/normal launcher, the exact sing-box core path is:

```text
/home/deus/.local/opt/veil/sing-box
```

That is a real path (not a symlink). It has the same SHA-256 as the vendored
checkout binary but is a distinct executable path; Throne matches the exact
path, not a hash. Use the installed path in the Throne rule.

`/home/deus/veil/vendor/sing-box-1.14.5-linux-amd64/sing-box` is only relevant
when a test app explicitly configures `corePath` to that vendor path. In that
case, the Throne rule must use that exact test path instead. Do not mix the two
paths in one run.

`settings.corePath` is authoritative. If Settings or Diagnostics reports a
different core path, use that exact path in the Throne exception instead. The
exception is for the core process that opens the outbound sockets, not for the
Veil GUI process.

The host also has Throne's TUN route. A read-only inspection used this shape:

```text
172.19.0.0/24 dev throne-tun proto kernel scope link src 172.19.0.1
```

That route overlaps the default Veil `tunAddress` (`172.19.0.1/30`) and is why
the TUN preflight reports a conflict. Do not remove it.

Tools used by the commands below: `curl`, `jq`, and `ss`. Install them through
the distribution if missing.

## Step 0: Add the Throne processPath Direct exception (manual)

Before interpreting any DNS, outbound, or egress result as Veil's, add and
enable this exact rule in Throne:

```text
Profile:      Default
Process Path: /home/deus/.local/opt/veil/sing-box
Outbound:     Direct
Enabled:      true
```

Do not use a `processName` match. Preserve Throne TUN and its existing routes,
DNS, and firewall state; this rule only adds a process-path bypass for the Veil
core. Place the rule ahead of catch-all proxy/TUN rules. If a test app
explicitly sets `corePath` to the vendored binary, use that exact vendor path
in the rule instead; do not use a mixture.

The Throne log shows that process-path lookup runs, but Veil's installed core
path has not been observed matching Direct. The bypass is therefore not proven
by that log line.

> **Warning:** until the rule is applied and Throne's logs confirm that
> `/home/deus/.local/opt/veil/sing-box` goes Direct, assume Veil's outbound
> egress traverses Throne. An egress IP measured through Veil in that state is
> not evidence about Veil's own outbound path. Do not attribute DNS, egress IP,
> or selected-outbound results to Veil until that confirmation exists.

## Step 1: Prepare an isolated Veil instance

Build the app once if needed. This does not launch it:

```bash
cd /home/deus/veil
pnpm build
(cd src-tauri && cargo build --release)
```

Create isolated data and config directories and a `system` settings file:

```bash
export VEIL_SAFE_DATA=/tmp/veil-dual-proxy-data
export VEIL_SAFE_CONFIG=/tmp/veil-dual-proxy-config

mkdir -p "$VEIL_SAFE_DATA/veil" "$VEIL_SAFE_CONFIG/veil"

cat > "$VEIL_SAFE_CONFIG/veil/settings.json" <<'JSON'
{
  "theme": "dark",
  "mode": "system",
  "mixedPort": 17890,
  "apiPort": 19090,
  "corePath": "/home/deus/.local/opt/veil/sing-box",
  "autoRedirect": false,
  "tunAddress": "172.19.0.1/30",
  "tunAddress6": "fdfe:dcba:9876::1/126",
  "autostart": false,
  "connectOnLaunch": false,
  "notifications": false,
  "minimizeToTray": false,
  "logLevel": "info",
  "subscriptionIntervalHours": 24,
  "language": "en"
}
JSON

chmod 600 "$VEIL_SAFE_CONFIG/veil/settings.json"
```

Check the two ports are free before launching:

```bash
ss -ltn | grep -E ':(17890|19090)\b' || echo "ports 17890/19090 are free"
```

If either port is in use, change both values in `settings.json` and use those
values consistently below.

## Step 2: Launch only in system/manual mode

This launch is intentionally isolated and forces the unsupported desktop-proxy
branch. It does not use the real profile database and does not start TUN:

```bash
cd /home/deus/veil

XDG_DATA_HOME="$VEIL_SAFE_DATA" \
XDG_CONFIG_HOME="$VEIL_SAFE_CONFIG" \
XDG_CURRENT_DESKTOP=Hyprland \
src-tauri/target/release/veil
```

In the window:

1. Import or select a profile in the isolated app. A direct-only profile is
   sufficient for listener and HTTP/SOCKS checks; a profile with a
   selector/URLTest group is required to inspect a real selected outbound.
2. Keep the mode on **System proxy**.
3. Do **not** select TUN, do not open an install/approval flow, and do not run
   any TUN setup action.
4. Connect.

The app should report `systemProxyKind` as `unsupported` because the forced
desktop is Hyprland. It should not write desktop proxy settings. Verify:

```bash
test ! -e "$VEIL_SAFE_DATA/veil/runtime/sysproxy.json" \
  && echo "no system-proxy state file"
```

## Step 3: Read the live ports and API secret

Run these in a second shell after the engine reports connected:

```bash
export VEIL_SAFE_DATA=/tmp/veil-dual-proxy-data
export VEIL_SAFE_CONFIG=/tmp/veil-dual-proxy-config

SETTINGS="$VEIL_SAFE_CONFIG/veil/settings.json"
RUNTIME="$VEIL_SAFE_DATA/veil/runtime/runtime.json"

MIXED=$(jq -r '.mixedPort' "$SETTINGS")
API=$(jq -r '.apiPort' "$RUNTIME")
SECRET=$(jq -r '.secret' "$RUNTIME")

printf 'mixed=%s api=%s\n' "$MIXED" "$API"
```

The secret is sensitive. Do not print it or put it in a committed file.

## Step 4: Checks

### Listener is loopback-only

```bash
ss -ltnp | grep -E "127\.0\.0\.1:($MIXED|$API)\b"
```

Expected: both the mixed and Clash API listeners are on `127.0.0.1`, not
`0.0.0.0` and not a non-loopback address. Also check that no wildcard listener
was created:

```bash
ss -ltnp | grep -E ":($MIXED|$API)\b" \
  | grep -E '\s(\*|0\.0\.0\.0|\[::\]):' || true
```

The final command should print nothing. An IPv6 loopback line such as
`[::1]:$MIXED` is not a wildcard listener; only `*:`, `0.0.0.0:`, and `[::]:`
are unexpected here.

### HTTP proxy

```bash
curl -sS -o /dev/null -w 'http=%{http_code}\n' \
  --proxy "http://127.0.0.1:$MIXED" \
  https://example.com/
```

Record the status code and the command's exit status. A non-`000` HTTP status
means the HTTP proxy path accepted the request; it does not by itself prove
which outbound or egress path was used.

### SOCKS5 with remote DNS

Use `socks5h://` when you intend the proxy to receive the hostname rather than
resolving it locally:

```bash
curl -sS -o /dev/null -w 'socks5h=%{http_code}\n' \
  --proxy "socks5h://127.0.0.1:$MIXED" \
  https://example.com/
```

For a remote-DNS check, inspect live Clash connections while a request is in
flight (connections are transient):

```bash
curl -sS -o /dev/null --max-time 10 \
  --proxy "socks5h://127.0.0.1:$MIXED" \
  https://example.com/ &
sleep 1

curl -fsS -H "Authorization: Bearer $SECRET" \
  "http://127.0.0.1:$API/connections" \
  | jq -r '.connections[] |
      select(.metadata.host == "example.com") |
      [.metadata.host, .metadata.destinationIP, (.chains | join(" > "))] |
      @tsv'

wait
```

Record whether `metadata.host` is the domain and how `chains` is populated.
Do not treat a transient `/connections` miss as a DNS failure; retry the check
while a request is open. `socks5://` (without `h`) resolves locally, so it does
not test remote DNS.

### Egress IP

```bash
echo "host direct: $(curl -sS --noproxy '*' https://api.ipify.org || echo 'failed')"
echo "via Veil:   $(curl -sS --proxy "socks5h://127.0.0.1:$MIXED" \
  https://api.ipify.org || echo 'failed')"
```

`--noproxy '*'` prevents an ambient `HTTP_PROXY`/`HTTPS_PROXY` from silently
becoming the direct baseline.

Record both addresses. If the Throne `processPath` exception is absent or not
verified, the Veil result may still be Throne's egress; do not label it a Veil
result. Do not infer a bypass from two equal addresses.

If `api.ipify.org` is unavailable, use another trusted public IP echo service
and record which one was used.

### Selected outbound

```bash
curl -fsS -H "Authorization: Bearer $SECRET" \
  "http://127.0.0.1:$API/proxies" \
  | jq -r '.proxies | to_entries[] |
      select(.value.type == "Selector" or .value.type == "URLTest") |
      [.key, .value.type, (.value.now // "-")] |
      @tsv'
```

Then inspect the live connection chain for the request:

```bash
curl -fsS -H "Authorization: Bearer $SECRET" \
  "http://127.0.0.1:$API/connections" \
  | jq -r '.connections[] |
      [.metadata.host, .metadata.destinationIP,
       (.chains | join(" > ")), .rule, .rulePayload] |
      @tsv'
```

The `chains`/`outbound` value should agree with the selector's `now` value for
the selected group. A selector with `now` set to a proxy does not prove that
the egress path bypassed Throne; the Throne exception and egress checks are
separate.

### System-wide proxy

This workflow intentionally does not test system-wide proxy changes:

- On the forced Hyprland desktop, `systemProxyKind` is `unsupported` and
  `sysproxy.json` must remain absent.
- If you explicitly test a real desktop backend, use a disposable user/session
  and record the exact previous settings before connecting. Veil's supported
  backends snapshot and restore desktop proxy values, but that is a separate
  test from this dual-proxy workflow.
- Do not manually delete a `sysproxy.json` state file. Let Veil disconnect or
  exit so its restore path runs.

## Step 5: Stop and verify cleanup

Use the app's Disconnect button, then quit the app normally. Do not kill the
engine or Throne processes.

```bash
test ! -e "$VEIL_SAFE_DATA/veil/runtime/runtime.json" \
  && echo "runtime state cleared"

test ! -e "$VEIL_SAFE_DATA/veil/runtime/sysproxy.json" \
  && echo "no system-proxy state to restore"

ss -ltn | grep -E ":($MIXED|$API)\b" || echo "no Veil listeners remain"
```

If the engine exits unexpectedly, the app records the error and removes its
runtime state through its normal path. Inspect the app log if state remains;
do not manually remove runtime files or kill `sing-box` unless you have already
confirmed it is the isolated test instance.

## TUN, Throne, routes, and firewall boundaries

- TUN is blocked on this host while Throne is active. The preflight is expected
  to report `ready=false`, `canInstall=false`, the active Throne TUN interface,
  and the overlapping `172.19.0.0/24` route. No TUN start or install is part of
  this workflow.
- Do not change `tunAddress`/`tunAddress6` to avoid the route conflict. The
  active non-Veil TUN interface still blocks TUN, and changing the settings
  would only hide an expected conflict.
- TUN must only be tested in a separate disposable network namespace in a
  future, explicitly authorized run. Host-level TUN verification is blocked.
- In this forced-unsupported workflow, system/manual mode creates no host
  routes, policy rules, DNS configuration, or nftables rules. It only starts
  the sing-box process and its loopback listeners; it also does not write
  desktop proxy settings.
- If system-proxy support were exercised, Veil would write `sysproxy.json` and
  restore the exact previous desktop values on disconnect/exit. Do not use
  manual `gsettings`, `kwriteconfig`, or `xfconf-query` cleanup.
- Veil TUN would create interface/route/nft state through the helper. None of
  that is permitted in this workflow. Never clean up Throne or shared firewall
  state with generic commands.

## Verification matrix

Status meanings: **BUILD-VERIFIED** means the repository build/tests pass;
**INTEGRATION-TESTED** means a real binary/tool path was exercised (with a
reference); **NOT RUN** means this document does not claim execution; **BLOCKED**
means the environment intentionally prevents the test.

| Area                                      | Status                                 | Notes                                                                                                                                        |
| ----------------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Backend config and validation             | BUILD-VERIFIED                         | `cargo test` and real `sing-box check` coverage; no root or network changes needed.                                                          |
| Engine lifecycle                          | INTEGRATION-TESTED (repository record) | `examples/engine_lifecycle` is the existing no-root harness; not re-run by this document.                                                    |
| Local proxy: mixed listener, HTTP, SOCKS5 | NOT RUN                                | Requires starting the app and connecting system mode.                                                                                        |
| Remote DNS through SOCKS5h                | NOT RUN                                | Inspect live `/connections`; `socks5h` is required.                                                                                          |
| Egress IP                                 | BLOCKED                                | Not evidence of Veil independence until the user applies the rule and Throne logs confirm `/home/deus/.local/opt/veil/sing-box` goes Direct. |
| Selected outbound                         | NOT RUN                                | Inspect Clash `/proxies` and `/connections`; requires a real profile.                                                                        |
| System-wide proxy                         | NOT RUN                                | Intentionally forced to `unsupported`; use a disposable desktop session for a separate test.                                                 |
| Veil TUN                                  | BLOCKED                                | Throne TUN is active and owns the overlapping route; do not test on the host.                                                                |
| Cleanup                                   | NOT RUN (boundaries documented)        | Use app Disconnect/Quit; never kill Throne or edit shared routes/firewall manually.                                                          |

## User actions required

1. Add and enable the exact Throne rule (`Profile: Default`, `Process Path:
/home/deus/.local/opt/veil/sing-box`, `Outbound: Direct`, enabled) before
   interpreting any egress result. Do not use `processName`.
2. Confirm Throne logs show `/home/deus/.local/opt/veil/sing-box` going Direct;
   until then, egress independence is BLOCKED and the bypass is unproven.
3. If the isolated test app explicitly sets `corePath` to the vendored binary,
   use that exact vendor path in the rule instead.
4. Import/select a profile in the isolated app; the setup above intentionally
   starts with an empty profile database.
5. Confirm ports `17890` and `19090` are free, or change them consistently.
6. Keep Throne running and untouched. Do not select TUN or run TUN setup.
