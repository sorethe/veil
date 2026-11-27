<script lang="ts">
  import { onDestroy } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { api, type Conn, type Outbound } from "../api";
  import Icon from "../components/Icon.svelte";
  import StatusGlyph from "../components/StatusGlyph.svelte";
  import MembraneCanvas from "../components/membrane/Membrane.svelte";
  import { buildConstellation } from "../constellation";
  import {
    errorMessage,
    formatBytes,
    formatDelay,
    formatDuration,
    modeLabel,
    statusWord,
  } from "../format";
  import {
    parseDestination,
    scanProfileEndpoints,
    type MembraneDestination,
    type MembraneDestinationLink,
    type MembraneEndpoint,
    type MembraneSelection,
    type MembraneUnresolvedEndpoint,
  } from "../membrane-ip";
  import { outboundStore, type DelayReading } from "../outbounds.svelte";
  import { app } from "../state.svelte";

  const POLL_MS = 1800;
  const DESTINATION_CAP = 180;

  interface WorkingLink {
    endpointKey: string;
    outbounds: string[];
    connectionCount: number;
    upload: number;
    download: number;
  }

  interface WorkingDestination {
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
    links: SvelteMap<string, WorkingLink>;
  }

  let { navigate }: { navigate: (view: "overview") => void } = $props();

  let pageEl = $state<HTMLDivElement | null>(null);
  let pageVisible = $state(!document.hidden);
  let selected = $state<MembraneSelection | null>(null);
  let resolvedActiveId = $state<string | null>(null);
  let routeError = $state<string | null>(null);
  let routeKey: string | null = null;

  let activeConnections = $state(0);
  let unparsedConnections = $state(0);
  let connectionError = $state<string | null>(null);
  let lastPoll = $state(0);
  let now = $state(Date.now());
  let polling = $state(false);
  let destinationRevision = $state(0);

  const destinations = new SvelteMap<string, MembraneDestination>();
  const unresolvedTags = new SvelteSet<string>();

  const sourceProfile = $derived(
    app.engine.profileId !== null
      ? (app.profiles.find((profile) => profile.id === app.engine.profileId) ?? app.selectedProfile)
      : app.selectedProfile,
  );

  const endpointScan = $derived(scanProfileEndpoints(sourceProfile?.content ?? null));

  const model = $derived(
    buildConstellation({
      profile: sourceProfile,
      liveItems: outboundStore.items,
      connected: app.connected,
      selectedId: null,
      primaryGroup: null,
    }),
  );

  const activeId = $derived(
    resolvedActiveId && model.nodes.some((node) => node.id === resolvedActiveId)
      ? resolvedActiveId
      : model.activeId,
  );

  const endpoints: MembraneEndpoint[] = $derived.by(() =>
    endpointScan.endpoints.map((endpoint) => {
      const members = endpoint.members.map((member) => ({
        tag: member.tag,
        type: member.type,
        delay: delayForTag(member.tag),
      }));
      return {
        key: endpoint.key,
        ip: endpoint.ip,
        members,
        unavailable:
          members.length > 0 && members.every((member) => unavailableDelay(member.delay)),
      };
    }),
  );

  const endpointByTag = $derived.by(() => {
    const map = new SvelteMap<string, string>();
    for (const endpoint of endpoints) {
      for (const member of endpoint.members) map.set(member.tag, endpoint.key);
    }
    return map;
  });

  const unresolved: MembraneUnresolvedEndpoint[] = $derived.by(() => {
    const list: MembraneUnresolvedEndpoint[] = [];
    for (const tag of unresolvedTags) {
      const live = outboundStore.items.find((outbound) => outbound.tag === tag);
      const delay = delayForTag(tag);
      list.push({
        key: `tag:${tag}`,
        tag,
        type: live?.type ?? "not reported",
        delay,
        unavailable: unavailableDelay(delay),
      });
    }
    return list.sort((a, b) => a.tag.localeCompare(b.tag));
  });

  const activeEndpointKey = $derived.by(() => {
    if (!activeId) return null;
    return endpointByTag.get(activeId) ?? (unresolvedTags.has(activeId) ? `tag:${activeId}` : null);
  });

  const destinationList = $derived.by(() => {
    void destinationRevision;
    return Array.from(destinations.values()).sort(
      (a, b) =>
        b.connectionCount - a.connectionCount ||
        b.download - a.download ||
        a.ip.localeCompare(b.ip),
    );
  });

  const renderDestinations = $derived.by(() => {
    if (destinationList.length <= DESTINATION_CAP) {
      return [...destinationList].sort((a, b) => a.ip.localeCompare(b.ip));
    }
    const selectedKey = selected?.kind === "destination" ? selected.key : null;
    const chosen: MembraneDestination[] = [];
    if (selectedKey) {
      const current = destinationList.find((destination) => destination.key === selectedKey);
      if (current) chosen.push(current);
    }
    for (const destination of destinationList) {
      if (chosen.length >= DESTINATION_CAP) break;
      if (destination.key === selectedKey) continue;
      chosen.push(destination);
    }
    return chosen.sort((a, b) => a.ip.localeCompare(b.ip));
  });

  const hiddenDestinations = $derived(
    Math.max(0, destinationList.length - renderDestinations.length),
  );

  const selectedDestination = $derived.by(() => {
    const current = selected;
    return current?.kind === "destination"
      ? (destinationList.find((destination) => destination.key === current.key) ?? null)
      : null;
  });

  const selectedEndpoint = $derived.by(() => {
    const current = selected;
    if (current?.kind !== "endpoint") return null;
    return (
      endpoints.find((endpoint) => endpoint.key === current.key) ??
      unresolved.find((endpoint) => endpoint.key === current.key) ??
      null
    );
  });

  const activeProfileName = $derived(
    app.connected
      ? (app.engine.profileName ?? sourceProfile?.name ?? "Not reported")
      : (sourceProfile?.name ?? "No profile selected"),
  );

  const engineLabel = $derived(
    `${statusWord(app.engine.status)}${app.engine.mode ? ` · ${modeLabel(app.engine.mode)}` : ""}`,
  );

  const activeOutbound = $derived(activeId ?? "not reported");

  const dataNote = $derived(
    outboundStore.loading
      ? "Reading outbounds from the engine…"
      : outboundStore.error
        ? outboundStore.error
        : model.source === "profile"
          ? (model.reason ?? "Engine is not connected; showing the stored profile.")
          : model.source === "empty"
            ? (model.reason ?? "No outbounds available.")
            : null,
  );

  const routeNote = $derived(
    app.connected && routeError !== null
      ? "Route lookup failed; the active outbound falls back to the engine selector snapshot."
      : null,
  );

  const pollLabel = $derived(
    !app.connected ? "stopped" : !pageVisible ? "paused" : polling ? "reading…" : "polling 1.8s",
  );

  function delayForTag(tag: string): DelayReading {
    const local = outboundStore.delays[tag];
    if (local !== undefined) return local;
    const live = outboundStore.items.find((outbound) => outbound.tag === tag);
    return live ? live.delay : null;
  }

  function unavailableDelay(delay: DelayReading): boolean {
    return delay === "error" || (typeof delay === "number" && delay < 0);
  }

  function delayLabel(value: DelayReading | null): string {
    if (value === "error") return "unavailable";
    if (typeof value === "number") return formatDelay(value);
    return "not measured";
  }

  function joinList(values: string[] | number[]): string {
    return values.length ? values.join(", ") : "not reported";
  }

  function durationLabel(earliestStart: number): string {
    if (!earliestStart) return "not reported";
    return formatDuration(Math.max(0, now / 1000 - earliestStart));
  }

  function selectorFor(tag: string): Outbound | null {
    if (!app.connected) return null;
    const node = model.nodes.find((candidate) => candidate.id === tag);
    if (!node || !node.live) return null;
    const group = outboundStore.groups.find((candidate) => candidate.tag === node.group);
    return group?.kind === "selector" ? group : null;
  }

  function requestOutbound(tag: string): void {
    const group = selectorFor(tag);
    if (!group || group.now === tag) return;
    void outboundStore.select(group, tag);
  }

  function selectEndpoint(key: string): void {
    selected = { kind: "endpoint", key };
  }

  function selectDestination(key: string): void {
    selected = { kind: "destination", key };
  }

  function tagKey(tag: string): string {
    return `tag:${tag}`;
  }

  function endpointKeyForOutbound(tag: string): string | null {
    if (!tag) return null;
    const endpoint = endpointByTag.get(tag);
    if (endpoint) {
      if (unresolvedTags.has(tag)) unresolvedTags.delete(tag);
      return endpoint;
    }
    unresolvedTags.add(tag);
    return tagKey(tag);
  }

  function addUnique(list: string[], value: string): void {
    if (value && !list.includes(value)) list.push(value);
  }

  function addUniquePort(list: number[], value: number): void {
    if (!list.includes(value)) list.push(value);
  }

  function applyConnections(rows: Conn[]): void {
    const grouped = new SvelteMap<string, WorkingDestination>();
    let unparsed = 0;

    for (const conn of rows) {
      const parsed = parseDestination(conn.destination);
      if (!parsed) {
        unparsed++;
        continue;
      }

      let destination = grouped.get(parsed.ip);
      if (!destination) {
        destination = {
          ip: parsed.ip,
          ports: [],
          networks: [],
          protocols: [],
          hosts: [],
          processes: [],
          outbounds: [],
          connectionCount: 0,
          upload: 0,
          download: 0,
          earliestStart: 0,
          links: new SvelteMap<string, WorkingLink>(),
        };
        grouped.set(parsed.ip, destination);
      }

      if (parsed.port !== null) addUniquePort(destination.ports, parsed.port);
      addUnique(destination.networks, conn.network);
      addUnique(destination.protocols, conn.protocol);
      addUnique(destination.hosts, conn.host);
      addUnique(destination.processes, conn.process);
      addUnique(destination.outbounds, conn.outbound);
      destination.connectionCount++;
      destination.upload += Math.max(0, conn.upload);
      destination.download += Math.max(0, conn.download);
      if (
        conn.start > 0 &&
        (destination.earliestStart === 0 || conn.start < destination.earliestStart)
      ) {
        destination.earliestStart = conn.start;
      }

      const endpointKey = endpointKeyForOutbound(conn.outbound);
      if (!endpointKey) continue;
      let link = destination.links.get(endpointKey);
      if (!link) {
        link = {
          endpointKey,
          outbounds: [],
          connectionCount: 0,
          upload: 0,
          download: 0,
        };
        destination.links.set(endpointKey, link);
      }
      addUnique(link.outbounds, conn.outbound);
      link.connectionCount++;
      link.upload += Math.max(0, conn.upload);
      link.download += Math.max(0, conn.download);
    }

    const seen = new SvelteSet<string>();
    for (const [ip, work] of grouped) {
      seen.add(ip);
      work.ports.sort((a, b) => a - b);
      work.networks.sort();
      work.protocols.sort();
      work.hosts.sort();
      work.processes.sort();
      work.outbounds.sort();
      const links: MembraneDestinationLink[] = Array.from(work.links.values())
        .map((link) => ({ ...link, outbounds: [...link.outbounds].sort() }))
        .sort((a, b) => a.endpointKey.localeCompare(b.endpointKey));

      const existing = destinations.get(ip);
      if (existing) {
        existing.ports = work.ports;
        existing.networks = work.networks;
        existing.protocols = work.protocols;
        existing.hosts = work.hosts;
        existing.processes = work.processes;
        existing.outbounds = work.outbounds;
        existing.connectionCount = work.connectionCount;
        existing.upload = work.upload;
        existing.download = work.download;
        existing.earliestStart = work.earliestStart;
        existing.links = links;
      } else {
        destinations.set(ip, {
          key: ip,
          ip,
          ports: work.ports,
          networks: work.networks,
          protocols: work.protocols,
          hosts: work.hosts,
          processes: work.processes,
          outbounds: work.outbounds,
          connectionCount: work.connectionCount,
          upload: work.upload,
          download: work.download,
          earliestStart: work.earliestStart,
          links,
        });
      }
    }

    for (const key of destinations.keys()) {
      if (!seen.has(key)) destinations.delete(key);
    }
    destinationRevision++;
    unparsedConnections = unparsed;
    activeConnections = rows.length;

    if (selected?.kind === "destination" && !destinations.has(selected.key)) selected = null;
    if (
      selected?.kind === "endpoint" &&
      selected.key.startsWith("tag:") &&
      !unresolvedTags.has(selected.key.slice(4))
    ) {
      selected = null;
    }
  }

  function resetConnectionState(): void {
    activeConnections = 0;
    unparsedConnections = 0;
    connectionError = null;
    lastPoll = 0;
    polling = false;
    destinations.clear();
    unresolvedTags.clear();
    destinationRevision++;
    if (
      selected?.kind === "destination" ||
      (selected?.kind === "endpoint" && selected.key.startsWith("tag:"))
    ) {
      selected = null;
    }
  }

  $effect(() => {
    const element = pageEl;
    if (!element) return;
    let inView = true;
    const observer = new IntersectionObserver(
      ([entry]) => {
        inView = entry.isIntersecting;
        pageVisible = !document.hidden && inView;
      },
      { threshold: 0 },
    );
    observer.observe(element);
    const visibility = () => {
      pageVisible = !document.hidden && inView;
    };
    document.addEventListener("visibilitychange", visibility);
    pageVisible = !document.hidden;
    return () => {
      observer.disconnect();
      document.removeEventListener("visibilitychange", visibility);
    };
  });

  $effect(() => {
    const live = app.connected;
    const visible = pageVisible;
    if (!live) {
      resetConnectionState();
      return;
    }
    if (!visible) {
      polling = false;
      return;
    }

    let cancelled = false;
    let inFlight = false;
    const load = async () => {
      if (inFlight) return;
      inFlight = true;
      polling = true;
      try {
        const rows = await api.getConnections();
        if (cancelled || !app.connected) return;
        connectionError = null;
        now = Date.now();
        lastPoll = now;
        applyConnections(rows);
      } catch (cause) {
        if (!cancelled) connectionError = errorMessage(cause);
      } finally {
        inFlight = false;
        if (!cancelled) polling = false;
      }
    };

    void load();
    const timer = window.setInterval(() => void load(), POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
      polling = false;
    };
  });

  $effect(() => {
    if (!app.connected || !activeId) return;
    if (endpointByTag.has(activeId)) {
      if (unresolvedTags.has(activeId)) unresolvedTags.delete(activeId);
    } else {
      unresolvedTags.add(activeId);
    }
  });

  $effect(() => {
    const current = selected;
    if (current?.kind !== "endpoint") return;
    const exists =
      endpoints.some((endpoint) => endpoint.key === current.key) ||
      unresolved.some((endpoint) => endpoint.key === current.key);
    if (!exists) selected = null;
  });

  $effect(() => {
    const key = `${app.engine.status}:${app.engine.startedAt ?? ""}`;
    if (!app.connected) {
      resolvedActiveId = null;
      routeError = null;
      routeKey = null;
      return;
    }
    if (routeKey === key) return;
    routeKey = key;
    void (async () => {
      try {
        const route = await api.getRoute();
        if (routeKey === key) {
          resolvedActiveId = route.final;
          routeError = null;
        }
      } catch (cause) {
        if (routeKey === key) {
          resolvedActiveId = null;
          routeError = errorMessage(cause);
        }
      }
    })();
  });

  onDestroy(() => {
    destinations.clear();
    unresolvedTags.clear();
  });
</script>

<div class="membrane-page" bind:this={pageEl}>
  <header class="membrane-head">
    <div class="membrane-head__text">
      <span class="kicker">Live feature</span>
      <h1 class="page-title">Network membrane</h1>
      <p class="page-desc">
        Literal server IPs form the outer endpoint shell; observed connections place destinations on
        an inner layer. Edges are logical, not physical hops.
      </p>
    </div>
    <div class="membrane-head__meta">
      <span class="tag tag--accent">{pollLabel}</span>
      <span class="tag mono">{model.source}</span>
    </div>
  </header>

  <div class="membrane-layout">
    <section class="membrane-stage panel" aria-label="Network membrane canvas">
      <MembraneCanvas
        {endpoints}
        {unresolved}
        destinations={renderDestinations}
        {activeEndpointKey}
        {selected}
        status={app.engine.status}
        onselect={(next) => (selected = next)}
      />
    </section>

    <aside class="membrane-panel panel" aria-label="Membrane details">
      <div class="panel__head">
        <span class="panel__title">Observed layer</span>
        <span class="membrane-badge">Live</span>
      </div>

      <div class="membrane-panel__scroll">
        <section class="membrane-section" aria-label="Session">
          <h2 class="membrane-section__title">Session</h2>
          <dl class="membrane-kv">
            <div>
              <dt>Engine</dt>
              <dd class="mono membrane-engine st-{app.engine.status}">
                <StatusGlyph status={app.engine.status} size={10} />
                {engineLabel}
              </dd>
            </div>
            <div>
              <dt>Profile</dt>
              <dd class="truncate" title={activeProfileName}>{activeProfileName}</dd>
            </div>
            <div>
              <dt>Active outbound</dt>
              <dd class="mono truncate" title={activeOutbound}>{activeOutbound}</dd>
            </div>
            <div>
              <dt>Endpoints</dt>
              <dd class="mono">{endpoints.length} IPs</dd>
            </div>
            <div>
              <dt>Destinations</dt>
              <dd class="mono">{destinationList.length}</dd>
            </div>
            <div>
              <dt>Connections</dt>
              <dd class="mono">{activeConnections}</dd>
            </div>
            <div>
              <dt>Unresolved</dt>
              <dd class="mono">{unresolved.length}</dd>
            </div>
            <div>
              <dt>No literal IP</dt>
              <dd class="mono">{endpointScan.withoutLiteralIp} outbounds</dd>
            </div>
          </dl>
          {#if dataNote}
            <p class="membrane-note" title={dataNote}>{dataNote}</p>
          {/if}
          {#if routeNote}
            <p class="membrane-note" title={routeError ?? undefined}>{routeNote}</p>
          {/if}
          {#if connectionError}
            <p class="membrane-note" title={connectionError} role="alert">
              Connection read failed: {connectionError}
            </p>
          {/if}
          {#if unparsedConnections > 0}
            <p class="membrane-note">
              {unparsedConnections} active connection{unparsedConnections === 1 ? "" : "s"} had no parseable
              destination IP and are not drawn.
            </p>
          {/if}
          {#if lastPoll > 0}
            <p class="membrane-note mono">
              Last connection read: {new Date(lastPoll).toLocaleTimeString([], {
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
                hour12: false,
              })}
            </p>
          {/if}
        </section>

        {#if selectedDestination}
          <section class="membrane-section" aria-label="Selected destination">
            <div class="membrane-section__head">
              <h2 class="membrane-section__title">Selected destination</h2>
              <span class="tag">{selectedDestination.connectionCount} conn</span>
            </div>
            <p class="membrane-selected__tag truncate" title={selectedDestination.ip}>
              {selectedDestination.ip}
            </p>
            <dl class="membrane-kv">
              <div>
                <dt>Ports</dt>
                <dd class="mono">{joinList(selectedDestination.ports)}</dd>
              </div>
              <div>
                <dt>Networks</dt>
                <dd class="mono">{joinList(selectedDestination.networks)}</dd>
              </div>
              <div>
                <dt>Protocols</dt>
                <dd class="mono">{joinList(selectedDestination.protocols)}</dd>
              </div>
              <div>
                <dt>Hosts</dt>
                <dd class="truncate" title={joinList(selectedDestination.hosts)}>
                  {joinList(selectedDestination.hosts)}
                </dd>
              </div>
              <div>
                <dt>Processes</dt>
                <dd class="truncate" title={joinList(selectedDestination.processes)}>
                  {joinList(selectedDestination.processes)}
                </dd>
              </div>
              <div>
                <dt>Outbounds</dt>
                <dd class="truncate" title={joinList(selectedDestination.outbounds)}>
                  {joinList(selectedDestination.outbounds)}
                </dd>
              </div>
              <div>
                <dt>Upload</dt>
                <dd class="mono">{formatBytes(selectedDestination.upload)}</dd>
              </div>
              <div>
                <dt>Download</dt>
                <dd class="mono">{formatBytes(selectedDestination.download)}</dd>
              </div>
              <div>
                <dt>Duration</dt>
                <dd class="mono">{durationLabel(selectedDestination.earliestStart)}</dd>
              </div>
              <div>
                <dt>Endpoints</dt>
                <dd class="mono">{selectedDestination.links.length}</dd>
              </div>
            </dl>
            {#if selectedDestination.links.length > 0}
              <ul class="member-list" aria-label="Destination endpoints">
                {#each selectedDestination.links as link (link.endpointKey)}
                  <li>
                    <button
                      type="button"
                      class="member-item"
                      onclick={() => (selected = { kind: "endpoint", key: link.endpointKey })}
                    >
                      <span class="member-item__tag truncate">
                        {link.endpointKey.startsWith("tag:")
                          ? link.endpointKey.slice(4)
                          : link.endpointKey}
                      </span>
                      <span class="member-item__meta mono">
                        {link.connectionCount} conn · {link.outbounds.join(", ") ||
                          "tag not reported"}
                      </span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </section>
        {:else if selectedEndpoint}
          <section class="membrane-section" aria-label="Selected endpoint">
            <div class="membrane-section__head">
              <h2 class="membrane-section__title">Selected endpoint</h2>
              <span class="tag">{"ip" in selectedEndpoint ? "IP" : "unresolved"}</span>
            </div>
            <p
              class="membrane-selected__tag truncate"
              title={"ip" in selectedEndpoint ? selectedEndpoint.ip : selectedEndpoint.tag}
            >
              {"ip" in selectedEndpoint ? selectedEndpoint.ip : selectedEndpoint.tag}
            </p>
            {#if "ip" in selectedEndpoint}
              {@const unavailableMembers = selectedEndpoint.members.filter((member) =>
                unavailableDelay(member.delay),
              ).length}
              <p class="membrane-note">
                {selectedEndpoint.members.length} outbound{selectedEndpoint.members.length === 1
                  ? ""
                  : "s"} share this literal server IP.
              </p>
              {#if unavailableMembers > 0 && unavailableMembers < selectedEndpoint.members.length}
                <p class="membrane-note">
                  Partial availability: {unavailableMembers} of {selectedEndpoint.members.length}
                  outbound tags report unavailable; other members remain measured, so this endpoint is
                  not marked unavailable.
                </p>
              {/if}
              <ul class="member-list" aria-label="Outbounds at this endpoint">
                {#each selectedEndpoint.members as member (member.tag)}
                  {@const group = selectorFor(member.tag)}
                  <li class="member-row">
                    <div class="member-row__identity">
                      <span class="member-item__tag truncate" title={member.tag}>{member.tag}</span>
                      <span class="member-item__meta mono">
                        {member.type} · {delayLabel(member.delay)}
                      </span>
                    </div>
                    {#if group && group.now !== member.tag}
                      <button
                        type="button"
                        class="btn btn--sm btn--quiet"
                        onclick={() => requestOutbound(member.tag)}
                        aria-label={`Select ${member.tag} in ${group.tag}`}
                      >
                        Select
                      </button>
                    {:else if activeId === member.tag}
                      <span class="tag tag--accent">active</span>
                    {:else if group}
                      <span class="tag">selected</span>
                    {:else}
                      <span class="tag tag--plain">inspect</span>
                    {/if}
                  </li>
                {/each}
              </ul>
            {:else}
              <dl class="membrane-kv">
                <div>
                  <dt>Type</dt>
                  <dd class="mono">{selectedEndpoint.type}</dd>
                </div>
                <div>
                  <dt>Delay</dt>
                  <dd class="mono">{delayLabel(selectedEndpoint.delay)}</dd>
                </div>
                <div>
                  <dt>State</dt>
                  <dd class="mono">unresolved endpoint · real tag only</dd>
                </div>
              </dl>
              <p class="membrane-note">
                No literal server IP is available for this outbound tag; no IP was invented.
              </p>
              <div class="membrane-actions">
                {#if selectorFor(selectedEndpoint.tag) && selectorFor(selectedEndpoint.tag)?.now !== selectedEndpoint.tag}
                  <button
                    type="button"
                    class="btn btn--sm"
                    onclick={() => requestOutbound(selectedEndpoint.tag)}
                  >
                    Select in group
                  </button>
                {/if}
              </div>
            {/if}
            <p class="membrane-note">
              Endpoint selection is real profile data. Outbound changes remain backend-confirmed.
            </p>
          </section>
        {:else}
          <section class="membrane-section">
            <h2 class="membrane-section__title">Selection</h2>
            <p class="membrane-note">
              Click a ringed endpoint or an inner destination dot, or use the lists below. No
              connection history is stored.
            </p>
          </section>
        {/if}

        <section class="membrane-section" aria-label="Real endpoints">
          <div class="membrane-section__head">
            <h2 class="membrane-section__title">Endpoints</h2>
            <span class="membrane-count mono">{endpoints.length} IPs</span>
          </div>
          {#if endpoints.length > 0 || unresolved.length > 0}
            <ul class="membrane-list">
              {#each endpoints as endpoint (endpoint.key)}
                <li>
                  <button
                    type="button"
                    class="membrane-list__item"
                    class:membrane-list__item--active={activeEndpointKey === endpoint.key}
                    class:membrane-list__item--selected={selected?.kind === "endpoint" &&
                      selected.key === endpoint.key}
                    aria-pressed={selected?.kind === "endpoint" && selected.key === endpoint.key}
                    onclick={() => selectEndpoint(endpoint.key)}
                  >
                    <span
                      class="membrane-list__marker membrane-list__marker--endpoint"
                      aria-hidden="true"
                    ></span>
                    <span class="membrane-list__name mono truncate">{endpoint.ip}</span>
                    <span class="membrane-list__meta small muted">
                      {endpoint.members.length} tags
                    </span>
                  </button>
                </li>
              {/each}
              {#each unresolved as endpoint (endpoint.key)}
                <li>
                  <button
                    type="button"
                    class="membrane-list__item"
                    class:membrane-list__item--active={activeEndpointKey === endpoint.key}
                    class:membrane-list__item--selected={selected?.kind === "endpoint" &&
                      selected.key === endpoint.key}
                    aria-pressed={selected?.kind === "endpoint" && selected.key === endpoint.key}
                    onclick={() => selectEndpoint(endpoint.key)}
                  >
                    <span
                      class="membrane-list__marker membrane-list__marker--unresolved"
                      aria-hidden="true"
                    ></span>
                    <span class="membrane-list__name mono truncate">{endpoint.tag}</span>
                    <span class="membrane-list__meta small muted">unresolved</span>
                  </button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="membrane-note">No outbound with a literal server IP in the stored profile.</p>
          {/if}
          <p class="membrane-note">
            {endpointScan.withoutLiteralIp} of {endpointScan.outboundCount} outbounds have no literal
            server IP ({endpointScan.groupOutbounds} groups, {endpointScan.hostnameServers} hostnames,
            {endpointScan.noServerOutbounds} without server). Hostnames are not resolved.
          </p>
        </section>

        <section class="membrane-section" aria-label="Active destinations">
          <div class="membrane-section__head">
            <h2 class="membrane-section__title">Destinations</h2>
            <span class="membrane-count mono">{destinationList.length}</span>
          </div>
          {#if renderDestinations.length > 0}
            <ul class="membrane-list">
              {#each renderDestinations as destination (destination.key)}
                <li>
                  <button
                    type="button"
                    class="membrane-list__item"
                    class:membrane-list__item--selected={selected?.kind === "destination" &&
                      selected.key === destination.key}
                    aria-pressed={selected?.kind === "destination" &&
                      selected.key === destination.key}
                    onclick={() => selectDestination(destination.key)}
                  >
                    <span
                      class="membrane-list__marker membrane-list__marker--destination"
                      aria-hidden="true"
                    ></span>
                    <span class="membrane-list__name mono truncate">{destination.ip}</span>
                    <span class="membrane-list__meta small muted">
                      {destination.connectionCount} conn
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="membrane-note">
              No active destinations are drawn. Idle sessions legitimately have none.
            </p>
          {/if}
          {#if hiddenDestinations > 0}
            <p class="membrane-note">
              {hiddenDestinations} destination{hiddenDestinations === 1 ? "" : "s"} not drawn (cap
              {DESTINATION_CAP}); totals above are real.
            </p>
          {/if}
        </section>

        <section class="membrane-section" aria-label="Legend">
          <h2 class="membrane-section__title">Legend</h2>
          <ul class="membrane-legend">
            <li>
              <span class="legend-swatch legend-swatch--point" aria-hidden="true"></span>
              <span>Membrane points · decorative background</span>
            </li>
            <li>
              <span class="legend-swatch legend-swatch--center" aria-hidden="true"></span>
              <span>YOU · local center, not a server</span>
            </li>
            <li>
              <span class="legend-swatch legend-swatch--endpoint" aria-hidden="true"></span>
              <span>Endpoint · literal server IP, grouped tags</span>
            </li>
            <li>
              <span class="legend-swatch legend-swatch--destination" aria-hidden="true"></span>
              <span>Destination · observed connection IP</span>
            </li>
            <li>
              <span class="legend-swatch legend-swatch--unresolved" aria-hidden="true"></span>
              <span>Unresolved endpoint · real outbound tag, no invented IP</span>
            </li>
            <li>
              <span class="legend-swatch legend-swatch--path" aria-hidden="true"></span>
              <span>Illuminated logical edge · no physical hop data</span>
            </li>
          </ul>
          <p class="membrane-legend__count mono">
            {endpoints.length} endpoints · {destinationList.length} destinations · {activeConnections}
            connections
          </p>
        </section>
      </div>

      <div class="panel__foot membrane-panel__foot">
        <p class="membrane-note">
          Polling stops when this page is hidden, disconnected, or unmounted. No IP history is
          persisted.
        </p>
        <button
          type="button"
          class="btn btn--primary membrane-back"
          onclick={() => navigate("overview")}
        >
          <Icon name="overview" size={13} />
          Back to observatory
        </button>
      </div>
    </aside>
  </div>
</div>

<style>
  .membrane-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    height: calc(100dvh - var(--topbar-h) - var(--status-h) - var(--space-6) - var(--space-7));
    min-height: 520px;
    min-width: 0;
  }

  .membrane-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-4);
    flex-wrap: wrap;
    flex: none;
  }

  .membrane-head__text {
    display: grid;
    gap: 4px;
    max-width: 78ch;
  }

  .membrane-head .page-title {
    font-size: var(--fs-xl);
  }

  .membrane-head .page-desc {
    font-size: var(--fs-sm);
  }

  .membrane-head__meta {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .membrane-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 350px;
    gap: var(--space-4);
    flex: 1;
    min-height: 0;
    min-width: 0;
  }

  .membrane-stage {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .membrane-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .membrane-panel__scroll {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4);
  }

  .membrane-panel__foot {
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-2);
  }

  .membrane-back {
    width: 100%;
  }

  .membrane-badge {
    padding: 1px 7px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-pill);
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    font-weight: 500;
    letter-spacing: 0.1em;
    line-height: 1.5;
    text-transform: uppercase;
    color: var(--text);
  }

  .membrane-section {
    display: grid;
    gap: var(--space-2);
  }

  .membrane-section__head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .membrane-section__title {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    font-weight: 500;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .membrane-count {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  .membrane-kv {
    display: grid;
    gap: 6px;
  }

  .membrane-kv > div {
    display: grid;
    grid-template-columns: 92px minmax(0, 1fr);
    gap: var(--space-2);
    align-items: baseline;
    font-size: var(--fs-xs);
  }

  .membrane-kv dt {
    color: var(--muted);
  }

  .membrane-kv dd {
    min-width: 0;
    color: var(--text);
  }

  .membrane-engine {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .membrane-note {
    font-size: var(--fs-xs);
    line-height: 1.45;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .membrane-selected__tag {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    color: var(--text);
  }

  .membrane-actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .membrane-list {
    display: flex;
    flex-direction: column;
    max-height: 240px;
    overflow-y: auto;
    border-top: 1px solid var(--line);
  }

  .membrane-list__item {
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) auto;
    align-items: center;
    column-gap: var(--space-2);
    width: 100%;
    padding: 5px 2px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    color: var(--muted);
  }

  .membrane-list__item:hover {
    background: var(--row-hover);
    color: var(--text);
  }

  .membrane-list__item--selected {
    background: var(--raised);
    color: var(--text);
  }

  .membrane-list__marker {
    justify-self: center;
    flex: none;
  }

  .membrane-list__marker--endpoint {
    width: 7px;
    height: 7px;
    border: 1px solid var(--text);
    border-radius: 50%;
  }

  .membrane-list__marker--destination {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--muted);
  }

  .membrane-list__marker--unresolved {
    width: 6px;
    height: 6px;
    border: 1px dashed var(--muted);
    transform: rotate(45deg);
  }

  .membrane-list__item--active .membrane-list__marker--endpoint,
  .membrane-list__item--active .membrane-list__marker--unresolved {
    border-color: var(--accent);
    background: var(--accent);
  }

  .membrane-list__name {
    font-size: var(--fs-xs);
    color: inherit;
  }

  .membrane-list__meta {
    font-size: var(--fs-2xs);
    white-space: nowrap;
  }

  .member-list {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--line);
  }

  .member-row,
  .member-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    padding: 5px 2px;
    border-bottom: 1px solid var(--line);
    text-align: left;
  }

  .member-item:hover {
    background: var(--row-hover);
  }

  .member-row__identity {
    display: grid;
    gap: 1px;
    min-width: 0;
  }

  .member-item__tag {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--text);
  }

  .member-item__meta {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  .membrane-legend {
    display: grid;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .membrane-legend li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .legend-swatch {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 12px;
    height: 12px;
  }

  .legend-swatch--point::before {
    content: "";
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--faint);
  }

  .legend-swatch--center::before {
    content: "";
    width: 7px;
    height: 7px;
    border: 1px solid var(--muted);
    border-radius: 50%;
  }

  .legend-swatch--endpoint {
    width: 9px;
    height: 9px;
    border: 1px solid var(--text);
    border-radius: 50%;
  }

  .legend-swatch--endpoint::after {
    content: "";
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--text);
  }

  .legend-swatch--destination::before {
    content: "";
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--muted);
  }

  .legend-swatch--unresolved::before {
    content: "";
    width: 7px;
    height: 7px;
    border: 1px dashed var(--muted);
    transform: rotate(45deg);
  }

  .legend-swatch--path {
    width: 18px;
  }

  .legend-swatch--path::before {
    content: "";
    width: 18px;
    height: 1px;
    background: color-mix(in srgb, var(--accent) 70%, var(--muted));
  }

  .membrane-legend__count {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  @media (max-width: 1080px) {
    .membrane-page {
      height: calc(100dvh - var(--topbar-h) - var(--status-h) - var(--space-5) - var(--space-7));
    }

    .membrane-layout {
      grid-template-columns: minmax(0, 1fr) 310px;
    }

    .membrane-kv > div {
      grid-template-columns: 80px minmax(0, 1fr);
    }
  }

  @media (max-width: 940px) {
    .membrane-page {
      height: calc(100dvh - var(--topbar-h) - var(--status-h) - var(--space-4) - var(--space-6));
    }
  }

  @media (max-width: 860px) {
    .membrane-page {
      height: auto;
      min-height: 0;
    }

    .membrane-layout {
      grid-template-columns: minmax(0, 1fr);
    }

    .membrane-stage {
      height: min(62vh, 560px);
    }

    .membrane-list {
      max-height: none;
    }
  }
</style>
