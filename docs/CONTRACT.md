# Veil backend / frontend contract

This file is the coordination artifact between the Rust backend (`src-tauri/`) and
the Svelte UI (`src/`). The backend implements exactly these commands and events.

- Every command is `async` on the Rust side and returns
  `Result<T, CommandError>`. Command rejections are always the typed envelope
  below; the backend never returns a bare string.
- Command **names** are snake_case; command **arguments** are camelCase (Tauri 2
  default). Example: `invoke("connect", { profileId: 3, mode: "tun" })`.
- Optional arguments may be `null`/omitted.
- All JSON is camelCase. Timestamps are epoch seconds unless stated otherwise.
- **Events are the only source of live engine truth.** `engine://state` is the
  canonical lifecycle stream; polling `get_state` is a snapshot, not a live
  feed.

## Command errors

```ts
type CommandError = {
  code:
    | 'not-ready'      // engine/API required but not connected or still busy
    | 'validation'     // profile/config/settings/link input rejected
    | 'permission'     // polkit/pkexec/root/TUN permission failure
    | 'engine'         // process lifecycle or Clash API failure
    | 'network'        // subscription HTTP/DNS/transport failure
    | 'io'             // filesystem, SQLite, export/import I/O failure
    | 'unsupported'    // feature/backend/tool is not available here
    | 'unknown'
  message: string      // original human-readable message, never rewritten
  detail: string | null // optional extra context (e.g. stderr tail)
}
```

Frontend handling: read `error.message` for display and switch on `error.code`
for behavior. Older builds returned strings; the frontend may defensively
normalize those, but this backend only emits the envelope.

## Commands

| Command | Arguments | Returns |
|---|---|---|
| `bootstrap` | – | `{settings, profiles, engine, core, engineInfo, platform, version}` |
| `get_state` | – | `EngineState` |
| `connect` | `mode?: 'system'\|'tun'`, `profileId?: number` | `EngineState` |
| `disconnect` | – | `EngineState` |
| `get_stats` | – | `Stats` |
| `get_connections` | – | `Conn[]` |
| `close_connection` | `id: string` | `null` |
| `get_logs` | `sinceSeq?: number` | `LogEntry[]` |
| `clear_logs` | – | `null` |
| `export_logs` | – | path string |
| `get_route` | – | `RouteInfo` |
| `get_outbounds` | – | `Outbound[]` |
| `select_outbound` | `group: string`, `tag: string` | `null` |
| `test_delay` | `tag: string` | `number` (ms) |
| `set_clash_mode` | `mode: string` | `null` |
| `list_profiles` | – | `Profile[]` |
| `create_profile` | `name: string`, `content: string` | `Profile` |
| `update_profile` | `id: number`, `name: string`, `content?: string` | `Profile` |
| `set_profile_auto_update` | `id: number`, `enabled: boolean` | `Profile` |
| `delete_profile` | `id: number` | `null` |
| `duplicate_profile` | `id: number`, `name: string` | `Profile` |
| `import_json` | `name: string`, `content: string`, `source?: string` | `Profile` |
| `import_subscription` | `url: string`, `name?: string` | `Profile` |
| `refresh_subscription` | `id: number` | `Profile` |
| `import_share_links` | `name: string`, `text: string` | `Profile` |
| `import_qr` | `imageBase64: string`, `name?: string` | `Profile` |
| `validate_content` | `content: string` | `{ok, error, outbounds: string[]}` |
| `export_profiles` | – | path string |
| `import_backup` | `content: string` | `number` |
| `get_settings` | – | `Settings` |
| `update_settings` | `patch: object` (partial settings) | `Settings` |
| `run_diagnostics` | – | `Diagnostics` |
| `get_tun_setup` | – | `TunSetup` |
| `install_tun_helper` | – | `TunSetup` (after installing) |
| `read_clipboard` | – | `string` |
| `open_config_dir` | – | `null` |
| `set_autostart` | `enabled: boolean` | `boolean` |
| `quit` | – | `null` |

`bootstrap.core` is the same shape as `CoreInfo` below (retained for
compatibility). `bootstrap.engineInfo` is the authoritative typed engine
description and capability set described below. `bootstrap.version` is the Veil
version string.

## Data shapes

```ts
type EngineState = {
  status: 'idle' | 'starting' | 'connected' | 'stopping' | 'error'
  mode: 'system' | 'tun' | null
  profileId: number | null
  profileName: string | null
  startedAt: number | null        // epoch seconds
  pid: number | null
  error: string | null
  proxy: { kind: string; detail: string } | null
  coreVersion: string | null
}

type Stats = {
  up: number          // bytes uploaded this session (Clash totals)
  down: number        // bytes downloaded this session
  upSpeed: number     // bytes/second from Clash /traffic
  downSpeed: number
  connections: number
  duration: number    // seconds since connected, 0 when idle
  latency: number | null   // ms from Clash proxy history, null when unknown
}

type Profile = {
  id: number
  name: string
  kind: 'local' | 'subscription' | 'links'
  source: string
  content: string           // raw stored sing-box JSON, never rewritten in place
  subscriptionUrl: string | null
  autoUpdate: boolean       // toggle with set_profile_auto_update
  updatedAt: number         // epoch seconds
  createdAt: number
  lastError: string | null
  size: number              // bytes of content
}

type Settings = {
  theme: 'dark' | 'light' | 'system'
  mode: 'system' | 'tun'
  mixedPort: number
  apiPort: number
  corePath: string | null
  autoRedirect: boolean
  autostart: boolean
  connectOnLaunch: boolean
  notifications: boolean
  minimizeToTray: boolean
  developerMode: boolean    // persisted; default false
  logLevel: string
  subscriptionIntervalHours: number
  language: string
}

type Outbound = {
  tag: string
  type: string              // Clash type, e.g. Selector, URLTest, VLESS
  now: string | null
  options: string[]
  delay: number | null
  kind: 'selector' | 'urltest' | 'other'
}

type Conn = {
  id: string
  host: string
  destination: string       // ip:port or host:port (IPv6 bracketed)
  network: string           // tcp / udp
  protocol: string          // HTTP / HTTPS / SOCKS5 / ...
  process: string           // process name or path, '' if unknown
  outbound: string          // final chain element
  rule: string
  upload: number
  download: number
  start: number             // epoch seconds
}

type LogEntry = {
  seq: number
  time: number              // epoch milliseconds
  level: string             // trace|debug|info|warn|error
  source: 'engine' | 'clash'
  message: string
}

type RouteRule = {
  index: number
  action: string            // route / reject / hijack-dns / sniff / resolve ...
  outbound: string | null
  match: string             // human-readable summary of the matcher fields
}

type RouteInfo = {
  rules: RouteRule[]
  final: string | null
  mode: string | null       // current Clash mode when connected
  modes: string[]           // ['rule','global','direct']
  dnsRules: RouteRule[]
  dnsFinal: string | null
}

type CoreInfo = { path: string | null; version: string | null; ok: boolean; error: string | null }

type EngineCapabilities = {
  clashApi: boolean              // binary build tags include with_clash_api
  tun: boolean                   // /dev/net/tun present on Linux + core found
  autoRoute: boolean             // tun + ip tooling available
  autoRedirect: boolean          // tun + nftables available
  nftables: boolean              // nft binary present (environment)
  ipv6: boolean                  // IPv6 enabled on this host
  dnsHijack: boolean             // tun + sing-box >= 1.10 supports dns_mode
  systemProxyBackends: string[]  // available CLI backends: gnome|kde|xfce
}

type EngineInfo = {
  id: 'sing-box'
  version: string | null         // from `sing-box version`; null if no binary
  path: string | null            // resolved core path; null if no binary
  api: 'clash'
  capabilities: EngineCapabilities
}

type Diagnostics = {
  core: CoreInfo
  capabilities: EngineCapabilities
  permissions: { user: string; helper: boolean; polkit: boolean; tunDevice: boolean; nft: boolean; root: boolean }
  interfaces: { name: string; addresses: string[]; up: boolean; default: boolean }[]
  tun: { present: boolean; name: string | null }
  dns: { mode: string; servers: string[] }
  errors: string[]
  tray: { available: boolean; detail: string }
  helper: { installed: boolean; path: string | null }
}

type TunSetup = {
  ready: boolean             // helper installed + policy + pkexec + /dev/net/tun + nft
  helperFound: boolean       // a veil-helper executable was located
  helperInstalled: boolean   // /usr/bin/veil-helper or /usr/local/bin/veil-helper
  helperPath: string | null  // located helper binary, null when not found
  policyInstalled: boolean   // dev.veil.helper.policy under /usr/share/polkit-1/actions
  pkexecAvailable: boolean
  tunDevice: boolean         // /dev/net/tun exists
  nft: boolean               // nft executable on PATH
  canInstall: boolean        // helperFound && pkexecAvailable && missing helper/policy
  missing: string[]          // human-readable labels for each unmet requirement
  detail: string             // summary suitable for display
}
```

`get_tun_setup` reports the live TUN prerequisite state; it never mutates
anything. `install_tun_helper` locates the helper (resources or installed
location), requires `pkexec`, and runs `pkexec <helper> install` (120s timeout).
The helper embeds the polkit policy and atomically installs itself to
`/usr/bin/veil-helper` as root; the GUI process never runs as root and no shell
is involved. Success resolves with the refreshed `TunSetup`; a missing helper
or pkexec rejects with `unsupported`, and a failed/cancelled authorization
rejects with `permission` plus a `detail` string.

`bootstrap.platform`:

```ts
type Platform = {
  desktop: string            // XDG_CURRENT_DESKTOP (or fallback), e.g. "Hyprland"
  session: string            // "wayland" | "x11" | "unknown"
  systemProxySupported: boolean
  systemProxyKind: string    // "gnome" | "kde" | "xfce" | "unsupported"
  trayAvailable: boolean
}
```

## Engine lifecycle

`engine://state` is the canonical lifecycle event. Every payload is a complete
`EngineState`. Statuses and allowed transitions:

| Status | Meaning | Allowed next states |
|---|---|---|
| `idle` | no engine; no polling | `starting` (connect) |
| `starting` | config checked, process spawned, waiting for health | `connected`, `error` |
| `connected` | Clash API `/version` health check passed | `stopping` (disconnect), `error` (unexpected exit) |
| `stopping` | SIGTERM/helper stop in progress | `idle`, `error` (stop failed; pid retained) |
| `error` | startup/health/exit/stop failure | `starting` (only when no engine process remains), `stopping` (disconnect retry) |

Rules:

- `connected` is **only** emitted after a real Clash API `/version` health check
  (12s limit) against the spawned engine. A spawned process alone is `starting`.
- Spawn/health failures kill the process, emit `engine://state` with `error` and
  the stderr tail, and emit `engine://notice`.
- Unexpected child exit while `connected` emits `error`, restores the desktop
  proxy, clears runtime state and emits a notice.
- `disconnect` emits `stopping`, then `idle` after SIGTERM (8s) or helper stop,
  proxy restore and runtime cleanup. If stopping fails, status becomes `error`
  and the pid/runtime state is retained so the UI can retry.
- Adoption on app restart: the backend reads `runtime/runtime.json`, verifies
  the pid is alive and `/proc/<pid>/cmdline` belongs to sing-box with that exact
  config, verifies the Clash API with the stored secret, then emits `connected`
  and reattaches stats/log/monitor tasks. Otherwise it cleans the runtime files,
  restores any leftover desktop proxy and stays `idle`.
- Live stats arrive only via `engine://stats` (once per second while connected);
  logs via `engine://log`; user notices via `engine://notice`.

## Events

| Event | Payload |
|---|---|
| `engine://state` | `EngineState` |
| `engine://stats` | `Stats` (emitted once per second while connected) |
| `engine://log` | `LogEntry` |
| `engine://notice` | `{ level: string; message: string }` |
| `profiles://changed` | `Profile[]` |

## Notes the UI can rely on

- `engine://state` is the only authority on engine state (see lifecycle above).
  `connect` resolves `connected` only after a real Clash API health check.
- `connect` with `mode: 'tun'` (or the persisted `settings.mode` when `mode` is
  omitted) first checks `get_tun_setup`. If `ready` is false it rejects before
  spawning anything with code `permission`, message `"TUN setup is required"`,
  and `detail` listing the missing requirements. The UI should offer
  `install_tun_helper` and then retry. `system` mode is unaffected.
- Stats are only produced by sing-box's Clash API. When idle, `get_stats`
  returns cached zeros and `duration: 0`; nothing is polled when idle.
- `proxy.kind` values: `gnome`, `kde`, `xfce`, `tun`, `unsupported`, `error`.
  `detail` is user-facing explanatory text. In TUN mode `kind` is `tun` and the
  system proxy is untouched. On Hyprland/sway/unknown desktops `kind` is
  `unsupported` and `detail` contains manual environment-variable instructions.
- Logs are capped at 2000 entries with monotonically increasing `seq`; use
  `get_logs(sinceSeq)` for incremental UI updates. Secrets, passwords, tokens,
  subscription query strings and URL userinfo are redacted server-side.
- `get_route` parses the active profile; `mode` comes from the live Clash API
  when connected. `get_outbounds`, `test_delay`, `select_outbound` and
  `set_clash_mode` require a connected engine and reject otherwise (except
  `get_outbounds`/`get_connections`, which return `[]` when idle).
- `select_outbound` persists the choice per profile when connected, and it is
  re-applied on the next successful connect.
- `import_json`, `create_profile`, `update_profile` and `import_subscription`
  validate with the real `sing-box check` binary. Clash YAML subscriptions are
  rejected with an explicit error; no conversion is faked.
- QR import requires `zbarimg`; it rejects with a clear error when unavailable
  or when no QR code is found.
- `update_settings` accepts a partial object; unknown keys are ignored and
  invalid values are rejected.
- `set_autostart` writes `~/.config/autostart/veil.desktop` and also updates the
  `autostart` setting.
- `engineInfo.capabilities` is derived from actual detection (binary tags,
  `/dev/net/tun`, `nft`, `ip`, IPv6 sysctls, available proxy CLIs). It is never
  hardcoded. `systemProxyBackends` lists mechanisms whose tools exist;
  `platform.systemProxyKind` says which one will actually be used for this
  desktop.
- Profile `id` values are JSON numbers (`number`), matching the SQLite row id.
  Frontend code must not send them as strings.
- No global shortcuts exist on Wayland. The UI must not show any.
