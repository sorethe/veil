/**
 * Pure IP parsing and deterministic spherical layout for the membrane.
 *
 * Only literal IPv4/IPv6 literals are accepted. Hostnames are never resolved,
 * and positions come from a seeded hash plus a fixed slot lattice, never from
 * array indexes, connection counts, latency, or sorting.
 */

const SLOT_COUNT = 1 << 24;
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));
const FNV_OFFSET = 0x811c9dc5;

export interface IpPoint {
  x: number;
  y: number;
  z: number;
}

export interface ParsedDestination {
  ip: string;
  port: number | null;
}

export interface EndpointMember {
  tag: string;
  type: string;
  delay: number | "error" | null;
}

export interface MembraneEndpoint {
  key: string;
  ip: string;
  members: EndpointMember[];
  unavailable: boolean;
}

export interface MembraneUnresolvedEndpoint {
  key: string;
  tag: string;
  type: string;
  delay: number | "error" | null;
  unavailable: boolean;
}

export interface MembraneDestinationLink {
  endpointKey: string;
  outbounds: string[];
  connectionCount: number;
  upload: number;
  download: number;
}

export interface MembraneDestination {
  key: string;
  ip: string;
  ports: number[];
  networks: string[];
  protocols: string[];
  hosts: string[];
  processes: string[];
  outbounds: string[];
  connectionCount: number;
  upload: number;
  download: number;
  earliestStart: number;
  links: MembraneDestinationLink[];
}

export type MembraneSelection = {
  kind: "endpoint" | "destination";
  key: string;
};

export interface ProfileEndpointScan {
  endpoints: MembraneEndpoint[];
  outboundCount: number;
  withoutLiteralIp: number;
  groupOutbounds: number;
  noServerOutbounds: number;
  hostnameServers: number;
}

function parseIpv4(value: string): number[] | null {
  const parts = value.split(".");
  if (parts.length !== 4) return null;
  const octets: number[] = [];
  for (const part of parts) {
    if (!/^\d{1,3}$/.test(part)) return null;
    const octet = Number(part);
    if (octet < 0 || octet > 255) return null;
    octets.push(octet);
  }
  return octets;
}

function formatIpv4(octets: number[]): string {
  return octets.join(".");
}

function parseIpv6Hextets(value: string): number[] | null {
  if (value.includes("%")) return null;

  let text = value.toLowerCase();
  const tailStart = text.lastIndexOf(":") + 1;
  const tail = text.slice(tailStart);
  if (tail.includes(".")) {
    const embedded = parseIpv4(tail);
    if (!embedded) return null;
    const high = (embedded[0] << 8) | embedded[1];
    const low = (embedded[2] << 8) | embedded[3];
    text = `${text.slice(0, tailStart)}${high.toString(16)}:${low.toString(16)}`;
  }

  if (text.split("::").length > 2) return null;
  const compressed = text.includes("::");
  const [leftText, rightText = ""] = text.split("::");
  const left = leftText ? leftText.split(":") : [];
  const right = rightText ? rightText.split(":") : [];
  if (left.some((part) => !/^[0-9a-f]{1,4}$/.test(part))) return null;
  if (right.some((part) => !/^[0-9a-f]{1,4}$/.test(part))) return null;

  const leftGroups = left.map((part) => Number.parseInt(part, 16));
  const rightGroups = right.map((part) => Number.parseInt(part, 16));

  if (compressed) {
    const missing = 8 - (leftGroups.length + rightGroups.length);
    if (missing < 1) return null;
    return [...leftGroups, ...new Array<number>(missing).fill(0), ...rightGroups];
  }

  if (leftGroups.length !== 8) return null;
  return leftGroups;
}

function formatIpv6(groups: number[]): string {
  let bestStart = -1;
  let bestLength = 0;
  let runStart = -1;
  let runLength = 0;

  for (let i = 0; i < groups.length; i++) {
    if (groups[i] === 0) {
      if (runStart < 0) {
        runStart = i;
        runLength = 0;
      }
      runLength++;
      if (runLength > bestLength) {
        bestStart = runStart;
        bestLength = runLength;
      }
    } else {
      runStart = -1;
      runLength = 0;
    }
  }

  if (bestLength < 2) return groups.map((group) => group.toString(16)).join(":");

  const left = groups
    .slice(0, bestStart)
    .map((group) => group.toString(16))
    .join(":");
  const right = groups
    .slice(bestStart + bestLength)
    .map((group) => group.toString(16))
    .join(":");
  return `${left}::${right}`;
}

/** Normalize a literal IPv4/IPv6 address; brackets are accepted, zones rejected. */
export function parseIp(value: string): string | null {
  let text = value.trim();
  if (!text) return null;
  if (text.startsWith("[") && text.endsWith("]")) text = text.slice(1, -1);
  if (!text || text.includes("%")) return null;

  const ipv4 = parseIpv4(text);
  if (ipv4) return formatIpv4(ipv4);
  if (!text.includes(":")) return null;

  const groups = parseIpv6Hextets(text);
  if (!groups) return null;

  const mapped =
    groups.slice(0, 5).every((group) => group === 0) && groups[5] === 0xffff
      ? [groups[6] >> 8, groups[6] & 0xff, groups[7] >> 8, groups[7] & 0xff]
      : null;
  if (mapped) return formatIpv4(mapped);

  return formatIpv6(groups);
}

function parsePort(value: string): number | null {
  if (!/^\d{1,5}$/.test(value)) return null;
  const port = Number(value);
  return port >= 0 && port <= 65535 ? port : null;
}

/** Parse `destination` values like `1.2.3.4:443` or `[2001:db8::1]:443`. No DNS. */
export function parseDestination(value: string): ParsedDestination | null {
  const raw = value.trim();
  if (!raw) return null;

  if (raw.startsWith("[")) {
    const close = raw.indexOf("]");
    if (close < 0) return null;
    const ip = parseIp(raw.slice(0, close + 1));
    if (!ip) return null;
    const rest = raw.slice(close + 1);
    if (!rest) return { ip, port: null };
    if (!rest.startsWith(":")) return null;
    const port = parsePort(rest.slice(1));
    return port === null ? null : { ip, port };
  }

  const colonCount = (raw.match(/:/g) ?? []).length;
  if (colonCount > 1) {
    const ip = parseIp(raw);
    return ip ? { ip, port: null } : null;
  }

  if (colonCount === 1) {
    const index = raw.indexOf(":");
    const ip = parseIp(raw.slice(0, index));
    if (!ip) return null;
    const port = parsePort(raw.slice(index + 1));
    return port === null ? null : { ip, port };
  }

  const ip = parseIp(raw);
  return ip ? { ip, port: null } : null;
}

function hashString(value: string, seed: number): number {
  let hash = seed >>> 0;
  for (let i = 0; i < value.length; i++) {
    hash = Math.imul(hash ^ value.charCodeAt(i), 16777619);
  }
  hash ^= hash >>> 16;
  hash = Math.imul(hash, 0x85ebca6b);
  hash ^= hash >>> 13;
  hash = Math.imul(hash, 0xc2b2ae35);
  return (hash ^ (hash >>> 16)) >>> 0;
}

function slotDirection(slot: number): { x: number; y: number; z: number } {
  const y = 1 - (2 * (slot + 0.5)) / SLOT_COUNT;
  const planar = Math.sqrt(Math.max(0, 1 - y * y));
  const theta = slot * GOLDEN_ANGLE;
  return { x: Math.cos(theta) * planar, y, z: Math.sin(theta) * planar };
}

/**
 * Fixed lattice allocator. Every key gets a hash-derived start slot and probe
 * step; existing keys never move when new keys are added.
 */
export class IpLattice {
  readonly #salt: string;
  readonly #radius: number;
  readonly #jitter: number;
  readonly #positions = new Map<string, IpPoint>();
  readonly #used = new Set<number>();

  constructor(salt: string, radius: number, jitter = 0) {
    this.#salt = salt;
    this.#radius = radius;
    this.#jitter = jitter;
  }

  position(key: string): IpPoint {
    const cached = this.#positions.get(key);
    if (cached) return cached;

    const seeded = hashString(`${this.#salt}|${key}`, FNV_OFFSET);
    const secondary = hashString(`${this.#salt}|probe|${key}`, 0x9e3779b9);
    const start = seeded % SLOT_COUNT;
    const step = (secondary % (SLOT_COUNT - 1)) | 1;

    let slot = start;
    for (let probe = 0; probe < SLOT_COUNT; probe++) {
      if (!this.#used.has(slot)) break;
      slot = (slot + step) % SLOT_COUNT;
    }
    this.#used.add(slot);

    const direction = slotDirection(slot);
    const jitter = this.#jitter === 0 ? 0 : ((secondary % 1000) / 1000 - 0.5) * 2 * this.#jitter;
    const radius = this.#radius + jitter;
    const point: IpPoint = {
      x: direction.x * radius,
      y: direction.y * radius,
      z: direction.z * radius,
    };
    this.#positions.set(key, point);
    return point;
  }
}

/**
 * Parse stored profile outbounds and group literal servers by normalized IP.
 * Selectors, urltest groups, direct/block nodes, and hostname servers are
 * counted but never given an invented endpoint.
 */
export function scanProfileEndpoints(content: string | null): ProfileEndpointScan {
  const empty: ProfileEndpointScan = {
    endpoints: [],
    outboundCount: 0,
    withoutLiteralIp: 0,
    groupOutbounds: 0,
    noServerOutbounds: 0,
    hostnameServers: 0,
  };
  if (!content?.trim()) return empty;

  let parsed: unknown;
  try {
    parsed = JSON.parse(content);
  } catch {
    return empty;
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return empty;
  if (!("outbounds" in parsed) || !Array.isArray(parsed.outbounds)) return empty;

  const grouped = new Map<string, Map<string, EndpointMember>>();
  let outboundCount = 0;
  let withoutLiteralIp = 0;
  let groupOutbounds = 0;
  let noServerOutbounds = 0;
  let hostnameServers = 0;

  for (const entry of parsed.outbounds) {
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) continue;
    if (!("tag" in entry) || typeof entry.tag !== "string" || !entry.tag) continue;
    outboundCount++;

    const tag = entry.tag;
    const type =
      "type" in entry && typeof entry.type === "string" && entry.type ? entry.type : "unknown";
    const kind = type.toLowerCase();
    const server =
      "server" in entry && typeof entry.server === "string" && entry.server.trim()
        ? entry.server
        : null;

    if (kind === "selector" || kind === "urltest") {
      groupOutbounds++;
      withoutLiteralIp++;
      continue;
    }
    if (kind === "direct" || kind === "block" || kind === "dns") {
      noServerOutbounds++;
      withoutLiteralIp++;
      continue;
    }

    if (!server) {
      noServerOutbounds++;
      withoutLiteralIp++;
      continue;
    }

    const ip = parseIp(server);
    if (!ip) {
      hostnameServers++;
      withoutLiteralIp++;
      continue;
    }

    let members = grouped.get(ip);
    if (!members) {
      members = new Map();
      grouped.set(ip, members);
    }
    members.set(tag, { tag, type, delay: null });
  }

  const endpoints: MembraneEndpoint[] = Array.from(grouped, ([ip, members]) => {
    const sorted = Array.from(members.values()).sort((a, b) => a.tag.localeCompare(b.tag));
    return {
      key: ip,
      ip,
      members: sorted,
      unavailable:
        sorted.length > 0 &&
        sorted.every(
          (member) =>
            member.delay === "error" || (typeof member.delay === "number" && member.delay < 0),
        ),
    };
  }).sort((a, b) => a.ip.localeCompare(b.ip));

  return {
    endpoints,
    outboundCount,
    withoutLiteralIp,
    groupOutbounds,
    noServerOutbounds,
    hostnameServers,
  };
}
