/**
 * Pure globe/orbital geometry for the Orbital View.
 *
 * Land geometry: Natural Earth 1:110m land (`ne_110m_land`), public domain.
 * Source: https://www.naturalearthdata.com/downloads/110m-physical-vectors/110m-land/
 * Local copy: ./orbit-land.json. No runtime network fetch is used.
 *
 * Manual coordinates are user-supplied approximations stored only in
 * localStorage under `veil.orbit.locations.v1`. No geolocation or network
 * lookup is performed.
 */
import { IpLattice, parseIp } from "./membrane-ip";
import landData from "./orbit-land.json";

const TAU = Math.PI * 2;
const DEG = Math.PI / 180;

export const ORBIT_LOCATIONS_KEY = "veil.orbit.locations.v1";
export const ORBIT_ALTITUDE = 1.38;
export const YOU_STATION = Object.freeze(lonLatToVector(82, 0, 1.08));

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export interface OrbitLocation {
  lat: number;
  lon: number;
}

export type OrbitLocationMap = Record<string, OrbitLocation>;

export interface OrbitCamera {
  cx: number;
  cy: number;
  radius: number;
  cosYaw: number;
  sinYaw: number;
  cosPitch: number;
  sinPitch: number;
}

export interface OrbitScreenPoint {
  x: number;
  y: number;
  depth: number;
  visible: boolean;
}

export interface OrbitRingScratch {
  rotated: Float32Array;
  xs: number[];
  ys: number[];
  rims: boolean[];
  out: number[];
}

interface LandFeatureCollection {
  features: Array<{
    geometry?: {
      type?: string;
      coordinates?: unknown;
    };
  }>;
}

const endpointOrbitLattice = new IpLattice("veil.orbit.endpoint.v1", ORBIT_ALTITUDE, 0.065);
const unresolvedOrbitLattice = new IpLattice("veil.orbit.unresolved.v1", 1.34, 0.06);

export function lonLatToVector(lat: number, lon: number, radius = 1): Vec3 {
  const latitude = lat * DEG;
  const longitude = lon * DEG;
  const cosLat = Math.cos(latitude);
  return {
    x: cosLat * Math.cos(longitude) * radius,
    y: Math.sin(latitude) * radius,
    z: cosLat * Math.sin(longitude) * radius,
  };
}

export function locationToOrbitVector(location: OrbitLocation, radius = ORBIT_ALTITUDE): Vec3 {
  return lonLatToVector(location.lat, location.lon, radius);
}

export function createOrbitCamera(): OrbitCamera {
  return {
    cx: 0,
    cy: 0,
    radius: 1,
    cosYaw: 1,
    sinYaw: 0,
    cosPitch: 1,
    sinPitch: 0,
  };
}

export function updateOrbitCamera(
  camera: OrbitCamera,
  width: number,
  height: number,
  zoom: number,
  yaw: number,
  pitch: number,
): void {
  camera.cx = width / 2;
  camera.cy = height / 2;
  camera.radius = Math.min(width, height) * 0.29 * zoom;
  camera.cosYaw = Math.cos(yaw);
  camera.sinYaw = Math.sin(yaw);
  camera.cosPitch = Math.cos(pitch);
  camera.sinPitch = Math.sin(pitch);
}

export function rotateVector(v: Vec3, camera: OrbitCamera, out: Vec3): void {
  const x1 = v.x * camera.cosYaw + v.z * camera.sinYaw;
  const z1 = v.z * camera.cosYaw - v.x * camera.sinYaw;
  const y2 = v.y * camera.cosPitch - z1 * camera.sinPitch;
  const z2 = v.y * camera.sinPitch + z1 * camera.cosPitch;
  out.x = x1;
  out.y = y2;
  out.z = z2;
}

/** Orthographic projection with orbit-altitude occlusion handling. */
export function projectOrbitPoint(
  v: Vec3,
  camera: OrbitCamera,
  out: OrbitScreenPoint,
  rotated: Vec3,
): void {
  rotateVector(v, camera, rotated);
  out.x = camera.cx + rotated.x * camera.radius;
  out.y = camera.cy - rotated.y * camera.radius;
  out.depth = rotated.z;
  out.visible = rotated.z >= 0 || rotated.x * rotated.x + rotated.y * rotated.y > 1;
}

export function createRingScratch(maxPoints: number): OrbitRingScratch {
  return {
    rotated: new Float32Array(maxPoints * 3),
    xs: [],
    ys: [],
    rims: [],
    out: [],
  };
}

/**
 * Rotate a unit-sphere ring, clip it against the visible hemisphere, and write
 * a flat [x, y, ...] screen path to scratch.out. Horizon crossings get short
 * rim arcs so fills do not cut straight across the globe silhouette.
 */
export function clipLandRing(
  unit: Float32Array,
  camera: OrbitCamera,
  scratch: OrbitRingScratch,
): boolean {
  const count = unit.length / 3;
  if (count < 3) return false;

  const rotated = scratch.rotated;
  for (let i = 0; i < count; i++) {
    const x = unit[i * 3];
    const y = unit[i * 3 + 1];
    const z = unit[i * 3 + 2];
    const x1 = x * camera.cosYaw + z * camera.sinYaw;
    const z1 = z * camera.cosYaw - x * camera.sinYaw;
    rotated[i * 3] = x1;
    rotated[i * 3 + 1] = y * camera.cosPitch - z1 * camera.sinPitch;
    rotated[i * 3 + 2] = y * camera.sinPitch + z1 * camera.cosPitch;
  }

  const xs = scratch.xs;
  const ys = scratch.ys;
  const rims = scratch.rims;
  xs.length = 0;
  ys.length = 0;
  rims.length = 0;

  let previous = count - 1;
  let previousVisible = rotated[previous * 3 + 2] >= 0;

  for (let current = 0; current < count; current++) {
    const currentVisible = rotated[current * 3 + 2] >= 0;
    const currentX = rotated[current * 3];
    const currentY = rotated[current * 3 + 1];

    if (previousVisible !== currentVisible) {
      const previousX = rotated[previous * 3];
      const previousY = rotated[previous * 3 + 1];
      const previousZ = rotated[previous * 3 + 2];
      const currentZ = rotated[current * 3 + 2];
      const t = previousZ / (previousZ - currentZ);
      let x = previousX + t * (currentX - previousX);
      let y = previousY + t * (currentY - previousY);
      const length = Math.hypot(x, y);
      if (length > 1e-6) {
        x /= length;
        y /= length;
      }
      xs.push(camera.cx + x * camera.radius);
      ys.push(camera.cy - y * camera.radius);
      rims.push(true);
    }

    if (currentVisible) {
      xs.push(camera.cx + currentX * camera.radius);
      ys.push(camera.cy - currentY * camera.radius);
      rims.push(false);
    }

    previous = current;
    previousVisible = currentVisible;
  }

  if (xs.length < 3) return false;

  const out = scratch.out;
  out.length = 0;
  for (let i = 0; i < xs.length; i++) {
    out.push(xs[i], ys[i]);
    const next = (i + 1) % xs.length;
    if (!rims[i] || !rims[next]) continue;

    const ax = (xs[i] - camera.cx) / camera.radius;
    const ay = (camera.cy - ys[i]) / camera.radius;
    const bx = (xs[next] - camera.cx) / camera.radius;
    const by = (camera.cy - ys[next]) / camera.radius;
    let delta = Math.atan2(by, bx) - Math.atan2(ay, ax);
    while (delta > Math.PI) delta -= TAU;
    while (delta < -Math.PI) delta += TAU;
    if (Math.abs(delta) < 1e-4) continue;

    const steps = Math.max(1, Math.ceil(Math.abs(delta) / 0.08));
    for (let step = 1; step < steps; step++) {
      const angle = Math.atan2(ay, ax) + (delta * step) / steps;
      out.push(
        camera.cx + Math.cos(angle) * camera.radius,
        camera.cy - Math.sin(angle) * camera.radius,
      );
    }
  }

  return true;
}

export function orbitalPositionForIp(ip: string): Vec3 {
  return endpointOrbitLattice.position(ip);
}

export function orbitalPositionForUnresolved(key: string): Vec3 {
  return unresolvedOrbitLattice.position(key);
}

export function isValidOrbitLocation(value: unknown): value is OrbitLocation {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as { lat?: unknown; lon?: unknown };
  return (
    typeof candidate.lat === "number" &&
    Number.isFinite(candidate.lat) &&
    candidate.lat >= -90 &&
    candidate.lat <= 90 &&
    typeof candidate.lon === "number" &&
    Number.isFinite(candidate.lon) &&
    candidate.lon >= -180 &&
    candidate.lon <= 180
  );
}

function roundCoordinate(value: number): number {
  return Math.round(value * 10000) / 10000;
}

function normalizeLocation(value: unknown): OrbitLocation | null {
  if (!isValidOrbitLocation(value)) return null;
  return { lat: roundCoordinate(value.lat), lon: roundCoordinate(value.lon) };
}

function normalizeLocationMap(value: unknown): OrbitLocationMap {
  const out: OrbitLocationMap = {};
  if (typeof value !== "object" || value === null || Array.isArray(value)) return out;
  for (const [key, entry] of Object.entries(value)) {
    const ip = parseIp(key);
    const location = normalizeLocation(entry);
    if (ip && location) out[ip] = location;
  }
  return out;
}

export function readOrbitLocations(): OrbitLocationMap {
  try {
    const raw = localStorage.getItem(ORBIT_LOCATIONS_KEY);
    if (!raw) return {};
    return normalizeLocationMap(JSON.parse(raw));
  } catch {
    return {};
  }
}

export function writeOrbitLocations(locations: OrbitLocationMap): OrbitLocationMap {
  const normalized = normalizeLocationMap(locations);
  try {
    localStorage.setItem(ORBIT_LOCATIONS_KEY, JSON.stringify(normalized));
  } catch {
    /* hardened webview can reject storage; the in-memory result still applies */
  }
  return normalized;
}

export function setOrbitLocation(ip: string, location: OrbitLocation): OrbitLocationMap {
  const normalizedIp = parseIp(ip);
  const normalized = normalizeLocation(location);
  const current = readOrbitLocations();
  if (!normalizedIp || !normalized) return current;
  current[normalizedIp] = normalized;
  return writeOrbitLocations(current);
}

export function removeOrbitLocation(ip: string): OrbitLocationMap {
  const normalizedIp = parseIp(ip);
  const current = readOrbitLocations();
  if (!normalizedIp) return current;
  delete current[normalizedIp];
  return writeOrbitLocations(current);
}

export function smoothstep(edge0: number, edge1: number, value: number): number {
  const t = Math.min(1, Math.max(0, (value - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

export function pulseProgress(time: number, speed = 0.18, offset = 0): number {
  const value = (time * speed + offset) % 1;
  return value < 0 ? value + 1 : value;
}

/** A lifted great-circle arc from YOU to an orbital endpoint. Logical, not a route. */
export function buildOrbitArc(from: Vec3, to: Vec3, samples = 56): Vec3[] {
  const aLength = Math.hypot(from.x, from.y, from.z) || 1;
  const bLength = Math.hypot(to.x, to.y, to.z) || 1;
  const a = { x: from.x / aLength, y: from.y / aLength, z: from.z / aLength };
  const b = { x: to.x / bLength, y: to.y / bLength, z: to.z / bLength };
  const dot = Math.min(1, Math.max(-1, a.x * b.x + a.y * b.y + a.z * b.z));
  const omega = Math.acos(dot);
  const sinOmega = Math.sin(omega);
  const out: Vec3[] = [];

  for (let i = 0; i <= samples; i++) {
    const t = i / samples;
    let x: number;
    let y: number;
    let z: number;
    if (sinOmega > 1e-6) {
      const first = Math.sin((1 - t) * omega) / sinOmega;
      const second = Math.sin(t * omega) / sinOmega;
      x = a.x * first + b.x * second;
      y = a.y * first + b.y * second;
      z = a.z * first + b.z * second;
    } else {
      x = a.x * (1 - t) + b.x * t;
      y = a.y * (1 - t) + b.y * t;
      z = a.z * (1 - t) + b.z * t;
    }
    const length = Math.hypot(x, y, z) || 1;
    const radius = aLength + (bLength - aLength) * t + Math.sin(Math.PI * t) * 0.12;
    out.push({
      x: (x / length) * radius,
      y: (y / length) * radius,
      z: (z / length) * radius,
    });
  }
  return out;
}

interface ParsedLand {
  rings: Float32Array[];
  maxPoints: number;
}

function buildLandRings(data: LandFeatureCollection): ParsedLand {
  const rings: Float32Array[] = [];
  let maxPoints = 0;

  const addRing = (ring: unknown): void => {
    if (!Array.isArray(ring)) return;
    const values: number[] = [];
    for (const point of ring) {
      if (!Array.isArray(point) || point.length < 2) continue;
      const lon = point[0];
      const lat = point[1];
      if (typeof lon !== "number" || typeof lat !== "number") continue;
      if (!Number.isFinite(lon) || !Number.isFinite(lat)) continue;
      if (lat < -90 || lat > 90 || lon < -180 || lon > 180) continue;
      const vector = lonLatToVector(lat, lon, 1);
      values.push(vector.x, vector.y, vector.z);
    }
    if (values.length < 9) return;
    const unit = new Float32Array(values);
    rings.push(unit);
    const points = unit.length / 3;
    if (points > maxPoints) maxPoints = points;
  };

  for (const feature of data.features) {
    const geometry = feature.geometry;
    if (!geometry || !Array.isArray(geometry.coordinates)) continue;
    if (geometry.type === "Polygon") {
      for (const ring of geometry.coordinates) addRing(ring);
    } else if (geometry.type === "MultiPolygon") {
      for (const polygon of geometry.coordinates) {
        if (!Array.isArray(polygon)) continue;
        for (const ring of polygon) addRing(ring);
      }
    }
  }

  return { rings, maxPoints };
}

const parsedLand = buildLandRings(landData as unknown as LandFeatureCollection);
export const LAND_RINGS: readonly Float32Array[] = parsedLand.rings;
export const LAND_MAX_POINTS = parsedLand.maxPoints;
