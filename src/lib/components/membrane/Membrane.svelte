<script lang="ts">
  import { SvelteMap } from "svelte/reactivity";
  import type { EngineStatus } from "../../api";
  import { clamp, formatDelay } from "../../format";
  import {
    buildMembraneGeometry,
    createCamera,
    projectPoint,
    routeTarget,
    stableHash,
    updateCamera,
    type MembraneAnchorInput,
    type MembraneCamera,
    type MembraneGeometry,
    type ScreenPoint,
  } from "../../membrane";

  let {
    outbounds,
    activeId,
    selectedId,
    status,
    onselect,
  }: {
    outbounds: MembraneAnchorInput[];
    activeId: string | null;
    selectedId: string | null;
    status: EngineStatus;
    onselect: (id: string | null) => void;
  } = $props();

  const TAU = Math.PI * 2;
  const ROUTE_STEPS = 20;
  const FADE_SECONDS = 1;
  const ZOOM_MIN = 0.78;
  const ZOOM_MAX = 1.25;

  let host = $state<HTMLDivElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let hoverId = $state<string | null>(null);
  let tip = $state<{ x: number; y: number; label: string; meta: string } | null>(null);

  const geometry = $derived(buildMembraneGeometry(outbounds));
  const canvasLabel = $derived(
    `Network membrane with ${outbounds.length} real outbound anchor${outbounds.length === 1 ? "" : "s"}. Drag to rotate, scroll to zoom. Use the anchor list beside the canvas to select an outbound.`,
  );

  let current: MembraneGeometry = { points: [], edges: [], anchors: [] };
  let pointOrder: number[] = [];
  let anchorOrder: number[] = [];
  let pointX = new Float32Array(0);
  let pointY = new Float32Array(0);
  let pointDepth = new Float32Array(0);
  let anchorX = new Float32Array(0);
  let anchorY = new Float32Array(0);
  let anchorDepth = new Float32Array(0);
  let edgeBins: number[][] = Array.from({ length: 12 }, () => []);
  const routeLevels = new SvelteMap<string, number>();
  const scratch: ScreenPoint = { x: 0, y: 0, depth: 0 };
  const camera: MembraneCamera = createCamera();

  let hostWidth = 0;
  let hostHeight = 0;
  let pixelRatio = 1;
  let ctx2d: CanvasRenderingContext2D | null = null;

  let yaw = 0.42;
  let pitch = -0.12;
  let zoom = 1;
  let animationTime = 0;
  let lastDraw = 0;

  let running = false;
  let frame = 0;
  let singleFrame = 0;
  let onScreen = true;
  let reducedMotion = false;

  let dragging = false;
  let moved = false;
  let lastPointerX = 0;
  let lastPointerY = 0;

  interface Rgb {
    r: number;
    g: number;
    b: number;
  }

  interface MembraneColors {
    surface: string;
    edge: string[][];
    point: string[];
    anchorRing: string;
    anchorDot: string;
    anchorActive: string;
    anchorSelected: string;
    hover: string;
    route: string[];
    signal: string;
    haloFill: string;
    haloRing: string;
    startRing: string;
  }

  let colors: MembraneColors = fallbackColors();

  function rgba(color: Rgb, alpha: number): string {
    return `rgba(${Math.round(color.r)}, ${Math.round(color.g)}, ${Math.round(color.b)}, ${alpha})`;
  }

  function mixRgb(a: Rgb, b: Rgb, amount: number): Rgb {
    return {
      r: a.r + (b.r - a.r) * amount,
      g: a.g + (b.g - a.g) * amount,
      b: a.b + (b.b - a.b) * amount,
    };
  }

  function parseColor(value: string): Rgb | null {
    const raw = value.trim();
    if (raw.startsWith("#")) {
      const hex = raw.slice(1);
      if (hex.length === 3) {
        return {
          r: Number.parseInt(`${hex[0]}${hex[0]}`, 16),
          g: Number.parseInt(`${hex[1]}${hex[1]}`, 16),
          b: Number.parseInt(`${hex[2]}${hex[2]}`, 16),
        };
      }
      if (hex.length === 6) {
        return {
          r: Number.parseInt(hex.slice(0, 2), 16),
          g: Number.parseInt(hex.slice(2, 4), 16),
          b: Number.parseInt(hex.slice(4, 6), 16),
        };
      }
      return null;
    }

    const match = /^rgba?\(([^)]+)\)$/i.exec(raw);
    if (!match) return null;
    const parts = match[1].split(",").map((part) => Number.parseFloat(part));
    if (parts.length < 3 || parts.some(Number.isNaN)) return null;
    return { r: parts[0], g: parts[1], b: parts[2] };
  }

  function readToken(style: CSSStyleDeclaration, token: string, fallback: Rgb): Rgb {
    return parseColor(style.getPropertyValue(token)) ?? fallback;
  }

  function createColors(
    surface: Rgb,
    background: Rgb,
    text: Rgb,
    muted: Rgb,
    faint: Rgb,
    line: Rgb,
    accent: Rgb,
  ): MembraneColors {
    const edgeBase = mixRgb(line, muted, 0.55);
    const steel = mixRgb(accent, { r: 111, g: 139, b: 146 }, 0.35);
    const edgeBases = [0.2, 0.11, 0.09];
    const edge = edgeBases.map((base) =>
      [0, 1, 2, 3].map((depth) => rgba(edgeBase, base * (0.18 + (0.82 * depth) / 3))),
    );
    const point = [0.04, 0.08, 0.13, 0.19, 0.26, 0.34].map((alpha) => rgba(faint, alpha));
    const route: string[] = [];
    for (let i = 0; i <= ROUTE_STEPS; i++) {
      route.push(rgba(steel, (i / ROUTE_STEPS) * 0.92));
    }

    return {
      surface: rgba(surface, 1),
      edge,
      point,
      anchorRing: rgba(text, 0.86),
      anchorDot: rgba(background, 1),
      anchorActive: rgba(steel, 1),
      anchorSelected: rgba(steel, 0.82),
      hover: rgba(text, 0.55),
      route,
      signal: rgba(steel, 1),
      haloFill: rgba(steel, 0.1),
      haloRing: rgba(steel, 0.2),
      startRing: rgba(steel, 1),
    };
  }

  function buildColors(): MembraneColors {
    const style = getComputedStyle(document.documentElement);
    return createColors(
      readToken(style, "--surface", { r: 13, g: 15, b: 17 }),
      readToken(style, "--bg", { r: 8, g: 9, b: 10 }),
      readToken(style, "--text", { r: 238, g: 241, b: 242 }),
      readToken(style, "--muted", { r: 154, g: 162, b: 167 }),
      readToken(style, "--faint", { r: 133, g: 141, b: 146 }),
      readToken(style, "--line", { r: 30, g: 34, b: 37 }),
      readToken(style, "--accent", { r: 233, g: 237, b: 240 }),
    );
  }

  function fallbackColors(): MembraneColors {
    return createColors(
      { r: 13, g: 15, b: 17 },
      { r: 8, g: 9, b: 10 },
      { r: 238, g: 241, b: 242 },
      { r: 154, g: 162, b: 167 },
      { r: 133, g: 141, b: 146 },
      { r: 30, g: 34, b: 37 },
      { r: 233, g: 237, b: 240 },
    );
  }

  function delayText(delay: number | "error" | null): string {
    if (delay === "error") return "unavailable";
    if (typeof delay === "number") return formatDelay(delay);
    return "not measured";
  }

  function comparePointDepth(a: number, b: number): number {
    return pointDepth[a] - pointDepth[b];
  }

  function compareAnchorDepth(a: number, b: number): number {
    return anchorDepth[a] - anchorDepth[b];
  }

  function updateRouteLevels(dt: number, instant: boolean): void {
    for (const anchor of current.anchors) {
      const target = routeTarget(anchor.id, activeId, selectedId, status);
      const value = routeLevels.get(anchor.id) ?? 0;
      if (instant) {
        if (value !== target) routeLevels.set(anchor.id, target);
        continue;
      }
      if (dt <= 0) continue;
      const delta = target - value;
      if (Math.abs(delta) < 0.001) {
        if (value !== target) routeLevels.set(anchor.id, target);
        continue;
      }
      const step = Math.sign(delta) * Math.min(Math.abs(delta), dt / FADE_SECONDS);
      routeLevels.set(anchor.id, value + step);
    }
    for (const id of routeLevels.keys()) {
      let present = false;
      for (const anchor of current.anchors) {
        if (anchor.id === id) {
          present = true;
          break;
        }
      }
      if (!present) routeLevels.delete(id);
    }
  }

  function drawEdges(): void {
    for (const bin of edgeBins) bin.length = 0;

    for (const edge of current.edges) {
      const a = edge.a;
      const b = edge.b;
      const normalized = Math.min(0.999, Math.max(0, (pointDepth[a] + pointDepth[b] + 2) / 4));
      const band = Math.floor(normalized * 4);
      const bin = edgeBins[edge.kind * 4 + band];
      bin.push(pointX[a], pointY[a], pointX[b], pointY[b]);
    }

    const ctx = ctx2d;
    if (!ctx) return;
    ctx.lineCap = "round";
    for (let kind = 0; kind < 3; kind++) {
      ctx.lineWidth = kind === 1 ? 0.75 : 0.55;
      for (let band = 0; band < 4; band++) {
        const bin = edgeBins[kind * 4 + band];
        if (bin.length === 0) continue;
        ctx.strokeStyle = colors.edge[kind][band];
        ctx.beginPath();
        for (let i = 0; i < bin.length; i += 4) {
          ctx.moveTo(bin[i], bin[i + 1]);
          ctx.lineTo(bin[i + 2], bin[i + 3]);
        }
        ctx.stroke();
      }
    }
  }

  function drawAnchors(): void {
    const ctx = ctx2d;
    if (!ctx) return;
    for (let i = 0; i < current.anchors.length; i++) {
      const anchor = current.anchors[i];
      projectPoint(camera, anchor.x, anchor.y, anchor.z, scratch);
      anchorX[i] = scratch.x;
      anchorY[i] = scratch.y;
      anchorDepth[i] = scratch.depth;
    }

    anchorOrder.sort(compareAnchorDepth);
    for (const index of anchorOrder) {
      const anchor = current.anchors[index];
      const depth = clamp((anchorDepth[index] + 1) / 2, 0, 1);
      const active = activeId === anchor.id && status === "connected";
      const selected = selectedId === anchor.id;
      const hovered = hoverId === anchor.id;
      const x = anchorX[index];
      const y = anchorY[index];
      const routeLevel = routeLevels.get(anchor.id) ?? 0;
      const presence = 0.6 + depth * 0.4;

      if (status !== "starting" && routeLevel > 0.28) {
        const phase = ((stableHash(anchor.id) % 1000) / 1000) * TAU;
        const breath = reducedMotion ? 0.5 : 0.5 + 0.5 * Math.sin(animationTime * 0.85 + phase);

        ctx.globalAlpha = routeLevel * presence * (reducedMotion ? 0.7 : 0.52 + breath * 0.24);
        ctx.beginPath();
        ctx.arc(x, y, 9 + (reducedMotion ? 0 : 1.2 * breath), 0, TAU);
        ctx.fillStyle = colors.haloFill;
        ctx.fill();

        ctx.globalAlpha = routeLevel * presence * (reducedMotion ? 0.5 : 0.34 + breath * 0.3);
        ctx.beginPath();
        ctx.arc(x, y, 11.8 + (reducedMotion ? 0 : 2.2 * breath), 0, TAU);
        ctx.strokeStyle = colors.haloRing;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      if (status === "starting" && selected) {
        if (reducedMotion) {
          ctx.globalAlpha = 0.5 * presence;
          ctx.beginPath();
          ctx.arc(x, y, 9, 0, TAU);
          ctx.fillStyle = colors.haloFill;
          ctx.fill();

          ctx.beginPath();
          ctx.arc(x, y, 11.8, 0, TAU);
          ctx.strokeStyle = colors.haloRing;
          ctx.lineWidth = 1;
          ctx.stroke();
        } else {
          for (let ring = 0; ring < 2; ring++) {
            const progress = (animationTime / 2.4 + ring * 0.5) % 1;
            ctx.globalAlpha = (1 - progress) * 0.2 * presence;
            ctx.beginPath();
            ctx.arc(x, y, 6.5 + progress * 17, 0, TAU);
            ctx.strokeStyle = colors.startRing;
            ctx.lineWidth = 1;
            ctx.stroke();
          }
        }
      }

      ctx.globalAlpha = active ? 1 : 0.5 + depth * 0.5;

      if (hovered) {
        ctx.beginPath();
        ctx.arc(x, y, 10, 0, TAU);
        ctx.strokeStyle = colors.hover;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      if (selected) {
        ctx.beginPath();
        ctx.setLineDash([2, 2]);
        ctx.arc(x, y, 7.6, 0, TAU);
        ctx.strokeStyle = colors.anchorSelected;
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.setLineDash([]);
      }

      ctx.beginPath();
      ctx.arc(x, y, active ? 5 : 4.2, 0, TAU);
      ctx.strokeStyle = active ? colors.anchorActive : colors.anchorRing;
      ctx.lineWidth = active ? 1.6 : 1.2;
      ctx.stroke();

      ctx.beginPath();
      ctx.arc(x, y, 1.5, 0, TAU);
      ctx.fillStyle = active ? colors.anchorActive : colors.anchorDot;
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }

  function drawRoutes(): void {
    const ctx = ctx2d;
    if (!ctx) return;

    for (const anchor of current.anchors) {
      const level = routeLevels.get(anchor.id) ?? 0;
      if (level <= 0.004) continue;

      ctx.beginPath();
      projectPoint(camera, anchor.path[0].x, anchor.path[0].y, anchor.path[0].z, scratch);
      ctx.moveTo(scratch.x, scratch.y);
      for (let i = 1; i < anchor.path.length; i++) {
        const point = anchor.path[i];
        projectPoint(camera, point.x, point.y, point.z, scratch);
        ctx.lineTo(scratch.x, scratch.y);
      }
      ctx.strokeStyle = colors.route[Math.round(level * ROUTE_STEPS)];
      ctx.lineWidth = 1.1 + level;
      ctx.stroke();

      if (!reducedMotion && level > 0.16) {
        const phase = (stableHash(anchor.id) % 1000) / 1000;
        const progress = (animationTime * 0.2 + phase) % 1;
        const index = Math.min(
          anchor.path.length - 1,
          Math.floor(progress * (anchor.path.length - 1)),
        );
        const point = anchor.path[index];
        projectPoint(camera, point.x, point.y, point.z, scratch);
        ctx.beginPath();
        ctx.arc(scratch.x, scratch.y, 1.4 + level * 1.3, 0, TAU);
        ctx.fillStyle = colors.signal;
        ctx.fill();
      }
    }
  }

  function draw(now: number): void {
    const ctx = ctx2d;
    if (!ctx || hostWidth <= 0 || hostHeight <= 0) return;

    const dt = lastDraw === 0 ? 0.016 : clamp((now - lastDraw) / 1000, 0, 0.05);
    lastDraw = now;
    const animating = !reducedMotion && onScreen && !document.hidden;
    if (animating) {
      animationTime += dt;
      if (!dragging) yaw += dt * 0.045;
    }
    updateRouteLevels(animating ? dt : 0, reducedMotion);

    updateCamera(camera, hostWidth, hostHeight, zoom, yaw, pitch);
    ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    ctx.fillStyle = colors.surface;
    ctx.fillRect(0, 0, hostWidth, hostHeight);

    const points = current.points;
    for (let i = 0; i < points.length; i++) {
      const point = points[i];
      const drift = reducedMotion
        ? 0
        : point.driftAmount * Math.sin(animationTime * point.driftSpeed + point.driftPhase);
      const factor = 1 + drift;
      projectPoint(camera, point.x * factor, point.y * factor, point.z * factor, scratch);
      pointX[i] = scratch.x;
      pointY[i] = scratch.y;
      pointDepth[i] = scratch.depth;
    }

    ctx.globalAlpha = status === "connected" ? 0.78 : 1;
    drawEdges();

    pointOrder.sort(comparePointDepth);
    let bucket = -1;
    for (const index of pointOrder) {
      const depth = clamp((pointDepth[index] + 1) / 2, 0, 1);
      const nextBucket = Math.min(5, Math.floor(depth * 6));
      if (nextBucket !== bucket) {
        ctx.fillStyle = colors.point[nextBucket];
        bucket = nextBucket;
      }
      const radius = points[index].size * (0.42 + depth);
      ctx.fillRect(pointX[index] - radius, pointY[index] - radius, radius * 2, radius * 2);
    }

    ctx.globalAlpha = 1;
    drawRoutes();
    drawAnchors();
  }

  function startLoop(): void {
    if (running || !canvas) return;
    running = true;
    lastDraw = performance.now();
    frame = requestAnimationFrame(step);
  }

  function stopLoop(): void {
    running = false;
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
  }

  function step(now: number): void {
    if (!running) return;
    draw(now);
    if (running) frame = requestAnimationFrame(step);
  }

  function scheduleDraw(): void {
    if (running || singleFrame || !canvas) return;
    singleFrame = requestAnimationFrame((now) => {
      singleFrame = 0;
      if (!running) draw(now);
    });
  }

  function syncLoop(): void {
    if (!reducedMotion && onScreen && !document.hidden) {
      startLoop();
    } else {
      stopLoop();
      scheduleDraw();
    }
  }

  function hitAnchor(clientX: number, clientY: number): number {
    const surface = canvas;
    if (!surface || current.anchors.length !== anchorX.length) return -1;
    const rect = surface.getBoundingClientRect();
    const x = clientX - rect.left;
    const y = clientY - rect.top;
    let best = -1;
    let bestDistance = 121;
    for (let i = 0; i < current.anchors.length; i++) {
      const dx = anchorX[i] - x;
      const dy = anchorY[i] - y;
      const distance = dx * dx + dy * dy;
      if (distance <= bestDistance) {
        bestDistance = distance;
        best = i;
      }
    }
    return best;
  }

  function updateHover(event: PointerEvent): void {
    const index = hitAnchor(event.clientX, event.clientY);
    if (index < 0) {
      if (hoverId !== null) {
        hoverId = null;
        tip = null;
        scheduleDraw();
      }
      return;
    }

    const anchor = current.anchors[index];
    if (hoverId !== anchor.id) {
      hoverId = anchor.id;
      scheduleDraw();
    }
    tip = {
      x: clamp(anchorX[index] + 14, 8, Math.max(8, hostWidth - 194)),
      y: clamp(anchorY[index] + 12, 8, Math.max(8, hostHeight - 58)),
      label: anchor.label,
      meta: `${anchor.type} · ${delayText(anchor.delay)}`,
    };
  }

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    dragging = true;
    moved = false;
    lastPointerX = event.clientX;
    lastPointerY = event.clientY;
    hoverId = null;
    tip = null;
    const target = event.currentTarget as HTMLCanvasElement;
    target.setPointerCapture(event.pointerId);
    target.focus();
    scheduleDraw();
    event.preventDefault();
  }

  function onPointerMove(event: PointerEvent): void {
    if (dragging) {
      const dx = event.clientX - lastPointerX;
      const dy = event.clientY - lastPointerY;
      if (Math.abs(dx) + Math.abs(dy) > 1.5) moved = true;
      lastPointerX = event.clientX;
      lastPointerY = event.clientY;
      yaw += dx * 0.0055;
      pitch = clamp(pitch + dy * 0.0045, -1.05, 1.05);
      scheduleDraw();
      return;
    }
    updateHover(event);
  }

  function onPointerUp(event: PointerEvent): void {
    if (!dragging) return;
    dragging = false;
    const target = event.currentTarget as HTMLCanvasElement;
    if (target.hasPointerCapture(event.pointerId)) target.releasePointerCapture(event.pointerId);
    if (moved) return;
    const index = hitAnchor(event.clientX, event.clientY);
    onselect(index >= 0 ? current.anchors[index].id : null);
  }

  function onPointerLeave(): void {
    if (dragging) return;
    if (hoverId !== null || tip !== null) {
      hoverId = null;
      tip = null;
      scheduleDraw();
    }
  }

  function onKeyDown(event: KeyboardEvent): void {
    let handled = true;
    if (event.key === "Escape") {
      onselect(null);
    } else if (event.key === "ArrowLeft") {
      yaw -= 0.1;
    } else if (event.key === "ArrowRight") {
      yaw += 0.1;
    } else if (event.key === "ArrowUp") {
      pitch = clamp(pitch - 0.08, -1.05, 1.05);
    } else if (event.key === "ArrowDown") {
      pitch = clamp(pitch + 0.08, -1.05, 1.05);
    } else if (event.key === "+" || event.key === "=") {
      zoom = clamp(zoom + 0.04, ZOOM_MIN, ZOOM_MAX);
    } else if (event.key === "-" || event.key === "_") {
      zoom = clamp(zoom - 0.04, ZOOM_MIN, ZOOM_MAX);
    } else if (event.key === "Home") {
      yaw = 0.42;
      pitch = -0.12;
      zoom = 1;
    } else {
      handled = false;
    }
    if (!handled) return;
    event.preventDefault();
    scheduleDraw();
  }

  function resetView(): void {
    yaw = 0.42;
    pitch = -0.12;
    zoom = 1;
    scheduleDraw();
  }

  $effect(() => {
    const next = geometry;
    current = next;
    pointOrder = next.points.map((_, index) => index);
    anchorOrder = next.anchors.map((_, index) => index);
    pointX = new Float32Array(next.points.length);
    pointY = new Float32Array(next.points.length);
    pointDepth = new Float32Array(next.points.length);
    anchorX = new Float32Array(next.anchors.length);
    anchorY = new Float32Array(next.anchors.length);
    anchorDepth = new Float32Array(next.anchors.length);
    scheduleDraw();
  });

  $effect(() => {
    const anchors = geometry.anchors;
    if (hoverId !== null && !anchors.some((anchor) => anchor.id === hoverId)) {
      hoverId = null;
      tip = null;
    }
  });

  $effect(() => {
    void status;
    void activeId;
    void selectedId;
    scheduleDraw();
  });

  $effect(() => {
    const update = () => {
      colors = buildColors();
      scheduleDraw();
    };
    update();
    const observer = new MutationObserver(update);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "class"],
    });
    return () => observer.disconnect();
  });

  $effect(() => {
    const element = host;
    const surface = canvas;
    if (!element || !surface) return;

    ctx2d = surface.getContext("2d");

    const measure = () => {
      const rect = element.getBoundingClientRect();
      hostWidth = rect.width;
      hostHeight = rect.height;
      pixelRatio = Math.min(2, window.devicePixelRatio || 1);
      const width = Math.max(1, Math.round(hostWidth * pixelRatio));
      const height = Math.max(1, Math.round(hostHeight * pixelRatio));
      if (surface.width !== width) surface.width = width;
      if (surface.height !== height) surface.height = height;
      scheduleDraw();
    };

    const resizeObserver = new ResizeObserver(measure);
    resizeObserver.observe(element);

    const intersectionObserver = new IntersectionObserver(
      ([entry]) => {
        onScreen = entry.isIntersecting;
        syncLoop();
      },
      { threshold: 0 },
    );
    intersectionObserver.observe(element);

    const visibility = () => syncLoop();
    document.addEventListener("visibilitychange", visibility);

    const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    reducedMotion = motionQuery.matches;
    const onMotionChange = () => {
      reducedMotion = motionQuery.matches;
      syncLoop();
      scheduleDraw();
    };
    motionQuery.addEventListener("change", onMotionChange);

    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      zoom = clamp(zoom * Math.exp(-event.deltaY * 0.0012), ZOOM_MIN, ZOOM_MAX);
      scheduleDraw();
    };
    surface.addEventListener("wheel", onWheel, { passive: false });

    measure();
    syncLoop();

    return () => {
      resizeObserver.disconnect();
      intersectionObserver.disconnect();
      document.removeEventListener("visibilitychange", visibility);
      motionQuery.removeEventListener("change", onMotionChange);
      surface.removeEventListener("wheel", onWheel);
      stopLoop();
      if (singleFrame) cancelAnimationFrame(singleFrame);
      singleFrame = 0;
      ctx2d = null;
    };
  });
</script>

<div class="membrane-canvas" bind:this={host}>
  <canvas
    bind:this={canvas}
    tabindex="0"
    aria-label={canvasLabel}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointerleave={onPointerLeave}
    onkeydown={onKeyDown}
  ></canvas>

  {#if tip}
    <div class="membrane-tip" style={`left:${tip.x}px; top:${tip.y}px`} aria-hidden="true">
      <span class="membrane-tip__label">{tip.label}</span>
      <span class="membrane-tip__meta">{tip.meta}</span>
    </div>
  {/if}

  <span class="membrane-hint" aria-hidden="true">drag to rotate · scroll to zoom</span>
  <button type="button" class="btn btn--sm btn--quiet membrane-reset" onclick={resetView}>
    Reset view
  </button>
</div>

<style>
  .membrane-canvas {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    border-radius: inherit;
    background: var(--surface);
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    touch-action: none;
    outline: none;
    cursor: grab;
  }

  canvas:active {
    cursor: grabbing;
  }

  canvas:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -3px;
  }

  .membrane-hint {
    position: absolute;
    left: var(--space-3);
    bottom: var(--space-3);
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--faint);
    pointer-events: none;
    user-select: none;
  }

  .membrane-reset {
    position: absolute;
    right: var(--space-3);
    bottom: var(--space-3);
    background: var(--surface);
  }

  .membrane-tip {
    position: absolute;
    z-index: 2;
    display: grid;
    gap: 1px;
    max-width: 186px;
    padding: 5px 7px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--raised);
    box-shadow: var(--glow);
    pointer-events: none;
  }

  .membrane-tip__label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--text);
  }

  .membrane-tip__meta {
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
</style>
