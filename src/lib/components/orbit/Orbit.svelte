<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import type { EngineStatus } from "../../api";
  import { clamp, formatDelay } from "../../format";
  import { createFramePacer } from "../../performance.svelte";
  import { stableHash } from "../../membrane";
  import type {
    EndpointMember,
    MembraneEndpoint,
    MembraneSelection,
    MembraneUnresolvedEndpoint,
  } from "../../membrane-ip";
  import {
    buildOrbitArc,
    clipLandRing,
    createOrbitCamera,
    createRingScratch,
    isValidOrbitLocation,
    LAND_MAX_POINTS,
    LAND_RINGS,
    locationToOrbitVector,
    orbitalPositionForIp,
    orbitalPositionForUnresolved,
    projectOrbitPoint,
    pulseProgress,
    smoothstep,
    updateOrbitCamera,
    YOU_STATION,
    type OrbitCamera,
    type OrbitLocation,
    type OrbitRingScratch,
    type OrbitScreenPoint,
    type Vec3,
  } from "../../orbit";

  let {
    endpoints,
    unresolved,
    activeEndpointKey,
    selected,
    status,
    connectionCounts,
    locations,
    onselect,
  }: {
    endpoints: MembraneEndpoint[];
    unresolved: MembraneUnresolvedEndpoint[];
    activeEndpointKey: string | null;
    selected: MembraneSelection | null;
    status: EngineStatus;
    connectionCounts: ReadonlyMap<string, number>;
    locations: Readonly<Record<string, OrbitLocation>>;
    onselect: (selection: MembraneSelection | null) => void;
  } = $props();

  const ZOOM_MIN = 0.85;
  const ZOOM_MAX = 1.15;
  const ARC_FADE_SECONDS = 1;

  let host = $state<HTMLDivElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let hovered = $state<MembraneSelection | null>(null);
  let tip = $state<{ x: number; y: number; label: string; meta: string } | null>(null);

  const canvasLabel = $derived(
    `Orbital globe with ${endpoints.length} endpoint markers and ${unresolved.length} unresolved markers. Drag to rotate, scroll to zoom. Marker locations are deterministic or user-supplied approximations.`,
  );

  interface OrbitMarker {
    key: string;
    kind: "endpoint" | "unresolved";
    ip: string | null;
    tag: string | null;
    members: EndpointMember[];
    unavailable: boolean;
    manual: boolean;
    lat: number | null;
    lon: number | null;
    base: Vec3;
    arc: Vec3[] | null;
    sx: number;
    sy: number;
    depth: number;
    visible: boolean;
  }

  interface Rgb {
    r: number;
    g: number;
    b: number;
  }

  interface OrbitColors {
    surface: string;
    ocean: string;
    oceanLight: string;
    land: string;
    coast: string;
    atmosphere: string;
    atmosphereSoft: string;
    marker: string;
    markerDim: string;
    active: string;
    manual: string;
    haloFill: string;
    haloRing: string;
    arc: string;
    arcHot: string;
    night: string;
    warmth: string;
    labelFont: string;
  }

  const camera: OrbitCamera = createOrbitCamera();
  const ringScratch: OrbitRingScratch = createRingScratch(LAND_MAX_POINTS);
  const markerNodes = new SvelteMap<string, OrbitMarker>();
  const arcLevels = new SvelteMap<string, number>();
  let markerList: OrbitMarker[] = [];
  const scratchScreen: OrbitScreenPoint = { x: 0, y: 0, depth: 0, visible: false };
  const rotatedScratch: Vec3 = { x: 0, y: 0, z: 0 };

  let colors: OrbitColors = fallbackColors();
  let hostWidth = 0;
  let hostHeight = 0;
  let pixelRatio = 1;
  let ctx2d: CanvasRenderingContext2D | null = null;

  let yaw = 0.72;
  let pitch = -0.12;
  let zoom = 1;
  let animationTime = 0;
  let lastDraw = 0;

  let onScreen = true;
  let reducedMotion = false;

  let dragging = false;
  let moved = false;
  let lastPointerX = 0;
  let lastPointerY = 0;

  const pacer = createFramePacer({
    onFrame: (now) => draw(now),
    shouldRun: () => !reducedMotion && onScreen && !document.hidden,
  });

  onDestroy(() => pacer.dispose());

  function rgba(color: Rgb, alpha: number): string {
    return `rgba(${Math.round(color.r)}, ${Math.round(color.g)}, ${Math.round(color.b)}, ${alpha})`;
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

  function createColors(surface: Rgb, fontFamily: string): OrbitColors {
    const line = { r: 174, g: 198, b: 202 };
    const muted = { r: 128, g: 150, b: 154 };
    const manual = { r: 198, g: 164, b: 106 };
    return {
      surface: rgba(surface, 1),
      ocean: "#07141c",
      oceanLight: "#0c222b",
      land: "#13262b",
      coast: rgba(line, 0.34),
      atmosphere: rgba(line, 0.18),
      atmosphereSoft: rgba(line, 0.06),
      marker: rgba(line, 0.9),
      markerDim: rgba(muted, 0.62),
      active: rgba(line, 1),
      manual: rgba(manual, 0.9),
      haloFill: rgba(line, 0.1),
      haloRing: rgba(line, 0.22),
      arc: rgba(line, 0.85),
      arcHot: rgba(line, 1),
      night: "rgba(0, 5, 8, 0.52)",
      warmth: "rgba(178, 130, 88, 0.1)",
      labelFont: `10px ${fontFamily || "ui-monospace, monospace"}`,
    };
  }

  function buildColors(): OrbitColors {
    const style = getComputedStyle(document.documentElement);
    return createColors(
      readToken(style, "--surface", { r: 13, g: 15, b: 17 }),
      style.getPropertyValue("--font-mono").trim(),
    );
  }

  function fallbackColors(): OrbitColors {
    return createColors({ r: 13, g: 15, b: 17 }, "ui-monospace, monospace");
  }

  function delayText(delay: number | "error" | null): string {
    if (delay === "error") return "unavailable";
    if (typeof delay === "number") return formatDelay(delay);
    return "not measured";
  }

  function markerMeta(marker: OrbitMarker): string {
    if (marker.kind === "unresolved") {
      const member = marker.members[0];
      return `unresolved endpoint · ${member?.type ?? "type unknown"} · ${delayText(
        member?.delay ?? null,
      )}`;
    }

    const types: string[] = [];
    const tags: string[] = [];
    let unavailableCount = 0;
    let measuredDelay: number | null = null;
    for (const member of marker.members) {
      if (member.type && !types.includes(member.type)) types.push(member.type);
      if (member.tag && !tags.includes(member.tag)) tags.push(member.tag);
      if (member.delay === "error" || (typeof member.delay === "number" && member.delay < 0)) {
        unavailableCount++;
      } else if (typeof member.delay === "number" && measuredDelay === null) {
        measuredDelay = member.delay;
      }
    }
    const count = marker.members.length;
    const tagText = tags.slice(0, 3).join(", ") + (tags.length > 3 ? "…" : "");
    const delay =
      measuredDelay !== null
        ? formatDelay(measuredDelay)
        : unavailableCount > 0
          ? "unavailable"
          : "not measured";
    const partial =
      unavailableCount > 0 && unavailableCount < count
        ? ` · ${unavailableCount}/${count} unavailable`
        : "";
    const manual =
      marker.manual && marker.lat !== null && marker.lon !== null
        ? ` · manual ≈ ${marker.lat.toFixed(2)}, ${marker.lon.toFixed(2)}`
        : "";
    return `${count} tag${count === 1 ? "" : "s"}: ${tagText || "not reported"} · ${
      types.slice(0, 3).join(", ") || "type unknown"
    } · ${delay}${partial}${manual}`;
  }

  function updateArcLevels(dt: number, instant: boolean): void {
    if (activeEndpointKey && !arcLevels.has(activeEndpointKey)) arcLevels.set(activeEndpointKey, 0);

    for (const [key, value] of arcLevels) {
      const target =
        (status === "starting" || status === "connected") && key === activeEndpointKey ? 1 : 0;
      if (instant) {
        if (target === 0) arcLevels.delete(key);
        else if (value !== 1) arcLevels.set(key, 1);
        continue;
      }
      if (dt <= 0) continue;
      const delta = target - value;
      if (Math.abs(delta) < 0.001) {
        if (target === 0) arcLevels.delete(key);
        else if (value !== target) arcLevels.set(key, target);
        continue;
      }
      const next = value + Math.sign(delta) * Math.min(Math.abs(delta), dt / ARC_FADE_SECONDS);
      if (target === 0 && next <= 0.002) arcLevels.delete(key);
      else if (next !== value) arcLevels.set(key, next);
    }
  }

  function drawGlobe(): void {
    const ctx = ctx2d;
    if (!ctx) return;
    const cx = camera.cx;
    const cy = camera.cy;
    const radius = camera.radius;
    if (radius <= 0) return;

    const ocean = ctx.createRadialGradient(
      cx - radius * 0.36,
      cy - radius * 0.42,
      radius * 0.08,
      cx,
      cy,
      radius * 1.05,
    );
    ocean.addColorStop(0, colors.oceanLight);
    ocean.addColorStop(0.62, colors.ocean);
    ocean.addColorStop(1, "#040c11");
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, Math.PI * 2);
    ctx.fillStyle = ocean;
    ctx.fill();

    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, Math.PI * 2);
    ctx.clip();

    ctx.beginPath();
    let anyLand = false;
    for (const ring of LAND_RINGS) {
      if (!clipLandRing(ring, camera, ringScratch)) continue;
      const out = ringScratch.out;
      ctx.moveTo(out[0], out[1]);
      for (let i = 2; i < out.length; i += 2) ctx.lineTo(out[i], out[i + 1]);
      ctx.closePath();
      anyLand = true;
    }

    if (anyLand) {
      ctx.fillStyle = colors.land;
      ctx.fill("evenodd");
      ctx.lineJoin = "round";
      ctx.strokeStyle = colors.coast;
      ctx.lineWidth = 0.55;
      ctx.stroke();
    }

    const shade = ctx.createLinearGradient(
      cx - radius * 0.9,
      cy - radius * 0.9,
      cx + radius * 0.95,
      cy + radius * 0.95,
    );
    shade.addColorStop(0, "rgba(220, 238, 242, 0.045)");
    shade.addColorStop(0.48, "rgba(0, 0, 0, 0)");
    shade.addColorStop(0.76, "rgba(0, 0, 0, 0.26)");
    shade.addColorStop(1, colors.night);
    ctx.fillStyle = shade;
    ctx.fillRect(cx - radius, cy - radius, radius * 2, radius * 2);

    const warmth = ctx.createRadialGradient(
      cx + radius * 0.56,
      cy + radius * 0.38,
      radius * 0.04,
      cx + radius * 0.52,
      cy + radius * 0.34,
      radius * 1.05,
    );
    warmth.addColorStop(0, colors.warmth);
    warmth.addColorStop(0.55, "rgba(178, 130, 88, 0.025)");
    warmth.addColorStop(1, "rgba(178, 130, 88, 0)");
    ctx.fillStyle = warmth;
    ctx.fillRect(cx - radius, cy - radius, radius * 2, radius * 2);

    const limb = ctx.createRadialGradient(cx, cy, radius * 0.58, cx, cy, radius);
    limb.addColorStop(0, "rgba(0, 0, 0, 0)");
    limb.addColorStop(1, "rgba(0, 0, 0, 0.28)");
    ctx.fillStyle = limb;
    ctx.fillRect(cx - radius, cy - radius, radius * 2, radius * 2);

    ctx.restore();

    ctx.beginPath();
    ctx.arc(cx, cy, radius + 0.8, 0, Math.PI * 2);
    ctx.strokeStyle = colors.atmosphere;
    ctx.lineWidth = 1;
    ctx.stroke();

    ctx.beginPath();
    ctx.arc(cx, cy, radius + 2.6, 0, Math.PI * 2);
    ctx.strokeStyle = colors.atmosphereSoft;
    ctx.lineWidth = 2;
    ctx.stroke();
  }

  function drawArcs(): void {
    const ctx = ctx2d;
    if (!ctx) return;
    ctx.lineCap = "round";

    for (const [key, level] of arcLevels) {
      if (level <= 0.004) continue;
      const marker = markerNodes.get(key);
      if (!marker) continue;
      if (!marker.arc) marker.arc = buildOrbitArc(YOU_STATION, marker.base);

      ctx.globalAlpha = level * 0.88;
      ctx.strokeStyle = level > 0.72 ? colors.arcHot : colors.arc;
      ctx.lineWidth = 0.65 + level * 0.6;
      ctx.beginPath();
      let drawing = false;
      for (const point of marker.arc) {
        projectOrbitPoint(point, camera, scratchScreen, rotatedScratch);
        if (!scratchScreen.visible) {
          drawing = false;
          continue;
        }
        if (!drawing) {
          ctx.moveTo(scratchScreen.x, scratchScreen.y);
          drawing = true;
        } else {
          ctx.lineTo(scratchScreen.x, scratchScreen.y);
        }
      }
      ctx.stroke();

      const count = connectionCounts.get(key) ?? 0;
      if (!reducedMotion && level > 0.2 && count > 0) {
        const dots = Math.min(3, count);
        const phase = (stableHash(key) % 1000) / 1000;
        for (let i = 0; i < dots; i++) {
          const progress = pulseProgress(animationTime, 0.14, i / dots + phase);
          const index = Math.min(
            marker.arc.length - 1,
            Math.floor(progress * (marker.arc.length - 1)),
          );
          const point = marker.arc[index];
          projectOrbitPoint(point, camera, scratchScreen, rotatedScratch);
          if (!scratchScreen.visible) continue;
          ctx.globalAlpha = level * 0.9;
          ctx.beginPath();
          ctx.arc(scratchScreen.x, scratchScreen.y, 1.45 + level * 0.7, 0, Math.PI * 2);
          ctx.fillStyle = colors.arcHot;
          ctx.fill();
        }
      }
    }

    ctx.globalAlpha = 1;
  }

  function drawMarkers(): void {
    const ctx = ctx2d;
    if (!ctx) return;

    for (const marker of markerList) {
      if (!marker.visible) continue;
      const node = marker;
      const level = arcLevels.get(node.key) ?? 0;
      const active = activeEndpointKey === node.key;
      const selectedHere = selected?.kind === "endpoint" && selected.key === node.key;
      const hoveredHere = hovered?.kind === "endpoint" && hovered.key === node.key;
      const presence = 0.42 + smoothstep(-0.65, 0.5, node.depth) * 0.58;
      const baseAlpha = (node.unavailable ? 0.58 : 1) * presence;

      if (level > 0.02) {
        ctx.globalAlpha = level * presence * 0.8;
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, 9.5, 0, Math.PI * 2);
        ctx.fillStyle = colors.haloFill;
        ctx.fill();

        ctx.globalAlpha = level * presence * 0.65;
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, 12.2, 0, Math.PI * 2);
        ctx.strokeStyle = colors.haloRing;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      ctx.globalAlpha = active ? Math.max(baseAlpha, 0.9) : baseAlpha;

      if (hoveredHere) {
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, 10.5, 0, Math.PI * 2);
        ctx.strokeStyle = colors.markerDim;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      if (selectedHere) {
        ctx.beginPath();
        ctx.setLineDash([2, 2]);
        ctx.arc(node.sx, node.sy, 8, 0, Math.PI * 2);
        ctx.strokeStyle = colors.active;
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.setLineDash([]);
      }

      if (node.manual) {
        ctx.beginPath();
        ctx.moveTo(node.sx - 5.5, node.sy);
        ctx.lineTo(node.sx - 2.5, node.sy);
        ctx.moveTo(node.sx + 2.5, node.sy);
        ctx.lineTo(node.sx + 5.5, node.sy);
        ctx.moveTo(node.sx, node.sy - 5.5);
        ctx.lineTo(node.sx, node.sy - 2.5);
        ctx.moveTo(node.sx, node.sy + 2.5);
        ctx.lineTo(node.sx, node.sy + 5.5);
        ctx.strokeStyle = colors.manual;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      if (node.unavailable) ctx.setLineDash([2, 2]);
      const radius = active ? 4.2 : 3.6;
      if (node.kind === "unresolved") {
        ctx.beginPath();
        ctx.moveTo(node.sx, node.sy - radius);
        ctx.lineTo(node.sx + radius, node.sy);
        ctx.lineTo(node.sx, node.sy + radius);
        ctx.lineTo(node.sx - radius, node.sy);
        ctx.closePath();
      } else {
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, radius, 0, Math.PI * 2);
      }
      ctx.strokeStyle = active ? colors.active : node.manual ? colors.manual : colors.marker;
      ctx.lineWidth = active ? 1.5 : 1.1;
      ctx.stroke();
      if (node.unavailable) ctx.setLineDash([]);

      ctx.beginPath();
      ctx.arc(node.sx, node.sy, 1.3, 0, Math.PI * 2);
      ctx.fillStyle = active ? colors.active : colors.marker;
      ctx.fill();
    }

    ctx.globalAlpha = 1;
  }

  function drawStation(): void {
    const ctx = ctx2d;
    if (!ctx) return;
    projectOrbitPoint(YOU_STATION, camera, scratchScreen, rotatedScratch);
    if (!scratchScreen.visible) return;
    const alpha = scratchScreen.depth >= 0 ? 0.9 : 0.5;
    ctx.globalAlpha = alpha;
    ctx.beginPath();
    ctx.arc(scratchScreen.x, scratchScreen.y, 3.4, 0, Math.PI * 2);
    ctx.strokeStyle = colors.marker;
    ctx.lineWidth = 1.2;
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(scratchScreen.x, scratchScreen.y, 1.2, 0, Math.PI * 2);
    ctx.fillStyle = colors.marker;
    ctx.fill();

    const left = scratchScreen.x > camera.cx;
    ctx.font = colors.labelFont;
    ctx.textAlign = left ? "right" : "left";
    ctx.textBaseline = "middle";
    ctx.fillStyle = colors.markerDim;
    ctx.fillText("YOU · LOCAL", scratchScreen.x + (left ? -8 : 8), scratchScreen.y - 8);
    ctx.globalAlpha = 1;
  }

  function draw(now: number): void {
    const ctx = ctx2d;
    if (!ctx || hostWidth <= 0 || hostHeight <= 0) return;

    const dt = lastDraw === 0 ? 0.016 : clamp((now - lastDraw) / 1000, 0, 0.05);
    lastDraw = now;
    const animating = !reducedMotion && onScreen && !document.hidden;
    if (animating) {
      animationTime += dt;
      if (!dragging) yaw += dt * 0.035;
    }
    updateArcLevels(animating ? dt : 0, reducedMotion);
    updateOrbitCamera(camera, hostWidth, hostHeight, zoom, yaw, pitch);

    ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    ctx.fillStyle = colors.surface;
    ctx.fillRect(0, 0, hostWidth, hostHeight);

    drawGlobe();

    for (const marker of markerList) {
      projectOrbitPoint(marker.base, camera, scratchScreen, rotatedScratch);
      marker.sx = scratchScreen.x;
      marker.sy = scratchScreen.y;
      marker.depth = scratchScreen.depth;
      marker.visible = scratchScreen.visible;
    }

    drawArcs();
    drawMarkers();
    drawStation();
  }

  function startLoop(): void {
    if (!canvas) return;
    if (!pacer.running) lastDraw = 0;
    pacer.start();
  }

  function stopLoop(): void {
    pacer.stop();
    lastDraw = 0;
  }

  function scheduleDraw(): void {
    if (!canvas || document.hidden || !onScreen) return;
    if (!pacer.running) pacer.requestDraw();
  }

  function syncLoop(): void {
    if (!reducedMotion && onScreen && !document.hidden) {
      startLoop();
    } else {
      stopLoop();
      if (onScreen && !document.hidden) pacer.requestDraw();
    }
  }

  function hitMarker(clientX: number, clientY: number): MembraneSelection | null {
    const surface = canvas;
    if (!surface) return null;
    const rect = surface.getBoundingClientRect();
    const x = clientX - rect.left;
    const y = clientY - rect.top;
    let best: MembraneSelection | null = null;
    let bestDistance = Infinity;

    for (const marker of markerList) {
      if (!marker.visible) continue;
      const dx = marker.sx - x;
      const dy = marker.sy - y;
      const distance = dx * dx + dy * dy;
      if (distance <= 144 && distance < bestDistance) {
        bestDistance = distance;
        best = { kind: "endpoint", key: marker.key };
      }
    }
    return best;
  }

  function updateHover(event: PointerEvent): void {
    const next = hitMarker(event.clientX, event.clientY);
    if (!next) {
      if (hovered !== null) {
        hovered = null;
        tip = null;
        scheduleDraw();
      }
      return;
    }

    const marker = markerNodes.get(next.key);
    if (!marker) return;
    const label =
      marker.kind === "unresolved" ? (marker.tag ?? marker.key) : (marker.ip ?? marker.key);
    tip = {
      x: clamp(marker.sx + 14, 8, Math.max(8, hostWidth - 224)),
      y: clamp(marker.sy + 12, 8, Math.max(8, hostHeight - 58)),
      label,
      meta: markerMeta(marker),
    };
    if (hovered?.kind !== next.kind || hovered.key !== next.key) {
      hovered = next;
      scheduleDraw();
    }
  }

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    dragging = true;
    moved = false;
    lastPointerX = event.clientX;
    lastPointerY = event.clientY;
    hovered = null;
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
      pitch = clamp(pitch + dy * 0.0045, -1.2, 1.2);
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
    onselect(hitMarker(event.clientX, event.clientY));
  }

  function onPointerLeave(): void {
    if (dragging) return;
    if (hovered !== null || tip !== null) {
      hovered = null;
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
      pitch = clamp(pitch - 0.08, -1.2, 1.2);
    } else if (event.key === "ArrowDown") {
      pitch = clamp(pitch + 0.08, -1.2, 1.2);
    } else if (event.key === "+" || event.key === "=") {
      zoom = clamp(zoom + 0.03, ZOOM_MIN, ZOOM_MAX);
    } else if (event.key === "-" || event.key === "_") {
      zoom = clamp(zoom - 0.03, ZOOM_MIN, ZOOM_MAX);
    } else if (event.key === "Home") {
      yaw = 0.72;
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
    yaw = 0.72;
    pitch = -0.12;
    zoom = 1;
    scheduleDraw();
  }

  $effect(() => {
    const literal = endpoints;
    const extra = unresolved;
    const locationMap = locations;

    untrack(() => {
      const seen = new SvelteSet<string>();

      for (const endpoint of literal) {
        seen.add(endpoint.key);
        const override = locationMap[endpoint.ip];
        const manual = isValidOrbitLocation(override);
        const lat = manual ? override.lat : null;
        const lon = manual ? override.lon : null;
        const base = manual ? locationToOrbitVector(override) : orbitalPositionForIp(endpoint.ip);
        let marker = markerNodes.get(endpoint.key);
        if (!marker) {
          marker = {
            key: endpoint.key,
            kind: "endpoint",
            ip: endpoint.ip,
            tag: null,
            members: endpoint.members,
            unavailable: endpoint.unavailable,
            manual,
            lat,
            lon,
            base,
            arc: null,
            sx: 0,
            sy: 0,
            depth: 0,
            visible: false,
          };
          markerNodes.set(endpoint.key, marker);
        } else {
          marker.members = endpoint.members;
          marker.unavailable = endpoint.unavailable;
          if (marker.manual !== manual || marker.lat !== lat || marker.lon !== lon) {
            marker.manual = manual;
            marker.lat = lat;
            marker.lon = lon;
            marker.base = base;
            marker.arc = null;
          }
        }
      }

      for (const endpoint of extra) {
        seen.add(endpoint.key);
        const base = orbitalPositionForUnresolved(endpoint.key);
        let marker = markerNodes.get(endpoint.key);
        if (!marker) {
          marker = {
            key: endpoint.key,
            kind: "unresolved",
            ip: null,
            tag: endpoint.tag,
            members: [{ tag: endpoint.tag, type: endpoint.type, delay: endpoint.delay }],
            unavailable: endpoint.unavailable,
            manual: false,
            lat: null,
            lon: null,
            base,
            arc: null,
            sx: 0,
            sy: 0,
            depth: 0,
            visible: false,
          };
          markerNodes.set(endpoint.key, marker);
        } else {
          marker.tag = endpoint.tag;
          marker.members = [{ tag: endpoint.tag, type: endpoint.type, delay: endpoint.delay }];
          marker.unavailable = endpoint.unavailable;
        }
      }

      for (const key of markerNodes.keys()) {
        if (!seen.has(key)) markerNodes.delete(key);
      }
      if (hovered !== null && !markerNodes.has(hovered.key)) {
        hovered = null;
        tip = null;
      }
      markerList = Array.from(markerNodes.values()).sort((a, b) => a.key.localeCompare(b.key));
      scheduleDraw();
    });
  });

  $effect(() => {
    void status;
    void activeEndpointKey;
    void selected;
    void connectionCounts;
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
      zoom = clamp(zoom * Math.exp(-event.deltaY * 0.0011), ZOOM_MIN, ZOOM_MAX);
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
      ctx2d = null;
    };
  });
</script>

<div class="orbit-canvas" bind:this={host}>
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
    <div class="orbit-tip" style={`left:${tip.x}px; top:${tip.y}px`} aria-hidden="true">
      <span class="orbit-tip__label">{tip.label}</span>
      <span class="orbit-tip__meta">{tip.meta}</span>
    </div>
  {/if}

  <span class="orbit-hint" aria-hidden="true">drag to rotate · scroll to zoom</span>
  <button type="button" class="btn btn--sm btn--quiet orbit-reset" onclick={resetView}>
    Reset view
  </button>
</div>

<style>
  .orbit-canvas {
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

  .orbit-hint {
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

  .orbit-reset {
    position: absolute;
    right: var(--space-3);
    bottom: var(--space-3);
    background: var(--surface);
  }

  .orbit-tip {
    position: absolute;
    z-index: 2;
    display: grid;
    gap: 1px;
    max-width: 218px;
    padding: 5px 7px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--raised);
    box-shadow: var(--glow);
    pointer-events: none;
  }

  .orbit-tip__label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--text);
  }

  .orbit-tip__meta {
    white-space: normal;
    overflow-wrap: anywhere;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
</style>
