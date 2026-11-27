<script lang="ts">
  import type { EngineStatus, Stats } from "../../api";
  import type { ConstellationCluster, ConstellationNode } from "../../constellation";
  import { formatBytes, formatDelay, formatSpeed, statusWord } from "../../format";
  import Icon from "../Icon.svelte";
  import StatusGlyph from "../StatusGlyph.svelte";

  let {
    node,
    cluster,
    phase,
    connected,
    activeId,
    profileName,
    stats,
    mixedPort,
    selectedGroup,
    testing,
    busy,
    enginePid,
    onconnect,
    ondisconnect,
    ontest,
  }: {
    node: ConstellationNode | null;
    cluster: ConstellationCluster | null;
    phase: EngineStatus;
    connected: boolean;
    activeId: string | null;
    profileName: string | null;
    stats: Stats;
    mixedPort: number | null;
    selectedGroup: { tag: string; kind: string; now: string | null } | null;
    testing: boolean;
    busy: boolean;
    enginePid: number | null;
    onconnect: () => void;
    ondisconnect: () => void;
    ontest: (() => void) | null;
  } = $props();

  function latencyText(value: ConstellationNode): string {
    if (value.delay === "error") return "unavailable";
    if (typeof value.delay === "number") return formatDelay(value.delay);
    return "not measured";
  }

  function connectionLabel(value: ConstellationNode): string {
    if (phase === "starting") return "connecting";
    if (!connected) return statusWord(phase).toLowerCase();
    if (activeId === value.id) return "active";
    if (value.live) return "live";
    return "available";
  }

  const connectDisabled = $derived(busy || phase === "starting" || phase === "stopping");
  const connectTitle = $derived(
    busy
      ? "A session action is already running"
      : phase === "starting" || phase === "stopping"
        ? "Wait for the engine to settle"
        : "Connect using the selected profile",
  );
</script>

<aside class="inspector" aria-label="Constellation inspector">
  <header class="inspector__head">
    <span class="kicker">Inspector</span>
    <span class="inspector__status" aria-live="polite">
      <StatusGlyph status={phase} size={12} />
      <span class="mono st-{phase}">{statusWord(phase)}</span>
    </span>
  </header>

  {#if node}
    <div class="inspector__identity">
      <h2 class="inspector__title truncate" title={node.label}>{node.label}</h2>
      <div class="row row--wrap">
        <span class="tag">{node.type}</span>
        {#if cluster}
          <span class="tag" title={cluster.label}>{cluster.label}</span>
        {/if}
        {#if node.selected}
          <span class="tag tag--accent">selected</span>
        {/if}
        {#if connected && activeId === node.id}
          <span class="tag tag--accent">active</span>
        {/if}
        {#if !node.live}
          <span class="tag">profile</span>
        {/if}
      </div>
    </div>

    <div class="inspector__body">
      <dl class="deflist">
        <div class="deflist__row">
          <dt class="deflist__term">Latency</dt>
          <dd class="deflist__value mono">{latencyText(node)}</dd>
        </div>
        <div class="deflist__row">
          <dt class="deflist__term">Connection</dt>
          <dd class="deflist__value mono">{connectionLabel(node)}</dd>
        </div>
        <div class="deflist__row">
          <dt class="deflist__term">Group</dt>
          <dd class="deflist__value mono">{cluster?.label ?? "—"}</dd>
        </div>
      </dl>

      <section class="inspector__section" aria-label="Session traffic">
        <h3 class="inspector__section-title">Session</h3>
        <dl class="deflist">
          <div class="deflist__row">
            <dt class="deflist__term">Upload</dt>
            <dd class="deflist__value mono">{formatBytes(stats.up)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Download</dt>
            <dd class="deflist__value mono">{formatBytes(stats.down)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Up speed</dt>
            <dd class="deflist__value mono">{formatSpeed(stats.upSpeed)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Down speed</dt>
            <dd class="deflist__value mono">{formatSpeed(stats.downSpeed)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Connections</dt>
            <dd class="deflist__value mono">{stats.connections}</dd>
          </div>
        </dl>
        <p class="inspector__hint">Session-wide totals; not per-node.</p>
      </section>

      {#if !connected}
        <p class="inspector__hint">
          Inspecting the stored profile. Connect to switch the active outbound.
        </p>
      {/if}

      <div class="inspector__actions">
        {#if connected}
          <button
            type="button"
            class="btn btn--danger"
            disabled={busy}
            title={busy ? "A session action is already running" : "Disconnect the engine"}
            onclick={ondisconnect}
          >
            <Icon name="power" size={13} />
            Disconnect
          </button>
        {:else if phase === "error" && enginePid !== null}
          <button
            type="button"
            class="btn btn--danger"
            disabled={busy}
            title={busy ? "A session action is already running" : "Stop the failed engine process"}
            onclick={onconnect}
          >
            <Icon name="power" size={13} />
            Stop engine
          </button>
        {:else}
          <button
            type="button"
            class="btn btn--primary"
            disabled={connectDisabled}
            title={connectTitle}
            onclick={onconnect}
          >
            <Icon name="power" size={13} />
            Connect
          </button>
        {/if}
        {#if connected && ontest}
          <button
            type="button"
            class="btn"
            disabled={testing || busy}
            title={testing ? "Delay test already running" : "Test delay for this node"}
            onclick={ontest}
          >
            <span class:spin={testing}><Icon name="refresh" size={13} /></span>
            {testing ? "Testing" : "Test delay"}
          </button>
        {/if}
      </div>
    </div>
  {:else}
    <div class="inspector__identity">
      <h2 class="inspector__title truncate" title={profileName ?? "No profile selected"}>
        {profileName ?? "No profile selected"}
      </h2>
      <div class="row row--wrap">
        <span class="tag">{connected ? "live" : "profile"}</span>
        {#if mixedPort !== null}
          <span class="tag mono">port {mixedPort}</span>
        {/if}
        {#if selectedGroup}
          <span class="tag" title={selectedGroup.tag}>{selectedGroup.tag}</span>
        {/if}
      </div>
    </div>

    <div class="inspector__body">
      <dl class="deflist">
        <div class="deflist__row">
          <dt class="deflist__term">Status</dt>
          <dd class="deflist__value mono">{statusWord(phase)}</dd>
        </div>
        <div class="deflist__row">
          <dt class="deflist__term">Profile</dt>
          <dd class="deflist__value mono">{profileName ?? "—"}</dd>
        </div>
        <div class="deflist__row">
          <dt class="deflist__term">Mixed port</dt>
          <dd class="deflist__value mono">{mixedPort ?? "—"}</dd>
        </div>
        {#if selectedGroup}
          <div class="deflist__row">
            <dt class="deflist__term">Group</dt>
            <dd class="deflist__value mono">{selectedGroup.tag}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Now</dt>
            <dd class="deflist__value mono">{selectedGroup.now ?? "not reported"}</dd>
          </div>
        {/if}
      </dl>

      <section class="inspector__section" aria-label="Session overview">
        <h3 class="inspector__section-title">Session</h3>
        <dl class="deflist">
          <div class="deflist__row">
            <dt class="deflist__term">Upload</dt>
            <dd class="deflist__value mono">{formatBytes(stats.up)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Download</dt>
            <dd class="deflist__value mono">{formatBytes(stats.down)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Up speed</dt>
            <dd class="deflist__value mono">{formatSpeed(stats.upSpeed)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Down speed</dt>
            <dd class="deflist__value mono">{formatSpeed(stats.downSpeed)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Connections</dt>
            <dd class="deflist__value mono">{stats.connections}</dd>
          </div>
        </dl>
        <p class="inspector__hint">Session-wide totals; not per-node.</p>
      </section>

      <div class="inspector__actions">
        {#if connected}
          <button
            type="button"
            class="btn btn--danger"
            disabled={busy}
            title={busy ? "A session action is already running" : "Disconnect the engine"}
            onclick={ondisconnect}
          >
            <Icon name="power" size={13} />
            Disconnect
          </button>
        {:else if phase === "error" && enginePid !== null}
          <button
            type="button"
            class="btn btn--danger"
            disabled={busy}
            title={busy ? "A session action is already running" : "Stop the failed engine process"}
            onclick={onconnect}
          >
            <Icon name="power" size={13} />
            Stop engine
          </button>
        {:else}
          <button
            type="button"
            class="btn btn--primary"
            disabled={connectDisabled}
            title={connectTitle}
            onclick={onconnect}
          >
            <Icon name="power" size={13} />
            Connect
          </button>
        {/if}
      </div>

      <p class="inspector__hint">Select a node in the constellation to inspect it.</p>
    </div>
  {/if}
</aside>

<style>
  .inspector {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface);
    overflow: hidden;
  }

  .inspector__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 10px var(--space-4);
    border-bottom: 1px solid var(--line);
    min-height: 38px;
  }

  .inspector__status {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--fs-2xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .inspector__identity {
    display: grid;
    gap: 6px;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  .inspector__title {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 500;
    letter-spacing: -0.01em;
  }

  .inspector__body {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-4);
    min-height: 0;
    overflow-y: auto;
  }

  .inspector__section {
    display: grid;
    gap: 6px;
  }

  .inspector__section-title {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    font-weight: 500;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .inspector__hint {
    margin: 0;
    font-size: var(--fs-xs);
    line-height: 1.45;
    color: var(--muted);
  }

  .inspector__actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .inspector .deflist__row {
    grid-template-columns: minmax(88px, 120px) minmax(0, 1fr);
    padding: 7px 0;
  }

  .inspector .deflist__term {
    font-size: var(--fs-xs);
  }

  .inspector .deflist__value {
    font-size: var(--fs-sm);
  }

  @media (max-width: 620px) {
    .inspector .deflist__row {
      grid-template-columns: minmax(0, 1fr);
      gap: 2px;
    }
  }
</style>
