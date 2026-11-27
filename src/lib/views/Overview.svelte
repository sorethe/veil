<script lang="ts">
  import { api, type Profile, type TunSetup } from "../api";
  import Icon from "../components/Icon.svelte";
  import Mark from "../components/Mark.svelte";
  import ProfileImport from "../components/ProfileImport.svelte";
  import Segmented from "../components/Segmented.svelte";
  import TunSetupDialog from "../components/TunSetupDialog.svelte";
  import Constellation from "../components/constellation/Constellation.svelte";
  import Inspector from "../components/constellation/Inspector.svelte";
  import ServerList from "../components/constellation/ServerList.svelte";
  import { buildConstellation } from "../constellation";
  import { copyText, errorMessage } from "../format";
  import { app, type View } from "../state.svelte";
  import { outboundStore } from "../outbounds.svelte";

  let { navigate }: { navigate: (view: View) => void } = $props();

  let importOpen = $state(false);
  let modeError = $state<string | null>(null);
  let setupOpen = $state(false);
  let setupIntent = $state<"enable" | "connect">("enable");
  let pendingConnect = $state(false);
  let tunSetup = $state<TunSetup | null>(null);
  let tunSetupError = $state<string | null>(null);
  let checkingSetup = $state(false);
  let finalizingSetup = $state(false);
  let setupSeq = 0;

  let viewMode = $state<"graph" | "list">("graph");
  let selectedNodeId = $state<string | null>(null);
  let resolvedActiveId = $state<string | null>(null);

  const mode = $derived(app.settings?.mode ?? "system");
  const locked = $derived(app.connected || app.busy);
  const developerMode = $derived(app.settings?.developerMode === true);

  const capabilities = $derived(app.capabilities);
  const engineReady = $derived(app.clashApiReady);
  const tunAvailable = $derived(app.tunReady);
  const systemProxySupported = $derived(app.platform?.systemProxySupported === true);
  const manualSession = $derived(app.manualSession);
  const manualCopy = $derived.by(() => {
    switch (app.manualSessionReason) {
      case "apply-error":
        return {
          title: "Desktop proxy not applied",
          body: "Veil could not apply this desktop's proxy settings. The session is manual-only; apps must be pointed at the manual proxy below. Automatic coverage is not active.",
        };
      case "unknown":
        return {
          title: "Manual proxy state unknown",
          body: "Veil adopted this system-mode session without a known desktop-proxy result. Treat it as manual-only; apps must use the manual proxy below. Automatic coverage is not confirmed.",
        };
      default:
        return {
          title: "Manual proxy session",
          body: "This desktop has no desktop-wide proxy API. Veil is live, but apps do not use it automatically; connections and traffic stay at zero until an app is pointed at the manual proxy.",
        };
    }
  });
  const modeReady = $derived(mode !== "tun" || tunAvailable);
  const tunHint = $derived(
    capabilities === null
      ? "Engine capabilities were not reported by the backend."
      : !capabilities.tun
        ? "Unavailable: the backend reports no TUN capability (missing /dev/net/tun or core)."
        : !capabilities.autoRoute
          ? "Unavailable: TUN exists but the backend reports no auto-route tooling (`ip`)."
          : !capabilities.dnsHijack
            ? "Unavailable: this core lacks the TUN DNS hijack support Veil requires."
            : "Route IPv4 and IPv6 at the interface.",
  );

  const canConnect = $derived(app.selectedProfile !== null && modeReady && engineReady);
  const disabledReason = $derived(
    app.connected
      ? null
      : !app.profiles.length
        ? "Import a profile to connect."
        : !app.selectedProfile
          ? "Select a profile to connect."
          : !engineReady
            ? capabilities === null
              ? "Engine capabilities were not reported by the backend."
              : "The engine core lacks the Clash API required by Veil."
            : !modeReady
              ? "The saved mode is TUN, but the backend reports TUN as unavailable. Choose System proxy."
              : finalizingSetup
                ? "Finishing TUN setup."
                : checkingSetup
                  ? "Checking TUN setup."
                  : app.connectBusy
                    ? "A connection command is already running."
                    : null,
  );

  const modeOptions = $derived([
    { value: "system", label: "System proxy", hint: "Apps that honor desktop proxy settings" },
    { value: "tun", label: "TUN", hint: tunHint, disabled: !tunAvailable },
  ]);

  const setupRequired = $derived(
    mode === "tun" && modeReady && (tunSetup ? !tunSetup.ready : tunSetupError !== null),
  );

  const mixedPort = $derived(
    app.engine.mixedPort ?? (app.connected ? null : (app.settings?.mixedPort ?? null)),
  );
  const proxyAddress = $derived(mixedPort === null ? "Unavailable" : `127.0.0.1:${mixedPort}`);
  const envBlock = $derived(
    mixedPort === null
      ? ""
      : [
          `export http_proxy=http://127.0.0.1:${mixedPort}`,
          `export https_proxy=http://127.0.0.1:${mixedPort}`,
          `export all_proxy=socks5h://127.0.0.1:${mixedPort}`,
        ].join("\n"),
  );

  const coreVersion = $derived(app.engineInfo?.version ?? app.core?.version ?? null);
  const corePath = $derived(app.engineInfo?.path ?? app.core?.path ?? null);
  const profileIdLabel = $derived(app.engine.profileId ?? app.selectedProfile?.id ?? "—");
  const modeSwitchLocked = $derived(locked || checkingSetup || finalizingSetup);
  const modeHint = $derived(
    locked
      ? "Disconnect to change profile or mode."
      : finalizingSetup
        ? "Finishing TUN setup."
        : checkingSetup
          ? "Checking TUN setup."
          : mode === "tun"
            ? "Interface routing; the helper and permissions are required."
            : "Apps that honor desktop proxy settings.",
  );

  const engineKey = $derived(`${app.engine.status}:${app.engine.startedAt ?? ""}`);

  const sourceProfile = $derived(
    app.engine.profileId !== null
      ? (app.profiles.find((p) => p.id === app.engine.profileId) ?? app.selectedProfile)
      : app.selectedProfile,
  );

  const inspectorProfileName = $derived(sourceProfile?.name ?? null);

  const model = $derived(
    buildConstellation({
      profile: sourceProfile,
      liveItems: outboundStore.items,
      connected: app.connected,
      selectedId: selectedNodeId,
      primaryGroup: null,
    }),
  );

  const constellationReason = $derived(
    outboundStore.loading
      ? "Reading outbounds from the engine…"
      : outboundStore.error
        ? outboundStore.error
        : model.reason,
  );

  const activeId = $derived(
    resolvedActiveId && model.nodes.some((node) => node.id === resolvedActiveId)
      ? resolvedActiveId
      : model.activeId,
  );

  $effect(() => {
    if (!app.connected) {
      resolvedActiveId = null;
      return;
    }
    const key = engineKey;
    void (async () => {
      try {
        const route = await api.getRoute();
        if (engineKey === key) {
          resolvedActiveId = route.final;
        }
      } catch {
        resolvedActiveId = null;
      }
    })();
  });

  $effect(() => {
    if (selectedNodeId && !model.nodes.some((n) => n.id === selectedNodeId)) {
      selectedNodeId = null;
    }
  });

  const selectedNode = $derived(model.nodes.find((node) => node.id === selectedNodeId) ?? null);
  const selectedCluster = $derived(
    selectedNode
      ? (model.clusters.find((cluster) => cluster.id === selectedNode.group) ?? null)
      : null,
  );

  const selectedGroup = $derived.by(() => {
    if (!selectedNode) return null;
    const cluster = model.clusters.find((c) => c.id === selectedNode.group);
    if (!cluster) return null;
    if (app.connected) {
      const liveGroup = outboundStore.groups.find((g) => g.tag === cluster.id);
      if (liveGroup) return { tag: liveGroup.tag, kind: liveGroup.kind, now: liveGroup.now };
    }
    return { tag: cluster.id, kind: cluster.kind, now: null };
  });

  const testing = $derived(outboundStore.testing === selectedNode?.id);
  const busy = $derived(app.busy || app.connectBusy || checkingSetup || finalizingSetup);
  const ontest = $derived(
    app.connected && selectedNode?.live
      ? () => {
          void outboundStore.test(selectedNode.id);
        }
      : null,
  );

  const provisioning = $derived(app.profiles.length === 0 && !app.connected);

  $effect(() => {
    if (mode !== "tun" || !modeReady) return;
    void refreshTunSetup();
  });

  async function refreshTunSetup(): Promise<TunSetup | null> {
    const seq = ++setupSeq;
    checkingSetup = true;
    try {
      const next = await api.getTunSetup();
      if (seq === setupSeq) {
        tunSetup = next;
        tunSetupError = null;
      }
      return next;
    } catch (cause) {
      if (seq === setupSeq) {
        tunSetup = null;
        tunSetupError = errorMessage(cause);
      }
      return null;
    } finally {
      if (seq === setupSeq) checkingSetup = false;
    }
  }

  function openTunSetup(intent: "enable" | "connect"): void {
    setupIntent = intent;
    pendingConnect = intent === "connect";
    setupOpen = true;
  }

  function closeTunSetup(): void {
    setupOpen = false;
    pendingConnect = false;
  }

  async function onTunReady(setup: TunSetup): Promise<void> {
    const shouldConnect = pendingConnect;
    setupOpen = false;
    pendingConnect = false;
    tunSetup = setup;
    tunSetupError = null;
    modeError = null;
    finalizingSetup = true;
    try {
      const error = await app.saveSettings({ mode: "tun" });
      if (error) {
        modeError = error;
        app.notify("error", "TUN mode not saved", error);
        return;
      }
      if (!shouldConnect) return;
      if (app.connected || app.busy || app.connectBusy) return;
      const profile = app.selectedProfile;
      if (!profile) {
        app.notify("error", "No profile selected", "Select a profile before connecting.");
        return;
      }
      await app.connect(profile.id, "tun");
    } finally {
      finalizingSetup = false;
    }
  }

  async function toggleConnection(): Promise<void> {
    if (app.busy || app.connectBusy || checkingSetup || finalizingSetup) return;
    if (app.connected || (app.engine.status === "error" && app.engine.pid !== null)) {
      await app.disconnect();
      return;
    }
    if (!app.selectedProfile) {
      app.notify("error", "No profile selected", "Import or select a profile before connecting.");
      return;
    }
    if (!engineReady) {
      app.notify(
        "error",
        "Engine is not ready",
        disabledReason ?? "The backend reports no Clash API support.",
      );
      return;
    }
    if (!modeReady) {
      app.notify("error", "TUN is unavailable", disabledReason ?? tunHint);
      return;
    }
    if (mode === "tun") {
      const setup = await refreshTunSetup();
      if (!setup?.ready) {
        openTunSetup("connect");
        return;
      }
    }
    await app.connect(app.selectedProfile.id, mode);
  }

  async function setMode(next: string): Promise<void> {
    if (next !== "system" && next !== "tun") return;
    modeError = null;
    if (next === "tun") {
      if (!tunAvailable) {
        modeError = tunHint;
        app.notify("error", "TUN is unavailable", tunHint);
        return;
      }
      const setup = await refreshTunSetup();
      if (!setup?.ready) {
        openTunSetup("enable");
        return;
      }
    }
    const error = await app.saveSettings({ mode: next });
    if (error) {
      modeError = error;
      app.notify("error", "Mode not changed", error);
    }
  }

  async function copy(label: string, text: string): Promise<void> {
    const ok = await copyText(text);
    if (ok) app.notify("success", `${label} copied`);
    else app.notify("error", "Copy failed", "The webview clipboard is not available.");
  }

  async function onImported(profile: Profile): Promise<void> {
    importOpen = false;
    await app.refreshProfiles();
    app.setSelectedProfile(profile.id);
  }

  function onKeydown(event: KeyboardEvent): void {
    if (!(event.key === "Enter" && (event.ctrlKey || event.metaKey))) return;
    if (importOpen || setupOpen) return;
    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.tagName === "TEXTAREA" || target.isContentEditable)
    )
      return;
    event.preventDefault();
    if (app.connected || canConnect) void toggleConnection();
  }

  function selectNode(id: string | null): void {
    selectedNodeId = id;
    if (id === null) return;
    if (!app.connected) return;
    const node = model.nodes.find((n) => n.id === id);
    if (!node) return;
    const cluster = model.clusters.find((c) => c.id === node.group);
    if (!cluster) return;
    if (cluster.kind === "selector") {
      const group = outboundStore.groups.find((g) => g.tag === cluster.id);
      if (group) void outboundStore.select(group, id);
    } else if (cluster.kind === "urltest") {
      app.notify(
        "info",
        "Automatic selection",
        "This group selects automatically; manual selection is ignored.",
      );
    }
  }

  async function onconnect(): Promise<void> {
    if (app.connected) return;
    await toggleConnection();
  }

  async function ondisconnect(): Promise<void> {
    if (!app.connected) return;
    await app.disconnect();
  }

  function toggleView(): void {
    viewMode = viewMode === "graph" ? "list" : "graph";
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if provisioning}
  <div class="page-head">
    <div class="page-head__text">
      <span class="kicker">Overview</span>
      <h1 class="page-title">First run</h1>
      <p class="page-desc">Veil is running but has no configuration to apply.</p>
    </div>
  </div>

  {#if app.engine.status === "error"}
    <div class="banner banner--error session-error" role="alert">
      <Icon name="error" size={16} />
      <div class="grow">
        <p class="banner__title">Engine error</p>
        <p class="banner__body mono">
          {app.engine.error ?? "The engine reported an error without a message."}
        </p>
      </div>
      <div class="row">
        {#if app.engine.pid !== null}
          <button
            type="button"
            class="btn btn--sm"
            disabled={app.connectBusy}
            onclick={() => void app.disconnect()}
          >
            Stop engine
          </button>
        {/if}
        <button type="button" class="btn btn--sm" onclick={() => navigate("diagnostics")}>
          Diagnostics
        </button>
      </div>
    </div>
  {/if}

  <section class="first-run panel">
    <div class="first-run__mark"><Mark size={42} /></div>
    <span class="kicker">No profiles</span>
    <h2 class="first-run__title">Import a configuration to begin.</h2>
    <p class="first-run__desc">
      Add a sing-box config, subscription, share links, or QR image. Veil validates it locally
      before saving.
    </p>
    <div class="first-run__actions">
      <button type="button" class="btn btn--primary" onclick={() => (importOpen = true)}>
        <Icon name="import" size={13} /> Import profile
      </button>
    </div>
  </section>
{:else}
  <div class="overview-shell">
    <div class="overview-toolbar">
      <div class="overview-toolbar__left" title={modeHint}>
        <Segmented
          value={mode}
          options={modeOptions}
          label="Proxy mode"
          disabled={modeSwitchLocked}
          onchange={(next) => void setMode(next)}
        />
      </div>
      <div class="overview-toolbar__right">
        <button
          type="button"
          class="btn btn--sm btn--quiet"
          title={viewMode === "graph" ? "Switch to server list" : "Switch to constellation graph"}
          aria-label={viewMode === "graph"
            ? "Switch to server list"
            : "Switch to constellation graph"}
          onclick={toggleView}
        >
          <Icon name={viewMode === "graph" ? "outbounds" : "overview"} size={13} />
          {viewMode === "graph" ? "List" : "Graph"}
        </button>
        {#if setupRequired}
          <button
            type="button"
            class="btn btn--sm btn--quiet"
            title={tunSetup ? tunSetup.missing.join("; ") : undefined}
            onclick={() => openTunSetup("enable")}
          >
            <Icon name="warning" size={11} /> Set up TUN
          </button>
        {/if}
      </div>
    </div>

    {#if app.engine.status === "error"}
      <div class="banner banner--error session-error" role="alert">
        <Icon name="error" size={16} />
        <div class="grow">
          <p class="banner__title">Engine error</p>
          <p class="banner__body mono">
            {app.engine.error ?? "The engine reported an error without a message."}
          </p>
        </div>
        <div class="row">
          {#if app.engine.pid !== null}
            <button
              type="button"
              class="btn btn--sm"
              disabled={app.connectBusy}
              title="Stop the failed engine process"
              onclick={() => void app.disconnect()}
            >
              Stop engine
            </button>
          {:else}
            <button
              type="button"
              class="btn btn--sm"
              disabled={!canConnect || app.connectBusy}
              title={disabledReason ?? undefined}
              onclick={() => void toggleConnection()}
            >
              Retry
            </button>
          {/if}
          <button type="button" class="btn btn--sm" onclick={() => navigate("diagnostics")}>
            Diagnostics
          </button>
        </div>
      </div>
    {/if}

    {#if manualSession}
      <div class="banner banner--accent manual-session" role="status">
        <Icon name="info" size={16} />
        <div class="grow">
          <p class="banner__title">{manualCopy.title}</p>
          <p class="banner__body">{manualCopy.body}</p>
          <div class="manual-session__row">
            <span class="kicker">Proxy</span>
            <span class="mono break">{proxyAddress}</span>
            <button
              type="button"
              class="btn btn--sm btn--quiet"
              disabled={mixedPort === null}
              onclick={() => void copy("Proxy address", proxyAddress)}
            >
              <Icon name="copy" size={11} /> Copy address
            </button>
          </div>
          {#if mixedPort === null}
            <p class="small muted">Proxy port unavailable.</p>
          {:else}
            <pre class="code manual-session__code">{envBlock}</pre>
            <button
              type="button"
              class="btn btn--sm btn--quiet"
              onclick={() => void copy("Environment block", envBlock)}
            >
              <Icon name="copy" size={11} /> Copy env
            </button>
          {/if}
        </div>
      </div>
    {/if}

    {#if modeError}
      <div class="banner banner--error" role="alert">
        <Icon name="error" size={16} />
        <div class="grow">
          <p class="banner__title">Mode error</p>
          <p class="banner__body">{modeError}</p>
        </div>
      </div>
    {/if}

    {#if !systemProxySupported && mode === "system"}
      <div class="banner banner--warning" role="status">
        <Icon name="warning" size={16} />
        <div class="grow">
          <p class="banner__title">System proxy unavailable</p>
          <p class="banner__body">
            This desktop reports no system-proxy backend. Use TUN or a manual proxy.
          </p>
        </div>
      </div>
    {/if}

    <div class="overview-content">
      <div class="overview-canvas">
        {#if viewMode === "graph"}
          <Constellation
            clusters={model.clusters}
            phase={app.engine.status}
            selectedId={selectedNodeId}
            {activeId}
            reason={constellationReason}
            onselect={selectNode}
          />
        {:else}
          <ServerList
            clusters={model.clusters}
            selectedId={selectedNodeId}
            {activeId}
            onselect={selectNode}
            emptyReason={constellationReason}
          />
        {/if}
      </div>
      <div class="overview-inspector">
        <Inspector
          node={selectedNode}
          cluster={selectedCluster}
          phase={app.engine.status}
          connected={app.connected}
          {activeId}
          profileName={inspectorProfileName}
          stats={app.stats}
          enginePid={app.engine.pid}
          {mixedPort}
          {selectedGroup}
          {testing}
          {busy}
          {onconnect}
          {ondisconnect}
          {ontest}
        />
        {#if developerMode}
          <details class="dev-details">
            <summary>Connection details</summary>
            <dl class="kv">
              <div class="kv__row">
                <dt>PID</dt>
                <dd class="mono">{app.engine.pid ?? "—"}</dd>
              </div>
              <div class="kv__row">
                <dt>Core version</dt>
                <dd class="mono">{coreVersion ?? "—"}</dd>
              </div>
              <div class="kv__row">
                <dt>Core path</dt>
                <dd class="mono break">{corePath ?? "—"}</dd>
              </div>
              <div class="kv__row">
                <dt>Mixed port</dt>
                <dd class="mono">{mixedPort ?? "—"}</dd>
              </div>
              <div class="kv__row">
                <dt>Profile ID</dt>
                <dd class="mono">{profileIdLabel}</dd>
              </div>
            </dl>
          </details>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if importOpen}
  <ProfileImport
    onclose={() => (importOpen = false)}
    onimported={(profile) => void onImported(profile)}
  />
{/if}

{#if setupOpen}
  <TunSetupDialog
    intent={setupIntent}
    onclose={closeTunSetup}
    onready={(setup) => void onTunReady(setup)}
  />
{/if}

<style>
  .overview-shell {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    height: 100%;
    min-height: 0;
  }

  .overview-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
    flex: none;
  }

  .overview-toolbar__left {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .overview-toolbar__right {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
  }

  .overview-content {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    gap: var(--space-4);
    flex: 1;
    min-height: 0;
    min-width: 0;
  }

  .overview-canvas {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .overview-inspector {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .dev-details {
    margin-top: var(--space-2);
    border-top: 1px solid var(--line);
    flex: none;
  }

  .dev-details summary {
    cursor: pointer;
    padding: var(--space-2) 0;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }

  .kv {
    display: grid;
    gap: var(--space-1);
    margin: var(--space-2) 0;
  }

  .kv__row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
    gap: var(--space-2);
    font-size: var(--fs-xs);
  }

  .kv__row dt {
    color: var(--muted);
  }

  .session-error {
    margin-bottom: var(--space-2);
    flex: none;
  }

  .manual-session {
    margin-bottom: var(--space-2);
    flex: none;
  }

  .manual-session__row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-top: var(--space-2);
  }

  .manual-session__code {
    margin-top: var(--space-2);
    white-space: pre-wrap;
  }

  .first-run {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-7) var(--space-5);
    text-align: center;
    color: var(--muted);
  }

  .first-run__mark {
    color: var(--faint);
    margin-bottom: var(--space-2);
  }

  .first-run__title {
    font-size: var(--fs-lg);
    color: var(--text);
  }

  .first-run__desc {
    font-size: var(--fs-sm);
    max-width: 52ch;
    line-height: 1.5;
  }

  .first-run__actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
    justify-content: center;
    margin-top: var(--space-2);
  }
</style>
