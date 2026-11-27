/**
 * Single reactive application store (Svelte 5 runes).
 *
 * Components read state from this module and call its actions; they never call
 * Tauri directly for shared state. Page-local queries stay in their views.
 */
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  api,
  events,
  type BootstrapInfo,
  type CoreInfo,
  type EngineCapabilities,
  type EngineInfo,
  type EngineState,
  type LogEntry,
  type Mode,
  type PlatformInfo,
  type Profile,
  type ProfileId,
  type Settings,
  type Stats,
  type ThemeMode,
} from "./api";
import { errorMessage } from "./format";

export const VIEW_IDS = [
  "overview",
  "profiles",
  "routing",
  "outbounds",
  "connections",
  "logs",
  "diagnostics",
  "settings",
] as const;
export type View = (typeof VIEW_IDS)[number];

export type ToastKind = "error" | "success" | "info";
export interface ToastItem {
  id: number;
  kind: ToastKind;
  title: string;
  detail?: string;
}

export interface TrafficSample {
  up: number;
  down: number;
  connections: number;
}

const LOG_LIMIT = 1500;
const HISTORY_LIMIT = 120;
const VIEW_KEY = "veil.view";

function emptyEngine(): EngineState {
  return {
    status: "idle",
    mode: null,
    profileId: null,
    profileName: null,
    startedAt: null,
    pid: null,
    error: null,
    proxy: null,
    coreVersion: null,
  };
}

function emptyStats(): Stats {
  return { up: 0, down: 0, upSpeed: 0, downSpeed: 0, connections: 0, duration: 0, latency: null };
}

function readView(): View {
  try {
    const stored = localStorage.getItem(VIEW_KEY);
    if (stored && (VIEW_IDS as readonly string[]).includes(stored)) return stored as View;
  } catch {
    /* storage can be unavailable in hardened webviews */
  }
  return "overview";
}

class AppStore {
  boot = $state<BootstrapInfo | null>(null);
  booting = $state(true);
  bootError = $state<string | null>(null);

  engine = $state<EngineState>(emptyEngine());
  stats = $state<Stats>(emptyStats());
  history = $state<TrafficSample[]>([]);
  profiles = $state<Profile[]>([]);

  logs = $state<LogEntry[]>([]);
  logPaused = $state(false);
  logBuffered = $state(0);

  toasts = $state<ToastItem[]>([]);
  view = $state<View>(readView());

  connectBusy = $state(false);
  systemDark = $state(true);

  #unlisten: UnlistenFn[] = [];
  #started = false;
  #toastId = 0;
  #pendingLogs: LogEntry[] = [];
  #media: MediaQueryList | null = null;
  #stateEventSeq = 0;
  #profilesEventSeq = 0;

  get settings(): Settings | null {
    return this.boot?.settings ?? null;
  }

  get platform(): PlatformInfo | null {
    return this.boot?.platform ?? null;
  }

  get core(): CoreInfo | null {
    return this.boot?.core ?? null;
  }

  get engineInfo(): EngineInfo | null {
    return this.boot?.engineInfo ?? null;
  }

  get capabilities(): EngineCapabilities | null {
    return this.boot?.engineInfo?.capabilities ?? null;
  }

  get clashApiReady(): boolean {
    return this.capabilities?.clashApi === true;
  }

  get tunReady(): boolean {
    const capabilities = this.capabilities;
    return (
      capabilities !== null &&
      capabilities.tun &&
      capabilities.autoRoute &&
      capabilities.dnsHijack
    );
  }

  get autoRedirectReady(): boolean {
    return this.capabilities?.autoRedirect === true;
  }

  get version(): string {
    return this.boot?.version ?? "";
  }

  get resolvedTheme(): "dark" | "light" {
    const theme: ThemeMode | undefined = this.boot?.settings.theme;
    if (!theme) {
      try {
        const cached = localStorage.getItem("veil.theme.resolved");
        if (cached === "light" || cached === "dark") return cached;
      } catch {
        /* ignore */
      }
      return "dark";
    }
    if (theme === "system") return this.systemDark ? "dark" : "light";
    return theme;
  }

  get connected(): boolean {
    return this.engine.status === "connected";
  }

  get busy(): boolean {
    return this.engine.status === "starting" || this.engine.status === "stopping";
  }

  get lastLogSeq(): number {
    return this.logs.length ? this.logs[this.logs.length - 1].seq : 0;
  }

  async init(): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    this.booting = true;
    this.bootError = null;
    this.watchTheme();
    await this.subscribe();
    const stateSeq = this.#stateEventSeq;
    const profilesSeq = this.#profilesEventSeq;
    try {
      const info = await api.bootstrap();
      this.boot = info;
      if (this.#stateEventSeq === stateSeq) this.engine = info.engine;
      if (this.#profilesEventSeq === profilesSeq) this.profiles = info.profiles;
      this.booting = false;
      void this.loadInitialData();
    } catch (error) {
      this.bootError = errorMessage(error);
      this.booting = false;
    }
  }

  async retryBoot(): Promise<void> {
    await this.dispose();
    this.#started = false;
    await this.init();
  }

  /** Re-reads bootstrap data (core path, platform, settings) without restarting the UI. */
  async reloadBootstrap(): Promise<string | null> {
    const stateSeq = this.#stateEventSeq;
    const profilesSeq = this.#profilesEventSeq;
    try {
      const info = await api.bootstrap();
      this.boot = info;
      if (this.#stateEventSeq === stateSeq) this.engine = info.engine;
      if (this.#profilesEventSeq === profilesSeq) this.profiles = info.profiles;
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  async dispose(): Promise<void> {
    for (const off of this.#unlisten) {
      try {
        off();
      } catch {
        /* listener already gone */
      }
    }
    this.#unlisten = [];
    this.#media?.removeEventListener("change", this.onSystemTheme);
  }

  private async subscribe(): Promise<void> {
    try {
      this.#unlisten.push(
        await events.onState((state) => {
          this.#stateEventSeq += 1;
          this.engine = state;
          if (state.status !== "connected") {
            this.history = [];
          }
          if (state.status === "idle" || state.status === "starting" || state.status === "error") {
            this.stats = emptyStats();
          }
        }),
        await events.onStats((stats) => {
          this.stats = stats;
          this.history.push({
            up: stats.upSpeed,
            down: stats.downSpeed,
            connections: stats.connections,
          });
          if (this.history.length > HISTORY_LIMIT) this.history.splice(0, this.history.length - HISTORY_LIMIT);
        }),
        await events.onLog((entry) => {
          if (this.logPaused) {
            this.#pendingLogs.push(entry);
            if (this.#pendingLogs.length > LOG_LIMIT) this.#pendingLogs.splice(0, this.#pendingLogs.length - LOG_LIMIT);
            this.logBuffered = this.#pendingLogs.length;
          } else {
            this.mergeLogs([entry]);
          }
        }),
        await events.onNotice((notice) => {
          const kind = notice.level === "error" ? "error" : notice.level === "warn" ? "info" : "info";
          this.notify(kind, notice.level === "warn" ? "Engine warning" : "Engine", notice.message);
        }),
        await events.onProfilesChanged((profiles) => {
          this.#profilesEventSeq += 1;
          this.profiles = profiles;
        }),
      );
    } catch {
      // Event bridge is unavailable outside Tauri; bootstrap reports the real error.
    }
  }

  private async loadInitialData(): Promise<void> {
    try {
      this.mergeLogs(await api.getLogs());
    } catch {
      /* the Logs view offers an explicit reload with a real error */
    }
  }

  private watchTheme(): void {
    if (typeof window === "undefined" || !window.matchMedia) return;
    this.#media = window.matchMedia("(prefers-color-scheme: dark)");
    this.systemDark = this.#media.matches;
    this.#media.addEventListener("change", this.onSystemTheme);
  }

  private onSystemTheme = (event: MediaQueryListEvent): void => {
    this.systemDark = event.matches;
  };

  setView(view: View): void {
    this.view = view;
    try {
      localStorage.setItem(VIEW_KEY, view);
    } catch {
      /* ignore */
    }
    if (typeof window !== "undefined") window.scrollTo({ top: 0, behavior: "auto" });
  }

  async connect(profileId?: ProfileId, mode?: Mode): Promise<boolean> {
    if (this.connectBusy) return false;
    this.connectBusy = true;
    try {
      await api.connect(mode ?? this.settings?.mode, profileId);
      return true;
    } catch (error) {
      this.notify("error", "Could not connect", errorMessage(error));
      return false;
    } finally {
      this.connectBusy = false;
    }
  }

  async disconnect(): Promise<boolean> {
    if (this.connectBusy) return false;
    this.connectBusy = true;
    try {
      await api.disconnect();
      return true;
    } catch (error) {
      this.notify("error", "Could not disconnect", errorMessage(error));
      return false;
    } finally {
      this.connectBusy = false;
    }
  }

  async refreshProfiles(silent = false): Promise<void> {
    try {
      this.profiles = await api.listProfiles();
    } catch (error) {
      if (!silent) this.notify("error", "Could not load profiles", errorMessage(error));
    }
  }

  /** Returns an error string instead of throwing so forms can render it inline. */
  async saveSettings(patch: Partial<Settings>): Promise<string | null> {
    try {
      const next = await api.updateSettings(patch);
      if (this.boot) this.boot.settings = next;
      return null;
    } catch (error) {
      return errorMessage(error);
    }
  }

  async cycleTheme(): Promise<void> {
    const next: ThemeMode = this.resolvedTheme === "dark" ? "light" : "dark";
    const error = await this.saveSettings({ theme: next });
    if (error) this.notify("error", "Could not change theme", error);
  }

  async setAutostart(enabled: boolean): Promise<string | null> {
    try {
      const applied = await api.setAutostart(enabled);
      if (this.boot) this.boot.settings = { ...this.boot.settings, autostart: applied };
      return applied === enabled ? null : "Desktop refused the autostart change.";
    } catch (error) {
      return errorMessage(error);
    }
  }

  setLogPaused(paused: boolean): void {
    if (this.logPaused === paused) return;
    this.logPaused = paused;
    if (!paused) {
      this.mergeLogs(this.#pendingLogs);
      this.#pendingLogs = [];
      this.logBuffered = 0;
      void this.loadLogs(true);
    }
  }

  async loadLogs(silent = false): Promise<void> {
    try {
      this.mergeLogs(await api.getLogs(this.lastLogSeq || undefined));
    } catch (error) {
      if (!silent) this.notify("error", "Could not load logs", errorMessage(error));
    }
  }

  async clearLogs(): Promise<boolean> {
    try {
      await api.clearLogs();
      this.logs = [];
      this.#pendingLogs = [];
      this.logBuffered = 0;
      return true;
    } catch (error) {
      this.notify("error", "Could not clear logs", errorMessage(error));
      return false;
    }
  }

  private mergeLogs(incoming: LogEntry[]): void {
    if (!incoming.length) return;
    if (this.logPaused) {
      this.#pendingLogs.push(...incoming);
      if (this.#pendingLogs.length > LOG_LIMIT) this.#pendingLogs.splice(0, this.#pendingLogs.length - LOG_LIMIT);
      this.logBuffered = this.#pendingLogs.length;
      return;
    }
    const bySeq = new Map<number, LogEntry>();
    for (const entry of this.logs) bySeq.set(entry.seq, entry);
    for (const entry of incoming) bySeq.set(entry.seq, entry);
    const merged = [...bySeq.values()].sort((a, b) => a.seq - b.seq);
    this.logs = merged.slice(-LOG_LIMIT);
  }

  notify(kind: ToastKind, title: string, detail?: string): void {
    const id = ++this.#toastId;
    this.toasts.push({ id, kind, title, detail });
    const ttl = kind === "error" ? 9000 : 4500;
    window.setTimeout(() => this.dismiss(id), ttl);
  }

  dismiss(id: number): void {
    const index = this.toasts.findIndex((toast) => toast.id === id);
    if (index >= 0) this.toasts.splice(index, 1);
  }
}

export const app = new AppStore();
