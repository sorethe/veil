<script lang="ts">
  import type { EngineState, Stats } from "../api";
  import { formatBytes, formatDelay, formatDuration, formatSpeed, logSpeedRatio, modeLabel, statusWord, toMillis } from "../format";
  import type { TrafficSample } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Sparkline from "./Sparkline.svelte";
  import StatusGlyph from "./StatusGlyph.svelte";

  let {
    engine,
    stats,
    history,
    connectBusy,
    canConnect,
    disabledReason,
    profileName,
    onToggle,
  }: {
    engine: EngineState;
    stats: Stats;
    history: TrafficSample[];
    connectBusy: boolean;
    canConnect: boolean;
    disabledReason: string | null;
    profileName: string;
    onToggle: () => void;
  } = $props();

  let now = $state(Date.now());

  $effect(() => {
    if (!engine.startedAt) return;
    now = Date.now();
    const timer = window.setInterval(() => {
      now = Date.now();
    }, 1000);
    return () => window.clearInterval(timer);
  });

  const word = $derived(statusWord(engine.status));
  const elapsed = $derived(
    engine.startedAt ? Math.max(0, (now - toMillis(engine.startedAt)) / 1000) : stats.duration,
  );
  const downRatio = $derived(logSpeedRatio(stats.downSpeed));
  const upRatio = $derived(logSpeedRatio(stats.upSpeed));
  const downSeries = $derived(history.map((sample) => sample.down));
  const upSeries = $derived(history.map((sample) => sample.up));

  const busy = $derived(engine.status === "starting" || engine.status === "stopping");
  const isLive = $derived(engine.status === "connected");
  const failedWithRuntime = $derived(engine.status === "error" && engine.pid !== null);
  const canStop = $derived(isLive || failedWithRuntime);
  const buttonDisabled = $derived(busy || connectBusy || (!canStop && !canConnect));
  const buttonLabel = $derived(
    engine.status === "starting"
      ? "Linking"
      : engine.status === "stopping"
        ? "Stopping"
        : isLive
          ? "Disconnect"
          : failedWithRuntime
            ? "Stop engine"
            : engine.status === "error"
              ? "Reconnect"
              : "Connect",
  );

  const ticks = Array.from({ length: 36 }, (_, index) => {
    const angle = (index / 36) * Math.PI * 2 - Math.PI / 2;
    const outer = 104;
    const inner = index % 6 === 0 ? 96 : index % 3 === 0 ? 99 : 101;
    return {
      x1: 120 + Math.cos(angle) * inner,
      y1: 120 + Math.sin(angle) * inner,
      x2: 120 + Math.cos(angle) * outer,
      y2: 120 + Math.sin(angle) * outer,
      major: index % 6 === 0,
    };
  });
</script>

<section class="instrument" class:instrument--live={isLive} aria-label="Connection instrument">
  <header class="instrument__head">
    <div class="instrument__identity">
      <span class="kicker">Session</span>
      <p class="instrument__profile truncate">{profileName}</p>
    </div>
    <div class="instrument__state" aria-live="polite">
      <StatusGlyph status={engine.status} size={14} />
      <span class="instrument__word {`st-${engine.status}`}">{word}</span>
      <span class="instrument__meta">
        {engine.mode ? modeLabel(engine.mode) : "no session"}{#if engine.pid} · pid {engine.pid}{/if}
      </span>
    </div>
  </header>

  <div class="instrument__main">
    <div class="dial">
      <svg viewBox="0 0 240 240" class="dial__svg" aria-hidden="true">
        <circle class="dial__track" cx="120" cy="120" r="84" />
        <circle class="dial__track dial__track--inner" cx="120" cy="120" r="64" />
        {#each ticks as tick}
          <line
            class="dial__tick"
            class:dial__tick--major={tick.major}
            x1={tick.x1}
            y1={tick.y1}
            x2={tick.x2}
            y2={tick.y2}
          />
        {/each}
        <circle
          class="dial__arc dial__arc--down"
          class:dial__arc--live={isLive}
          cx="120"
          cy="120"
          r="84"
          pathLength="100"
          stroke-dasharray={`${(downRatio * 100).toFixed(2)} 100`}
          transform="rotate(-90 120 120)"
        />
        <circle
          class="dial__arc dial__arc--up"
          cx="120"
          cy="120"
          r="64"
          pathLength="100"
          stroke-dasharray={`${(upRatio * 100).toFixed(2)} 100`}
          transform="rotate(-90 120 120)"
        />
        <line class="dial__cross" x1="120" y1="8" x2="120" y2="20" />
        <line class="dial__cross" x1="120" y1="220" x2="120" y2="232" />
        <line class="dial__cross" x1="8" y1="120" x2="20" y2="120" />
        <line class="dial__cross" x1="220" y1="120" x2="232" y2="120" />
      </svg>
      <div class="dial__center">
        <span class="dial__label">DOWN</span>
        <span class="dial__value">{formatSpeed(stats.downSpeed)}</span>
        <span class="dial__up">↑ {formatSpeed(stats.upSpeed)}</span>
      </div>
    </div>

    <div class="instrument__side">
      <div class="readouts">
        <div class="readouts__cell">
          <span class="kicker">Duration</span>
          <span class="readouts__value">{formatDuration(elapsed)}</span>
        </div>
        <div class="readouts__cell">
          <span class="kicker">Latency</span>
          <span class="readouts__value">{formatDelay(stats.latency)}</span>
        </div>
        <div class="readouts__cell">
          <span class="kicker">Connections</span>
          <span class="readouts__value">{stats.connections}</span>
        </div>
        <div class="readouts__cell">
          <span class="kicker">Session</span>
          <span class="readouts__value readouts__value--sm">
            ↓ {formatBytes(stats.down)} · ↑ {formatBytes(stats.up)}
          </span>
        </div>
      </div>

      <button
        type="button"
        class="power"
        class:power--live={isLive}
        disabled={buttonDisabled}
        title={disabledReason ?? undefined}
        onclick={onToggle}
      >
        <Icon name="power" size={16} />
        <span>{buttonLabel}</span>
      </button>
      <p class="instrument__hint">
        {#if disabledReason}
          {disabledReason}
        {:else}
          Enter on the control · <span class="kbd">Ctrl</span> + <span class="kbd">Enter</span> anywhere
        {/if}
      </p>
    </div>
  </div>

  <div class="instrument__telemetry">
    <div class="telemetry">
      <div class="telemetry__head">
        <span class="kicker">Download</span>
        <span class="telemetry__now">{formatSpeed(stats.downSpeed)}</span>
      </div>
      <Sparkline values={downSeries} tone={isLive ? "accent" : "muted"} label="Download speed history" />
    </div>
    <div class="telemetry">
      <div class="telemetry__head">
        <span class="kicker">Upload</span>
        <span class="telemetry__now">{formatSpeed(stats.upSpeed)}</span>
      </div>
      <Sparkline values={upSeries} tone="muted" label="Upload speed history" />
    </div>
    <p class="telemetry__scale">Speed arcs and traces use a shared logarithmic scale, 1 KiB/s to 1 GiB/s.</p>
  </div>
</section>

<style>
  .instrument {
    border: 1px solid var(--line);
    background: var(--surface);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .instrument__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: 10px var(--space-4);
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }

  .instrument__identity {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    min-width: 0;
  }

  .instrument__profile {
    font-family: var(--font-display);
    font-size: var(--fs-md);
    letter-spacing: -0.01em;
  }

  .instrument__state {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .instrument__word {
    font-family: var(--font-mono);
    font-size: var(--fs-md);
    font-weight: 500;
    letter-spacing: 0.16em;
  }

  .instrument__meta {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .instrument__main {
    display: grid;
    grid-template-columns: minmax(240px, 300px) minmax(0, 1fr);
    gap: var(--space-5);
    align-items: center;
    padding: var(--space-5) var(--space-4);
  }

  @media (max-width: 780px) {
    .instrument__main {
      grid-template-columns: minmax(0, 1fr);
      justify-items: center;
    }
  }

  .dial {
    position: relative;
    width: 240px;
    max-width: 100%;
    aspect-ratio: 1;
  }

  .dial__svg {
    width: 100%;
    height: 100%;
  }

  .dial__track {
    fill: none;
    stroke: var(--line);
    stroke-width: 1;
  }

  .dial__track--inner {
    stroke-opacity: 0.7;
  }

  .dial__tick {
    stroke: var(--line-strong);
    stroke-width: 1;
  }

  .dial__tick--major {
    stroke: var(--muted);
  }

  .dial__arc {
    fill: none;
    stroke-width: 2;
    stroke-linecap: butt;
    transition: stroke-dasharray 160ms var(--ease), stroke var(--fast) var(--ease);
    stroke: var(--muted);
  }

  .dial__arc--down.dial__arc--live {
    stroke: var(--accent);
  }

  .dial__arc--up {
    stroke: var(--text);
    stroke-opacity: 0.55;
    stroke-width: 1.5;
  }

  .dial__cross {
    stroke: var(--line-strong);
    stroke-width: 1;
  }

  .dial__center {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    pointer-events: none;
    text-align: center;
    padding: 0 48px;
  }

  .dial__label {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    letter-spacing: 0.14em;
    color: var(--muted);
  }

  .dial__value {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: var(--fs-xl);
    letter-spacing: -0.02em;
    line-height: 1.1;
  }

  .dial__up {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .instrument__side {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }

  .readouts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  .readouts__cell {
    padding: 9px 12px;
    display: grid;
    gap: 3px;
    border-right: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  .readouts__cell:nth-child(2n) {
    border-right: 0;
  }

  .readouts__cell:nth-last-child(-n + 2) {
    border-bottom: 0;
  }

  .readouts__value {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: var(--fs-lg);
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .readouts__value--sm {
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .power {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    width: 100%;
    height: 52px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    font-weight: 500;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    clip-path: polygon(0 0, calc(100% - 10px) 0, 100% 10px, 100% 100%, 10px 100%, 0 calc(100% - 10px));
    transition: background-color var(--fast) var(--ease), color var(--fast) var(--ease),
      border-color var(--fast) var(--ease);
  }

  .power:hover:not(:disabled) {
    border-color: var(--text);
  }

  .power--live {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-ink);
  }

  .power--live:hover:not(:disabled) {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 88%, var(--text));
  }

  .power:disabled {
    opacity: 0.45;
  }

  .instrument__hint {
    font-size: var(--fs-2xs);
    color: var(--faint);
    line-height: 1.5;
  }

  .instrument__telemetry {
    border-top: 1px solid var(--line);
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0 var(--space-5);
    padding: var(--space-3) var(--space-4);
  }

  @media (max-width: 620px) {
    .instrument__telemetry {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .telemetry {
    min-width: 0;
    padding: 4px 0;
  }

  .telemetry__head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 4px;
  }

  .telemetry__now {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .telemetry__scale {
    grid-column: 1 / -1;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--faint);
    padding-top: 4px;
  }
</style>
