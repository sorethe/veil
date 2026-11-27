<script lang="ts">
  import { api, type Outbound, type Profile } from "../api";
  import EmptyState from "../components/EmptyState.svelte";
  import Field from "../components/Field.svelte";
  import Icon from "../components/Icon.svelte";
  import Instrument from "../components/Instrument.svelte";
  import Mark from "../components/Mark.svelte";
  import ProfileImport from "../components/ProfileImport.svelte";
  import ProfileMenu from "../components/ProfileMenu.svelte";
  import Segmented from "../components/Segmented.svelte";
  import { copyText, errorMessage, hostOf, modeLabel } from "../format";
  import { app, type View } from "../state.svelte";

  let { navigate }: { navigate: (view: View) => void } = $props();

  let importOpen = $state(false);
  let modeError = $state<string | null>(null);
  let selectedId = $state<string | null>(null);

  let outbounds = $state<Outbound[]>([]);
  let outboundsLoading = $state(false);
  let outboundsError = $state<string | null>(null);
  let groupTag = $state<string | null>(null);
  let switching = $state(false);
  let lastEngineKey = "";

  const selectedProfile = $derived(app.profiles.find((profile) => String(profile.id) === selectedId) ?? null);
  const profileName = $derived(
    app.engine.profileName ?? selectedProfile?.name ?? (app.profiles.length ? "Select a profile" : "No profile"),
  );
  const mode = $derived(app.settings?.mode ?? "system");
  const locked = $derived(app.connected || app.busy);
  const groups = $derived(outbounds.filter((outbound) => outbound.kind === "selector" || outbound.kind === "urltest"));
  const activeGroup = $derived(groups.find((group) => group.tag === groupTag) ?? null);

  const capabilities = $derived(app.capabilities);
  const engineReady = $derived(app.clashApiReady);
  const tunAvailable = $derived(app.tunReady);
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
            : "Route IPv4 and IPv6 at the interface. Needs the helper and permissions.",
  );

  const canConnect = $derived(selectedProfile !== null && modeReady && engineReady);
  const disabledReason = $derived(
    app.connected
      ? null
      : !app.profiles.length
        ? "Import a profile to connect."
        : !selectedProfile
          ? "Select a profile to connect."
          : !engineReady
            ? capabilities === null
              ? "Engine capabilities were not reported by the backend."
              : "The engine core lacks the Clash API required by Veil."
            : !modeReady
              ? "The saved mode is TUN, but the backend reports TUN as unavailable. Choose System proxy."
              : app.connectBusy
                ? "A connection command is already running."
                : null,
  );

  const modeOptions = $derived([
    { value: "system", label: "System proxy", hint: "Apps that honor desktop proxy settings" },
    {
      value: "tun",
      label: "TUN",
      hint: tunHint,
      disabled: !tunAvailable,
    },
  ]);

  const mixedPort = $derived(app.settings?.mixedPort ?? null);
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

  const mechanism = $derived.by(() => {
    const platform = app.platform;
    if (!platform) return "Desktop capability was not reported.";
    if (app.engine.proxy) return `${app.engine.proxy.kind}: ${app.engine.proxy.detail}`;
    if (app.connected && app.engine.mode === "tun") {
      return "TUN routes traffic at the interface; no desktop proxy settings are applied.";
    }
    if (!platform.systemProxySupported) {
      return `System proxy is not available on this desktop (${platform.desktop} · ${platform.session}).`;
    }
    return `Desktop mechanism: ${platform.systemProxyKind}. Applied when a system-proxy session starts.`;
  });

  $effect(() => {
    const engineProfile = app.engine.profileId;
    const profiles = app.profiles;
    if (engineProfile !== null && profiles.some((profile) => String(profile.id) === String(engineProfile))) {
      const next = String(engineProfile);
      if (selectedId !== next) selectedId = next;
      return;
    }
    if (!selectedId || !profiles.some((profile) => String(profile.id) === selectedId)) {
      selectedId = profiles[0] ? String(profiles[0].id) : null;
    }
  });

  $effect(() => {
    const key = `${app.engine.status}:${app.engine.startedAt ?? 0}`;
    if (key === lastEngineKey) return;
    lastEngineKey = key;
    if (app.engine.status === "connected") {
      void loadOutbounds();
    } else {
      outbounds = [];
      outboundsError = null;
      groupTag = null;
    }
  });

  async function loadOutbounds(): Promise<void> {
    if (app.engine.status !== "connected") return;
    outboundsLoading = true;
    try {
      const list = await api.getOutbounds();
      outbounds = list;
      outboundsError = null;
      const nextGroups = list.filter((outbound) => outbound.kind === "selector" || outbound.kind === "urltest");
      if (!groupTag || !nextGroups.some((group) => group.tag === groupTag)) {
        groupTag = nextGroups[0]?.tag ?? null;
      }
    } catch (cause) {
      outboundsError = errorMessage(cause);
      outbounds = [];
    } finally {
      outboundsLoading = false;
    }
  }

  async function toggleConnection(): Promise<void> {
    if (app.busy || app.connectBusy) return;
    if (app.connected || (app.engine.status === "error" && app.engine.pid !== null)) {
      await app.disconnect();
      return;
    }
    if (!selectedProfile) {
      app.notify("error", "No profile selected", "Import or select a profile before connecting.");
      return;
    }
    if (!engineReady) {
      app.notify("error", "Engine is not ready", disabledReason ?? "The backend reports no Clash API support.");
      return;
    }
    if (!modeReady) {
      app.notify("error", "TUN is unavailable", disabledReason ?? tunHint);
      return;
    }
    await app.connect(selectedProfile.id, app.settings?.mode ?? "system");
  }

  async function setMode(next: string): Promise<void> {
    if (next !== "system" && next !== "tun") return;
    modeError = null;
    const error = await app.saveSettings({ mode: next });
    if (error) {
      modeError = error;
      app.notify("error", "Mode not changed", error);
    }
  }

  async function switchOutbound(tag: string): Promise<void> {
    if (!activeGroup || switching) return;
    switching = true;
    try {
      await api.selectOutbound(activeGroup.tag, tag);
      await loadOutbounds();
      app.notify("success", "Outbound selected", `${activeGroup.tag} → ${tag}`);
    } catch (cause) {
      app.notify("error", "Could not switch outbound", errorMessage(cause));
    } finally {
      switching = false;
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
    selectedId = String(profile.id);
  }

  function onKeydown(event: KeyboardEvent): void {
    if (!(event.key === "Enter" && (event.ctrlKey || event.metaKey))) return;
    if (importOpen) return;
    const target = event.target;
    if (target instanceof HTMLElement && (target.tagName === "TEXTAREA" || target.isContentEditable)) return;
    event.preventDefault();
    if (app.connected || canConnect) void toggleConnection();
  }

  const provisioning = $derived(app.profiles.length === 0 && !app.connected);
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
        <p class="banner__body mono">{app.engine.error ?? "The engine reported an error without a message."}</p>
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
        <button type="button" class="btn btn--sm" onclick={() => navigate("diagnostics")}>Diagnostics</button>
      </div>
    </div>
  {/if}

  <section class="first-run panel">
    <div class="first-run__mark"><Mark size={42} /></div>
    <span class="kicker">No profiles</span>
    <h2 class="first-run__title">Add a configuration to open the veil.</h2>
    <p class="first-run__desc">
      Import a sing-box config, a subscription URL, share links, or a QR image. Everything is parsed and
      validated by the local engine before it is written.
    </p>
    <div class="first-run__actions">
      <button type="button" class="btn btn--primary" onclick={() => (importOpen = true)}>
        <Icon name="import" size={13} /> Import profile
      </button>
      <button type="button" class="btn" onclick={() => navigate("diagnostics")}>Run diagnostics</button>
    </div>
    <div class="first-run__facts">
      <span><span class="kicker">Core</span> {app.engineInfo?.version ?? app.core?.version ?? "unknown"}</span>
      <span><span class="kicker">Clash API</span> {capabilities ? (capabilities.clashApi ? "available" : "missing") : "unknown"}</span>
      <span><span class="kicker">Desktop</span> {app.platform?.desktop ?? "unknown"} · {app.platform?.session ?? "unknown"}</span>
      <span><span class="kicker">TUN</span> {capabilities ? (tunAvailable ? "available" : "unavailable") : "unknown"}</span>
      <span><span class="kicker">Tray</span> {app.platform?.trayAvailable ? "available" : "not available"}</span>
    </div>
  </section>
{:else}
  <div class="page-head">
    <div class="page-head__text">
      <span class="kicker">Overview</span>
      <h1 class="page-title">Session</h1>
      <p class="page-desc">One profile, one engine process, one honest view of the link.</p>
    </div>
    <div class="head-actions">
      <button type="button" class="btn" onclick={() => navigate("outbounds")}>Outbounds</button>
      <button type="button" class="btn" onclick={() => (importOpen = true)}>
        <Icon name="import" size={12} /> Import
      </button>
    </div>
  </div>

  <div class="controls">
    <ProfileMenu
      profiles={app.profiles}
      activeId={selectedId}
      disabled={locked}
      reason="Disconnect before switching profiles."
      onselect={(id) => (selectedId = id)}
      onmanage={() => navigate("profiles")}
    />

    <div class="mode">
      <span class="kicker">Default mode</span>
      <div class="mode__row">
        <Segmented
          value={mode}
          options={modeOptions}
          label="Proxy mode"
          disabled={locked}
          onchange={(next) => void setMode(next)}
        />
        {#if locked}<span class="mode__lock">Locked while a session is active</span>{/if}
      </div>
      <p class="mode__desc">
        System proxy covers applications that honor desktop proxy settings. TUN routes IPv4 and IPv6 at the
        interface and needs the helper plus permissions.
      </p>
      {#if !tunAvailable}
        <p class="mode__warn">{tunHint}{#if mode === "tun"} Choose System proxy before connecting.{/if}</p>
      {/if}
      {#if !engineReady}
        <p class="mode__warn" role="alert">
          {capabilities === null
            ? "Engine capabilities were not reported; connecting is disabled until the backend reports them."
            : "Connecting is disabled: the core lacks the Clash API Veil needs for health, stats, and outbounds."}
        </p>
      {/if}
      {#if modeError}<p class="mode__error" role="alert">{modeError}</p>{/if}
    </div>

    <div class="controls__facts">
      <div>
        <span class="kicker">Applied mode</span>
        <p class="mono">{app.engine.mode ? modeLabel(app.engine.mode) : "—"}</p>
      </div>
      <div>
        <span class="kicker">Profile source</span>
        <p class="mono truncate">
          {#if selectedProfile}
            {selectedProfile.kind === "subscription" && selectedProfile.subscriptionUrl
              ? hostOf(selectedProfile.subscriptionUrl)
              : selectedProfile.kind}
          {:else}
            —
          {/if}
        </p>
      </div>
    </div>
  </div>

  {#if app.engine.status === "error"}
    <div class="banner banner--error session-error" role="alert">
      <Icon name="error" size={16} />
      <div class="grow">
        <p class="banner__title">Engine error</p>
        <p class="banner__body mono">{app.engine.error ?? "The engine reported an error without a message."}</p>
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
        <button type="button" class="btn btn--sm" onclick={() => navigate("diagnostics")}>Diagnostics</button>
      </div>
    </div>
  {/if}

  <Instrument
    engine={app.engine}
    stats={app.stats}
    history={app.history}
    connectBusy={app.connectBusy}
    canConnect={canConnect}
    disabledReason={disabledReason}
    profileName={profileName}
    onToggle={() => void toggleConnection()}
  />

  <div class="lower">
    <section class="panel" aria-labelledby="quick-outbound-title">
      <div class="panel__head">
        <span class="panel__title" id="quick-outbound-title">Quick outbound</span>
        <button type="button" class="btn btn--sm btn--quiet" onclick={() => navigate("outbounds")}>
          All outbounds
        </button>
      </div>
      {#if !app.connected}
        <EmptyState
          title={app.engine.status === "starting" ? "Session is starting" : "Outbounds need a live session"}
          description="Selector and URL-test groups are read from the running engine."
          icon="outbounds"
        >
          <button type="button" class="btn btn--sm" onclick={() => navigate("outbounds")}>Open outbounds</button>
        </EmptyState>
      {:else if outboundsLoading}
        <div class="panel__body"><p class="muted small">Reading outbounds from the engine…</p></div>
      {:else if outboundsError}
        <div class="panel__body stack">
          <p class="danger small break">{outboundsError}</p>
          <button type="button" class="btn btn--sm" onclick={() => void loadOutbounds()}>Retry</button>
        </div>
      {:else if !groups.length}
        <EmptyState
          title="No selector groups"
          description="This configuration has no selector or URL-test outbounds to switch."
          icon="outbounds"
        />
      {:else if activeGroup}
        <div class="panel__body stack">
          <Field label="Group" forId="quick-group">
            <select
              id="quick-group"
              class="select"
              onchange={(event) => (groupTag = event.currentTarget.value)}
            >
              {#each groups as group (group.tag)}
                <option value={group.tag} selected={group.tag === groupTag}>
                  {group.tag} · {group.kind}
                </option>
              {/each}
            </select>
          </Field>
          <div class="options" role="group" aria-label={`Options in ${activeGroup.tag}`}>
            {#each activeGroup.options as option (option)}
              <button
                type="button"
                class="option"
                class:option--active={option === activeGroup.now}
                aria-pressed={option === activeGroup.now}
                disabled={switching}
                onclick={() => void switchOutbound(option)}
              >
                <span class="truncate">{option}</span>
                {#if option === activeGroup.now}<span class="tag tag--accent">selected</span>{/if}
              </button>
            {/each}
          </div>
          {#if switching}<p class="small muted">Switching…</p>{/if}
        </div>
      {/if}
    </section>

    <section class="panel" aria-labelledby="manual-proxy-title">
      <div class="panel__head">
        <span class="panel__title" id="manual-proxy-title">Manual proxy</span>
        <span class="tag">{app.engine.proxy ? "applied" : "standby"}</span>
      </div>
      <div class="panel__body stack">
        <div class="proxy-address">
          <span class="mono proxy-address__value">{proxyAddress}</span>
          <button
            type="button"
            class="icon-btn"
            aria-label="Copy proxy address"
            title="Copy proxy address"
            disabled={mixedPort === null}
            onclick={() => void copy("Proxy address", proxyAddress)}
          >
            <Icon name="copy" size={14} />
          </button>
        </div>
        <p class="small muted">Mixed HTTP + SOCKS5 on loopback.</p>

        <div class="stack stack--sm">
          <div class="row">
            <span class="kicker">Shell environment</span>
            <button
              type="button"
              class="btn btn--sm btn--quiet right"
              disabled={mixedPort === null}
              onclick={() => void copy("Environment block", envBlock)}
            >
              <Icon name="copy" size={11} /> Copy
            </button>
          </div>
          <pre class="code">{envBlock}</pre>
        </div>

        <div class="mechanism">
          <span class="kicker">Actual desktop mechanism</span>
          <p class="small">{mechanism}</p>
          {#if capabilities?.systemProxyBackends.length}
            <p class="small faint">
              Detected backends: {capabilities.systemProxyBackends.join(" · ")}
            </p>
          {/if}
        </div>

        <p class="small faint">
          System proxy only covers applications that honor desktop proxy settings. Traffic from apps that
          ignore it stays direct; use TUN when you need interface-level coverage.
        </p>
      </div>
    </section>
  </div>
{/if}

{#if importOpen}
  <ProfileImport onclose={() => (importOpen = false)} onimported={(profile) => void onImported(profile)} />
{/if}

<style>
  .controls {
    display: grid;
    grid-template-columns: minmax(220px, 320px) minmax(260px, 1fr) auto;
    gap: var(--space-5);
    align-items: start;
    padding: 0 0 var(--space-4);
    border-bottom: 1px solid var(--line);
    margin-bottom: var(--space-4);
  }

  @media (max-width: 980px) {
    .controls {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    }

    .controls__facts {
      grid-column: 1 / -1;
      display: flex;
      gap: var(--space-6);
    }
  }

  @media (max-width: 680px) {
    .controls {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .mode {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .mode__row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .mode__lock {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--faint);
  }

  .mode__desc {
    font-size: var(--fs-xs);
    color: var(--muted);
    max-width: 56ch;
    line-height: 1.5;
  }

  .mode__error {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--error);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .mode__warn {
    font-size: var(--fs-2xs);
    color: var(--warning);
    line-height: 1.45;
  }

  .controls__facts {
    display: grid;
    gap: var(--space-3);
    min-width: 140px;
  }

  .controls__facts p {
    font-size: var(--fs-sm);
    margin-top: 2px;
  }

  .session-error {
    margin-bottom: var(--space-4);
    align-items: center;
  }

  .session-error .mono {
    overflow-wrap: anywhere;
  }

  .lower {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-4);
    margin-top: var(--space-4);
  }

  @media (max-width: 900px) {
    .lower {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .options {
    display: grid;
    gap: 2px;
    max-height: 220px;
    overflow: auto;
    border: 1px solid var(--line);
    padding: 2px;
    background: var(--field);
  }

  .option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    padding: 6px 8px;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--muted);
    text-align: left;
    border: 1px solid transparent;
  }

  .option:hover:not(:disabled) {
    color: var(--text);
    background: var(--raised);
  }

  .option--active {
    color: var(--text);
    border-color: var(--line);
    background: var(--raised);
  }

  .option:disabled {
    opacity: 0.6;
  }

  .proxy-address {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 8px 10px;
    border: 1px solid var(--line);
    background: var(--field);
  }

  .proxy-address__value {
    font-size: var(--fs-lg);
    letter-spacing: -0.01em;
  }

  .mechanism {
    display: grid;
    gap: 3px;
    padding-top: var(--space-3);
    border-top: 1px solid var(--line);
  }

  .first-run {
    padding: var(--space-7) var(--space-5);
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-3);
  }

  .first-run__mark {
    color: var(--muted);
    margin-bottom: var(--space-2);
  }

  .first-run__title {
    font-size: var(--fs-xl);
    max-width: 24ch;
  }

  .first-run__desc {
    max-width: 58ch;
    color: var(--muted);
    font-size: var(--fs-sm);
  }

  .first-run__actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
    justify-content: center;
    margin-top: var(--space-2);
  }

  .first-run__facts {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--space-5);
    margin-top: var(--space-4);
    padding-top: var(--space-4);
    border-top: 1px solid var(--line);
    width: 100%;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--muted);
  }
</style>
