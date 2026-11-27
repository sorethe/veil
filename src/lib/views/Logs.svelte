<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api, type LogEntry } from "../api";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { errorMessage, formatClock } from "../format";
  import { app } from "../state.svelte";

  const PREFS_KEY = "veil.logs.prefs";
  const RENDER_LIMIT = 800;

  let query = $state("");
  let source = $state("all");
  let autoScroll = $state(true);
  let levels = $state<Record<string, boolean>>({ debug: true, info: true, warn: true, error: true });
  let viewport = $state<HTMLDivElement | null>(null);
  let exporting = $state(false);
  let confirmClear = $state(false);
  let prefsLoaded = false;

  const selectedLevels = $derived(Object.entries(levels).filter(([, enabled]) => enabled).map(([level]) => level));
  const counts = $derived(countLevels(app.logs));
  const filtered = $derived(app.logs.filter((entry) => matches(entry, query, source, levels, selectedLevels.length)));
  const displayed = $derived(filtered.slice(-RENDER_LIMIT));
  const live = $derived(app.engine.status === "connected");

  $effect(() => {
    const anchor = displayed.length ? displayed[displayed.length - 1].seq : 0;
    if (!autoScroll || app.logPaused || !anchor) return;
    void tick().then(() => {
      if (viewport) viewport.scrollTop = viewport.scrollHeight;
    });
  });

  $effect(() => {
    if (!prefsLoaded) return;
    try {
      localStorage.setItem(
        PREFS_KEY,
        JSON.stringify({ levels: selectedLevels, source, autoScroll, query }),
      );
    } catch {
      /* UI preferences are best effort */
    }
  });

  onMount(() => {
    loadPrefs();
    void app.loadLogs();
  });

  function loadPrefs(): void {
    try {
      const raw = localStorage.getItem(PREFS_KEY);
      if (raw) {
        const parsed: unknown = JSON.parse(raw);
        if (parsed && typeof parsed === "object") {
          if ("autoScroll" in parsed && typeof parsed.autoScroll === "boolean") autoScroll = parsed.autoScroll;
          if ("source" in parsed && typeof parsed.source === "string" && ["all", "engine", "clash"].includes(parsed.source)) {
            source = parsed.source;
          }
          if ("query" in parsed && typeof parsed.query === "string") query = parsed.query;
          if ("levels" in parsed && Array.isArray(parsed.levels)) {
            const stored = parsed.levels.filter((level): level is string => typeof level === "string");
            if (stored.length) {
              levels = { debug: false, info: false, warn: false, error: false };
              for (const level of stored) {
                if (level in levels) levels[level] = true;
              }
            }
          }
        }
      }
    } catch {
      /* ignore malformed preferences */
    }
    prefsLoaded = true;
  }

  function countLevels(entries: LogEntry[]): Record<string, number> {
    const result: Record<string, number> = { debug: 0, info: 0, warn: 0, error: 0 };
    for (const entry of entries) {
      const level = entry.level.toLowerCase();
      if (level in result) result[level] += 1;
    }
    return result;
  }

  function matches(
    entry: LogEntry,
    needle: string,
    sourceFilter: string,
    levelMap: Record<string, boolean>,
    enabledCount: number,
  ): boolean {
    if (sourceFilter !== "all" && entry.source !== sourceFilter) return false;
    const level = entry.level.toLowerCase();
    if (level in levelMap) {
      if (!levelMap[level]) return false;
    } else if (enabledCount !== 4) {
      return false;
    }
    const trimmed = needle.trim().toLowerCase();
    if (!trimmed) return true;
    return `${entry.message} ${entry.source} ${entry.level}`.toLowerCase().includes(trimmed);
  }

  function toggleLevel(level: string): void {
    levels[level] = !levels[level];
  }

  function clearFilters(): void {
    query = "";
    source = "all";
    levels = { debug: true, info: true, warn: true, error: true };
  }

  async function exportLogs(): Promise<void> {
    if (exporting) return;
    exporting = true;
    try {
      const path = await api.exportLogs();
      app.notify("success", "Logs exported", path);
    } catch (cause) {
      app.notify("error", "Export failed", errorMessage(cause));
    } finally {
      exporting = false;
    }
  }

  async function clearLogs(): Promise<void> {
    const cleared = await app.clearLogs();
    if (cleared) {
      confirmClear = false;
      app.notify("success", "Logs cleared");
    }
  }

  function levelClass(level: string): string {
    const normalized = level.toLowerCase();
    if (normalized === "error" || normalized === "fatal" || normalized === "panic") return "log__level--error";
    if (normalized === "warn") return "log__level--warn";
    if (normalized === "debug" || normalized === "trace") return "log__level--debug";
    return "log__level--info";
  }
</script>

<div class="page-head">
  <div class="page-head__text">
    <span class="kicker">Logs</span>
    <h1 class="page-title">Engine journal</h1>
    <p class="page-desc">
      Engine and Clash messages, newest last. The backend redacts credentials before entries reach this view.
    </p>
  </div>
  <div class="head-actions">
    <span class="tag {live ? 'tag--accent' : ''}">{live ? "live feed" : "engine " + app.engine.status}</span>
    {#if app.logPaused}<span class="tag tag--warning">paused</span>{/if}
    <button type="button" class="btn" disabled={exporting || !app.logs.length} onclick={() => void exportLogs()}>
      <Icon name="export" size={12} /> {exporting ? "Exporting" : "Export"}
    </button>
    <button type="button" class="btn" disabled={!app.logs.length} onclick={() => (confirmClear = true)}>
      <Icon name="trash" size={12} /> Clear
    </button>
  </div>
</div>

{#if !live}
  <div class="banner">
    <Icon name="info" size={15} />
    <div class="grow">
      <p class="banner__title">Engine is {app.engine.status}</p>
      <p class="banner__body">Showing buffered entries. New entries appear when the engine emits them.</p>
    </div>
  </div>
{/if}

<div class="toolbar">
  <div class="search">
    <span class="search__icon"><Icon name="search" size={14} /></span>
    <input
      class="input"
      type="search"
      placeholder="Search messages…"
      aria-label="Search logs"
      bind:value={query}
    />
  </div>

  <div class="levels" role="group" aria-label="Log levels">
    {#each ["debug", "info", "warn", "error"] as level (level)}
      <button
        type="button"
        class="level-chip"
        class:level-chip--on={levels[level]}
        aria-pressed={levels[level]}
        onclick={() => toggleLevel(level)}
      >
        {level}<span class="level-chip__count">{counts[level]}</span>
      </button>
    {/each}
  </div>

  <select class="select select--inline" bind:value={source} aria-label="Log source">
    <option value="all">All sources</option>
    <option value="engine">Engine</option>
    <option value="clash">Clash</option>
  </select>

  <div class="toggle">
    <Toggle checked={autoScroll} label="Auto-scroll logs" onchange={(next) => (autoScroll = next)} />
    <span class="small muted">Auto-scroll</span>
  </div>

  <div class="row right">
    <button
      type="button"
      class="btn btn--sm"
      class:btn--primary={app.logPaused}
      onclick={() => app.setLogPaused(!app.logPaused)}
    >
      {app.logPaused ? "Resume" : "Pause"}
    </button>
    <button type="button" class="icon-btn" aria-label="Reload logs" title="Reload" onclick={() => void app.loadLogs()}>
      <Icon name="refresh" size={14} />
    </button>
  </div>
</div>

{#if app.logPaused && app.logBuffered > 0}
  <p class="buffered mono">
    {app.logBuffered} {app.logBuffered === 1 ? "entry" : "entries"} buffered while paused · resume to merge
  </p>
{/if}

<section class="panel log-panel" aria-label="Log entries">
  <div class="log-head mono" aria-hidden="true">
    <span>Time</span>
    <span>Level</span>
    <span>Source</span>
    <span>Message</span>
  </div>
  {#if !app.logs.length}
    <EmptyState
      title="No log entries"
      description={live ? "The engine is live but has not emitted any entries yet." : "Logs appear once the engine runs. Start a session or reload."}
      icon="logs"
    >
      <button type="button" class="btn btn--sm" onclick={() => void app.loadLogs()}>Reload</button>
    </EmptyState>
  {:else if !filtered.length}
    <EmptyState
      title="No matching entries"
      description="Nothing in the current buffer matches the query, level, or source filters."
      icon="search"
    >
      <button type="button" class="btn btn--sm" onclick={clearFilters}>Clear filters</button>
    </EmptyState>
  {:else}
    <div class="log-scroll" bind:this={viewport}>
      {#if filtered.length > RENDER_LIMIT}
        <p class="truncated mono">
          {filtered.length - RENDER_LIMIT} earlier matching entries are outside the render window; export for the
          full buffer.
        </p>
      {/if}
      {#each displayed as entry (entry.seq)}
        <div class="log {levelClass(entry.level)}">
          <span class="log__time mono">{formatClock(entry.time)}</span>
          <span class="log__level mono {levelClass(entry.level)}">{entry.level}</span>
          <span class="log__source mono">{entry.source}</span>
          <span class="log__message">{entry.message}</span>
        </div>
      {/each}
    </div>
    <div class="log-foot mono">
      <span>{displayed.length} of {filtered.length} matching · {app.logs.length} buffered</span>
      {#if app.logPaused}<span class="warning">paused</span>{:else}<span class="faint">following</span>{/if}
    </div>
  {/if}
</section>

{#if confirmClear}
  <Modal
    title="Clear log buffer"
    subtitle={`${app.logs.length} stored entries`}
    onclose={() => (confirmClear = false)}
    width="440px"
  >
    <p class="small">
      This clears the in-memory buffer shown here. The backend may keep its own log file; export first if you
      need the current entries.
    </p>
    {#snippet footer()}
      <button type="button" class="btn" onclick={() => (confirmClear = false)}>Cancel</button>
      <button type="button" class="btn btn--danger" onclick={() => void clearLogs()}>Clear buffer</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-bottom: var(--space-3);
  }

  .levels {
    display: flex;
    gap: 2px;
    border: 1px solid var(--line);
    padding: 2px;
    background: var(--field);
  }

  .level-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 7px;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    border: 1px solid transparent;
  }

  .level-chip:hover {
    color: var(--muted);
  }

  .level-chip--on {
    color: var(--text);
    background: var(--raised);
    border-color: var(--line);
  }

  .level-chip__count {
    font-size: 9px;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .buffered {
    font-size: var(--fs-2xs);
    color: var(--warning);
    margin-bottom: var(--space-2);
  }

  .log-panel {
    display: flex;
    flex-direction: column;
    min-height: 320px;
  }

  .log-head,
  .log {
    display: grid;
    grid-template-columns: 84px 54px 50px minmax(0, 1fr);
    gap: 10px;
    padding: 3px 12px;
    align-items: baseline;
  }

  .log-head {
    position: sticky;
    top: 0;
    z-index: 1;
    padding-top: 7px;
    padding-bottom: 7px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    font-size: var(--fs-2xs);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .log-scroll {
    flex: 1;
    overflow: auto;
    max-height: calc(100vh - 330px);
    padding: 4px 0;
    background: var(--surface);
  }

  .truncated {
    font-size: var(--fs-2xs);
    color: var(--faint);
    padding: 4px 12px 6px;
    border-bottom: 1px dashed var(--line);
    margin-bottom: 3px;
  }

  .log {
    border-bottom: 1px solid color-mix(in srgb, var(--line) 45%, transparent);
    font-size: var(--fs-xs);
  }

  .log:hover {
    background: var(--row-hover);
  }

  .log__time {
    color: var(--faint);
    font-size: var(--fs-2xs);
    white-space: nowrap;
  }

  .log__level {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    white-space: nowrap;
  }

  .log__level--debug {
    color: var(--faint);
  }

  .log__level--info {
    color: var(--muted);
  }

  .log__level--warn {
    color: var(--warning);
  }

  .log__level--error {
    color: var(--error);
  }

  .log__source {
    color: var(--faint);
    font-size: var(--fs-2xs);
    white-space: nowrap;
    text-transform: uppercase;
  }

  .log__message {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    min-width: 0;
  }

  .log-foot {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 6px 12px;
    border-top: 1px solid var(--line);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  .right {
    margin-left: auto;
  }

  @media (max-width: 760px) {
    .log-head,
    .log {
      grid-template-columns: 70px 48px 40px minmax(0, 1fr);
      gap: 6px;
      padding-left: 8px;
      padding-right: 8px;
    }
  }
</style>
