<script lang="ts">
  import { api, type Outbound } from "../api";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import { errorMessage, formatDelay } from "../format";
  import { app, type View } from "../state.svelte";

  let { navigate }: { navigate: (view: View) => void } = $props();

  let outbounds = $state<Outbound[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let switching = $state<string | null>(null);
  let testing = $state<string | null>(null);
  let delays = $state<Record<string, number | "error">>({});
  let lastEngineKey = "";

  const groups = $derived(outbounds.filter((outbound) => outbound.kind === "selector" || outbound.kind === "urltest"));
  const others = $derived(outbounds.filter((outbound) => outbound.kind !== "selector" && outbound.kind !== "urltest"));

  $effect(() => {
    const key = `${app.engine.status}:${app.engine.startedAt ?? 0}`;
    if (key === lastEngineKey) return;
    lastEngineKey = key;
    delays = {};
    if (app.engine.status === "connected") {
      void load();
    } else {
      loading = false;
    }
  });

  async function load(silent = false): Promise<void> {
    if (!silent) loading = true;
    error = null;
    try {
      outbounds = await api.getOutbounds();
    } catch (cause) {
      outbounds = [];
      error = errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  async function switchTag(group: Outbound, tag: string): Promise<void> {
    if (!tag || switching) return;
    switching = group.tag;
    try {
      await api.selectOutbound(group.tag, tag);
      await load(true);
      app.notify("success", "Outbound selected", `${group.tag} → ${tag}`);
    } catch (cause) {
      app.notify("error", "Could not switch outbound", errorMessage(cause));
    } finally {
      switching = null;
    }
  }

  async function test(outbound: Outbound): Promise<void> {
    if (testing) return;
    testing = outbound.tag;
    try {
      const milliseconds = await api.testDelay(outbound.tag);
      delays[outbound.tag] = milliseconds;
    } catch (cause) {
      delays[outbound.tag] = "error";
      app.notify("error", `Delay test failed · ${outbound.tag}`, errorMessage(cause));
    } finally {
      testing = null;
    }
  }

  function delayOf(outbound: Outbound): number | "error" | null {
    return delays[outbound.tag] ?? outbound.delay;
  }

  function selectedIn(outbound: Outbound): string | null {
    const group = groups.find((candidate) => candidate.now === outbound.tag);
    return group?.tag ?? null;
  }

  function delayLabel(value: number | "error" | null): string {
    if (value === "error") return "error";
    return formatDelay(value);
  }
</script>

<div class="page-head">
  <div class="page-head__text">
    <span class="kicker">Outbounds</span>
    <h1 class="page-title">Egress</h1>
    <p class="page-desc">
      Groups and nodes are read from the running engine. Switching a selector applies immediately and is
      persisted by the backend for this profile.
    </p>
  </div>
  <div class="head-actions">
    <button
      type="button"
      class="btn"
      disabled={loading || app.engine.status !== "connected"}
      title={app.engine.status !== "connected" ? "Connect first: outbounds are read from a live engine." : undefined}
      onclick={() => void load()}
    >
      <Icon name="refresh" size={12} /> {loading ? "Reading" : "Refresh"}
    </button>
  </div>
</div>

{#if app.engine.status !== "connected" && !loading}
  <div class="banner banner--warning">
    <Icon name="info" size={15} />
    <div class="grow">
      <p class="banner__title">Engine is {app.engine.status}</p>
      <p class="banner__body">
        Outbound groups and delays require a live session. Start one from Overview.
        {#if outbounds.length}
          The table below is the last snapshot before the engine stopped.
        {/if}
      </p>
    </div>
    <button type="button" class="btn btn--sm" onclick={() => navigate("overview")}>Overview</button>
  </div>
{/if}

{#if error}
  <div class="banner banner--error" role="alert">
    <Icon name="error" size={16} />
    <div class="grow">
      <p class="banner__title">Outbounds unavailable</p>
      <p class="banner__body mono">{error}</p>
    </div>
    <button type="button" class="btn btn--sm" onclick={() => void load()}>Retry</button>
  </div>
{:else if loading && !outbounds.length}
  <section class="panel">
    <div class="panel__body"><p class="muted small">Reading outbounds from the engine…</p></div>
  </section>
{:else if !outbounds.length}
  <section class="panel">
    <EmptyState
      title={app.engine.status === "connected" ? "No outbounds reported" : "Outbounds need a live session"}
      description={app.engine.status === "connected"
        ? "The engine did not expose any outbound tags. Check the profile and the core version in Diagnostics."
        : "Connect from Overview, then reload this view. The engine only reports outbounds while running."}
      icon="outbounds"
    >
      <button
        type="button"
        class="btn btn--sm"
        disabled={app.engine.status !== "connected"}
        title={app.engine.status !== "connected" ? "Connect first: outbounds are read from a live engine." : undefined}
        onclick={() => void load()}
      >
        Reload
      </button>
    </EmptyState>
  </section>
{:else}
  {#if groups.length}
    <div class="groups">
      {#each groups as group (group.tag)}
        <section class="panel group" aria-label={`${group.tag} ${group.kind}`}>
          <div class="panel__head">
            <div class="row">
              <span class="group__tag mono">{group.tag}</span>
              <span class="tag">{group.kind}</span>
            </div>
            <span class="group__now">
              <span class="kicker">Selected</span>
              <span class="mono">{group.now ?? "—"}</span>
            </span>
          </div>
          <div class="panel__body group__body">
            <label class="field__label" for={`group-${group.tag}`}>Switch outbound</label>
            <select
              id={`group-${group.tag}`}
              class="select group__select"
              disabled={switching !== null || app.engine.status !== "connected"}
              onchange={(event) => void switchTag(group, event.currentTarget.value)}
            >
              {#each group.options as option (option)}
                <option value={option} selected={option === group.now}>{option}</option>
              {/each}
            </select>
            <p class="small faint">
              {group.options.length} {group.options.length === 1 ? "option" : "options"} · switching applies to
              new connections
            </p>
            {#if switching === group.tag}
              <p class="small muted">Applying selection…</p>
            {/if}
          </div>
        </section>
      {/each}
    </div>
  {:else}
    <div class="banner">
      <Icon name="info" size={15} />
      <div class="grow">
        <p class="banner__title">No selector groups</p>
        <p class="banner__body">This configuration exposes only fixed outbounds.</p>
      </div>
    </div>
  {/if}

  <section class="panel" aria-label="All outbounds">
    <div class="panel__head">
      <span class="panel__title">All outbounds · {outbounds.length}</span>
      <span class="mono small faint">
        {app.engine.status === "connected" ? "delay tests run against the live engine" : "stale snapshot"}
      </span>
    </div>
    <div class="table-wrap">
      <table class="table outbounds">
        <thead>
          <tr>
            <th>Outbound</th>
            <th>Type</th>
            <th>Kind</th>
            <th>State</th>
            <th class="ta-r">Delay</th>
            <th class="ta-r">Test</th>
          </tr>
        </thead>
        <tbody>
          {#each outbounds as outbound (outbound.tag)}
            <tr>
              <td>
                <span class="mono outbound__tag">{outbound.tag}</span>
                {#if others.includes(outbound) && !selectedIn(outbound)}
                  <span class="outbound__note">fixed</span>
                {/if}
              </td>
              <td><span class="mono detail">{outbound.type}</span></td>
              <td><span class="tag">{outbound.kind}</span></td>
              <td>
                {#if selectedIn(outbound)}
                  <span class="tag tag--accent" title={`Selected in ${selectedIn(outbound)}`}>selected</span>
                {:else if outbound.now}
                  <span class="mono detail">{outbound.now}</span>
                {:else}
                  <span class="faint small">—</span>
                {/if}
              </td>
              <td class="ta-r">
                <span
                  class="mono detail"
                  class:danger={delayOf(outbound) === "error"}
                  class:accent={typeof delayOf(outbound) === "number"}
                >
                  {delayLabel(delayOf(outbound))}
                </span>
              </td>
              <td class="ta-r">
                <button
                  type="button"
                  class="btn btn--sm"
                  disabled={testing !== null || app.engine.status !== "connected"}
                  onclick={() => void test(outbound)}
                >
                  {#if testing === outbound.tag}
                    <Icon name="refresh" size={11} /> Testing
                  {:else}
                    Test
                  {/if}
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>
{/if}

<style>
  .groups {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }

  .group__tag {
    font-size: var(--fs-sm);
    font-weight: 500;
  }

  .group__now {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }

  .group__now .mono {
    font-size: var(--fs-xs);
    overflow-wrap: anywhere;
  }

  .group__body {
    display: grid;
    gap: 6px;
  }

  .group__select {
    max-width: 100%;
  }

  .outbound__tag {
    font-size: var(--fs-xs);
    overflow-wrap: anywhere;
  }

  .outbound__note {
    margin-left: 8px;
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .detail {
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .detail.danger {
    color: var(--error);
  }

  .detail.accent {
    color: var(--accent);
  }

  .outbounds th:last-child,
  .outbounds td:last-child {
    text-align: right;
  }
</style>
