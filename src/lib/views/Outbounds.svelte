<script lang="ts">
  import type { Outbound } from "../api";
  import Icon from "../components/Icon.svelte";
  import OutboundBrowser from "../components/OutboundBrowser.svelte";
  import { formatDelay } from "../format";
  import { outboundStore, type DelayReading } from "../outbounds.svelte";
  import { app, type View } from "../state.svelte";

  let { navigate }: { navigate: (view: View) => void } = $props();

  const groups = $derived(outboundStore.groups);

  function memberOfAnyGroup(outbound: Outbound): boolean {
    return groups.some((group) => group.options.includes(outbound.tag));
  }

  function activeGroupOf(outbound: Outbound): Outbound | null {
    return groups.find((group) => group.now === outbound.tag) ?? null;
  }

  function selectedIn(outbound: Outbound): string | null {
    return activeGroupOf(outbound)?.tag ?? null;
  }

  function delayOf(outbound: Outbound): DelayReading {
    return outboundStore.delayOf(outbound);
  }

  function delayLabel(value: DelayReading): string {
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
      disabled={outboundStore.loading || app.engine.status !== "connected"}
      title={app.engine.status !== "connected" ? "Connect first: outbounds are read from a live engine." : undefined}
      onclick={() => void outboundStore.load()}
    >
      <Icon name="refresh" size={12} /> {outboundStore.loading ? "Reading" : "Refresh"}
    </button>
  </div>
</div>

{#if app.engine.status !== "connected"}
  <div class="banner banner--warning">
    <Icon name="info" size={15} />
    <div class="grow">
      <p class="banner__title">Engine is {app.engine.status}</p>
      <p class="banner__body">
        Outbound groups and delays require a live session. Start one from Overview.
      </p>
    </div>
    <button type="button" class="btn btn--sm" onclick={() => navigate("overview")}>Overview</button>
  </div>
{/if}

<section class="browser-section" aria-label="Outbound workspace">
  <OutboundBrowser full />
</section>

{#if outboundStore.items.length}
  <section class="panel outbound-table" aria-label="All outbounds">
    <div class="panel__head">
      <span class="panel__title">All outbounds · {outboundStore.items.length}</span>
      <span class="mono small faint">delay tests run against the live engine</span>
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
          {#each outboundStore.items as outbound (outbound.tag)}
            {@const delay = delayOf(outbound)}
            <tr>
              <td>
                <span class="mono outbound__tag">{outbound.tag}</span>
                {#if !memberOfAnyGroup(outbound) && !selectedIn(outbound)}
                  <span class="outbound__note">fixed</span>
                {/if}
              </td>
              <td><span class="mono detail">{outbound.type}</span></td>
              <td><span class="tag">{outbound.kind}</span></td>
              <td>
                {#if activeGroupOf(outbound)}
                  <span
                    class="tag tag--accent"
                    title={`${activeGroupOf(outbound)?.kind === "urltest" ? "Automatic choice" : "Selected"} in ${selectedIn(outbound)}`}
                  >
                    {activeGroupOf(outbound)?.kind === "urltest" ? "auto" : "selected"}
                  </span>
                {:else if outbound.now}
                  <span class="mono detail">{outbound.now}</span>
                {:else}
                  <span class="faint small">—</span>
                {/if}
              </td>
              <td class="ta-r">
                <span
                  class="mono detail"
                  class:danger={delay === "error" || (typeof delay === "number" && delay < 0)}
                  class:accent={typeof delay === "number" && delay >= 0}
                >
                  {delayLabel(delay)}
                </span>
              </td>
              <td class="ta-r">
                <button
                  type="button"
                  class="btn btn--sm"
                  disabled={outboundStore.testing !== null || app.engine.status !== "connected"}
                  onclick={() => void outboundStore.test(outbound.tag)}
                >
                  {#if outboundStore.testing === outbound.tag}
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
  .browser-section {
    margin-bottom: var(--space-4);
    min-width: 0;
  }

  .outbound-table {
    margin-top: var(--space-2);
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
