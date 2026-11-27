/**
 * Pure 3D geometry and state helpers for the experimental membrane prototype.
 *
 * The decorative web is generated locally from a fixed seed. Real anchors are
 * only positioned and labelled from the live constellation model; nothing here
 * invents servers, regions, hops, or latency.
 */
import type { EngineStatus } from "./api";

export const ANCHOR_CAP = 42;

const TAU = Math.PI * 2;
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

const SHELLS = [
  { radius: 0.78, count: 128, phase: 0.7, seed: 11 },
  { radius: 0.9, count: 144, phase: 2.1, seed: 29 },
  { radius: 1.0, count: 168, phase: 4.3, seed: 47 },
] as const;

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export interface MembranePoint {
  x: number;
  y: number;
  z: number;
  size: number;
  driftPhase: number;
  driftSpeed: number;
  driftAmount: number;
}

export interface MembraneEdge {
  a: number;
  b: number;
  /** 0 = short intra-shell, 1 = sparse long link, 2 = inter-shell bridge. */
  kind: 0 | 1 | 2;
}

export interface MembraneAnchorInput {
  id: string;
  label: string;
  type: string;
  delay: number | "error" | null;
}

export interface MembraneAnchor extends MembraneAnchorInput {
  x: number;
  y: number;
  z: number;
  /** Perspective-safe route curve from the membrane interior to this anchor. */
  path: Vec3[];
}

export interface MembraneGeometry {
  points: MembranePoint[];
  edges: MembraneEdge[];
  anchors: MembraneAnchor[];
}

export interface MembraneCamera {
  cx: number;
  cy: number;
  focal: number;
  depth: number;
  cosYaw: number;
  sinYaw: number;
  cosPitch: number;
  sinPitch: number;
}

export interface ScreenPoint {
  x: number;
  y: number;
  /** Rotated z before perspective division, -1.1..1.1. */
  depth: number;
}

export interface AnchorCap<T> {
  visible: T[];
  hidden: number;
}

function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Stable FNV-1a hash; used for anchor placement and the deterministic cap. */
export function stableHash(value: string): number {
  let hash = 2166136261;
  for (let i = 0; i < value.length; i++) hash = Math.imul(hash ^ value.charCodeAt(i), 16777619);
  return hash >>> 0;
}

function buildEdges(
  points: MembranePoint[],
  sections: readonly { start: number; end: number; radius: number }[],
): MembraneEdge[] {
  const edges: MembraneEdge[] = [];
  const seen = new Set<number>();
  const count = points.length;
  const neighbourCount = 5;
  const bestIndex = new Int32Array(neighbourCount);
  const bestDistance = new Float64Array(neighbourCount);

  const add = (a: number, b: number, kind: 0 | 1 | 2): void => {
    if (a === b) return;
    const low = a < b ? a : b;
    const high = a < b ? b : a;
    const key = low * count + high;
    if (seen.has(key)) return;
    seen.add(key);
    edges.push({ a: low, b: high, kind });
  };

  for (const section of sections) {
    for (let i = section.start; i < section.end; i++) {
      bestIndex.fill(-1);
      bestDistance.fill(Infinity);
      const px = points[i].x;
      const py = points[i].y;
      const pz = points[i].z;

      for (let j = section.start; j < section.end; j++) {
        if (j === i) continue;
        const dx = points[j].x - px;
        const dy = points[j].y - py;
        const dz = points[j].z - pz;
        const distance = dx * dx + dy * dy + dz * dz;
        if (distance >= bestDistance[neighbourCount - 1]) continue;

        let slot = neighbourCount - 1;
        while (slot > 0 && distance < bestDistance[slot - 1]) {
          bestDistance[slot] = bestDistance[slot - 1];
          bestIndex[slot] = bestIndex[slot - 1];
          slot--;
        }
        bestDistance[slot] = distance;
        bestIndex[slot] = j;
      }

      for (let k = 0; k < neighbourCount; k++) {
        if (bestIndex[k] >= 0) add(i, bestIndex[k], 0);
      }
    }

    const random = mulberry32(section.start * 2654435761 + section.end * 97);
    const longLinks = Math.max(10, Math.round((section.end - section.start) * 0.09));
    for (let link = 0; link < longLinks; link++) {
      const a = section.start + Math.floor(random() * (section.end - section.start));
      const b = section.start + Math.floor(random() * (section.end - section.start));
      const dx = points[a].x - points[b].x;
      const dy = points[a].y - points[b].y;
      const dz = points[a].z - points[b].z;
      if (dx * dx + dy * dy + dz * dz > (section.radius * 1.15) ** 2) add(a, b, 1);
    }
  }

  for (let s = 0; s < sections.length - 1; s++) {
    const inner = sections[s];
    const outer = sections[s + 1];
    for (let i = inner.start; i < inner.end; i += 3) {
      let nearest = outer.start;
      let nearestDistance = Infinity;
      for (let j = outer.start; j < outer.end; j++) {
        const dx = points[j].x - points[i].x;
        const dy = points[j].y - points[i].y;
        const dz = points[j].z - points[i].z;
        const distance = dx * dx + dy * dy + dz * dz;
        if (distance < nearestDistance) {
          nearestDistance = distance;
          nearest = j;
        }
      }
      add(i, nearest, 2);
    }
  }

  return edges;
}

function buildRoutePath(anchor: Vec3): Vec3[] {
  const radius = Math.hypot(anchor.x, anchor.y, anchor.z) || 1;
  const dir = { x: anchor.x / radius, y: anchor.y / radius, z: anchor.z / radius };
  const helper = Math.abs(dir.y) > 0.92 ? { x: 1, y: 0, z: 0 } : { x: 0, y: 1, z: 0 };
  let tx = dir.y * helper.z - dir.z * helper.y;
  let ty = dir.z * helper.x - dir.x * helper.z;
  let tz = dir.x * helper.y - dir.y * helper.x;
  const tangentLength = Math.hypot(tx, ty, tz) || 1;
  tx /= tangentLength;
  ty /= tangentLength;
  tz /= tangentLength;

  const p0 = { x: dir.x * 0.14, y: dir.y * 0.14, z: dir.z * 0.14 };
  const p1 = { x: dir.x * 0.4 + tx * 0.22, y: dir.y * 0.4 + ty * 0.22, z: dir.z * 0.4 + tz * 0.22 };
  const p2 = {
    x: dir.x * 0.76 - tx * 0.17,
    y: dir.y * 0.76 - ty * 0.17,
    z: dir.z * 0.76 - tz * 0.17,
  };

  const samples: Vec3[] = [];
  const steps = 30;
  for (let i = 0; i <= steps; i++) {
    const t = i / steps;
    const mt = 1 - t;
    const a = mt * mt * mt;
    const b = 3 * mt * mt * t;
    const c = 3 * mt * t * t;
    const d = t * t * t;
    samples.push({
      x: a * p0.x + b * p1.x + c * p2.x + d * anchor.x,
      y: a * p0.y + b * p1.y + c * p2.y + d * anchor.y,
      z: a * p0.z + b * p1.z + c * p2.z + d * anchor.z,
    });
  }
  return samples;
}

function anchorPosition(id: string): Vec3 {
  const first = stableHash(id);
  const second = stableHash(`membrane:${id}`);
  const u = (first % 100003) / 100003;
  const v = (second % 1000033) / 1000033;
  const y = 1 - 2 * u;
  const planar = Math.sqrt(Math.max(0, 1 - y * y));
  const theta = v * TAU + 0.16 * Math.sin(u * 5.1 + v * 2.7);
  const radius = 1.052 + 0.016 * Math.sin(theta * 2 + u * 7);
  return {
    x: Math.cos(theta) * planar * radius,
    y: y * radius,
    z: Math.sin(theta) * planar * radius,
  };
}

/** Build the decorative shells, their edges, and positions for the real anchors. */
export function buildMembraneGeometry(anchors: readonly MembraneAnchorInput[]): MembraneGeometry {
  const points: MembranePoint[] = [];
  const sections: { start: number; end: number; radius: number }[] = [];

  for (let shell = 0; shell < SHELLS.length; shell++) {
    const spec = SHELLS[shell];
    const start = points.length;
    const random = mulberry32(spec.seed * 7919 + 43);
    for (let i = 0; i < spec.count; i++) {
      const t = (i + 0.5) / spec.count;
      const baseY = 1 - 2 * t;
      const baseTheta = i * GOLDEN_ANGLE + spec.phase;
      const theta =
        baseTheta +
        0.075 * Math.sin(baseTheta * 2.3 + spec.phase) +
        0.04 * Math.cos(baseY * Math.PI * 3 - spec.phase);
      const y = Math.min(
        0.985,
        Math.max(
          -0.985,
          baseY +
            0.035 * Math.sin(baseTheta * 1.7 + spec.phase) +
            0.015 * Math.cos(baseTheta * 4.1),
        ),
      );
      const planar = Math.sqrt(Math.max(0, 1 - y * y));
      const radial =
        spec.radius *
        (1 +
          0.038 * Math.sin(theta * 3 + y * 2.4 + spec.phase) +
          0.02 * Math.cos(y * Math.PI * 2 - spec.phase));

      points.push({
        x: Math.cos(theta) * planar * radial + (random() - 0.5) * 0.008,
        y: y * radial + (random() - 0.5) * 0.008,
        z: Math.sin(theta) * planar * radial + (random() - 0.5) * 0.008,
        size: 0.78 + random() * 0.52,
        driftPhase: random() * TAU,
        driftSpeed: 0.12 + random() * 0.2,
        driftAmount: 0.0025 + random() * 0.0055,
      });
    }
    sections.push({ start, end: points.length, radius: spec.radius });
  }

  const seen = new Set<string>();
  const placed: MembraneAnchor[] = [];
  for (const input of anchors) {
    if (seen.has(input.id)) continue;
    seen.add(input.id);
    const position = anchorPosition(input.id);
    placed.push({ ...input, ...position, path: buildRoutePath(position) });
  }

  return { points, edges: buildEdges(points, sections), anchors: placed };
}

export function createCamera(): MembraneCamera {
  return {
    cx: 0,
    cy: 0,
    focal: 1,
    depth: 3.2,
    cosYaw: 1,
    sinYaw: 0,
    cosPitch: 1,
    sinPitch: 0,
  };
}

/** Mutates the camera so the render loop does not allocate one per frame. */
export function updateCamera(
  camera: MembraneCamera,
  width: number,
  height: number,
  zoom: number,
  yaw: number,
  pitch: number,
): void {
  const fit = Math.min(width, height) * 0.26 * zoom;
  camera.cx = width / 2;
  camera.cy = height / 2;
  camera.depth = 3.2;
  camera.focal = fit * camera.depth;
  camera.cosYaw = Math.cos(yaw);
  camera.sinYaw = Math.sin(yaw);
  camera.cosPitch = Math.cos(pitch);
  camera.sinPitch = Math.sin(pitch);
}

/** Perspective projection into a reusable output object. */
export function projectPoint(
  camera: MembraneCamera,
  x: number,
  y: number,
  z: number,
  out: ScreenPoint,
): void {
  const rx = x * camera.cosYaw + z * camera.sinYaw;
  const rz = z * camera.cosYaw - x * camera.sinYaw;
  const ry = y * camera.cosPitch - rz * camera.sinPitch;
  const rz2 = y * camera.sinPitch + rz * camera.cosPitch;
  const distance = camera.depth - rz2;
  const scale = camera.focal / (distance > 0.4 ? distance : 0.4);
  out.x = camera.cx + rx * scale;
  out.y = camera.cy - ry * scale;
  out.depth = rz2;
}

/**
 * Which path should be lit for this anchor. A real active-outbound change makes
 * the previous id fall to zero and the new id rise to one at the same rate.
 */
export function routeTarget(
  id: string,
  activeId: string | null,
  selectedId: string | null,
  status: EngineStatus,
): number {
  if (status === "connected") {
    if (activeId !== null && id === activeId) return 1;
    if (selectedId !== null && id === selectedId) return 0.22;
    return 0;
  }
  if (status === "starting") {
    if (selectedId !== null && id === selectedId) return 0.72;
    return 0;
  }
  return 0;
}

/**
 * Active and selected anchors are always visible; the rest is a deterministic
 * hash-ordered slice so the same outbounds stay visible across renders.
 */
export function capAnchors<T extends { id: string }>(
  nodes: readonly T[],
  activeId: string | null,
  selectedId: string | null,
  cap: number = ANCHOR_CAP,
): AnchorCap<T> {
  if (nodes.length <= cap) return { visible: nodes.slice(), hidden: 0 };

  const essential = new Set<string>();
  for (const id of [activeId, selectedId]) {
    if (id && nodes.some((node) => node.id === id)) essential.add(id);
  }

  const rest = nodes.filter((node) => !essential.has(node.id));
  rest.sort((a, b) => {
    const hashA = stableHash(a.id);
    const hashB = stableHash(b.id);
    if (hashA !== hashB) return hashA - hashB;
    return a.id.localeCompare(b.id);
  });

  const chosen = new Set(essential);
  const room = Math.max(0, cap - chosen.size);
  for (let i = 0; i < room; i++) chosen.add(rest[i].id);

  return {
    visible: nodes.filter((node) => chosen.has(node.id)),
    hidden: nodes.length - chosen.size,
  };
}
