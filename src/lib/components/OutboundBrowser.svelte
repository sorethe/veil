<script lang="ts">
  import type { Outbound } from "../api";
  import { formatDelay, statusWord } from "../format";
  import { app } from "../state.svelte";
  import { outboundStore } from "../outbounds.svelte";
  import EmptyState from "./EmptyState.svelte";
  import Icon from "./Icon.svelte";

  let { full = false }: { full?: boolean } = $props();

  interface Row {
    tag: string;
    node: Outbound | null;
  }

  let selectedGroupTag = $state<string | null>(null);
  let query = $state("");

  const groups = $derived(outboundStore.groups);
  const selectedGroup = $derived(
    groups.find((group) => group.tag === selectedGroupTag) ??
      groups.find((group) => group.kind === "selector") ??
      groups[0] ??
      null,
  );
  const options = $derived(selectedGroup?.options ?? []);
  const showSearch = $derived(options.length > 8);
  const switchable = $derived(selectedGroup?.kind === "selector");
  const busy = $derived(outboundStore.switching !== null);
  const rows = $derived.by(() => {
    const group = selectedGroup;
    if (!group) return [] as Row[];
    const byTag = new Map(outboundStore.items.map((outbound) => [outbound.tag, outbound]));
    const needle = query.trim().toLowerCase();
    return group.options
      .map((tag): Row => ({ tag, node: byTag.get(tag) ?? null }))
      .filter((row) => {
        if (!needle) return true;
        return (
          row.tag.toLowerCase().includes(needle) ||
          (row.node?.type ?? "").toLowerCase().includes(needle)
        );
      });
  });
  const readOnlyReason = $derived(
    selectedGroup?.kind === "urltest"
      ? "URLTest selects automatically; manual selection is ignored for this group."
      : "Only selector groups are switchable.",
  );

  $effect(() => {
    outboundStore.syncEngine(app.engine.status, app.engine.startedAt);
  });

  $effect(() => {
    const list = outboundStore.groups;
    if (!list.length) {
      if (selectedGroupTag !== null) selectedGroupTag = null;
      return;
    }
    if (!selectedGroupTag || !list.some((group) => group.tag === selectedGroupTag)) {
      selectedGroupTag = (list.find((group) => group.kind === "selector") ?? list[0]).tag;
    }
  });

  // A filter belongs to the group that was open when it was typed.
  $effect(() => {
    selectedGroupTag;
    query = "";
  });

  $effect(() => {
    if (!showSearch) query = "";
  });

  function delayState(row: Row): { text: string; tone: string; title: string } {
    if (outboundStore.testing === row.tag) {
      return { text: "testing", tone: "testing", title: "Delay test running" };
    }
    const value = outboundStore.delays[row.tag] ?? row.node?.delay ?? null;
    if (value === "error") {
      return { text: "error", tone: "error", title: "The last delay test failed for this outbound" };
    }
    if (value === null || value === undefined) {
      return { text: "—", tone: "untested", title: "Not tested in this session" };
    }
    const text = formatDelay(value);
    return {
      text,
      tone: value < 0 ? "error" : "measured",
      title: value < 0 ? "Delay test timed out" : `Measured ${text}`,
    };
  }

  function selectRow(row: Row): void {
    const group = selectedGroup;
    if (!group || group.kind !== "selector" || busy) return;
    void outboundStore.select(group, row.tag);
  }
</script>

<div class="browser" class:browser--full={full}>
  <header class="ob-head">
    <div class="ob-head__top">
      <div class="ob-head__identity">
        <span class="kicker">Egress control</span>
        <div class="ob-head__line">
          <h2 class="ob-head__title">Outbounds</h2>
          {#if selectedGroup}
            <span class="ob-head__group mono" title={selectedGroup.tag}>{selectedGroup.tag}</span>
            <span class="tag">{selectedGroup.kind}</span>
          {/if}
        </div>
      </div>
      <div class="ob-head__actions">
        {#if busy}
          <span class="tag tag--warning">Applying</span>
        {/if}
        <button
          type="button"
          class="icon-btn"
          aria-label="Refresh outbounds"
          title={app.engine.status === "connected"
            ? "Re-read groups and nodes from the engine"
            : "Connect first: outbounds are read from a live engine."}
          disabled={outboundStore.loading || app.engine.status !== "connected"}
          onclick={() => void outboundStore.load()}
        >
          <span class:spin={outboundStore.loading}><Icon name="refresh" size={14} /></span>
        </button>
      </div>
    </div>

    {#if selectedGroup}
      <div class="ob-selected" aria-live="polite">
        <span class="kicker">{selectedGroup.kind === "urltest" ? "Automatic choice" : "Selected"}</span>
        <p class="ob-selected__value">
          <span class="ob-selected__group mono">{selectedGroup.tag}</span>
          <Icon name="chevronRight" size={13} />
          {#if selectedGroup.now}
            <span class="ob-selected__node mono" title={selectedGroup.now}>{selectedGroup.now}</span>
          {:else}
            <span class="ob-selected__node ob-selected__node--none mono">not reported</span>
          {/if}
        </p>
      </div>
    {/if}

    {#if groups.length > 1}
      <div class="ob-groups" role="tablist" aria-label="Outbound groups">
        {#each groups as group (group.tag)}
          <button
            type="button"
            class="ob-group"
            class:ob-group--active={selectedGroupTag === group.tag}
            role="tab"
            aria-selected={selectedGroupTag === group.tag}
            title={`${group.tag} · ${group.kind}`}
            onclick={() => (selectedGroupTag = group.tag)}
          >
            <span class="ob-group__name mono truncate">{group.tag}</span>
            <span class="tag">{group.kind}</span>
          </button>
        {/each}
      </div>
    {/if}

    {#if selectedGroup}
      <div class="ob-tools">
        {#if showSearch}
          <label class="search ob-search">
            <span class="search__icon"><Icon name="search" size={12} /></span>
            <input
              class="input"
              type="search"
              placeholder="Filter nodes or types"
              aria-label="Filter outbounds"
              bind:value={query}
            />
          </label>
        {/if}
        <span class="ob-count mono">
          {rows.length}/{options.length} nodes
        </span>
      </div>
    {/if}
  </header>

  <div class="ob-body">
    {#if app.engine.status !== "connected"}
      <div class="ob-fill">
        <EmptyState
          title="Outbounds need a live session"
          description={`The engine is ${statusWord(app.engine.status).toLowerCase()}. Groups, nodes, and delays are read from a running engine; start a session from the monitor panel.`}
          icon="outbounds"
        />
      </div>
    {:else if outboundStore.error}
      <div class="ob-fill">
        <div class="banner banner--error ob-banner" role="alert">
          <Icon name="error" size={16} />
          <div class="grow">
            <p class="banner__title">Outbounds unavailable</p>
            <p class="banner__body mono">{outboundStore.error}</p>
          </div>
          <button type="button" class="btn btn--sm" onclick={() => void outboundStore.load()}>Retry</button>
        </div>
      </div>
    {:else if outboundStore.loading && !outboundStore.items.length}
      <div class="ob-fill">
        <p class="muted small">Reading outbounds from the engine…</p>
      </div>
    {:else if !groups.length}
      <div class="ob-fill">
        <EmptyState
          title="No selector groups"
          description="This configuration exposes only fixed outbounds. The Outbounds page lists them; there is nothing to switch here."
          icon="info"
        />
      </div>
    {:else if !selectedGroup}
      <div class="ob-fill">
        <p class="muted small">No outbound group reported.</p>
      </div>
    {:else if !options.length}
      <div class="ob-fill">
        <EmptyState
          title="No options in this group"
          description="The engine reported the group without member outbounds. Check the profile and the core version in Diagnostics."
          icon="warning"
        />
      </div>
    {:else if !rows.length}
      <div class="ob-fill">
        <p class="muted small">No nodes match “{query}”.</p>
      </div>
    {:else}
      {#if selectedGroup.kind === "urltest"}
        <div class="ob-note">
          <span class="ob-note__icon"><Icon name="info" size={13} /></span>
          <p>{readOnlyReason}</p>
        </div>
      {/if}
      <ul class="ob-list" aria-label={`${selectedGroup.tag} nodes`}>
        {#each rows as row (row.tag)}
          {@const selected = selectedGroup.now === row.tag}
          {@const delay = delayState(row)}
          <li class="ob-row" class:ob-row--selected={selected}>
            {#if switchable}
              <button
                type="button"
                class="ob-row__main"
                disabled={busy}
                title={selected
                  ? `${row.tag} is already selected`
                  : `Switch ${selectedGroup.tag} to ${row.tag}`}
                onclick={() => selectRow(row)}
              >
                <span class="ob-row__mark" aria-hidden="true">
                  {#if selected}<Icon name="check" size={12} />{/if}
                </span>
                <span class="ob-row__name mono truncate" title={row.tag}>{row.tag}</span>
                <span class="ob-row__type mono" title={row.node?.type ?? "Type not reported"}>
                  {row.node?.type ?? "type not reported"}
                </span>
                <span
                  class="ob-row__delay mono ob-row__delay--{delay.tone}"
                  title={delay.title}
                >{delay.text}</span>
              </button>
            {:else}
              <div class="ob-row__main ob-row__main--static" title={readOnlyReason}>
                <span class="ob-row__mark" aria-hidden="true">
                  {#if selected}<span class="ob-row__auto" title="Automatic choice"></span>{/if}
                </span>
                <span class="ob-row__name mono truncate" title={row.tag}>{row.tag}</span>
                <span class="ob-row__type mono" title={row.node?.type ?? "Type not reported"}>
                  {row.node?.type ?? "type not reported"}
                </span>
                <span
                  class="ob-row__delay mono ob-row__delay--{delay.tone}"
                  title={delay.title}
                >{delay.text}</span>
              </div>
            {/if}
            <button
              type="button"
              class="btn btn--sm ob-row__test"
              disabled={outboundStore.testing !== null}
              title={app.engine.status === "connected"
                ? `Test delay for ${row.tag}`
                : "Connect first: delay tests run against a live engine."}
              onclick={() => void outboundStore.test(row.tag)}
            >
              {#if outboundStore.testing === row.tag}Testing{:else}Test{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .browser {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--surface);
    overflow: hidden;
  }

  .browser--full {
    height: auto;
  }

  .ob-head {
    flex: none;
    border-bottom: 1px solid var(--line);
  }

  .ob-head__top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
    padding: 11px var(--space-4) 9px;
  }

  .ob-head__identity {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .ob-head__line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    flex-wrap: wrap;
  }

  .ob-head__title {
    font-size: var(--fs-lg);
    font-weight: 500;
    letter-spacing: -0.01em;
  }

  .ob-head__group {
    font-size: var(--fs-xs);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 34ch;
  }

  .ob-head__actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
  }

  .ob-selected {
    display: grid;
    gap: 2px;
    padding: 8px var(--space-4) 10px;
    border-top: 1px solid var(--line);
    background: var(--field);
    min-width: 0;
  }

  .ob-selected__value {
    display: flex;
    align-items: baseline;
    gap: 5px;
    min-width: 0;
    flex-wrap: wrap;
    font-size: var(--fs-md);
    line-height: 1.3;
  }

  .ob-selected__group {
    color: var(--muted);
    font-size: var(--fs-xs);
  }

  .ob-selected__node {
    color: var(--accent);
    overflow-wrap: anywhere;
  }

  .ob-selected__node--none {
    color: var(--faint);
    font-size: var(--fs-xs);
  }

  .ob-groups {
    display: flex;
    align-items: stretch;
    gap: 2px;
    padding: 0 var(--space-3);
    border-top: 1px solid var(--line);
    overflow-x: auto;
    scrollbar-width: thin;
  }

  .ob-group {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
    max-width: 280px;
    padding: 7px 8px;
    border-bottom: 2px solid transparent;
    color: var(--muted);
    transition: color var(--fast) var(--ease), border-color var(--fast) var(--ease);
  }

  .ob-group:hover {
    color: var(--text);
  }

  .ob-group--active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }

  .ob-group__name {
    font-size: var(--fs-xs);
    max-width: 190px;
  }

  .ob-tools {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 7px var(--space-4);
    border-top: 1px solid var(--line);
    min-width: 0;
  }

  .ob-search {
    flex: 1 1 220px;
    max-width: 360px;
    min-width: 160px;
  }

  .ob-count {
    margin-left: auto;
    font-size: var(--fs-2xs);
    color: var(--faint);
    white-space: nowrap;
  }

  .ob-body {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }

  .ob-fill {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    justify-content: center;
    padding: var(--space-4);
    min-height: 0;
    text-align: center;
  }

  .ob-banner {
    text-align: left;
    width: 100%;
    max-width: 720px;
  }

  .ob-note {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 7px var(--space-4);
    border-bottom: 1px solid var(--line);
    color: var(--muted);
    background: var(--field);
    font-size: var(--fs-2xs);
    line-height: 1.45;
  }

  .ob-note__icon {
    display: flex;
    margin-top: 1px;
    flex: none;
  }

  .ob-list {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .browser--full .ob-list {
    max-height: 46vh;
  }

  .ob-row {
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  .ob-row:last-child {
    border-bottom: 0;
  }

  .ob-row__main {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr) minmax(72px, 128px) 70px;
    align-items: center;
    gap: 10px;
    flex: 1 1 auto;
    min-width: 0;
    padding: 7px 10px 7px 12px;
    border-left: 2px solid transparent;
    text-align: left;
    color: var(--text);
    transition: background-color var(--fast) var(--ease);
  }

  .ob-row__main:hover:not(:disabled):not(.ob-row__main--static) {
    background: var(--row-hover);
  }

  .ob-row__main:disabled {
    cursor: progress;
    opacity: 0.68;
  }

  .ob-row__main--static {
    cursor: default;
  }

  .ob-row--selected .ob-row__main {
    border-left-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }

  .ob-row__mark {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
  }

  .ob-row__auto {
    width: 5px;
    height: 5px;
    background: var(--accent);
  }

  .ob-row__name {
    font-size: var(--fs-xs);
    line-height: 1.4;
  }

  .ob-row--selected .ob-row__name {
    color: var(--text);
    font-weight: 500;
  }

  .ob-row__type {
    font-size: var(--fs-2xs);
    color: var(--faint);
    letter-spacing: 0.04em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ob-row__delay {
    font-size: var(--fs-xs);
    text-align: right;
    white-space: nowrap;
  }

  .ob-row__delay--measured {
    color: var(--accent);
  }

  .ob-row__delay--untested {
    color: var(--faint);
  }

  .ob-row__delay--testing {
    color: var(--muted);
    letter-spacing: 0.04em;
  }

  .ob-row__delay--error {
    color: var(--error);
  }

  .ob-row__test {
    align-self: center;
    margin-right: 10px;
    flex: none;
  }

  @media (max-width: 620px) {
    .ob-row__main {
      grid-template-columns: 14px minmax(0, 1fr) 64px;
    }

    .ob-row__type {
      display: none;
    }
  }
</style>
