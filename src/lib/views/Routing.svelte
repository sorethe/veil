<script lang="ts">
  import { api, type RouteInfo, type RouteRule } from "../api";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import Segmented from "../components/Segmented.svelte";
  import { errorMessage } from "../format";
  import { app } from "../state.svelte";

  let route = $state<RouteInfo | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let switching = $state(false);
  let modeError = $state<string | null>(null);
  let lastEngineKey = "";

  interface RuleGroup {
    key: string;
    label: string;
    rules: RouteRule[];
  }

  const groups = $derived(groupRules(route?.rules ?? []));

  const modeOptions = $derived(
    (route?.modes ?? []).map((mode) => ({ value: mode, label: mode })),
  );

  $effect(() => {
    const key = `${app.engine.status}:${app.engine.startedAt ?? 0}`;
    if (key === lastEngineKey) return;
    lastEngineKey = key;
    void load();
  });

  function groupRules(rules: RouteRule[]): RuleGroup[] {
    const buckets = new Map<string, RouteRule[]>();
    for (const rule of rules) {
      const action = rule.action || "other";
      const list = buckets.get(action);
      if (list) list.push(rule);
      else buckets.set(action, [rule]);
    }
    const preferred = ["route", "reject", "hijack-dns", "sniff", "resolve"];
    const keys = [
      ...preferred.filter((key) => buckets.has(key)),
      ...[...buckets.keys()].filter((key) => !preferred.includes(key)).sort(),
    ];
    return keys.map((key) => ({
      key,
      label: key
        .split("-")
        .map((part) => (part ? part.charAt(0).toUpperCase() + part.slice(1) : part))
        .join(" "),
      rules: buckets.get(key) ?? [],
    }));
  }

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      route = await api.getRoute();
    } catch (cause) {
      route = null;
      error = errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  async function setMode(mode: string): Promise<void> {
    if (!mode || switching) return;
    switching = true;
    modeError = null;
    try {
      await api.setClashMode(mode);
      await load();
      app.notify("success", "Routing mode changed", mode);
    } catch (cause) {
      modeError = errorMessage(cause);
      app.notify("error", "Could not change routing mode", errorMessage(cause));
    } finally {
      switching = false;
    }
  }
</script>

<div class="page-head">
  <div class="page-head__text">
    <span class="kicker">Routing</span>
    <h1 class="page-title">Rules</h1>
    <p class="page-desc">
      The live route as reported by the engine. This view is read-only; rules come from the active profile.
    </p>
  </div>
  <div class="head-actions">
    <span class="tag">read-only</span>
    <button type="button" class="btn" disabled={loading} onclick={() => void load()}>
      <Icon name="refresh" size={12} /> {loading ? "Reading" : "Reload"}
    </button>
  </div>
</div>

{#if loading && !route}
  <section class="panel">
    <div class="panel__body">
      <p class="muted small">Reading the active route from the engine…</p>
    </div>
  </section>
{:else if error}
  <div class="banner banner--error" role="alert">
    <Icon name="error" size={16} />
    <div class="grow">
      <p class="banner__title">Route unavailable</p>
      <p class="banner__body mono">{error}</p>
    </div>
    <button type="button" class="btn btn--sm" onclick={() => void load()}>Retry</button>
  </div>
{:else if route}
  <div class="summary">
    <div class="summary__cell">
      <span class="kicker">Final outbound</span>
      <span class="mono summary__value">{route.final ?? "—"}</span>
    </div>
    <div class="summary__cell">
      <span class="kicker">Rules</span>
      <span class="mono summary__value">{route.rules.length}</span>
    </div>
    <div class="summary__cell summary__cell--mode">
      <span class="kicker">Mode</span>
      {#if route.modes.length}
        <Segmented
          value={route.mode ?? ""}
          options={modeOptions}
          label="Routing mode"
          disabled={switching}
          onchange={(mode) => void setMode(mode)}
        />
        <span class="summary__hint">{switching ? "Applying…" : "Switches the live Clash mode."}</span>
      {:else}
        <span class="mono summary__value">{route.mode ?? "—"}</span>
      {/if}
    </div>
  </div>

  {#if modeError}
    <p class="inline-error" role="alert">{modeError}</p>
  {/if}

  {#if !route.rules.length}
    <section class="panel">
      <EmptyState
        title="No routing rules"
        description={`The engine reports no rules. Unmatched traffic follows the final outbound${route.final ? `: ${route.final}` : "."}`}
        icon="routing"
      />
    </section>
  {:else}
    <div class="groups">
      {#each groups as group (group.key)}
        <section class="panel" aria-label={`${group.label} rules`}>
          <div class="panel__head">
            <span class="panel__title">
              {group.label} · {group.rules.length}
            </span>
            <span class="tag {group.key === 'reject' ? 'tag--error' : group.key === 'route' ? 'tag--accent' : ''}">
              {group.key}
            </span>
          </div>
          <div class="table-wrap">
            <table class="table rules">
              <thead>
                <tr>
                  <th class="rules__index">#</th>
                  <th>Match</th>
                  <th>Outbound</th>
                  <th>Action</th>
                </tr>
              </thead>
              <tbody>
                {#each group.rules as rule (rule.index)}
                  <tr>
                    <td class="mono rules__index">{rule.index}</td>
                    <td><span class="mono rules__match">{rule.match || "—"}</span></td>
                    <td><span class="mono rules__outbound">{rule.outbound ?? "—"}</span></td>
                    <td><span class="tag">{rule.action}</span></td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>
      {/each}
    </div>
  {/if}

  <section class="panel dns" aria-label="DNS rules">
    <div class="panel__head">
      <span class="panel__title">DNS · {route.dnsRules.length}</span>
      <span class="mono dns__final">final {route.dnsFinal ?? "—"}</span>
    </div>
    {#if route.dnsRules.length}
      <div class="table-wrap">
        <table class="table rules">
          <thead>
            <tr>
              <th class="rules__index">#</th>
              <th>Match</th>
              <th>Outbound</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {#each route.dnsRules as rule (rule.index)}
              <tr>
                <td class="mono rules__index">{rule.index}</td>
                <td><span class="mono rules__match">{rule.match || "—"}</span></td>
                <td><span class="mono rules__outbound">{rule.outbound ?? "—"}</span></td>
                <td><span class="tag">{rule.action}</span></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <EmptyState
        title="No DNS rules"
        description={route.dnsFinal ? `The engine reports only a final DNS outbound: ${route.dnsFinal}.` : "The engine reports no DNS rules."}
        icon="routing"
      />
    {/if}
  </section>
{:else}
  <section class="panel">
    <EmptyState
      title="No route information"
      description="Connect with a profile to read the live route from the engine."
      icon="routing"
    >
      <button type="button" class="btn btn--sm" onclick={() => void load()}>Try again</button>
    </EmptyState>
  </section>
{/if}

<style>
  .summary {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 100px minmax(280px, 1.4fr);
    gap: 0;
    border: 1px solid var(--line);
    background: var(--surface);
    margin-bottom: var(--space-4);
  }

  @media (max-width: 760px) {
    .summary {
      grid-template-columns: minmax(0, 1fr);
    }

    .summary__cell {
      border-right: 0 !important;
      border-bottom: 1px solid var(--line);
    }

    .summary__cell:last-child {
      border-bottom: 0;
    }
  }

  .summary__cell {
    display: grid;
    gap: 4px;
    padding: 10px var(--space-4);
    border-right: 1px solid var(--line);
    min-width: 0;
  }

  .summary__cell:last-child {
    border-right: 0;
  }

  .summary__value {
    font-size: var(--fs-md);
    overflow-wrap: anywhere;
  }

  .summary__hint {
    font-size: var(--fs-2xs);
    color: var(--faint);
  }

  .summary__cell--mode {
    align-content: start;
    justify-items: start;
  }

  .inline-error {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--error);
    margin: calc(-1 * var(--space-2)) 0 var(--space-4);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .groups {
    display: grid;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }

  .rules th:first-child,
  .rules td:first-child {
    width: 48px;
  }

  .rules__index {
    color: var(--faint);
    font-size: var(--fs-2xs);
  }

  .rules__match {
    font-size: var(--fs-xs);
    overflow-wrap: anywhere;
    min-width: 240px;
    display: block;
  }

  .rules__outbound {
    font-size: var(--fs-xs);
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .dns {
    margin-bottom: var(--space-6);
  }

  .dns__final {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
</style>
