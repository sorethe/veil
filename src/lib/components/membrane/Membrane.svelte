<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import type { EngineStatus } from "../../api";
  import { clamp, formatDelay } from "../../format";
  import {
    buildMembraneGeometry,
    createCamera,
    projectPoint,
    routeTarget,
    stableHash,
    updateCamera,
    type MembraneCamera,
    type ScreenPoint,
  } from "../../membrane";
  import {
    IpLattice,
    type EndpointMember,
    type IpPoint,
    type MembraneDestination,
    type MembraneEndpoint,
    type MembraneSelection,
    type MembraneUnresolvedEndpoint,
  } from "../../membrane-ip";

  let {
    endpoints,
    unresolved,
    destinations,
    activeEndpointKey,
    selected,
    status,
    onselect,
  }: {
    endpoints: MembraneEndpoint[];
    unresolved: MembraneUnresolvedEndpoint[];
    destinations: MembraneDestination[];
    activeEndpointKey: string | null;
    selected: MembraneSelection | null;
    status: EngineStatus;
    onselect: (selection: MembraneSelection | null) => void;
  } = $props();

  const TAU = Math.PI * 2;
  const FADE_IN_SECONDS = 0.8;
  const FADE_OUT_SECONDS = 1;
  const ZOOM_MIN = 0.78;
  const ZOOM_MAX = 1.25;

  let host = $state<HTMLDivElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let hoverTarget = $state<MembraneSelection | null>(null);
  let tip = $state<{ x: number; y: number; label: string; meta: string } | null>(null);

  const canvasLabel = $derived(
    `Network membrane with ${endpoints.length} real IP endpoints and ${destinations.length} active destinations. Drag to rotate, scroll to zoom. Use the endpoint and connection lists beside the canvas.`,
  );

  const background = buildMembraneGeometry();
  const pointOrder = background.points.map((_, index) => index);
  const pointX = new Float32Array(background.points.length);
  const pointY = new Float32Array(background.points.length);
  const pointDepth = new Float32Array(background.points.length);
  const scratch: ScreenPoint = { x: 0, y: 0, depth: 0 };
  const camera: MembraneCamera = createCamera();

  interface RenderEndpoint {
    key: string;
    kind: "endpoint" | "unresolved";
    ip: string | null;
    tag: string | null;
    members: EndpointMember[];
    unavailable: boolean;
    position: IpPoint;
    sx: number;
    sy: number;
    depth: number;
    linkCount: number;
  }

  interface RenderDestination {
    key: string;
    data: MembraneDestination;
    position: IpPoint;
    sx: number;
    sy: number;
    depth: number;
    fade: number;
    retiring: boolean;
  }

  interface RenderLink {
    endpointKey: string;
    destinationKey: string;
    connectionCount: number;
    fade: number;
    retiring: boolean;
  }

  const endpointLattice = new IpLattice("veil.membrane.endpoint.v1", 1.055, 0.008);
  const unresolvedLattice = new IpLattice("veil.membrane.unresolved.v1", 1.025, 0.008);
  const destinationLattice = new IpLattice("veil.membrane.destination.v1", 0.58, 0.055);
  const endpointNodes = new SvelteMap<string, RenderEndpoint>();
  const destinationNodes = new SvelteMap<string, RenderDestination>();
  const linkStates = new SvelteMap<string, RenderLink>();
  const routeLevels = new SvelteMap<string, number>();
  let endpointList: RenderEndpoint[] = [];
  let destinationList: RenderDestination[] = [];
  let linkList: RenderLink[] = [];

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
    endpointRing: string;
    endpointDot: string;
    endpointActive: string;
    endpointSelected: string;
    destination: string;
    destinationSelected: string;
    hover: string;
    connection: string;
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

    return {
      surface: rgba(surface, 1),
      edge,
      point,
      endpointRing: rgba(text, 0.88),
      endpointDot: rgba(text, 0.92),
      endpointActive: rgba(steel, 1),
      endpointSelected: rgba(steel, 0.85),
      destination: rgba(muted, 1),
      destinationSelected: rgba(steel, 1),
      hover: rgba(text, 0.55),
      connection: rgba(steel, 1),
      haloFill: rgba(steel, 0.1),
      haloRing: rgba(steel, 0.2),
      startRing: rgba(steel, 1),
    };
  }

  function buildColors(): MembraneColors {
    const style = getComputedStyle(document.documentElement);
    return createColors(
      readToken(style, "--surface", { r: 13, g: 15, b: 17 }),
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

  function endpointMeta(node: RenderEndpoint): string {
    if (node.kind === "unresolved") {
      const member = node.members[0];
      return `unresolved endpoint · ${member?.type ?? "type unknown"} · ${delayText(
        member?.delay ?? null,
      )}`;
    }

    const types: string[] = [];
    for (const member of node.members) {
      if (member.type && !types.includes(member.type)) types.push(member.type);
    }
    const measured = node.members.find(
      (member) => member.delay === "error" || typeof member.delay === "number",
    );
    const count = node.members.length;
    return `${count} outbound${count === 1 ? "" : "s"} · ${
      types.slice(0, 3).join(", ") || "type unknown"
    } · ${delayText(measured?.delay ?? null)}${node.unavailable ? " · unavailable" : ""}`;
  }

  function destinationMeta(node: MembraneDestination): string {
    const host = node.hosts[0] ?? node.processes[0] ?? node.protocols[0] ?? "no host reported";
    return `${node.connectionCount} conn · ${host}`;
  }

  function recomputeEndpointLoads(): void {
    for (const node of endpointNodes.values()) {
      node.linkCount = 0;
    }
    for (const link of linkList) {
      const endpoint = endpointNodes.get(link.endpointKey);
      if (!endpoint) continue;
      endpoint.linkCount += link.connectionCount;
    }
  }

  function updateRouteLevels(dt: number, instant: boolean): void {
    const selectedKey = selected?.kind === "endpoint" ? selected.key : null;
    for (const endpoint of endpointList) {
      const target = routeTarget(endpoint.key, activeEndpointKey, selectedKey, status);
      const value = routeLevels.get(endpoint.key) ?? 0;
      if (instant) {
        if (value !== target) routeLevels.set(endpoint.key, target);
        continue;
      }
      if (dt <= 0) continue;
      const delta = target - value;
      if (Math.abs(delta) < 0.001) {
        if (value !== target) routeLevels.set(endpoint.key, target);
        continue;
      }
      const step = Math.sign(delta) * Math.min(Math.abs(delta), dt);
      routeLevels.set(endpoint.key, value + step);
    }
    for (const key of routeLevels.keys()) {
      if (!endpointNodes.has(key)) routeLevels.delete(key);
    }
  }

  function updateLifecycleFades(dt: number): void {
    if (reducedMotion) {
      let removed = false;
      for (const node of destinationList) {
        if (node.retiring) {
          destinationNodes.delete(node.key);
          removed = true;
        } else {
          node.fade = 1;
        }
      }
      for (const link of linkList) {
        if (link.retiring) {
          linkStates.delete(`${link.endpointKey}|${link.destinationKey}`);
          removed = true;
        } else {
          link.fade = 1;
        }
      }
      if (removed) {
        destinationList = Array.from(destinationNodes.values());
        linkList = Array.from(linkStates.values());
        recomputeEndpointLoads();
      }
      return;
    }

    for (const node of destinationList) {
      if (node.retiring) {
        node.fade = Math.max(0, node.fade - dt / FADE_OUT_SECONDS);
      } else if (node.fade < 1) {
        node.fade = Math.min(1, node.fade + dt / FADE_IN_SECONDS);
      }
    }
    for (const link of linkList) {
      if (link.retiring) {
        link.fade = Math.max(0, link.fade - dt / FADE_OUT_SECONDS);
      } else if (link.fade < 1) {
        link.fade = Math.min(1, link.fade + dt / FADE_IN_SECONDS);
      }
    }

    let removed = false;
    for (const node of destinationList) {
      if (node.retiring && node.fade <= 0.001) {
        destinationNodes.delete(node.key);
        removed = true;
      }
    }
    for (const link of linkList) {
      if (link.retiring && link.fade <= 0.001) {
        linkStates.delete(`${link.endpointKey}|${link.destinationKey}`);
        removed = true;
      }
    }
    if (removed) {
      destinationList = Array.from(destinationNodes.values());
      linkList = Array.from(linkStates.values());
      recomputeEndpointLoads();
    }
  }

  function drawDecorativeEdges(): void {
    const ctx = ctx2d;
    if (!ctx) return;
    const bins = backgroundBins;
    for (const bin of bins) bin.length = 0;

    for (const edge of background.edges) {
      const a = edge.a;
      const b = edge.b;
      const normalized = Math.min(0.999, Math.max(0, (pointDepth[a] + pointDepth[b] + 2) / 4));
      const band = Math.floor(normalized * 4);
      const bin = bins[edge.kind * 4 + band];
      bin.push(pointX[a], pointY[a], pointX[b], pointY[b]);
    }

    ctx.lineCap = "round";
    for (let kind = 0; kind < 3; kind++) {
      ctx.lineWidth = kind === 1 ? 0.75 : 0.55;
      for (let band = 0; band < 4; band++) {
        const bin = bins[kind * 4 + band];
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

  const backgroundBins: number[][] = Array.from({ length: 12 }, () => []);

  function drawConnectionEdges(): void {
    const ctx = ctx2d;
    if (!ctx) return;
    ctx.lineCap = "round";
    ctx.strokeStyle = colors.connection;

    for (const endpoint of endpointList) {
      if (endpoint.linkCount <= 0) continue;
      const level = routeLevels.get(endpoint.key) ?? 0;
      const selectedHere = selected?.kind === "endpoint" && selected.key === endpoint.key;
      const activity = 0.07 + Math.min(0.16, Math.log10(1 + endpoint.linkCount) * 0.08);
      const alpha = Math.min(0.7, activity + level * 0.4 + (selectedHere ? 0.18 : 0));
      ctx.globalAlpha = alpha;
      ctx.lineWidth = 0.55 + level * 0.55 + (selectedHere ? 0.25 : 0);
      ctx.beginPath();
      ctx.moveTo(camera.cx, camera.cy);
      ctx.lineTo(endpoint.sx, endpoint.sy);
      ctx.stroke();
    }

    for (const link of linkList) {
      if (link.fade <= 0.004) continue;
      const destination = destinationNodes.get(link.destinationKey);
      if (!destination || destination.fade <= 0.004) continue;
      const endpoint = endpointNodes.get(link.endpointKey);
      if (!endpoint) continue;
      const selectedDestination =
        selected?.kind === "destination" && selected.key === destination.key;
      const level = routeLevels.get(endpoint.key) ?? 0;
      const selectedEndpoint = selected?.kind === "endpoint" && selected.key === endpoint.key;
      const activity = 0.16 + Math.min(0.2, Math.log10(1 + link.connectionCount) * 0.1);
      const alpha =
        link.fade *
        destination.fade *
        Math.min(
          0.82,
          activity + level * 0.3 + (selectedDestination ? 0.4 : 0) + (selectedEndpoint ? 0.12 : 0),
        );
      ctx.globalAlpha = alpha;
      ctx.strokeStyle = selectedDestination ? colors.destinationSelected : colors.connection;
      ctx.lineWidth =
        0.55 + level * 0.45 + (selectedDestination ? 0.7 : 0) + (selectedEndpoint ? 0.2 : 0);
      ctx.beginPath();
      ctx.moveTo(endpoint.sx, endpoint.sy);
      ctx.lineTo(destination.sx, destination.sy);
      ctx.stroke();
    }

    ctx.globalAlpha = 1;
    ctx.strokeStyle = colors.connection;
  }

  function drawDestinations(): void {
    const ctx = ctx2d;
    if (!ctx) return;

    for (const node of destinationList) {
      if (node.fade <= 0.004) continue;
      const depth = clamp((node.depth + 1) / 2, 0, 1);
      const selectedHere = selected?.kind === "destination" && selected.key === node.key;
      const hovered = hoverTarget?.kind === "destination" && hoverTarget.key === node.key;
      const radius = 1.6 + depth * 1.2 + (selectedHere ? 0.8 : 0);

      ctx.globalAlpha = node.fade * (0.38 + depth * 0.52);
      ctx.fillStyle = selectedHere ? colors.destinationSelected : colors.destination;
      ctx.beginPath();
      ctx.arc(node.sx, node.sy, radius, 0, TAU);
      ctx.fill();

      if (selectedHere) {
        ctx.globalAlpha = node.fade * 0.85;
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, radius + 4, 0, TAU);
        ctx.strokeStyle = colors.destinationSelected;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      if (hovered) {
        ctx.globalAlpha = node.fade * 0.65;
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, radius + 5, 0, TAU);
        ctx.strokeStyle = colors.hover;
        ctx.lineWidth = 1;
        ctx.stroke();
      }
    }
    ctx.globalAlpha = 1;
  }

  function drawEndpoints(): void {
    const ctx = ctx2d;
    if (!ctx) return;

    for (const node of endpointList) {
      const depth = clamp((node.depth + 1) / 2, 0, 1);
      const presence = 0.6 + depth * 0.4;
      const level = routeLevels.get(node.key) ?? 0;
      const active = activeEndpointKey === node.key;
      const selectedHere = selected?.kind === "endpoint" && selected.key === node.key;
      const hovered = hoverTarget?.kind === "endpoint" && hoverTarget.key === node.key;

      if (status !== "starting" && level > 0.28) {
        const phase = ((stableHash(node.key) % 1000) / 1000) * TAU;
        const breath = reducedMotion ? 0.5 : 0.5 + 0.5 * Math.sin(animationTime * 0.85 + phase);

        ctx.globalAlpha = level * presence * (reducedMotion ? 0.7 : 0.52 + breath * 0.24);
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, 9 + (reducedMotion ? 0 : 1.2 * breath), 0, TAU);
        ctx.fillStyle = colors.haloFill;
        ctx.fill();

        ctx.globalAlpha = level * presence * (reducedMotion ? 0.5 : 0.34 + breath * 0.3);
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, 11.8 + (reducedMotion ? 0 : 2.2 * breath), 0, TAU);
        ctx.strokeStyle = colors.haloRing;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      if (status === "starting" && selectedHere) {
        if (reducedMotion) {
          ctx.globalAlpha = 0.5 * presence;
          ctx.beginPath();
          ctx.arc(node.sx, node.sy, 9, 0, TAU);
          ctx.fillStyle = colors.haloFill;
          ctx.fill();

          ctx.beginPath();
          ctx.arc(node.sx, node.sy, 11.8, 0, TAU);
          ctx.strokeStyle = colors.haloRing;
          ctx.lineWidth = 1;
          ctx.stroke();
        } else {
          for (let ring = 0; ring < 2; ring++) {
            const progress = (animationTime / 2.4 + ring * 0.5) % 1;
            ctx.globalAlpha = (1 - progress) * 0.2 * presence;
            ctx.beginPath();
            ctx.arc(node.sx, node.sy, 6.5 + progress * 17, 0, TAU);
            ctx.strokeStyle = colors.startRing;
            ctx.lineWidth = 1;
            ctx.stroke();
          }
        }
      }

      ctx.globalAlpha = active ? 1 : 0.62 + depth * 0.38;

      if (hovered) {
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, 11, 0, TAU);
        ctx.strokeStyle = colors.hover;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      if (selectedHere) {
        ctx.beginPath();
        ctx.setLineDash([2, 2]);
        ctx.arc(node.sx, node.sy, 8.5, 0, TAU);
        ctx.strokeStyle = colors.endpointSelected;
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.setLineDash([]);
      }

      if (node.unavailable) ctx.setLineDash([2, 2]);
      const radius = active ? 5.2 : 4.5;
      if (node.kind === "unresolved") {
        ctx.beginPath();
        ctx.moveTo(node.sx, node.sy - radius);
        ctx.lineTo(node.sx + radius, node.sy);
        ctx.lineTo(node.sx, node.sy + radius);
        ctx.lineTo(node.sx - radius, node.sy);
        ctx.closePath();
      } else {
        ctx.beginPath();
        ctx.arc(node.sx, node.sy, radius, 0, TAU);
      }
      ctx.strokeStyle = active ? colors.endpointActive : colors.endpointRing;
      ctx.lineWidth = active ? 1.6 : 1.15;
      ctx.stroke();
      if (node.unavailable) ctx.setLineDash([]);

      ctx.beginPath();
      ctx.arc(node.sx, node.sy, 1.4, 0, TAU);
      ctx.fillStyle = active ? colors.endpointActive : colors.endpointDot;
      ctx.fill();
    }
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
      if (!dragging) yaw += dt * 0.045;
    }
    updateRouteLevels(animating ? dt : 0, reducedMotion);
    updateLifecycleFades(animating ? dt : 0);

    updateCamera(camera, hostWidth, hostHeight, zoom, yaw, pitch);
    ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    ctx.fillStyle = colors.surface;
    ctx.fillRect(0, 0, hostWidth, hostHeight);

    for (let i = 0; i < background.points.length; i++) {
      const point = background.points[i];
      const drift = reducedMotion
        ? 0
        : point.driftAmount * Math.sin(animationTime * point.driftSpeed + point.driftPhase);
      const factor = 1 + drift;
      projectPoint(camera, point.x * factor, point.y * factor, point.z * factor, scratch);
      pointX[i] = scratch.x;
      pointY[i] = scratch.y;
      pointDepth[i] = scratch.depth;
    }

    ctx.globalAlpha = status === "connected" ? 0.72 : 1;
    drawDecorativeEdges();

    pointOrder.sort(comparePointDepth);
    let bucket = -1;
    for (const index of pointOrder) {
      const depth = clamp((pointDepth[index] + 1) / 2, 0, 1);
      const nextBucket = Math.min(5, Math.floor(depth * 6));
      if (nextBucket !== bucket) {
        ctx.fillStyle = colors.point[nextBucket];
        bucket = nextBucket;
      }
      const radius = background.points[index].size * (0.42 + depth);
      ctx.fillRect(pointX[index] - radius, pointY[index] - radius, radius * 2, radius * 2);
    }
    ctx.globalAlpha = 1;

    for (const node of endpointList) {
      projectPoint(camera, node.position.x, node.position.y, node.position.z, scratch);
      node.sx = scratch.x;
      node.sy = scratch.y;
      node.depth = scratch.depth;
    }
    for (const node of destinationList) {
      projectPoint(camera, node.position.x, node.position.y, node.position.z, scratch);
      node.sx = scratch.x;
      node.sy = scratch.y;
      node.depth = scratch.depth;
    }

    drawConnectionEdges();

    ctx.globalAlpha = 0.65;
    ctx.beginPath();
    ctx.arc(camera.cx, camera.cy, 2.4, 0, TAU);
    ctx.strokeStyle = colors.endpointRing;
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.globalAlpha = 1;

    drawDestinations();
    drawEndpoints();
  }

  function comparePointDepth(a: number, b: number): number {
    return pointDepth[a] - pointDepth[b];
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
    if (running || singleFrame || !canvas || document.hidden || !onScreen) return;
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

  function hitSelection(clientX: number, clientY: number): MembraneSelection | null {
    const surface = canvas;
    if (!surface) return null;
    const rect = surface.getBoundingClientRect();
    const x = clientX - rect.left;
    const y = clientY - rect.top;
    let best: MembraneSelection | null = null;
    let bestDistance = Infinity;

    for (const node of endpointList) {
      const dx = node.sx - x;
      const dy = node.sy - y;
      const distance = dx * dx + dy * dy;
      if (distance <= 121 && distance < bestDistance) {
        bestDistance = distance;
        best = { kind: "endpoint", key: node.key };
      }
    }
    for (const node of destinationList) {
      if (node.fade <= 0.15) continue;
      const dx = node.sx - x;
      const dy = node.sy - y;
      const distance = dx * dx + dy * dy;
      if (distance <= 81 && distance < bestDistance) {
        bestDistance = distance;
        best = { kind: "destination", key: node.key };
      }
    }
    return best;
  }

  function updateHover(event: PointerEvent): void {
    const next = hitSelection(event.clientX, event.clientY);
    if (!next) {
      if (hoverTarget !== null) {
        hoverTarget = null;
        tip = null;
        scheduleDraw();
      }
      return;
    }

    let label: string;
    let meta: string;
    if (next.kind === "endpoint") {
      const node = endpointNodes.get(next.key);
      if (!node) return;
      label = node.kind === "unresolved" ? (node.tag ?? node.key) : (node.ip ?? node.key);
      meta = endpointMeta(node);
      tip = {
        x: clamp(node.sx + 14, 8, Math.max(8, hostWidth - 214)),
        y: clamp(node.sy + 12, 8, Math.max(8, hostHeight - 58)),
        label,
        meta,
      };
    } else {
      const node = destinationNodes.get(next.key);
      if (!node) return;
      label = node.data.ip;
      meta = destinationMeta(node.data);
      tip = {
        x: clamp(node.sx + 14, 8, Math.max(8, hostWidth - 214)),
        y: clamp(node.sy + 12, 8, Math.max(8, hostHeight - 58)),
        label,
        meta,
      };
    }

    if (hoverTarget?.kind !== next.kind || hoverTarget.key !== next.key) {
      hoverTarget = next;
      scheduleDraw();
    }
  }

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    dragging = true;
    moved = false;
    lastPointerX = event.clientX;
    lastPointerY = event.clientY;
    hoverTarget = null;
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
    onselect(hitSelection(event.clientX, event.clientY));
  }

  function onPointerLeave(): void {
    if (dragging) return;
    if (hoverTarget !== null || tip !== null) {
      hoverTarget = null;
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
    const literal = endpoints;
    const extra = unresolved;

    untrack(() => {
      const seen = new SvelteSet<string>();

      for (const endpoint of literal) {
        seen.add(endpoint.key);
        let node = endpointNodes.get(endpoint.key);
        if (!node) {
          node = {
            key: endpoint.key,
            kind: "endpoint",
            ip: endpoint.ip,
            tag: null,
            members: endpoint.members,
            unavailable: endpoint.unavailable,
            position: endpointLattice.position(endpoint.key),
            sx: 0,
            sy: 0,
            depth: 0,
            linkCount: 0,
          };
          endpointNodes.set(endpoint.key, node);
        } else {
          node.ip = endpoint.ip;
          node.members = endpoint.members;
          node.unavailable = endpoint.unavailable;
        }
      }

      for (const endpoint of extra) {
        seen.add(endpoint.key);
        let node = endpointNodes.get(endpoint.key);
        if (!node) {
          node = {
            key: endpoint.key,
            kind: "unresolved",
            ip: null,
            tag: endpoint.tag,
            members: [{ tag: endpoint.tag, type: endpoint.type, delay: endpoint.delay }],
            unavailable: endpoint.unavailable,
            position: unresolvedLattice.position(endpoint.key),
            sx: 0,
            sy: 0,
            depth: 0,
            linkCount: 0,
          };
          endpointNodes.set(endpoint.key, node);
        } else {
          node.tag = endpoint.tag;
          node.members = [{ tag: endpoint.tag, type: endpoint.type, delay: endpoint.delay }];
          node.unavailable = endpoint.unavailable;
        }
      }

      for (const key of endpointNodes.keys()) {
        if (!seen.has(key)) endpointNodes.delete(key);
      }
      endpointList = Array.from(endpointNodes.values());
      recomputeEndpointLoads();
      scheduleDraw();
    });
  });

  $effect(() => {
    const incoming = destinations;

    untrack(() => {
      const seenDestinations = new SvelteSet<string>();
      const seenLinks = new SvelteSet<string>();

      for (const destination of incoming) {
        seenDestinations.add(destination.key);
        let node = destinationNodes.get(destination.key);
        if (!node) {
          node = {
            key: destination.key,
            data: destination,
            position: destinationLattice.position(destination.key),
            sx: 0,
            sy: 0,
            depth: 0,
            fade: reducedMotion ? 1 : 0,
            retiring: false,
          };
          destinationNodes.set(destination.key, node);
        } else {
          node.data = destination;
          if (node.retiring) {
            node.retiring = false;
            if (reducedMotion) node.fade = 1;
          }
        }

        for (const link of destination.links) {
          const key = `${link.endpointKey}|${destination.key}`;
          seenLinks.add(key);
          let state = linkStates.get(key);
          if (!state) {
            state = {
              endpointKey: link.endpointKey,
              destinationKey: destination.key,
              connectionCount: link.connectionCount,
              fade: reducedMotion ? 1 : 0,
              retiring: false,
            };
            linkStates.set(key, state);
          } else {
            state.connectionCount = link.connectionCount;
            if (state.retiring) {
              state.retiring = false;
              if (reducedMotion) state.fade = 1;
            }
          }
        }
      }

      for (const [key, node] of destinationNodes) {
        if (seenDestinations.has(key)) continue;
        if (reducedMotion) {
          destinationNodes.delete(key);
        } else if (!node.retiring) {
          node.retiring = true;
        }
      }

      for (const [key, state] of linkStates) {
        if (seenLinks.has(key)) continue;
        if (reducedMotion) {
          linkStates.delete(key);
        } else if (!state.retiring) {
          state.retiring = true;
        }
      }

      destinationList = Array.from(destinationNodes.values());
      linkList = Array.from(linkStates.values());
      recomputeEndpointLoads();
      scheduleDraw();
    });
  });

  $effect(() => {
    void status;
    void activeEndpointKey;
    void selected;
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
    max-width: 206px;
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
