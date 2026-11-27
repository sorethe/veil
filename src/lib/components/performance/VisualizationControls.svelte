<script lang="ts">
  import { onMount } from "svelte";
  import { parseFrameTarget, performanceStore } from "../../performance.svelte";
  import Toggle from "../Toggle.svelte";

  onMount(() => {
    performanceStore.ensureDisplayEstimate();
  });

  const options = $derived(performanceStore.targetOptions);
  const selected = $derived(String(performanceStore.target));
  const unsupported = $derived(!performanceStore.isTargetAllowed(performanceStore.target));
  const displayLabel = $derived(
    performanceStore.detecting
      ? "measuring…"
      : performanceStore.display === null
        ? "not measured"
        : `${performanceStore.display.hz} Hz · ${performanceStore.display.confidence}`,
  );

  function changeTarget(event: Event): void {
    const next = parseFrameTarget((event.currentTarget as HTMLSelectElement).value);
    if (next !== null) performanceStore.setTarget(next);
  }
</script>

<div class="viz-controls" role="group" aria-label="Visualization performance">
  <div class="viz-controls__field">
    <label class="viz-controls__label" for="viz-fps-target">Frame cap</label>
    <select
      id="viz-fps-target"
      class="select select--inline viz-controls__select"
      value={selected}
      aria-describedby="viz-fps-note"
      onchange={changeTarget}
    >
      {#each options as option (option.value)}
        <option
          value={String(option.value)}
          disabled={!option.allowed}
          title={option.reason ?? undefined}
        >
          {option.label}
        </option>
      {/each}
    </select>
  </div>

  <div class="viz-controls__field">
    <span class="viz-controls__label">Overlay</span>
    <div class="viz-controls__toggle">
      <Toggle
        checked={performanceStore.overlay}
        label="Performance overlay"
        onchange={(next) => performanceStore.setOverlay(next)}
      />
      <span class="viz-controls__state mono">{performanceStore.overlay ? "on" : "off"}</span>
    </div>
  </div>

  <div class="viz-controls__field viz-controls__field--display">
    <span class="viz-controls__label">Monitor</span>
    <div class="viz-controls__display">
      <span class="viz-controls__hz mono">{displayLabel}</span>
      <button
        type="button"
        class="btn btn--sm btn--quiet"
        disabled={performanceStore.detecting}
        title="Measure the render loop's refresh rate with requestAnimationFrame"
        onclick={() => void performanceStore.detectDisplay()}
      >
        {performanceStore.detecting ? "Detecting" : "Re-detect"}
      </button>
    </div>
  </div>

  <p class="viz-controls__note" id="viz-fps-note">
    Frame cap limits redraws; monitor Hz is what the display can show. A cap above the display
    estimate is disabled. {performanceStore.effectiveNote}
  </p>
  <p class="viz-controls__note viz-controls__note--dim" id="viz-display-note">
    {performanceStore.displaySummary}
  </p>
  {#if unsupported}
    <p class="viz-controls__note viz-controls__note--warn" role="status">
      Selected {performanceStore.target} fps is unavailable — the renderer uses Auto at
      {Math.round(performanceStore.effectiveHz)} fps.
    </p>
  {/if}
</div>

<style>
  .viz-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-2) var(--space-4);
    min-width: 0;
  }

  .viz-controls__field {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .viz-controls__label {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    font-weight: 500;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .viz-controls__select {
    width: 148px;
    min-width: 148px;
  }

  .viz-controls__toggle {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 30px;
  }

  .viz-controls__state {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  .viz-controls__display {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 30px;
  }

  .viz-controls__hz {
    font-size: var(--fs-xs);
    color: var(--text);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .viz-controls__note {
    flex-basis: 100%;
    margin: 0;
    font-size: var(--fs-2xs);
    line-height: 1.5;
    color: var(--muted);
  }

  .viz-controls__note--dim {
    color: var(--faint);
  }

  .viz-controls__note--warn {
    color: var(--error);
  }

  @media (max-width: 940px) {
    .viz-controls {
      gap: var(--space-2) var(--space-3);
    }

    .viz-controls__select {
      width: 132px;
      min-width: 132px;
    }
  }
</style>
