<script lang="ts">
  import { api } from "../api";
  import Icon from "../components/Icon.svelte";
  import StatusGlyph from "../components/StatusGlyph.svelte";
  import MembraneCanvas from "../components/membrane/Membrane.svelte";
  import { buildConstellation, type ConstellationNode } from "../constellation";
  import { errorMessage, formatDelay, modeLabel, statusWord } from "../format";
  import { ANCHOR_CAP, capAnchors, type MembraneAnchorInput } from "../membrane";
  import { outboundStore, type DelayReading } from "../outbounds.svelte";
  import { app } from "../state.svelte";

  let { navigate }: { navigate: (view: "overview") => void } = $props();

  let selectedId = $state<string | null>(null);
  let resolvedActiveId = $state<string | null>(null);
  let routeError = $state<string | null>(null);
  let routeKey: string | null = null;

  const sourceProfile = $derived(
    app.engine.profileId !== null
      ? (app.profiles.find((profile) => profile.id === app.engine.profileId) ?? app.selectedProfile)
      : app.selectedProfile,
  );

  const model = $derived(
    buildConstellation({
      profile: sourceProfile,
      liveItems: outboundStore.items,
      connected: app.connected,
      selectedId,
      primaryGroup: null,
    }),
  );

  const activeId = $derived(
    resolvedActiveId && model.nodes.some((node) => node.id === resolvedActiveId)
      ? resolvedActiveId
      : model.activeId,
  );

  const anchorCap = $derived(capAnchors(model.nodes, activeId, selectedId));

  const selectedNode = $derived(model.nodes.find((node) => node.id === selectedId) ?? null);

  const selectedGroup = $derived(
    selectedNode && app.connected
      ? (outboundStore.groups.find((group) => group.tag === selectedNode.group) ?? null)
      : null,
  );

  const selectedDelay = $derived(selectedNode ? delayFor(selectedNode) : null);

  const anchorInputs: MembraneAnchorInput[] = $derived(
    anchorCap.visible.map((node) => ({
      id: node.id,
      label: node.label,
      type: node.type,
      delay: delayFor(node),
    })),
  );

  const activeProfileName = $derived(
    app.connected
      ? (app.engine.profileName ?? sourceProfile?.name ?? "Not reported")
      : (sourceProfile?.name ?? "No profile selected"),
  );

  const engineLabel = $derived(
    `${statusWord(app.engine.status)}${app.engine.mode ? ` · ${modeLabel(app.engine.mode)}` : ""}`,
  );

  const activeOutbound = $derived(activeId ?? "not reported");

  const dataNote = $derived(
    outboundStore.loading
      ? "Reading outbounds from the engine…"
      : outboundStore.error
        ? outboundStore.error
        : model.source === "profile"
          ? (model.reason ?? "Engine is not connected; showing the stored profile.")
          : model.source === "empty"
            ? (model.reason ?? "No outbounds available.")
            : null,
  );

  const routeNote = $derived(
    app.connected && routeError !== null
      ? "Route lookup failed; the active outbound falls back to the engine selector snapshot."
      : null,
  );

  const selectionNote = $derived.by(() => {
    const node = selectedNode;
    if (!node) return null;
    if (!app.connected) {
      return "Profile inspection only. Connect from Overview to use a live selector.";
    }
    if (!node.live) {
      return "This anchor comes from the stored profile; inspect only until the engine reports it live.";
    }
    const group = selectedGroup;
    if (!group) {
      return "No live selector or urltest group controls this outbound; inspecting only.";
    }
    if (outboundStore.switching === group.tag) {
      return `Selector change in flight for ${group.tag}; waiting for the backend to confirm.`;
    }
    if (group.kind === "urltest") {
      return `Managed by urltest group ${group.tag}; the backend chooses the member. Inspecting only.`;
    }
    if (group.now === node.id) {
      return `Backend confirms ${group.tag} → ${node.id}.`;
    }
    return `Selector ${group.tag} currently points at ${group.now ?? "not reported"}. Click the anchor to request a switch.`;
  });

  function delayFor(node: ConstellationNode): DelayReading {
    const live = outboundStore.items.find((outbound) => outbound.tag === node.id);
    return live ? outboundStore.delayOf(live) : node.delay;
  }

  function delayLabel(value: DelayReading | null): string {
    if (value === "error") return "unavailable";
    if (typeof value === "number") return formatDelay(value);
    return "not measured";
  }

  function selectAnchor(id: string | null): void {
    selectedId = id;
    if (!id || !app.connected) return;
    const node = model.nodes.find((candidate) => candidate.id === id);
    if (!node || !node.live) return;
    const group = outboundStore.groups.find((candidate) => candidate.tag === node.group);
    if (!group || group.kind !== "selector" || group.now === id) return;
    void outboundStore.select(group, id);
  }

  $effect(() => {
    const key = `${app.engine.status}:${app.engine.startedAt ?? ""}`;
    if (!app.connected) {
      resolvedActiveId = null;
      routeError = null;
      routeKey = null;
      return;
    }
    if (routeKey === key) return;
    routeKey = key;
    void (async () => {
      try {
        const route = await api.getRoute();
        if (routeKey === key) {
          resolvedActiveId = route.final;
          routeError = null;
        }
      } catch (cause) {
        if (routeKey === key) {
          resolvedActiveId = null;
          routeError = errorMessage(cause);
        }
      }
    })();
  });

  $effect(() => {
    if (selectedId && !model.nodes.some((node) => node.id === selectedId)) selectedId = null;
  });
</script>

<div class="membrane-page">
  <header class="membrane-head">
    <div class="membrane-head__text">
      <span class="kicker">Experimental</span>
      <h1 class="page-title">Network membrane</h1>
      <p class="page-desc">
        A spherical prototype of the live outbound layer. Decorative points are not servers; only
        ringed anchors carry real outbound data.
      </p>
    </div>
    <div class="membrane-head__meta">
      <span class="tag tag--accent">prototype</span>
      <span class="tag mono">{model.source}</span>
    </div>
  </header>

  <div class="membrane-layout">
    <section class="membrane-stage panel" aria-label="Network membrane canvas">
      <MembraneCanvas
        outbounds={anchorInputs}
        {activeId}
        {selectedId}
        status={app.engine.status}
        onselect={selectAnchor}
      />
    </section>

    <aside class="membrane-panel panel" aria-label="Membrane prototype details">
      <div class="panel__head">
        <span class="panel__title">Live anchors</span>
        <span class="membrane-badge">Prototype</span>
      </div>

      <div class="membrane-panel__scroll">
        <section class="membrane-section" aria-label="Session">
          <h2 class="membrane-section__title">Session</h2>
          <dl class="membrane-kv">
            <div>
              <dt>Engine</dt>
              <dd class="mono membrane-engine st-{app.engine.status}">
                <StatusGlyph status={app.engine.status} size={10} />
                {engineLabel}
              </dd>
            </div>
            <div>
              <dt>Profile</dt>
              <dd class="truncate" title={activeProfileName}>{activeProfileName}</dd>
            </div>
            <div>
              <dt>Active outbound</dt>
              <dd class="mono truncate" title={activeOutbound}>{activeOutbound}</dd>
            </div>
            <div>
              <dt>Anchors</dt>
              <dd class="mono">{anchorCap.visible.length} of {model.nodes.length}</dd>
            </div>
          </dl>
          {#if dataNote}
            <p class="membrane-note" title={dataNote}>{dataNote}</p>
          {/if}
          {#if routeNote}
            <p class="membrane-note" title={routeError ?? undefined}>{routeNote}</p>
          {/if}
        </section>

        <section class="membrane-section" aria-label="Selected anchor">
          <div class="membrane-section__head">
            <h2 class="membrane-section__title">Selected anchor</h2>
            {#if selectedNode}
              <span class="tag">{selectedNode.live ? "live" : "profile"}</span>
            {/if}
          </div>
          {#if selectedNode}
            <p class="membrane-selected__tag truncate" title={selectedNode.label}>
              {selectedNode.label}
            </p>
            <dl class="membrane-kv">
              <div>
                <dt>Type</dt>
                <dd class="mono">{selectedNode.type}</dd>
              </div>
              <div>
                <dt>Delay</dt>
                <dd class="mono">{delayLabel(selectedDelay)}</dd>
              </div>
              <div>
                <dt>Group</dt>
                <dd class="mono truncate" title={selectedNode.group || "ungrouped"}>
                  {selectedNode.group || "ungrouped"}
                </dd>
              </div>
              <div>
                <dt>Role</dt>
                <dd class="mono">
                  {activeId === selectedNode.id
                    ? "active"
                    : selectedGroup?.now === selectedNode.id
                      ? "selected in group"
                      : "inspect only"}
                </dd>
              </div>
            </dl>
            {#if selectionNote}
              <p class="membrane-note">{selectionNote}</p>
            {/if}
          {:else}
            <p class="membrane-note">
              Click a ringed anchor in the membrane, or choose one below. Only real outbounds are
              listed.
            </p>
          {/if}
        </section>

        <section class="membrane-section" aria-label="Real outbound anchors">
          <div class="membrane-section__head">
            <h2 class="membrane-section__title">Real anchors</h2>
            <span class="membrane-count mono">
              {anchorCap.visible.length} of {model.nodes.length}
            </span>
          </div>
          {#if anchorCap.visible.length > 0}
            <ul class="anchor-list">
              {#each anchorCap.visible as node (node.id)}
                <li>
                  <button
                    type="button"
                    class="anchor-item"
                    class:anchor-item--active={activeId === node.id}
                    class:anchor-item--selected={selectedId === node.id}
                    aria-pressed={selectedId === node.id}
                    aria-current={activeId === node.id ? "true" : undefined}
                    onclick={() => selectAnchor(node.id)}
                  >
                    <span class="anchor-item__marker" aria-hidden="true"></span>
                    <span class="anchor-item__name truncate">{node.label}</span>
                    <span class="anchor-item__type small muted">{node.type}</span>
                    <span class="anchor-item__delay mono">{delayLabel(delayFor(node))}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="membrane-note">
              {dataNote ?? "No outbounds reported by the engine or profile."}
            </p>
          {/if}
          {#if anchorCap.hidden > 0}
            <p class="membrane-note">
              {anchorCap.hidden} more real anchors are not drawn. The visible set is active, selected,
              and a deterministic hash slice (cap {ANCHOR_CAP}).
            </p>
          {/if}
        </section>

        <section class="membrane-section" aria-label="Legend">
          <h2 class="membrane-section__title">Legend</h2>
          <ul class="membrane-legend">
            <li>
              <span class="legend-swatch legend-swatch--point" aria-hidden="true"></span>
              <span>Membrane points · decorative, not servers</span>
            </li>
            <li>
              <span class="legend-swatch legend-swatch--anchor" aria-hidden="true"></span>
              <span>Real outbound anchor · real tag, type, delay</span>
            </li>
            <li>
              <span class="legend-swatch legend-swatch--path" aria-hidden="true"></span>
              <span>Illuminated route path · no hop data implied</span>
            </li>
            <li>
              <span class="legend-swatch legend-swatch--selected" aria-hidden="true"></span>
              <span>Selected anchor</span>
            </li>
          </ul>
          <p class="membrane-legend__count mono">
            {anchorCap.visible.length} of {model.nodes.length} anchors · no geographic grouping
          </p>
        </section>
      </div>

      <div class="panel__foot membrane-panel__foot">
        <p class="membrane-note">
          Connect and disconnect stay in Overview. This page inspects real outbounds and requests
          selector changes the backend confirms.
        </p>
        <button
          type="button"
          class="btn btn--primary membrane-back"
          onclick={() => navigate("overview")}
        >
          <Icon name="overview" size={13} />
          Back to observatory
        </button>
      </div>
    </aside>
  </div>
</div>

<style>
  .membrane-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    height: calc(100dvh - var(--topbar-h) - var(--status-h) - var(--space-6) - var(--space-7));
    min-height: 520px;
    min-width: 0;
  }

  .membrane-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-4);
    flex-wrap: wrap;
    flex: none;
  }

  .membrane-head__text {
    display: grid;
    gap: 4px;
    max-width: 78ch;
  }

  .membrane-head .page-title {
    font-size: var(--fs-xl);
  }

  .membrane-head .page-desc {
    font-size: var(--fs-sm);
  }

  .membrane-head__meta {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .membrane-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: var(--space-4);
    flex: 1;
    min-height: 0;
    min-width: 0;
  }

  .membrane-stage {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .membrane-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .membrane-panel__scroll {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4);
  }

  .membrane-panel__foot {
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-2);
  }

  .membrane-back {
    width: 100%;
  }

  .membrane-badge {
    padding: 1px 7px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-pill);
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    font-weight: 500;
    letter-spacing: 0.1em;
    line-height: 1.5;
    text-transform: uppercase;
    color: var(--text);
  }

  .membrane-section {
    display: grid;
    gap: var(--space-2);
  }

  .membrane-section__head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .membrane-section__title {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    font-weight: 500;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .membrane-count {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  .membrane-kv {
    display: grid;
    gap: 6px;
  }

  .membrane-kv > div {
    display: grid;
    grid-template-columns: 90px minmax(0, 1fr);
    gap: var(--space-2);
    align-items: baseline;
    font-size: var(--fs-xs);
  }

  .membrane-kv dt {
    color: var(--muted);
  }

  .membrane-kv dd {
    min-width: 0;
    color: var(--text);
  }

  .membrane-engine {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .membrane-note {
    font-size: var(--fs-xs);
    line-height: 1.45;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .membrane-selected__tag {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    color: var(--text);
  }

  .anchor-list {
    display: flex;
    flex-direction: column;
    max-height: 280px;
    overflow-y: auto;
    border-top: 1px solid var(--line);
  }

  .anchor-item {
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) auto;
    grid-template-rows: auto auto;
    column-gap: var(--space-2);
    width: 100%;
    padding: 5px 2px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    color: var(--muted);
  }

  .anchor-item:hover {
    background: var(--row-hover);
    color: var(--text);
  }

  .anchor-item--selected {
    background: var(--raised);
    color: var(--text);
  }

  .anchor-item__marker {
    grid-row: 1 / span 2;
    align-self: center;
    width: 7px;
    height: 7px;
    border: 1px solid var(--faint);
    border-radius: 50%;
  }

  .anchor-item--active .anchor-item__marker {
    border-color: var(--accent);
    background: var(--accent);
  }

  .anchor-item--selected .anchor-item__marker {
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 3px var(--accent);
  }

  .anchor-item__name {
    grid-column: 2;
    grid-row: 1;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: inherit;
  }

  .anchor-item__type {
    grid-column: 2;
    grid-row: 2;
    font-size: var(--fs-2xs);
  }

  .anchor-item__delay {
    grid-column: 3;
    grid-row: 1 / span 2;
    align-self: center;
    font-size: var(--fs-2xs);
  }

  .membrane-legend {
    display: grid;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .membrane-legend li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .legend-swatch {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 10px;
    height: 10px;
  }

  .legend-swatch--point::before {
    content: "";
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--faint);
  }

  .legend-swatch--anchor {
    width: 9px;
    height: 9px;
    border: 1px solid var(--text);
    border-radius: 50%;
  }

  .legend-swatch--anchor::after {
    content: "";
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--text);
  }

  .legend-swatch--path {
    width: 18px;
  }

  .legend-swatch--path::before {
    content: "";
    width: 18px;
    height: 1px;
    background: color-mix(in srgb, var(--accent) 70%, var(--muted));
  }

  .legend-swatch--selected {
    width: 11px;
    height: 11px;
    border: 1px dashed var(--accent);
    border-radius: 50%;
  }

  .membrane-legend__count {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  @media (max-width: 1080px) {
    .membrane-page {
      height: calc(100dvh - var(--topbar-h) - var(--status-h) - var(--space-5) - var(--space-7));
    }

    .membrane-layout {
      grid-template-columns: minmax(0, 1fr) 300px;
    }

    .membrane-kv > div {
      grid-template-columns: 80px minmax(0, 1fr);
    }
  }

  @media (max-width: 940px) {
    .membrane-page {
      height: calc(100dvh - var(--topbar-h) - var(--status-h) - var(--space-4) - var(--space-6));
    }
  }

  @media (max-width: 860px) {
    .membrane-page {
      height: auto;
      min-height: 0;
    }

    .membrane-layout {
      grid-template-columns: minmax(0, 1fr);
    }

    .membrane-stage {
      height: min(62vh, 560px);
    }

    .anchor-list {
      max-height: none;
    }
  }
</style>
