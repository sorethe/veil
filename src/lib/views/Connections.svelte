<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Conn } from "../api";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import { errorMessage, formatAge, formatBytes } from "../format";
  import { app, type View } from "../state.svelte";

  let { navigate }: { navigate: (view: View) => void } = $props();

  let connections = $state<Conn[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let query = $state("");
  let network = $state("all");
  let closing = $state<string | null>(null);
  let visible = $state(true);
  let now = $state(Date.now());
  let inFlight = false;

  const networks = $derived([
    "all",
    ...Array.from(new Set(connections.map((conn) => conn.network).filter(Boolean))),
  ]);
  const filtered = $derived(
    connections.filter((conn) => matches(conn, query.trim().toLowerCase(), network)),
  );
  const live = $derived(app.engine.status === "connected");

  $effect(() => {
    if (!live || !visible) {
      if (!live) {
        connections = [];
        error = null;
      }
      return;
    }
    void load(true);
    const timer = window.setInterval(() => {
      void load(false);
    }, 1500);
    return () => window.clearInterval(timer);
  });

  onMount(() => {
    visible = !document.hidden;
    const onChange = () => {
      visible = !document.hidden;
    };
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  });

  function matches(conn: Conn, needle: string, networkFilter: string): boolean {
    if (networkFilter !== "all" && conn.network !== networkFilter) return false;
    if (!needle) return true;
    return [
      conn.host,
      conn.destination,
      conn.process,
      conn.outbound,
      conn.rule,
      conn.protocol,
      conn.network,
    ]
      .join(" ")
      .toLowerCase()
      .includes(needle);
  }

  async function load(showLoading: boolean): Promise<void> {
    if (inFlight) return;
    inFlight = true;
    if (showLoading) loading = true;
    try {
      connections = await api.getConnections();
      now = Date.now();
      error = null;
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      inFlight = false;
      loading = false;
    }
  }

  async function closeConn(conn: Conn): Promise<void> {
    if (closing) return;
    closing = conn.id;
    try {
      await api.closeConnection(conn.id);
      await load(false);
    } catch (cause) {
      app.notify("error", "Could not close connection", errorMessage(cause));
    } finally {
      closing = null;
    }
  }

  function clearFilters(): void {
    query = "";
    network = "all";
  }
</script>

<div class="page-head">
  <div class="page-head__text">
    <span class="kicker">Connections</span>
    <h1 class="page-title">Live streams</h1>
    <p class="page-desc">
      Active connections reported by the engine. This view refreshes every 1.5 seconds only while
      the engine is live and the window is visible.
    </p>
  </div>
  <div class="head-actions">
    <span class="tag {live ? 'tag--accent' : ''}">{live ? "polling 1.5s" : "stopped"}</span>
    <button type="button" class="btn" disabled={!live || loading} onclick={() => void load(true)}>
      <Icon name="refresh" size={12} /> Refresh
    </button>
  </div>
</div>

{#if error}
  <div class="banner banner--error" role="alert">
    <Icon name="error" size={16} />
    <div class="grow">
      <p class="banner__title">Could not read connections</p>
      <p class="banner__body mono">{error}</p>
    </div>
    <button type="button" class="btn btn--sm" onclick={() => void load(true)}>Retry</button>
  </div>
{/if}

{#if !live}
  <section class="panel">
    <EmptyState
      title={app.engine.status === "starting" ? "Engine is starting" : "Engine is not connected"}
      description="Connection tracking begins when a session is live. Start one from Overview, then return here."
      icon="connections"
    >
      <button type="button" class="btn btn--sm" onclick={() => navigate("overview")}>
        Overview
      </button>
    </EmptyState>
  </section>
{:else if loading && !connections.length}
  <section class="panel">
    <div class="panel__body"><p class="muted small">Reading active connections…</p></div>
  </section>
{:else if !connections.length}
  <section class="panel">
    <EmptyState
      title={error ? "Connection data unavailable" : "No active connections"}
      description={error
        ? "The last read failed; the error banner above carries the backend message."
        : "Streams opened through the engine appear here. Idle sessions legitimately have none."}
      icon={error ? "error" : "connections"}
    >
      <button type="button" class="btn btn--sm" onclick={() => void load(true)}>
        {error ? "Retry" : "Refresh"}
      </button>
    </EmptyState>
  </section>
{:else}
  <div class="filters connections__filters">
    <div class="search">
      <span class="search__icon"><Icon name="search" size={14} /></span>
      <input
        class="input"
        type="search"
        placeholder="Filter host, process, outbound…"
        aria-label="Filter connections"
        bind:value={query}
      />
    </div>
    <label class="inline-field">
      <span class="field__label">Network</span>
      <select class="select select--inline" bind:value={network} aria-label="Filter by network">
        {#each networks as option (option)}
          <option value={option}>{option === "all" ? "All networks" : option}</option>
        {/each}
      </select>
    </label>
    <span class="mono small faint right">
      {filtered.length} of {connections.length}
    </span>
  </div>

  {#if !filtered.length}
    <section class="panel">
      <EmptyState
        title="No matching connections"
        description="No active stream matches the current query and network filter."
        icon="search"
      >
        <button type="button" class="btn btn--sm" onclick={clearFilters}>Clear filters</button>
      </EmptyState>
    </section>
  {:else}
    <section class="panel" aria-label="Active connections">
      <div class="table-wrap connections-scroll">
        <table class="table connections">
          <thead>
            <tr>
              <th>Host / destination</th>
              <th>Net</th>
              <th>Process</th>
              <th>Outbound</th>
              <th>Rule</th>
              <th class="ta-r">Traffic</th>
              <th class="ta-r">Age</th>
              <th class="ta-r"><span class="sr-only">Close</span></th>
            </tr>
          </thead>
          <tbody>
            {#each filtered as conn (conn.id)}
              <tr>
                <td>
                  <div class="dest">
                    <span class="dest__host truncate" title={conn.host}>{conn.host || "—"}</span>
                    <span class="dest__addr mono truncate" title={conn.destination}>
                      {conn.destination}
                    </span>
                  </div>
                </td>
                <td>
                  <div class="net">
                    <span class="tag">{conn.network || "—"}</span>
                    <span class="net__protocol mono">{conn.protocol}</span>
                  </div>
                </td>
                <td>
                  <span class="mono detail truncate process" title={conn.process}>
                    {conn.process || "—"}
                  </span>
                </td>
                <td><span class="mono detail">{conn.outbound}</span></td>
                <td><span class="mono detail rule" title={conn.rule}>{conn.rule || "—"}</span></td>
                <td class="ta-r">
                  <div class="traffic">
                    <span>↑ {formatBytes(conn.upload)}</span>
                    <span>↓ {formatBytes(conn.download)}</span>
                  </div>
                </td>
                <td class="ta-r">
                  <span class="mono detail nowrap">{formatAge(conn.start, now)}</span>
                </td>
                <td class="ta-r">
                  <button
                    type="button"
                    class="icon-btn icon-btn--danger"
                    aria-label={`Close connection to ${conn.host || conn.destination}`}
                    title="Close connection"
                    disabled={closing !== null}
                    onclick={() => void closeConn(conn)}
                  >
                    <Icon name="close" size={13} />
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}
{/if}

<style>
  .connections__filters {
    margin-bottom: var(--space-3);
  }

  .inline-field {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .inline-field .field__label {
    white-space: nowrap;
  }

  .connections-scroll {
    max-height: calc(100vh - 250px);
  }

  .connections th:last-child,
  .connections td:last-child {
    text-align: right;
  }

  .dest {
    display: grid;
    gap: 1px;
    min-width: 180px;
    max-width: 320px;
  }

  .dest__host {
    font-size: var(--fs-sm);
  }

  .dest__addr {
    font-size: var(--fs-2xs);
    color: var(--faint);
  }

  .net {
    display: grid;
    gap: 2px;
    justify-items: start;
  }

  .net__protocol {
    font-size: 9px;
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .detail {
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .process {
    max-width: 150px;
    display: block;
  }

  .rule {
    max-width: 160px;
    display: inline-block;
  }

  .traffic {
    display: grid;
    gap: 1px;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    justify-items: end;
    white-space: nowrap;
  }
</style>
