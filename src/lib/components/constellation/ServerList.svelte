<script lang="ts">
  import type { ConstellationCluster } from "../../constellation";
  import { formatDelay } from "../../format";

  let {
    clusters,
    selectedId,
    activeId,
    onselect,
    emptyReason,
  }: {
    clusters: ConstellationCluster[];
    selectedId: string | null;
    activeId: string | null;
    onselect: (id: string | null) => void;
    emptyReason: string | null;
  } = $props();

  let query = $state("");

  const filtered = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return clusters;
    return clusters
      .map((cluster) => ({
        ...cluster,
        nodes: cluster.nodes.filter(
          (node) =>
            node.label.toLowerCase().includes(needle) || node.type.toLowerCase().includes(needle),
        ),
      }))
      .filter(
        (cluster) => cluster.nodes.length > 0 || cluster.label.toLowerCase().includes(needle),
      );
  });

  function delayText(delay: number | "error" | null): string {
    if (delay === "error") return "error";
    if (delay === null) return "not measured";
    if (delay < 0) return "timeout";
    return formatDelay(delay);
  }

  function delayTone(delay: number | "error" | null): string {
    if (delay === "error" || (delay !== null && delay < 0)) return "error";
    if (delay === null) return "untested";
    return "measured";
  }

  function unavailable(delay: number | "error" | null): boolean {
    return delay === "error" || (delay !== null && delay < 0);
  }
</script>

<div class="server-list">
  <div class="server-list__search">
    <input
      type="search"
      placeholder="Search servers…"
      bind:value={query}
      aria-label="Search servers"
    />
  </div>

  {#if filtered.length === 0}
    <div class="server-list__empty">
      <p>{emptyReason ?? "No servers available."}</p>
    </div>
  {:else}
    <div class="server-list__clusters">
      {#each filtered as cluster (cluster.id)}
        <section class="server-list__cluster" aria-label={cluster.label}>
          <h3 class="server-list__header">{cluster.label}</h3>
          <ul class="server-list__nodes">
            {#each cluster.nodes as node (node.id)}
              <li>
                <button
                  type="button"
                  class="server-list__node"
                  class:server-list__node--selected={node.id === selectedId}
                  class:server-list__node--active={node.id === activeId}
                  class:server-list__node--unavailable={unavailable(node.delay)}
                  title={`${node.label} (${node.type})`}
                  aria-label={`${node.label}, type ${node.type}, delay ${delayText(node.delay)}`}
                  onclick={() => onselect(node.id)}
                >
                  <span class="server-list__node-label truncate">{node.label}</span>
                  <span class="server-list__node-type mono">{node.type}</span>
                  <span
                    class="server-list__node-delay mono server-list__node-delay--{delayTone(
                      node.delay,
                    )}"
                  >
                    {delayText(node.delay)}
                  </span>
                  {#if node.id === activeId}
                    <span class="server-list__active-marker" title="Active connection">*</span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .server-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .server-list__search {
    flex: none;
    padding: var(--space-3);
    border-bottom: 1px solid var(--line);
  }

  .server-list__search input {
    width: 100%;
    padding: 6px 8px;
    background: var(--field);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }

  .server-list__search input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .server-list__clusters {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-2);
  }

  .server-list__cluster {
    margin-bottom: var(--space-3);
  }

  .server-list__header {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    font-weight: 500;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
    padding: var(--space-2) var(--space-2) var(--space-1);
    margin: 0;
  }

  .server-list__nodes {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 2px;
  }

  .server-list__node {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 6px var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
    gap: var(--space-2);
    transition: background-color var(--fast) var(--ease);
    cursor: pointer;
    font-size: var(--fs-xs);
  }

  .server-list__node:hover {
    background: var(--raised);
  }

  .server-list__node:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .server-list__node--selected {
    background: var(--field);
    border-color: var(--line-strong);
  }

  .server-list__node--active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .server-list__node--unavailable {
    opacity: 0.5;
  }

  .server-list__node-label {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono);
  }

  .server-list__node-type {
    font-size: var(--fs-2xs);
    color: var(--muted);
    flex: none;
    max-width: 60px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .server-list__node-delay {
    font-size: var(--fs-2xs);
    flex: none;
    min-width: 40px;
    text-align: right;
  }

  .server-list__node-delay--error {
    color: var(--error);
  }

  .server-list__node-delay--untested {
    color: var(--faint);
  }

  .server-list__node-delay--measured {
    color: var(--muted);
  }

  .server-list__active-marker {
    color: var(--accent);
    font-size: var(--fs-2xs);
    font-weight: bold;
  }

  .server-list__empty {
    padding: var(--space-4);
    text-align: center;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
</style>
