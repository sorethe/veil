# Veil backend / frontend contract

This file is the coordination artifact between the Rust backend (`src-tauri/`) and
the Svelte UI (`src/`). The backend implements exactly these commands and events.

- Every command is `async` on the Rust side and returns `Result<T, String>`.
- Command **names** are snake_case; command **arguments** are camelCase (Tauri 2
  default). Example: `invoke("connect", { profileId: 3, mode: "tun" })`.
- Optional arguments may be `null`/omitted.
- Rejections carry a human-readable error string. The UI should display it; the
  backend never fabricates success.
- All JSON is camelCase. Timestamps are epoch seconds unless stated otherwise.

## Commands

| Command | Arguments | Returns |
|---|---|---|
| `bootstrap` | – | `{settings, profiles, engine, core, platform, version}` |
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
| `read_clipboard` | – | `string` |
| `open_config_dir` | – | `null` |
| `set_autostart` | `enabled: boolean` | `boolean` |
| `quit` | – | `null` |

`bootstrap.core` is the same shape as `CoreInfo` below. `bootstrap.core` error is
non-null when the core is missing or unusable. `bootstrap.version` is the Veil
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
  autoUpdate: boolean
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

type Diagnostics = {
  core: CoreInfo
  permissions: { user: string; helper: boolean; polkit: boolean; tunDevice: boolean; nft: boolean; root: boolean }
  interfaces: { name: string; addresses: string[]; up: boolean; default: boolean }[]
  tun: { present: boolean; name: string | null }
  dns: { mode: string; servers: string[] }
  errors: string[]
  tray: { available: boolean; detail: string }
  helper: { installed: boolean; path: string | null }
}
```

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

## Events

| Event | Payload |
|---|---|
| `engine://state` | `EngineState` |
| `engine://stats` | `Stats` (emitted once per second while connected) |
| `engine://log` | `LogEntry` |
| `engine://notice` | `{ level: string; message: string }` |
| `profiles://changed` | `Profile[]` |

## Notes the UI can rely on

- `EngineState.status` is the only authority on engine state. `connect` resolves
  `connected` only after a real Clash API health check.
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
- No global shortcuts exist on Wayland. The UI must not show any.
