<script lang="ts">
  import { performanceStore } from "../../performance.svelte";

  const stats = $derived(performanceStore.stats);
  const targetLabel = $derived(stats.target === "auto" ? "Auto" : `${stats.target} fps`);
  const effectiveLabel = $derived(`${Math.round(stats.effectiveHz)} fps`);
  const displayLabel = $derived(
    stats.displayHz === null ? "not measured" : `${stats.displayHz} Hz`,
  );
  const pacingLabel = $derived(
    stats.pacing === "vsync" ? "vsync-paced" : stats.pacing === "fixed" ? "capped" : "idle",
  );
  const fpsLabel = $derived(stats.fps === null ? "—" : stats.fps.toFixed(1));
  const frameLabel = $derived(stats.frameMs === null ? "—" : `${stats.frameMs.toFixed(1)} ms`);
  const unsupported = $derived(!performanceStore.isTargetAllowed(stats.target));
</script>

{#if performanceStore.overlay}
  <!-- Place inside a positioned container (e.g. the membrane stage). -->
  <aside class="perf-overlay" aria-label="Visualization performance">
    <dl class="perf-overlay__grid">
      <dt>FPS</dt>
      <dd>{fpsLabel}</dd>
      <dt>Frame time</dt>
      <dd>{frameLabel}</dd>
      <dt>Target</dt>
      <dd>{targetLabel}</dd>
      <dt>Effective</dt>
      <dd>{effectiveLabel}</dd>
      <dt>Display</dt>
      <dd>{displayLabel}</dd>
      <dt>Pacing</dt>
      <dd>{pacingLabel}</dd>
    </dl>
    {#if unsupported}
      <p class="perf-overlay__warn">Target unavailable — using Auto</p>
    {/if}
    <p class="perf-overlay__note" title={performanceStore.displaySummary}>
      rAF estimate · WebKitGTK may cap at 60 Hz<br />
      GPU/CPU: not exposed by WebKitGTK
    </p>
  </aside>
{/if}

<style>
  .perf-overlay {
    position: absolute;
    top: var(--space-3);
    right: var(--space-3);
    z-index: 3;
    display: grid;
    gap: 4px;
    max-width: 232px;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--raised);
    box-shadow: var(--glow);
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
    pointer-events: none;
  }

  .perf-overlay__grid {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 2px var(--space-3);
    margin: 0;
  }

  .perf-overlay__grid dt {
    color: var(--faint);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .perf-overlay__grid dd {
    margin: 0;
    color: var(--text);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .perf-overlay__warn {
    margin: 0;
    color: var(--error);
    line-height: 1.4;
  }

  .perf-overlay__note {
    margin: 0;
    padding-top: 4px;
    border-top: 1px solid var(--line);
    color: var(--faint);
    line-height: 1.4;
    white-space: normal;
  }
</style>
