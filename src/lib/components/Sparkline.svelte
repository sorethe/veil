<script lang="ts">
  import { logSpeedRatio } from "../format";

  let {
    values,
    tone = "muted",
    label,
    height = 34,
  }: {
    values: number[];
    tone?: "muted" | "accent";
    label: string;
    height?: number;
  } = $props();

  const points = $derived(buildPoints(values));

  function buildPoints(values: number[]): string {
    if (!values.length) return "";
    const width = 120;
    const heightUnits = 36;
    const pad = 2;
    const span = Math.max(values.length - 1, 1);
    return values
      .map((value, index) => {
        const x = (index / span) * width;
        const y = heightUnits - pad - logSpeedRatio(value) * (heightUnits - pad * 2);
        return `${x.toFixed(2)},${y.toFixed(2)}`;
      })
      .join(" ");
  }
</script>

<svg
  class="spark {`spark--${tone}`}"
  viewBox="0 0 120 36"
  preserveAspectRatio="none"
  role="img"
  aria-label={label}
  style={`height:${height}px`}
>
  <line class="spark__grid" x1="0" y1="34" x2="120" y2="34" />
  <line class="spark__grid" x1="0" y1="18" x2="120" y2="18" opacity="0.5" />
  <line class="spark__grid" x1="0" y1="2" x2="120" y2="2" opacity="0.5" />
  {#if points}
    <polyline class="spark__line" points={points} vector-effect="non-scaling-stroke" />
  {/if}
</svg>

<style>
  .spark {
    width: 100%;
    display: block;
    overflow: visible;
  }

  .spark__grid {
    stroke: var(--line);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }

  .spark__line {
    fill: none;
    stroke-width: 1.25;
    stroke-linejoin: miter;
    stroke-linecap: butt;
    stroke: var(--muted);
  }

  .spark--accent .spark__line {
    stroke: var(--accent);
  }
</style>
