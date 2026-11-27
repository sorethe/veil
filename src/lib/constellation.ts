/**
 * Pure adapter from real engine/profile data to the constellation model.
 *
 * Nothing here invents servers, regions, latency or traffic. Live clusters come
 * from the engine's own selector/urltest groups; when idle we fall back to the
 * selected profile's stored sing-box JSON. Invalid JSON yields an empty model
 * with an honest reason instead of fabricated nodes.
 */
import type { Outbound, Profile } from "./api";

export interface ConstellationNode {
  id: string;
  label: string;
  type: string;
  group: string;
  delay: number | "error" | null;
  selected: boolean;
  live: boolean;
}

export interface ConstellationCluster {
  id: string;
  label: string;
  kind: "selector" | "urltest" | "ungrouped";
  nodes: ConstellationNode[];
}

export interface ConstellationModel {
  clusters: ConstellationCluster[];
  nodes: ConstellationNode[];
  activeId: string | null;
  selectedId: string | null;
  primaryGroup: string | null;
  source: "live" | "profile" | "empty";
  reason: string | null;
}

export interface ProfileGroup {
  tag: string;
  kind: "selector" | "urltest";
  options: string[];
}

export interface ProfileNode {
  id: string;
  label: string;
  type: string;
  group: string;
  delay: number | "error" | null;
}

/**
 * Parse the stored sing-box JSON of a profile. Defensive: never throws and
 * never fabricates a node from a malformed entry.
 */
export function parseProfileNodes(profile: Profile | null): {
  nodes: ProfileNode[];
  groups: ProfileGroup[];
  error: string | null;
} {
  if (!profile) return { nodes: [], groups: [], error: "No profile selected." };
  const raw = profile.content?.trim();
  if (!raw) return { nodes: [], groups: [], error: "Profile has no stored configuration." };

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { nodes: [], groups: [], error: "Profile configuration is not valid JSON." };
  }

  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { nodes: [], groups: [], error: "Profile configuration has no outbounds array." };
  }
  if (!("outbounds" in parsed) || !Array.isArray(parsed.outbounds)) {
    return { nodes: [], groups: [], error: "Profile configuration has no outbounds array." };
  }

  const outbounds: unknown[] = parsed.outbounds;
  const nodes: ProfileNode[] = [];
  const groups: ProfileGroup[] = [];
  const seen = new Set<string>();

  for (const entry of outbounds) {
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) continue;
    if (!("tag" in entry) || typeof entry.tag !== "string" || entry.tag.length === 0) continue;
    const tag = entry.tag;
    if (seen.has(tag)) continue;
    seen.add(tag);

    const type =
      "type" in entry && typeof entry.type === "string" && entry.type.length > 0
        ? entry.type
        : "unknown";
    const kind = type === "selector" || type === "urltest" ? type : null;
    const options =
      "outbounds" in entry && Array.isArray(entry.outbounds)
        ? entry.outbounds.filter((option): option is string => typeof option === "string")
        : [];

    nodes.push({ id: tag, label: tag, type, group: "", delay: null });
    if (kind) groups.push({ tag, kind, options });
  }

  if (nodes.length === 0) {
    return { nodes: [], groups: [], error: "Profile defines no outbounds." };
  }

  // Assign leaf nodes to the first real group that references them. A node may
  // appear in several groups; the first wins so clusters stay stable.
  const nodeByTag = new Map(nodes.map((node) => [node.id, node]));
  for (const group of groups) {
    for (const option of group.options) {
      const node = nodeByTag.get(option);
      if (node && node.group === "") node.group = group.tag;
    }
  }

  return { nodes, groups, error: null };
}

/** Build the constellation from either live engine data or the stored profile. */
export function buildConstellation(args: {
  profile: Profile | null;
  liveItems: Outbound[];
  connected: boolean;
  selectedId: string | null;
  primaryGroup?: string | null;
}): ConstellationModel {
  const { profile, liveItems, connected, selectedId } = args;

  if (connected && liveItems.length > 0) {
    const groups = liveItems.filter((item) => item.kind === "selector" || item.kind === "urltest");
    const byTag = new Map(liveItems.map((item) => [item.tag, item]));
    const groupTags = new Set(groups.map((group) => group.tag));
    const members = new Set<string>();
    for (const group of groups) for (const option of group.options) members.add(option);

    const groupNodes: ConstellationNode[] = [];
    for (const group of groups) {
      for (const tag of group.options) {
        const live = byTag.get(tag);
        groupNodes.push({
          id: tag,
          label: tag,
          type: live?.type ?? "unknown",
          group: group.tag,
          delay: live?.delay ?? null,
          selected: group.now === tag,
          live: true,
        });
      }
    }

    const ungrouped: ConstellationNode[] = liveItems
      .filter((item) => item.kind === "other" && !members.has(item.tag) && !groupTags.has(item.tag))
      .map((item) => ({
        id: item.tag,
        label: item.tag,
        type: item.type,
        group: "",
        delay: item.delay,
        selected: false,
        live: true,
      }));

    const seen = new Set<string>();
    const allNodes: ConstellationNode[] = [];
    for (const node of [...groupNodes, ...ungrouped]) {
      if (seen.has(node.id)) continue;
      seen.add(node.id);
      allNodes.push(node);
    }

    const selectedTags = new Set(
      groups.map((group) => group.now).filter((now): now is string => now !== null),
    );
    for (const node of allNodes) node.selected = selectedTags.has(node.id);

    const byGroup = new Map<string, ConstellationNode[]>();
    for (const node of allNodes) {
      const list = byGroup.get(node.group);
      if (list) list.push(node);
      else byGroup.set(node.group, [node]);
    }

    const primary = groups.find((group) => group.tag === args.primaryGroup) ?? groups[0] ?? null;

    const clusters: ConstellationCluster[] = groups.map((group) => ({
      id: group.tag,
      label: group.tag,
      kind: group.kind === "urltest" ? "urltest" : "selector",
      nodes: byGroup.get(group.tag) ?? [],
    }));

    const ungroupedNodes = byGroup.get("") ?? [];
    if (ungroupedNodes.length > 0) {
      clusters.push({
        id: "ungrouped",
        label: "Ungrouped",
        kind: "ungrouped",
        nodes: ungroupedNodes,
      });
    }

    return {
      clusters,
      nodes: allNodes,
      activeId: primary?.now ?? null,
      selectedId,
      primaryGroup: primary?.tag ?? null,
      source: "live",
      reason: null,
    };
  }

  const parsed = parseProfileNodes(profile);
  if (parsed.error || parsed.nodes.length === 0) {
    return {
      clusters: [],
      nodes: [],
      activeId: null,
      selectedId,
      primaryGroup: null,
      source: "empty",
      reason: parsed.error ?? "No outbounds available.",
    };
  }

  const groupTags = new Set(parsed.groups.map((group) => group.tag));
  const nodes: ConstellationNode[] = parsed.nodes
    .filter((node) => !(groupTags.has(node.id) && node.group === ""))
    .map((node) => ({
      ...node,
      selected: node.id === selectedId,
      live: false,
    }));

  const byGroup = new Map<string, ConstellationNode[]>();
  for (const node of nodes) {
    const list = byGroup.get(node.group);
    if (list) list.push(node);
    else byGroup.set(node.group, [node]);
  }

  const clusters: ConstellationCluster[] = parsed.groups.map((group) => ({
    id: group.tag,
    label: group.tag,
    kind: group.kind,
    nodes: byGroup.get(group.tag) ?? [],
  }));

  const ungrouped = byGroup.get("") ?? [];
  if (ungrouped.length > 0) {
    clusters.push({
      id: "ungrouped",
      label: "Ungrouped",
      kind: "ungrouped",
      nodes: ungrouped,
    });
  }

  const primary =
    parsed.groups.find((group) => group.kind === "selector") ?? parsed.groups[0] ?? null;

  return {
    clusters,
    nodes,
    activeId: null,
    selectedId,
    primaryGroup: primary?.tag ?? null,
    source: "profile",
    reason: "Engine is not connected; showing the stored profile.",
  };
}
