/**
 * Shared outbound state for Overview and the Outbounds page.
 *
 * Both views render the same engine snapshot, delays, and switching actions
 * from here so selection logic cannot diverge. Nothing is written optimistically:
 * the snapshot is re-read after every command and the backend stays authoritative.
 */
import { api, type EngineStatus, type Outbound } from "./api";
import { errorMessage } from "./format";
import { app } from "./state.svelte";

export type DelayReading = number | "error" | null;

class OutboundStore {
  items = $state<Outbound[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  /** Group tag whose selection command is in flight. */
  switching = $state<string | null>(null);
  /** Outbound tag whose delay test is in flight. */
  testing = $state<string | null>(null);
  /** Local test results keyed by outbound tag; backend delay is the fallback. */
  delays = $state<Record<string, number | "error">>({});

  groups = $derived(
    this.items.filter((outbound) => outbound.kind === "selector" || outbound.kind === "urltest"),
  );

  #engineKey = "";

  /** Reset and reload whenever the engine session changes. Idempotent. */
  syncEngine(status: EngineStatus, startedAt: number | null): void {
    const key = `${status}:${startedAt ?? 0}`;
    if (key === this.#engineKey) return;
    this.#engineKey = key;
    this.delays = {};
    this.switching = null;
    this.testing = null;
    this.error = null;
    if (status === "connected") {
      void this.load();
    } else {
      this.items = [];
      this.loading = false;
    }
  }

  async load(silent = false): Promise<void> {
    if (app.engine.status !== "connected") {
      this.loading = false;
      return;
    }
    if (!silent) this.loading = true;
    try {
      const next = await api.getOutbounds();
      // The session may have stopped while the request was in flight.
      if (app.engine.status === "connected") {
        this.items = next;
        this.error = null;
      }
    } catch (cause) {
      this.items = [];
      this.error = errorMessage(cause);
    } finally {
      this.loading = false;
    }
  }

  async select(group: Outbound, tag: string): Promise<void> {
    if (this.switching || app.engine.status !== "connected") return;
    if (group.kind !== "selector") return;
    if (!tag || tag === group.now) return;
    this.switching = group.tag;
    try {
      await api.selectOutbound(group.tag, tag);
      await this.load(true);
      app.notify("success", "Outbound selected", `${group.tag} → ${tag}`);
    } catch (cause) {
      app.notify("error", "Could not switch outbound", errorMessage(cause));
      await this.load(true);
    } finally {
      this.switching = null;
    }
  }

  async test(tag: string): Promise<void> {
    if (this.testing || !tag || app.engine.status !== "connected") return;
    this.testing = tag;
    try {
      const milliseconds = await api.testDelay(tag);
      this.delays = { ...this.delays, [tag]: milliseconds };
    } catch (cause) {
      this.delays = { ...this.delays, [tag]: "error" };
      app.notify("error", `Delay test failed · ${tag}`, errorMessage(cause));
    } finally {
      this.testing = null;
    }
  }

  delayOf(outbound: Outbound): DelayReading {
    return this.delays[outbound.tag] ?? outbound.delay;
  }
}

export const outboundStore = new OutboundStore();
