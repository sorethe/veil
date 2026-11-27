/**
 * Typed boundary to the Tauri backend.
 *
 * Every value the UI displays comes through here. Command names and payloads
 * mirror docs/CONTRACT.md exactly; arguments are camelCase because Tauri maps
 * them onto Rust's snake_case parameters.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Mode = "system" | "tun";
export type ThemeMode = "dark" | "light" | "system";
export type EngineStatus = "idle" | "starting" | "connected" | "stopping" | "error";
export type ProfileKind = "local" | "subscription" | "links";
export type OutboundKind = "selector" | "urltest" | "other";
/** Contract: profile identifiers are numbers. */
export type ProfileId = number;

export interface ProxyInfo {
  kind: string;
  detail: string;
}

export interface EngineState {
  status: EngineStatus;
  mode: Mode | null;
  profileId: ProfileId | null;
  profileName: string | null;
  startedAt: number | null;
  pid: number | null;
  error: string | null;
  proxy: ProxyInfo | null;
  coreVersion: string | null;
}

export interface CoreInfo {
  path: string | null;
  version: string | null;
  ok: boolean;
  error: string | null;
}

export interface PlatformInfo {
  desktop: string;
  session: string;
  systemProxySupported: boolean;
  systemProxyKind: string;
  trayAvailable: boolean;
}

export interface EngineCapabilities {
  clashApi: boolean;
  tun: boolean;
  autoRoute: boolean;
  autoRedirect: boolean;
  nftables: boolean;
  ipv6: boolean;
  dnsHijack: boolean;
  systemProxyBackends: string[];
}

export interface EngineInfo {
  id: "sing-box";
  version: string | null;
  path: string | null;
  api: "clash";
  capabilities: EngineCapabilities;
}

export type ErrorCode =
  | "not-ready"
  | "validation"
  | "permission"
  | "engine"
  | "network"
  | "io"
  | "unsupported"
  | "unknown";

export interface CommandError {
  code: ErrorCode;
  message: string;
  detail: string | null;
}

export interface Settings {
  theme: ThemeMode;
  mode: Mode;
  mixedPort: number;
  apiPort: number;
  corePath: string | null;
  autoRedirect: boolean;
  autostart: boolean;
  connectOnLaunch: boolean;
  notifications: boolean;
  minimizeToTray: boolean;
  logLevel: string;
  subscriptionIntervalHours: number;
  language: string;
}

export interface Profile {
  id: ProfileId;
  name: string;
  kind: ProfileKind;
  source: string;
  content: string;
  subscriptionUrl: string | null;
  autoUpdate: boolean;
  updatedAt: number;
  createdAt: number;
  lastError: string | null;
  size: number;
}

export interface BootstrapInfo {
  settings: Settings;
  profiles: Profile[];
  engine: EngineState;
  core: CoreInfo;
  engineInfo: EngineInfo;
  platform: PlatformInfo;
  version: string;
}

export interface Stats {
  up: number;
  down: number;
  upSpeed: number;
  downSpeed: number;
  connections: number;
  duration: number;
  latency: number | null;
}

export interface Outbound {
  tag: string;
  type: string;
  now: string | null;
  options: string[];
  delay: number | null;
  kind: OutboundKind;
}

export interface Conn {
  id: string;
  host: string;
  destination: string;
  network: string;
  protocol: string;
  process: string;
  outbound: string;
  rule: string;
  upload: number;
  download: number;
  start: number;
}

export interface LogEntry {
  seq: number;
  time: number;
  level: string;
  source: "engine" | "clash";
  message: string;
}

export interface RouteRule {
  index: number;
  action: string;
  outbound: string | null;
  match: string;
}

export interface RouteInfo {
  rules: RouteRule[];
  final: string | null;
  mode: string | null;
  modes: string[];
  dnsRules: RouteRule[];
  dnsFinal: string | null;
}

export interface PermissionsInfo {
  user: string;
  helper: boolean;
  polkit: boolean;
  tunDevice: boolean;
  nft: boolean;
  root: boolean;
}

export interface InterfaceInfo {
  name: string;
  addresses: string[];
  up: boolean;
  default: boolean;
}

export interface TunInfo {
  present: boolean;
  name: string | null;
}

export interface DnsInfo {
  mode: string;
  servers: string[];
}

export interface TrayInfo {
  available: boolean;
  detail: string;
}

export interface HelperInfo {
  installed: boolean;
  path: string | null;
}

export interface Diagnostics {
  core: CoreInfo;
  capabilities: EngineCapabilities;
  permissions: PermissionsInfo;
  interfaces: InterfaceInfo[];
  tun: TunInfo;
  dns: DnsInfo;
  errors: string[];
  tray: TrayInfo;
  helper: HelperInfo;
}

export interface Validation {
  ok: boolean;
  error: string | null;
  outbounds: string[];
}

/** Optional arguments are omitted rather than sent as null so Rust defaults apply. */
function args(entries: Record<string, string | number | null | undefined>): Record<string, string | number> {
  const out: Record<string, string | number> = {};
  for (const [key, value] of Object.entries(entries)) {
    if (value !== null && value !== undefined && value !== "") out[key] = value;
  }
  return out;
}

export const api = {
  bootstrap: () => invoke<BootstrapInfo>("bootstrap"),
  getState: () => invoke<EngineState>("get_state"),
  connect: (mode?: Mode, profileId?: ProfileId) =>
    invoke<EngineState>("connect", args({ mode, profileId })),
  disconnect: () => invoke<EngineState>("disconnect"),
  getStats: () => invoke<Stats>("get_stats"),
  getConnections: () => invoke<Conn[]>("get_connections"),
  closeConnection: (id: string) => invoke<null>("close_connection", { id }),
  getLogs: (sinceSeq?: number) =>
    invoke<LogEntry[]>("get_logs", sinceSeq === undefined ? {} : { sinceSeq }),
  clearLogs: () => invoke<null>("clear_logs"),
  exportLogs: () => invoke<string>("export_logs"),
  getRoute: () => invoke<RouteInfo>("get_route"),
  getOutbounds: () => invoke<Outbound[]>("get_outbounds"),
  selectOutbound: (group: string, tag: string) =>
    invoke<null>("select_outbound", { group, tag }),
  testDelay: (tag: string) => invoke<number>("test_delay", { tag }),
  setClashMode: (mode: string) => invoke<null>("set_clash_mode", { mode }),

  listProfiles: () => invoke<Profile[]>("list_profiles"),
  createProfile: (name: string, content: string) =>
    invoke<Profile>("create_profile", { name, content }),
  updateProfile: (id: ProfileId, name: string, content?: string) =>
    invoke<Profile>("update_profile", content === undefined ? { id, name } : { id, name, content }),
  deleteProfile: (id: ProfileId) => invoke<null>("delete_profile", { id }),
  duplicateProfile: (id: ProfileId, name: string) =>
    invoke<Profile>("duplicate_profile", { id, name }),
  importJson: (name: string, content: string, source?: string) =>
    invoke<Profile>("import_json", args({ name, content, source })),
  importSubscription: (url: string, name?: string) =>
    invoke<Profile>("import_subscription", args({ url, name })),
  refreshSubscription: (id: ProfileId) => invoke<Profile>("refresh_subscription", { id }),
  importShareLinks: (name: string, text: string) =>
    invoke<Profile>("import_share_links", { name, text }),
  importQr: (imageBase64: string, name?: string) =>
    invoke<Profile>("import_qr", args({ imageBase64, name })),
  validateContent: (content: string) => invoke<Validation>("validate_content", { content }),
  exportProfiles: () => invoke<string>("export_profiles"),
  importBackup: (content: string) => invoke<number>("import_backup", { content }),

  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (patch: Partial<Settings>) => invoke<Settings>("update_settings", { patch }),
  runDiagnostics: () => invoke<Diagnostics>("run_diagnostics"),
  readClipboard: () => invoke<string>("read_clipboard"),
  openConfigDir: () => invoke<null>("open_config_dir"),
  setAutostart: (enabled: boolean) => invoke<boolean>("set_autostart", { enabled }),
  quit: () => invoke<null>("quit"),
};

export const events = {
  onState: (cb: (state: EngineState) => void): Promise<UnlistenFn> =>
    listen<EngineState>("engine://state", (event) => cb(event.payload)),
  onStats: (cb: (stats: Stats) => void): Promise<UnlistenFn> =>
    listen<Stats>("engine://stats", (event) => cb(event.payload)),
  onLog: (cb: (entry: LogEntry) => void): Promise<UnlistenFn> =>
    listen<LogEntry>("engine://log", (event) => cb(event.payload)),
  onNotice: (cb: (notice: { level: string; message: string }) => void): Promise<UnlistenFn> =>
    listen<{ level: string; message: string }>("engine://notice", (event) => cb(event.payload)),
  onProfilesChanged: (cb: (profiles: Profile[]) => void): Promise<UnlistenFn> =>
    listen<Profile[]>("profiles://changed", (event) => cb(event.payload)),
};
