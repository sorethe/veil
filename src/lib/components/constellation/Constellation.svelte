<script lang="ts">
  import type { EngineStatus } from "../../api";
  import type { ConstellationCluster, ConstellationNode } from "../../constellation";
  import { clamp } from "../../format";
  import { SvelteMap } from "svelte/reactivity";
  import Icon from "../Icon.svelte";

  let {
    clusters,
    phase,
    selectedId,
    activeId,
    onselect,
    reason = null,
  }: {
    clusters: ConstellationCluster[];
    phase: EngineStatus;
    selectedId: string | null;
    activeId: string | null;
    onselect: (id: string | null) => void;
    reason?: string | null;
  } = $props();

  const THRESHOLD = 14;
  const PAD = 70;
  const NODE_R = 7;

  let container = $state<HTMLDivElement | null>(null);
  let size = $state({ w: 0, h: 0 });
  let zoom = $state(1);
  let pan = $state({ x: 0, y: 0 });
  let expanded = $state<string[]>([]);
  let hovered = $state<string | null>(null);
  let focused = $state<string | null>(null);
  let reduced = $state(false);
  let drift = $state(0);
  let dragging = false;
  let moved = false;
  let last = { x: 0, y: 0 };

  function hash(value: string): number {
    let h = 2166136261;
    for (let i = 0; i < value.length; i++) h = Math.imul(h ^ value.charCodeAt(i), 16777619);
    return h >>> 0;
  }

  function random(seed: number): number {
    let t = (seed + 0x6d2b79f5) | 0;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }

  const layout = $derived.by(() => {
    const set = new Set(expanded);
    const items = [...clusters]
      .sort((a, b) => a.id.localeCompare(b.id))
      .map((cluster) => {
        const count = cluster.nodes.length;
        const collapsed = count > THRESHOLD && !set.has(cluster.id);
        const radius = count === 0 || collapsed ? 0 : Math.max(34, count * 7);
        return { cluster, count, collapsed, radius };
      });
    let hub = 320;
    const measure = (item: (typeof items)[number]) =>
      item.collapsed
        ? 0.45
        : item.radius
          ? 2 * Math.asin(Math.min(1, item.radius / hub)) + 0.18
          : 0.35;
    for (let i = 0; i < 24; i++) {
      const total = items.reduce((sum, item) => sum + measure(item), 0);
      if (total <= Math.PI * 2) break;
      hub *= 1.12;
    }
    const total = items.reduce((sum, item) => sum + measure(item), 0);
    const scale = total > Math.PI * 2 ? (Math.PI * 2) / total : 1;
    let cursor = -Math.PI / 2;
    const out = items.map((item) => {
      const width = measure(item) * scale;
      const angle = cursor + width / 2;
      const hubX = Math.cos(angle) * hub;
      const hubY = Math.sin(angle) * hub;
      const nodes = item.collapsed
        ? []
        : item.cluster.nodes.map((node, index) => {
            const a =
              (index / item.cluster.nodes.length) * Math.PI * 2 +
              random(hash(`${item.cluster.id}:${node.id}`)) * 0.2;
            return {
              node,
              x: hubX + Math.cos(a) * item.radius,
              y: hubY + Math.sin(a) * item.radius,
            };
          });
      cursor += width;
      return {
        cluster: item.cluster,
        count: item.count,
        collapsed: item.collapsed,
        hubX,
        hubY,
        nodes,
      };
    });
    const xs = out.flatMap((c) => [c.hubX, ...c.nodes.map((n) => n.x)]);
    const ys = out.flatMap((c) => [c.hubY, ...c.nodes.map((n) => n.y)]);
    return {
      clusters: out,
      minX: Math.min(-80, ...xs) - PAD,
      maxX: Math.max(80, ...xs) + PAD,
      minY: Math.min(-80, ...ys) - PAD,
      maxY: Math.max(80, ...ys) + PAD,
    };
  });

  const fit = $derived.by(() => {
    const w = layout.maxX - layout.minX;
    const h = layout.maxY - layout.minY;
    const aspect = size.w && size.h ? size.w / size.h : 1;
    const fw = w / h > aspect ? w : h * aspect;
    return {
      cx: (layout.minX + layout.maxX) / 2,
      cy: (layout.minY + layout.maxY) / 2,
      fw,
      fh: fw / aspect,
    };
  });

  const view = $derived.by(() => {
    const vw = fit.fw / zoom;
    const vh = fit.fh / zoom;
    return { x: fit.cx - vw / 2 + pan.x, y: fit.cy - vh / 2 + pan.y, w: vw, h: vh };
  });

  const nodes = $derived.by(() => {
    const map = new SvelteMap<
      string,
      { node: ConstellationNode; cluster: (typeof layout.clusters)[number]; x: number; y: number }
    >();
    for (const cluster of layout.clusters)
      for (const node of cluster.nodes)
        map.set(node.node.id, { node: node.node, cluster, x: node.x, y: node.y });
    return map;
  });

  const clustersOf = $derived.by(() => {
    const map = new SvelteMap<string, string>();
    for (const cluster of clusters) for (const node of cluster.nodes) map.set(node.id, cluster.id);
    return map;
  });

  function pathFor(id: string | null): string | null {
    if (!id) return null;
    const hit = nodes.get(id);
    if (hit) return `M0 0 L${hit.cluster.hubX} ${hit.cluster.hubY} L${hit.x} ${hit.y}`;
    const cluster = layout.clusters.find((c) => c.cluster.id === clustersOf.get(id));
    return cluster ? `M0 0 L${cluster.hubX} ${cluster.hubY}` : null;
  }

  let lastActiveId = $state<string | null>(null);

  $effect(() => {
    if (phase === "connected" && activeId) lastActiveId = activeId;
  });

  $effect(() => {
    if (lastActiveId && !clustersOf.has(lastActiveId)) lastActiveId = null;
  });

  const activePath = $derived(phase === "connected" && activeId ? pathFor(activeId) : null);
  const selectedPath = $derived(phase === "starting" && selectedId ? pathFor(selectedId) : null);
  const brokenPath = $derived(
    phase === "stopping" || phase === "error"
      ? (pathFor(selectedId) ?? pathFor(lastActiveId))
      : null,
  );

  function latencyText(node: ConstellationNode): string {
    if (node.delay === "error") return "unavailable";
    if (typeof node.delay === "number")
      return node.delay < 0 ? "timeout" : `${Math.round(node.delay)} ms`;
    return "not measured";
  }

  function truncate(value: string, max: number): string {
    return value.length > max ? `${value.slice(0, max - 1)}…` : value;
  }

  function isStatic(target: EventTarget | null): boolean {
    return target instanceof Element && !!target.closest("[data-interactive], [data-static]");
  }

  const tooltip = $derived.by(() => {
    const id = hovered ?? focused;
    if (!id || !size.w) return null;
    if (id.startsWith("aggregate:")) {
      const cluster = layout.clusters.find((c) => c.cluster.id === id.slice(10));
      if (!cluster) return null;
      return {
        label: cluster.cluster.label,
        meta: `${cluster.count} nodes · collapsed`,
        x: ((cluster.hubX - view.x) / view.w) * size.w,
        y: ((cluster.hubY - view.y) / view.h) * size.h,
      };
    }
    const hit = nodes.get(id);
    if (!hit) return null;
    return {
      label: hit.node.label,
      meta: `${hit.node.type} · ${latencyText(hit.node)}`,
      x: ((hit.x - view.x) / view.w) * size.w,
      y: ((hit.y - view.y) / view.h) * size.h,
    };
  });

  $effect(() => {
    if (!container) return;
    const observer = new ResizeObserver(([entry]) => {
      size = { w: entry.contentRect.width, h: entry.contentRect.height };
    });
    observer.observe(container);
    return () => observer.disconnect();
  });

  $effect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    reduced = query.matches;
    const onChange = (event: MediaQueryListEvent) => (reduced = event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  });

  $effect(() => {
    if (reduced || clusters.length === 0) return;
    let frame = 0;
    let running = false;
    let lastTime = 0;
    const step = (time: number) => {
      if (document.hidden) {
        running = false;
        return;
      }
      if (time - lastTime >= 1000 / 30) {
        lastTime = time;
        drift += 0.0004;
      }
      frame = requestAnimationFrame(step);
    };
    const start = () => {
      if (running) return;
      running = true;
      frame = requestAnimationFrame(step);
    };
    const onVisibility = () => {
      if (document.hidden) {
        cancelAnimationFrame(frame);
        running = false;
      } else start();
    };
    start();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  });

  $effect(() => {
    const el = container;
    if (!el) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = el.getBoundingClientRect();
      const next = clamp(zoom * Math.exp(-event.deltaY * 0.001), 0.6, 2.5);
      if (next === zoom) return;
      const px = (event.clientX - rect.left) / rect.width;
      const py = (event.clientY - rect.top) / rect.height;
      const wx = view.x + px * view.w;
      const wy = view.y + py * view.h;
      const vw = fit.fw / next;
      const vh = fit.fh / next;
      pan = {
        x: wx - px * vw - fit.cx + vw / 2,
        y: wy - py * vh - fit.cy + vh / 2,
      };
      zoom = next;
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  function toggle(id: string): void {
    expanded = expanded.includes(id) ? expanded.filter((value) => value !== id) : [...expanded, id];
  }

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0 || isStatic(event.target)) return;
    dragging = true;
    moved = false;
    last = { x: event.clientX, y: event.clientY };
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent): void {
    if (!dragging) return;
    const dx = event.clientX - last.x;
    const dy = event.clientY - last.y;
    if (Math.abs(dx) + Math.abs(dy) > 2) moved = true;
    last = { x: event.clientX, y: event.clientY };
    pan = {
      x: pan.x - (dx / Math.max(1, size.w)) * view.w,
      y: pan.y - (dy / Math.max(1, size.h)) * view.h,
    };
  }

  function onPointerUp(event: PointerEvent): void {
    if (!dragging) return;
    dragging = false;
    (event.currentTarget as Element).releasePointerCapture(event.pointerId);
  }

  function onBackgroundClick(event: MouseEvent): void {
    if (moved || isStatic(event.target)) return;
    onselect(null);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="constellation"
  bind:this={container}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onclick={onBackgroundClick}
  onkeydown={(event) => {
    if (event.key === "Escape") onselect(null);
  }}
  role="application"
  tabindex="0"
  aria-label="Outbound constellation"
>
  <!-- The pan/zoom canvas is intentionally a role="application" interactive surface; Escape clears selection. -->
  <svg width="100%" height="100%" viewBox={`${view.x} ${view.y} ${view.w} ${view.h}`}>
    <g aria-hidden="true" class="ambient">
      {#each Array.from({ length: 28 }, (_, i) => i) as i (i)}
        <circle
          cx={Math.cos(i * 2.399 + drift) * (120 + i * 16)}
          cy={Math.sin(i * 2.399 + drift) * (120 + i * 16)}
          r="1.3"
          class="particle"
        />
      {/each}
    </g>

    <g class="graph" class:dim={phase === "stopping" || phase === "error"}>
      {#each layout.clusters as cluster (cluster.cluster.id)}
        <line x1="0" y1="0" x2={cluster.hubX} y2={cluster.hubY} class="edge" />
        {#each cluster.nodes as node (node.node.id)}
          <line x1={cluster.hubX} y1={cluster.hubY} x2={node.x} y2={node.y} class="edge" />
        {/each}
      {/each}
    </g>

    {#if activePath}
      <path d={activePath} class="active-glow" />
      <path d={activePath} class="active-path" />
    {/if}
    {#if selectedPath}
      <path d={selectedPath} class:pulse={!reduced} class:pulse-static={reduced} />
    {/if}
    {#if brokenPath}
      <path d={brokenPath} class="broken" class:broken-stop={phase === "stopping"} />
    {/if}

    {#each layout.clusters as cluster (cluster.cluster.id)}
      {#if cluster.collapsed}
        {@const active = activeId !== null && clustersOf.get(activeId) === cluster.cluster.id}
        <g
          class="aggregate"
          class:active
          role="button"
          tabindex="0"
          aria-label={`Show ${cluster.count} nodes in ${cluster.cluster.label}`}
          data-interactive
          onpointerenter={() => (hovered = `aggregate:${cluster.cluster.id}`)}
          onpointerleave={() => (hovered = null)}
          onfocus={() => (focused = `aggregate:${cluster.cluster.id}`)}
          onblur={() => (focused = null)}
          onclick={() => toggle(cluster.cluster.id)}
          onkeydown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              toggle(cluster.cluster.id);
            }
          }}
        >
          <circle cx={cluster.hubX} cy={cluster.hubY} r="13" class="aggregate__circle" />
          <text
            x={cluster.hubX}
            y={cluster.hubY}
            class="aggregate__count"
            text-anchor="middle"
            dominant-baseline="central"
          >
            {cluster.count}
          </text>
          <text x={cluster.hubX} y={cluster.hubY - 22} class="cluster-label" text-anchor="middle">
            {truncate(cluster.cluster.label, 20)}
          </text>
          <text
            x={cluster.hubX}
            y={cluster.hubY + 28}
            class="aggregate__action"
            text-anchor="middle"
          >
            show {cluster.count}
          </text>
          {#if focused === `aggregate:${cluster.cluster.id}`}
            <circle cx={cluster.hubX} cy={cluster.hubY} r="17" class="focus-ring" />
          {/if}
        </g>
      {:else}
        <circle cx={cluster.hubX} cy={cluster.hubY} r="3.5" class="hub" />
        <text
          x={cluster.hubX}
          y={cluster.hubY + (cluster.hubY < 0 ? -10 : 20)}
          class="cluster-label"
          text-anchor="middle"
          data-static
        >
          {truncate(cluster.cluster.label, 20)}
        </text>
        {#if cluster.count > THRESHOLD}
          <g
            class="collapse"
            role="button"
            tabindex="0"
            aria-label={`Collapse ${cluster.count} nodes in ${cluster.cluster.label}`}
            data-interactive
            onpointerenter={() => (hovered = `aggregate:${cluster.cluster.id}`)}
            onpointerleave={() => (hovered = null)}
            onfocus={() => (focused = `collapse:${cluster.cluster.id}`)}
            onblur={() => (focused = null)}
            onclick={() => toggle(cluster.cluster.id)}
            onkeydown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                toggle(cluster.cluster.id);
              }
            }}
          >
            <circle cx={cluster.hubX} cy={cluster.hubY} r="8.5" class="collapse__circle" />
            <text
              x={cluster.hubX}
              y={cluster.hubY}
              class="collapse__mark"
              text-anchor="middle"
              dominant-baseline="central"
            >
              −
            </text>
            {#if focused === `collapse:${cluster.cluster.id}`}
              <circle cx={cluster.hubX} cy={cluster.hubY} r="12.5" class="focus-ring" />
            {/if}
          </g>
        {/if}
        {#each cluster.nodes as node (node.node.id)}
          {@const active = phase === "connected" && activeId === node.node.id}
          {@const selected = selectedId === node.node.id}
          {@const unavailable =
            node.node.delay === "error" ||
            (typeof node.node.delay === "number" && node.node.delay < 0)}
          <g
            class="node"
            class:active
            class:selected
            class:unavailable
            class:live={phase === "connected" && node.node.live}
            role="button"
            tabindex="0"
            aria-label={`${node.node.label}, ${node.node.type}, ${latencyText(node.node)}`}
            data-interactive
            onpointerenter={() => (hovered = node.node.id)}
            onpointerleave={() => (hovered = null)}
            onfocus={() => (focused = node.node.id)}
            onblur={() => (focused = null)}
            onclick={() => onselect(node.node.id)}
            onkeydown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onselect(node.node.id);
              }
            }}
          >
            {#if active || (phase === "connected" && node.node.live)}
              <circle cx={node.x} cy={node.y} r={NODE_R + 7} class="node__halo" />
            {/if}
            {#if selected}
              <circle cx={node.x} cy={node.y} r={NODE_R + 4} class="node__ring" />
            {/if}
            <circle cx={node.x} cy={node.y} r={NODE_R} class="node__dot" />
            {#if unavailable}
              <line
                x1={node.x + 4}
                y1={node.y - 2}
                x2={node.x + 9}
                y2={node.y - 7}
                class="node__gap"
              />
            {/if}
            <text x={node.x} y={node.y + NODE_R + 12} class="node__label" text-anchor="middle">
              {truncate(node.node.label, cluster.count > THRESHOLD ? 12 : 18)}
            </text>
            {#if focused === node.node.id}
              <circle cx={node.x} cy={node.y} r={NODE_R + 6} class="focus-ring" />
            {/if}
          </g>
        {/each}
      {/if}
    {/each}

    <g class="center" data-static aria-hidden="true">
      <circle cx="0" cy="0" r="68" class="center__halo" />
      <circle cx="0" cy="0" r="56" class="center__ring" />
      <circle cx="0" cy="0" r="46" class="center__ring" />
      <circle cx="0" cy="0" r="36" class="center__ring" />
      <circle cx="0" cy="0" r="26" class="center__core" />
      <text x="0" y="0" class="center__label" text-anchor="middle" dominant-baseline="central">
        you
      </text>
    </g>
  </svg>

  <div class="toolbar" data-interactive>
    <button
      class="icon-btn"
      aria-label="Zoom in"
      onclick={() => (zoom = clamp(zoom * 1.2, 0.6, 2.5))}
    >
      +
    </button>
    <button
      class="icon-btn"
      aria-label="Zoom out"
      onclick={() => (zoom = clamp(zoom / 1.2, 0.6, 2.5))}
    >
      −
    </button>
    <button
      class="icon-btn"
      aria-label="Fit constellation"
      onclick={() => {
        zoom = 1;
        pan = { x: 0, y: 0 };
      }}
    >
      <Icon name="refresh" size={14} />
    </button>
  </div>

  {#if tooltip}
    <div
      class="tooltip"
      role="tooltip"
      style={`left:${clamp(tooltip.x, 8, size.w - 8)}px;top:${clamp(tooltip.y, 8, size.h - 8)}px`}
    >
      <span class="tooltip__label mono">{tooltip.label}</span>
      <span class="tooltip__meta mono">{tooltip.meta}</span>
    </div>
  {/if}

  {#if clusters.length === 0}
    <div class="empty-overlay">
      <p class="empty-overlay__message">
        {reason ??
          "No outbounds to display. Connect a session or select a profile with a valid configuration."}
      </p>
    </div>
  {/if}
</div>

<style>
  .constellation {
    position: relative;
    height: 100%;
    min-height: 320px;
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--bg);
    overflow: hidden;
    touch-action: none;
  }

  .constellation > svg {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
  }

  .constellation > svg:active {
    cursor: grabbing;
  }

  .edge {
    stroke: var(--line);
    stroke-width: 1;
  }

  .node__dot,
  .node__halo,
  .node__ring,
  .node__gap,
  .edge,
  .active-path,
  .active-glow,
  .broken,
  .pulse,
  .pulse-static,
  .graph.dim {
    transition:
      fill 220ms var(--ease),
      stroke 220ms var(--ease),
      opacity 220ms var(--ease);
  }

  .graph.dim {
    opacity: 0.45;
  }

  .active-glow {
    fill: none;
    stroke: var(--accent);
    stroke-width: 7;
    opacity: 0.16;
  }

  .active-path {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.8;
  }

  .pulse {
    fill: none;
    stroke: var(--warning);
    stroke-width: 1.6;
    stroke-dasharray: 4 9;
    animation: pulse 1.4s linear infinite;
  }

  .pulse-static {
    fill: none;
    stroke: var(--warning);
    stroke-width: 1.4;
    stroke-dasharray: 4 9;
    opacity: 0.8;
  }

  .broken {
    fill: none;
    stroke: var(--error);
    stroke-width: 1.6;
    stroke-dasharray: 3 9;
  }

  .broken-stop {
    stroke: var(--warning);
  }

  .hub {
    fill: var(--surface);
    stroke: var(--line-strong);
    stroke-width: 1.2;
  }

  .cluster-label {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 9px;
    pointer-events: none;
  }

  .aggregate {
    cursor: pointer;
  }

  .aggregate__circle {
    fill: var(--surface);
    stroke: var(--line-strong);
    stroke-width: 1.2;
  }

  .aggregate.active .aggregate__circle {
    stroke: var(--accent);
  }

  .aggregate__count {
    fill: var(--text);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    pointer-events: none;
  }

  .aggregate__action {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 9px;
    pointer-events: none;
  }

  .collapse {
    cursor: pointer;
  }

  .collapse__circle {
    fill: var(--surface);
    stroke: var(--line-strong);
    stroke-width: 1.2;
  }

  .collapse__mark {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 11px;
    pointer-events: none;
  }

  .node {
    cursor: pointer;
  }

  .node__dot {
    fill: var(--surface);
    stroke: var(--line-strong);
    stroke-width: 1.4;
  }

  .node.live .node__dot {
    stroke: var(--accent);
  }

  .node.active .node__dot {
    fill: var(--accent);
    stroke: var(--accent);
  }

  .node.unavailable .node__dot {
    opacity: 0.4;
  }

  .node__halo {
    fill: var(--accent);
    opacity: 0.12;
    pointer-events: none;
  }

  .node.active .node__halo {
    opacity: 0.22;
  }

  .node__ring {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.2;
    pointer-events: none;
  }

  .node__gap {
    stroke: var(--error);
    stroke-width: 1.6;
    stroke-linecap: round;
  }

  .node__label {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 9px;
    pointer-events: none;
  }

  .focus-ring {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.4;
    stroke-dasharray: 2 2;
    pointer-events: none;
  }

  .center__halo {
    fill: var(--accent);
    opacity: 0.08;
    animation: halo-breathe 5s ease-in-out infinite;
  }

  .center__ring {
    fill: none;
    stroke: var(--line-strong);
    stroke-width: 1;
    opacity: 0.6;
  }

  .center__core {
    fill: var(--surface);
    stroke: var(--accent);
    stroke-width: 1.4;
    animation: core-breathe 5s ease-in-out infinite;
  }

  .center__label {
    fill: var(--text);
    font-family: var(--font-mono);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .particle {
    fill: var(--faint);
    opacity: 0.35;
    pointer-events: none;
  }

  .toolbar {
    position: absolute;
    top: 8px;
    right: 8px;
    display: flex;
    gap: 4px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--surface);
  }

  .toolbar .icon-btn {
    width: 24px;
    height: 24px;
    font-size: 14px;
  }

  .tooltip {
    position: absolute;
    transform: translate(-50%, calc(-100% - 12px));
    max-width: 240px;
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--surface);
    display: grid;
    gap: 2px;
    pointer-events: none;
  }

  .tooltip__label {
    font-size: var(--fs-xs);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tooltip__meta {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  .empty-overlay {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: var(--space-4);
    pointer-events: none;
  }

  .empty-overlay__message {
    max-width: 46ch;
    color: var(--muted);
    font-size: var(--fs-sm);
    text-align: center;
  }

  @keyframes pulse {
    to {
      stroke-dashoffset: -26;
    }
  }

  @keyframes core-breathe {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.88;
    }
  }

  @keyframes halo-breathe {
    0%,
    100% {
      opacity: 0.06;
    }
    50% {
      opacity: 0.14;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .pulse {
      animation: none;
    }

    .center__core,
    .center__halo {
      animation: none;
    }

    .node__dot,
    .node__halo,
    .node__ring,
    .node__gap,
    .edge,
    .active-path,
    .active-glow,
    .broken,
    .pulse,
    .pulse-static,
    .graph.dim {
      transition: none;
    }
  }
</style>
