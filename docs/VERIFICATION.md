# Veil verification

Date: 2026-11-27. Host: Arch Linux, Hyprland 0.57.2 on Wayland
(`WAYLAND_DISPLAY=wayland-1`), user `deus` (uid 1000). Core:
`vendor/sing-box-1.14.5-linux-amd64/sing-box`, real version 1.14.5
(tags include `with_clash_api`). No root/polkit authentication was used.

Status vocabulary used below:

- **IMPLEMENTED** — code exists and is wired into the app.
- **BUILD-VERIFIED** — compiled/linked from a clean tree with zero errors and
  zero warnings in the project logs.
- **INTEGRATION-TESTED** — exercised against the real sing-box binary or real
  desktop D-Bus/gsettings, with assertions/cleanup.
- **MANUALLY-VERIFIED** — observed in the running release app (window,
  processes, Clash API, screenshots).
- **UNVERIFIED** — not proven in this environment (details given).

## Clean build from scratch

```bash
cd /home/deus/veil
rm -rf dist src-tauri/target src-tauri/helper/target
pnpm exec vite build
(cd src-tauri/helper && cargo build --release)
(cd src-tauri && cargo build --release)
```

Result: **BUILD-VERIFIED**. Frontend built (real `dist/`, no placeholder),
helper 457K, Tauri release binary 12M. `grep` over the cargo/helper/vite logs
found zero `warning:`/`error` lines. No display was needed for the build.

## Test suite (real-binary checks included)

```bash
cd /home/deus/veil/src-tauri
VEIL_SINGBOX=/home/deus/veil/vendor/sing-box-1.14.5-linux-amd64/sing-box cargo test
```

Result: **51 passed, 0 failed**, 0 warnings. Coverage includes:

- Link parsers for vless/vmess/trojan/ss/hysteria2/tuic/socks/http, plus
  generated link config passing the real `sing-box check`.
- Config generation: field preservation, Clash API injection, TUN fields,
  auto_redirect on/off; generated TUN config passes the real `sing-box check`.
- DB CRUD/backups, settings partial patch, log redaction, Clash mapping,
  diagnostics/error/capability serialization.
- `core::tests::validates_real_sing_box_configs` (real binary): valid JSON OK;
  subscription-style selector config OK; malformed JSON, missing outbounds and
  empty outbounds rejected.
- Subscription normalization tests: sing-box JSON accepted; base64 JSON and
  base64 link lists accepted; plain link list accepted; missing outbounds
  rejected; **Clash YAML rejected** with the explicit Clash-YAML error.

## Real engine: smoke + lifecycle

```bash
cd /home/deus/veil/src-tauri
VEIL_SINGBOX=/home/deus/veil/vendor/sing-box-1.14.5-linux-amd64/sing-box \
  cargo run --example engine_smoke

XDG_CURRENT_DESKTOP=Hyprland \
VEIL_SINGBOX=/home/deus/veil/vendor/sing-box-1.14.5-linux-amd64/sing-box \
  cargo run --example engine_lifecycle
```

`engine_smoke` result: **INTEGRATION-TESTED**. Real config check with a free
mixed port + Clash API; `/version` returned
`{"meta":true,"premium":true,"version":"sing-box 1.14.5"}`; `/connections` and
`/traffic` answered (sample `{"down":0,"up":0}`); 5 real log lines captured;
SIGTERM stop; `ENGINE SMOKE OK`.

`engine_lifecycle` result: **INTEGRATION-TESTED**. It uses the same `Engine`
code and a recording `EventSink` (not a simulated engine) with an isolated
`XDG_DATA_HOME`/`XDG_CONFIG_HOME` and forced unsupported desktop proxy:

```
connect status events: ["starting", "starting", "connected"]
clash /proxies outbounds: 3
captured log entries: 7 (includes "sing-box started (0.00s)")
stats event: {"connections":0,"down":0,"downSpeed":0.0,"duration":0,
               "latency":null,"up":0,"upSpeed":0.0}
disconnect status events: ["stopping", "idle"]
unexpected-exit status=error
  error=Some("sing-box exited unexpectedly (signal: 9 (SIGKILL)). Last output:\n...")
unexpected-exit status events: ["starting", "starting", "connected", "error"]
ENGINE LIFECYCLE OK
```

Assertions: `connected` only after Clash health; graceful disconnect kills the
pid and removes `runtime.json` + `config.json`; SIGKILL produces `error` with
the stderr tail and removes runtime files. No root used.

## System proxy apply/restore (real gsettings)

Gated, safe test commands (they snapshot exact previous raw values and restore
them even on assertion failure via a drop guard):

```bash
cd /home/deus/veil/src-tauri
VEIL_TEST_GSETTINGS=1 cargo test --lib \
  sysproxy::tests::gnome_apply_restore_roundtrip_when_enabled -- --nocapture
VEIL_TEST_GSETTINGS=1 cargo test --lib \
  sysproxy::tests::unsupported_desktop_apply_is_a_noop_when_enabled -- --nocapture
```

Result: **INTEGRATION-TESTED** on the real GNOME gsettings backend (schemas
exist on this host), despite the active desktop being Hyprland.

- Before: `mode='none'`, `http.host=''`, `http.port=8080`, `socks.port=0`,
  no `sysproxy.json` state file.
- Test applied GNOME proxy on port 18081, asserted `mode='manual'` and
  `http.port=18081`, then restored. External verification after the test showed
  exactly the same four raw values again and no `sysproxy.json`.
- Unsupported path: `proxy_kind_from_desktop("Hyprland") == "unsupported"`;
  `apply(18082)` returned `kind='unsupported'`, created no state file, and left
  gsettings unchanged. The running Hyprland app also reports
  `systemProxySupported=false`, `systemProxyKind="unsupported"` and does not
  create `runtime/sysproxy.json`.
- KDE (`kwriteconfig6`) and XFCE (`xfconf-query`) implementations are
  **IMPLEMENTED**; on this host KDE is not the active desktop and
  `xfconf-query` is absent, so they are **UNVERIFIED**. No user proxy values
  remain modified.
- Diagnostics honesty: `permissions.helper` now means a **usable privileged
  helper + installed polkit action**, not merely a dev-build binary found beside
  the target directory. On this host the dev helper exists, polkit is absent, so
  the UI reports “Helper permission: no”; the helper path is still listed as a
  dev build. TUN mode is not claimed as working.

## Real subscription (URL redacted)

The real subscription was handled as a secret: the URL lives only in
`/tmp/veil-sub.url` (mode 0600, outside the repository) and was never printed by
the harness. It was not committed or written to any repository/log file.

Harness: `src-tauri/examples/subscription_integration.rs` (uses the same
normalization, real `sing-box check`, DB creation and `Engine` code as the
backend; `normalize_subscription_body` is shared).

| Check | Result |
|---|---|
| HTTP | `200` |
| Classification | **base64 share-link list** (not Clash YAML) |
| Outbound count | `31` |
| Real `sing-box check` | **ok** (31 outbound tags) |
| Connect | `starting, starting, connected` |
| HTTP proxy `https://example.com` | HTTP `200` |
| SOCKS5h proxy `https://example.com` | HTTP `200` |
| Clash `/traffic` sample max | up `3914`, down `10412` |
| Clash `/connections` totals | up `52839`, down `140494` |
| Clash `/proxies` | `32` |
| `test_delay` for selector node | `188 ms` |
| Refresh | HTTP `200`, revalidated, `updated_at` advanced, `last_error=null` |
| Disconnect | `idle`, pid gone, runtime clean, no `sysproxy.json` |
| Log redaction | URL absent from ring + file; 18 credential values from the profile checked against 3 logs, **0 leaks** |
| Repo scan | no subscription URL in repository files |

No Clash YAML conversion was needed. The validated profile is stored as
**“Subscription Test”** (kind `subscription`, source `subscription`, URL not
displayed) in the isolated app data; the original **“Seeded Direct”** profile is
retained.

**Traffic attribution caveat:** a concurrent **Throne TUN** was active on this
host during the later runs. All Veil traffic claims above are attributed to
Veil's own mixed inbound (`127.0.0.1:<mixed>`) and its Clash `/connections`
totals plus Veil log lines for that request (e.g. `inbound/mixed[veil-mixed]`
→ `outbound/vless[...]`). No system-wide routing or TUN behavior is attributed
to Veil. Throne was not stopped or modified.

## Running release application (left running on Hyprland workspace 4)

The release app is currently running from the isolated data set, on workspace 4,
focused, with the validated subscription profile connected. Reproduce/relaunch
with:

```bash
cd /home/deus/veil
nohup env \
  XDG_DATA_HOME=/tmp/veil-app-data \
  XDG_CONFIG_HOME=/tmp/veil-app-config \
  GDK_BACKEND=wayland \
  WEBKIT_DISABLE_COMPOSITING_MODE=1 \
  src-tauri/target/release/veil \
  > /tmp/veil-app.log 2>&1 < /dev/null &
```

- Profiles: **"Subscription Test"**, id `2`, kind `subscription` (validated
  real subscription; seeded from the harness DB, URL not displayed), and
  **"Seeded Direct"**, id `1`, kind `local` (direct fallback). Last profile
  selected: id `2`.
- Mode: `system`; mixed port `54159`; Clash API `127.0.0.1:38267`;
  `connectOnLaunch=true`, notifications off, minimize-to-tray off.
- Data: `/tmp/veil-app-data/veil/veil.db`; settings:
  `/tmp/veil-app-config/veil/settings.json`; logs:
  `/tmp/veil-app-data/veil/logs/veil.log`; runtime state:
  `/tmp/veil-app-data/veil/runtime/runtime.json`.
- Final running instance: app pid `81408`, window class `veil`, address
  `0x55fd73b91700`, workspace `4`, focused/focusHistoryID `0`, maximized
  1900x1026; engine pid `81581`, profile `Subscription Test` (id 2), system
  mode, Clash `/version` = sing-box 1.14.5. Real HTTP and SOCKS5h requests to
  `https://example.com` through `127.0.0.1:54159` returned `200`; Clash
  `/connections` totals advanced (up `3914`, down `10416` in the recorded
  sample); `/proxies` has 32 entries. Screenshot shows the Overview with all 8
  nav views, `LIVE: SYSTEM`, selected node, and manual proxy. Left running for
  the user/frontend. Engine pids change on reconnect; read `runtime.json`.

Seed commands (already applied; for a fresh reproduction after stopping the
app):

```bash
mkdir -p /tmp/veil-app-data/veil /tmp/veil-app-config/veil
cat > /tmp/veil-app-config/veil/settings.json <<'JSON'
{"theme":"dark","mode":"system","mixedPort":54159,"apiPort":38267,
 "corePath":"/home/deus/veil/vendor/sing-box-1.14.5-linux-amd64/sing-box",
 "autoRedirect":false,"autostart":false,"connectOnLaunch":true,
 "notifications":false,"minimizeToTray":false,"logLevel":"info",
 "subscriptionIntervalHours":24,"language":"en"}
JSON
chmod 600 /tmp/veil-app-config/veil/settings.json
DB=/tmp/veil-app-data/veil/veil.db
sqlite3 "$DB" <<'SQL'
CREATE TABLE profiles (
 id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL,
 kind TEXT NOT NULL DEFAULT 'local', source TEXT NOT NULL DEFAULT '',
 content TEXT NOT NULL DEFAULT '', subscription_url TEXT,
 auto_update INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0,
 created_at INTEGER NOT NULL DEFAULT 0, last_error TEXT);
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE selections (profile_id INTEGER NOT NULL, group_tag TEXT NOT NULL,
 outbound_tag TEXT NOT NULL, PRIMARY KEY (profile_id, group_tag));
INSERT INTO profiles (name, kind, source, content, subscription_url,
 auto_update, updated_at, created_at, last_error)
VALUES ('Seeded Direct','local','seed',
 '{"outbounds":[{"type":"direct","tag":"direct"},{"type":"selector","tag":"Proxy","outbounds":["direct"]}],"route":{"final":"direct"}}',
 NULL,0,strftime('%s','now'),strftime('%s','now'),NULL);
INSERT INTO meta (key,value) SELECT 'last_profile_id', id FROM profiles
 ORDER BY id DESC LIMIT 1;
SQL
chmod 600 "$DB"
```

Inspect window, engine and screenshot:

```bash
hyprctl clients -j | jq -c '.[] | select(.class=="veil") | {address,title,at,size,pid}'
geometry=$(hyprctl clients -j | jq -r '.[] | select(.class=="veil") |
  "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"' | head -1)
mkdir -p /tmp/veil-shots
grim -g "$geometry" /tmp/veil-shots/veil-app-live.png

RT=/tmp/veil-app-data/veil/runtime/runtime.json
jq '{pid,mode,profileId,apiPort}' "$RT"
ps -p "$(jq -r .pid "$RT")" -o pid,ppid,args
curl -s -H "Authorization: Bearer $(jq -r .secret "$RT")" \
  "http://127.0.0.1:$(jq -r .apiPort "$RT")/version"
tail -20 /tmp/veil-app-data/veil/logs/veil.log
```

Screenshots already captured:

- `/tmp/veil-shots/veil-workspace4-final.png` — final maximized window on
  workspace 4, Overview, active profile `Subscription Test`, `LIVE: SYSTEM`,
  manual proxy `127.0.0.1:54159`.
- `/tmp/veil-shots/veil-workspace4-final-full.png` — full-screen workspace 4
  capture.
- Earlier screenshots (`veil-app-connected.png`, `veil-app-live.png`,
  `veil-workspace4-subscription*.png`) are retained for reference.

Graceful close on Hyprland 0.57 (Lua dispatcher; make sure Veil is the active
window first, otherwise this closes the active window):

```bash
[ "$(hyprctl activewindow -j | jq -r .class)" = "veil" ] && \
  hyprctl dispatch 'hl.dsp.window.close()'
```

Close test result (**MANUALLY-VERIFIED**): earlier app/engine pids 63560/63719;
after close the app exited, sing-box stopped, the window disappeared,
`runtime.json` + `config.json` were removed, and the log ended with
`stopping sing-box` / `engine stopped`. The final instance is currently left
running on workspace 4 as recorded above.

## Contract status

`docs/CONTRACT.md` is final for this build. Commands reject with
`CommandError { code, message, detail }` (codes `not-ready`, `validation`,
`permission`, `engine`, `network`, `io`, `unsupported`, `unknown`);
`bootstrap.engineInfo` and `Diagnostics.capabilities` expose derived
capabilities; `engine://state` is the canonical lifecycle event. No command or
event names changed.

## Verification matrix

| Area | Level |
|---|---|
| Rust backend + Svelte frontend compile | BUILD-VERIFIED (clean tree, zero warnings) |
| Unit/integration tests (52) | INTEGRATION-TESTED where gated by real binary/tools |
| Real sing-box config validation (valid/malformed/missing/Clash YAML/subscription) | INTEGRATION-TESTED |
| Real subscription import/traffic/delay/refresh/redaction | INTEGRATION-TESTED (`subscription_integration`) |
| Engine start/health/logs/stats/disconnect/unexpected-kill/cleanup | INTEGRATION-TESTED (`engine_lifecycle`) |
| GNOME system proxy apply + exact restore; unsupported no-op | INTEGRATION-TESTED with real gsettings |
| KDE/XFCE proxy backends | IMPLEMENTED, UNVERIFIED on this host |
| Release app window + embedded assets + engine state + screenshot | MANUALLY-VERIFIED |
| Real app connect + traffic + graceful disconnect via window close | MANUALLY-VERIFIED |
| TUN activation / root / polkit authentication | **PASS (2026-11-27)** — real pkexec-approved end-to-end run; see “TUN end-to-end — PASS” |
| Tray menu interaction and all 8 views | UI present; full interaction left to frontend inspection |
| Concurrent Throne TUN | Acknowledged in attribution; not stopped/modified; no Veil system-wide routing claimed |

TUN/root evidence: helper built and
`veil-helper check --core vendor/.../sing-box` returned:
`{"core":{"error":null,"ok":true,"path":"./vendor/sing-box-1.14.5-linux-amd64/sing-box","version":"1.14.5"},"ip":true,"nft":true,"ok":true,"root":false,"tunDevice":true,"uid":1000}`.
Generated TUN configs (auto_redirect false and true) pass the real
`sing-box check`. During inspection the mode was switched to `tun`; the helper
started root sing-box but it failed before an engine pid was recorded:
`starting TUN interface: set routes: add route 6: file exists` (a pre-existing
`172.19.0.0/24 dev throne-tun` route). No `veil0` device remained afterwards and
mode was reset to `system`. The conflicting route belonged to the concurrent
Throne TUN (`172.19.0.0/24 dev throne-tun`), which was left untouched. **TUN
end-to-end was blocked at that time; superseded by the PASS below.**

### TUN acceptance check — blocked by concurrent Throne TUN

Read-only checks were run; **Throne was not stopped or reconfigured** and no
Veil TUN retry was performed:

```bash
ip -d link show throne-tun
ip route show
ip rule
pgrep -af 'throne|sing-box|target/release/veil'
```

Observed state:

- `throne-tun`: `<POINTOPOINT,MULTICAST,NOARP,UP,LOWER_UP> mtu 1500 ... tun type tun ...`
- Route conflict: `172.19.0.0/24 dev throne-tun proto kernel scope link src 172.19.0.1`
  (also a separate `198.18.0.0/30 dev Meta` route).
- `ip rule` contains Throne/Meta policy rules (tables `1436053744`/`2022`,
  fwmarks `0x2023/0x2024/0x2027`); these were left untouched.
- `veil0` does not exist.
- Veil remains in **system mode**: app pid `81408`, engine pid `81581`,
  profile `Subscription Test` (id 2), Clash API 38267, focused on workspace 4.

Why TUN is blocked: Veil's TUN config needs `172.19.0.0/30` on `veil0` with
`172.19.0.1`; Throne already owns `172.19.0.0/24` on `throne-tun` with
`172.19.0.1`, so adding Veil's route/address fails (`add route ... file
exists`). Per instruction, TUN was **not retried** and Throne was not touched.
Veil TUN end-to-end remains **UNVERIFIED/BLOCKED**; no TUN success is claimed.

Diagnostics honesty: after tightening `permissions.helper`, the gated test
reports `helper_installed=true`, `helper_path=.../helper/target/release/veil-helper`
(dev build), `polkit=false`, `helper_permission=false`, `policy=false`; the UI
therefore shows “Helper permission: no” and “Polkit available: no” while
“Helper installed: yes” still shows the dev path. TUN is not claimed.

The final integrated commit is recorded in the delivery report; the earlier
scaffold/typed-contract commits are `e38068e` and `50f1f36`.

## TUN end-to-end — PASS (2026-11-27)

Host: Linux 7.3.5-2-cachyos, sing-box 1.14.5, Veil 0.1.0 release built with
`pnpm build` + `cargo build --release` from `dfc9be1` plus the two fixes below.
Throne's TUN was off, its processes (`3927/3928/3938`) were not touched, and
no other TUN/VPN interface existed. Every root start/stop used
`pkexec /usr/bin/veil-helper` with the `dev.veil.helper` policy; no fake
success was reported.

### Fixes verified

1. **Outbound loop.** The generated route now sets
   `"auto_detect_interface": true`, so sing-box binds its own outbound sockets
   to `enp0s31f6` instead of letting the `auto_route` table (2022) route proxy
   dials back into `veil0`. Before this, the engine logged
   `dial tcp 45.145.40.13:443: i/o timeout`; `ip route get 45.145.40.13`
   resolved to `via 172.19.0.2 dev veil0 table 2022`, and an unbound socket
   was stuck `SYN-SENT` from `172.19.0.1`.
2. **TUN TCP stack.** The TUN inbound now selects `"stack": "gvisor"`. With
   the default `mixed` stack (system TCP + gvisor UDP), DNS/UDP worked but
   local TCP never reached the proxy: the kernel-backed listener bound to
   `172.19.0.1:<port>`, the client socket stayed `SYN-SENT`, and the engine
   logged no `inbound/tun ... inbound connection`. Using a TUN address outside
   the excluded LAN ranges did not change it. `gvisor` handles TCP and UDP in
   userspace and passes both end-to-end.

### Isolated test app (`/tmp/veil-tun-e2e-*`, new binary)

- mode `tun`, connectOnLaunch; root engine pid 29012; `veil0` up with
  `172.19.0.1/30` and `fdfe:dcba:9876::1/126`.
- Generated config:
  `{"route":{"auto_detect_interface":true,"final":"Proxy"}}` and TUN
  `{"stack":"gvisor","auto_route":true,"strict_route":true,"dns_mode":"hijack"}`.
- `ip rule`: 9000→2022 for the TUN subnet, 9001 lookup 2022
  `suppress_prefixlength 0`, 9002 dport 53/main and `iif veil0 goto 9010`,
  9003 `iif lo` rules, 9010 nop. Table 2022 carries the split public routes
  via `172.19.0.2 dev veil0`. Unbound `ip route get 195.72.61.241` still
  entered veil0, while the engine's socket was
  `192.168.0.181%enp0s31f6:<port> → 195.72.61.241:443`.
- DNS resolved real addresses (`ifconfig.co` → `104.21.54.91` /
  `172.67.168.106`).
- Proxy env unset for the test commands:
  `curl https://ifconfig.co/json` → `195.72.61.192`, GeoIP country `GB`;
  `curl https://api.ipify.org?format=json` → `194.87.159.72`. Clash
  `/connections` chain: `["🇸🇪 Sweden", "Proxy"]`. The selected outbound is
  Sweden; the node's exit IP geolocates to GB, reported as observed.
- No dial/TUN timeouts. The only error was a probe to `95.216.195.133:80`
  reporting `report handshake success: connection refused` (target refused,
  not a TUN failure).
- Disconnect via app SIGTERM (`veil-helper stop`): `veil0` removed, `ip rule`
  back to `local/main/default`, table 2022 empty, 17890/19090 listeners
  closed, runtime dir empty.

### Normal installed app (normal XDG, new binary)

- Relaunched in TUN mode: app pid 29288, root engine pid 29515, `veil0` up,
  window on Hyprland workspace 4, Clash API 200.
- Through TUN: `curl https://ifconfig.co/json` → `195.72.61.192` (`GB`);
  `api.ipify` → `78.153.136.14`; chain `["🇸🇪 Sweden", "Proxy"]`; engine
  socket `192.168.0.181%enp0s31f6:50008 → 195.72.61.241:443`.
- Left connected in TUN mode. `connectOnLaunch` was restored to its previous
  `false`; the running session stays connected.
