<script lang="ts">
  import type { Profile } from "../api";
  import Icon from "../components/Icon.svelte";
  import Mark from "../components/Mark.svelte";
  import OutboundBrowser from "../components/OutboundBrowser.svelte";
  import ProfileImport from "../components/ProfileImport.svelte";
  import ProfileMenu from "../components/ProfileMenu.svelte";
  import Segmented from "../components/Segmented.svelte";
  import Sparkline from "../components/Sparkline.svelte";
  import StatusGlyph from "../components/StatusGlyph.svelte";
  import {
    copyText,
    formatBytes,
    formatDelay,
    formatDuration,
    formatSpeed,
    modeLabel,
    statusWord,
    toMillis,
  } from "../format";
  import { app, type View } from "../state.svelte";

  let { navigate }: { navigate: (view: View) => void } = $props();

  let importOpen = $state(false);
  let modeError = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let now = $state(Date.now());

  const selectedProfile = $derived(app.profiles.find((profile) => String(profile.id) === selectedId) ?? null);
  const mode = $derived(app.settings?.mode ?? "system");
  const locked = $derived(app.connected || app.busy);
  const developerMode = $derived(app.settings?.developerMode === true);

  const capabilities = $derived(app.capabilities);
  const engineReady = $derived(app.clashApiReady);
  const tunAvailable = $derived(app.tunReady);
  const systemProxySupported = $derived(app.platform?.systemProxySupported === true);
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
    { value: "tun", label: "TUN", hint: tunHint, disabled: !tunAvailable },
  ]);

  const isLive = $derived(app.connected);
  const failedWithRuntime = $derived(app.engine.status === "error" && app.engine.pid !== null);
  const canStop = $derived(isLive || failedWithRuntime);
  const buttonDisabled = $derived(app.busy || app.connectBusy || (!canStop && !canConnect));
  const buttonLabel = $derived(
    app.engine.status === "starting"
      ? "Linking"
      : app.engine.status === "stopping"
        ? "Stopping"
        : isLive
          ? "Disconnect"
          : failedWithRuntime
            ? "Stop engine"
            : app.engine.status === "error"
              ? "Reconnect"
              : "Connect",
  );
  const stateWord = $derived(statusWord(app.engine.status));
  const activeMode = $derived(app.engine.mode ?? mode);

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

  const downSpeedLabel = $derived(app.connected ? formatSpeed(app.stats.downSpeed) : "—");
  const upSpeedLabel = $derived(app.connected ? formatSpeed(app.stats.upSpeed) : "—");
  const trafficLabel = $derived(
    app.connected ? `↓ ${formatBytes(app.stats.down)} · ↑ ${formatBytes(app.stats.up)}` : "—",
  );
  const durationLabel = $derived.by(() => {
    if (!app.connected) return "—";
    if (app.engine.startedAt) {
      return formatDuration(Math.max(0, (now - toMillis(app.engine.startedAt)) / 1000));
    }
    return app.stats.duration > 0 ? formatDuration(app.stats.duration) : "—";
  });
  const latencyLabel = $derived(
    !app.connected ? "—" : app.stats.latency === null ? "not measured" : formatDelay(app.stats.latency),
  );
  const connectionsLabel = $derived(app.connected ? String(app.stats.connections) : "—");
  const downSeries = $derived(app.history.map((sample) => sample.down));
  const upSeries = $derived(app.history.map((sample) => sample.up));
  const coreVersion = $derived(app.engineInfo?.version ?? app.core?.version ?? null);
  const corePath = $derived(app.engineInfo?.path ?? app.core?.path ?? null);
  const profileIdLabel = $derived(app.engine.profileId ?? selectedProfile?.id ?? "—");
  const modeHint = $derived(
    locked
      ? "Disconnect to change profile or mode."
      : mode === "tun"
        ? "Interface routing; the helper and permissions are required."
        : "Apps that honor desktop proxy settings.",
  );

  $effect(() => {
    if (!app.connected || !app.engine.startedAt) return;
    now = Date.now();
    const timer = window.setInterval(() => {
      now = Date.now();
    }, 1000);
    return () => window.clearInterval(timer);
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
    <h2 class="first-run__title">Import a configuration to begin.</h2>
    <p class="first-run__desc">
      Add a sing-box config, subscription, share links, or QR image. Veil validates it locally before saving.
    </p>
    <div class="first-run__actions">
      <button type="button" class="btn btn--primary" onclick={() => (importOpen = true)}>
        <Icon name="import" size={13} /> Import profile
      </button>
    </div>
  </section>
{:else}
  <div class="page-head overview-head">
    <div class="page-head__text">
      <span class="kicker">Overview</span>
      <h1 class="page-title">Session</h1>
    </div>
    <div class="head-actions overview-head__status">
      <StatusGlyph status={app.engine.status} size={11} />
      <span class="overview-head__word mono {`st-${app.engine.status}`}">{stateWord}</span>
      <span class="tag">{modeLabel(activeMode)}</span>
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

  <div class="overview-grid">
    <aside class="monitor panel" aria-label="Session monitor">
      <header class="monitor__head">
        <div class="monitor__state" aria-live="polite">
          <StatusGlyph status={app.engine.status} size={13} />
          <span class="monitor__word {`st-${app.engine.status}`}">{stateWord}</span>
        </div>
        <span class="tag">{modeLabel(activeMode)}</span>
      </header>

      <div class="monitor__body">
        <ProfileMenu
          profiles={app.profiles}
          activeId={selectedId}
          disabled={locked}
          reason="Disconnect before switching profiles."
          onselect={(id) => (selectedId = id)}
          onmanage={() => navigate("profiles")}
        />

        <div class="monitor__field">
          <span class="kicker">Mode</span>
          <Segmented
            value={mode}
            options={modeOptions}
            label="Proxy mode"
            disabled={locked}
            onchange={(next) => void setMode(next)}
          />
          <p class="monitor__hint">{modeHint}</p>
          {#if mode === "system" && !systemProxySupported}
            <p class="monitor__warn">
              This desktop reports no usable system-proxy backend. The session
              {app.connected ? "is" : "will be"} manual-only; use the proxy below.
            </p>
          {/if}
          {#if mode === "tun" && !modeReady}
            <p class="monitor__warn">{tunHint} Choose System proxy before connecting.</p>
          {/if}
          {#if !engineReady}
            <p class="monitor__warn" role="alert">
              {capabilities === null
                ? "Engine capabilities were not reported; connecting is disabled."
                : "Connecting is disabled: the core lacks the Clash API Veil needs."}
            </p>
          {/if}
          {#if modeError}<p class="monitor__error" role="alert">{modeError}</p>{/if}
        </div>

        <div class="monitor__rates">
          <div class="monitor__rate">
            <span class="kicker">Download</span>
            <span class="monitor__rate-value mono">{downSpeedLabel}</span>
            {#if app.connected && app.history.length > 1}
              <Sparkline values={downSeries} tone={isLive ? "accent" : "muted"} label="Download speed history" height={20} />
            {/if}
          </div>
          <div class="monitor__rate">
            <span class="kicker">Upload</span>
            <span class="monitor__rate-value mono">{upSpeedLabel}</span>
            {#if app.connected && app.history.length > 1}
              <Sparkline values={upSeries} tone="muted" label="Upload speed history" height={20} />
            {/if}
          </div>
        </div>

        <dl class="monitor__stats">
          <div class="monitor__stat">
            <dt class="kicker">Traffic</dt>
            <dd class="mono">{trafficLabel}</dd>
          </div>
          <div class="monitor__stat">
            <dt class="kicker">Connections</dt>
            <dd class="mono">{connectionsLabel}</dd>
          </div>
          <div class="monitor__stat">
            <dt class="kicker">Duration</dt>
            <dd class="mono">{durationLabel}</dd>
          </div>
          <div class="monitor__stat">
            <dt class="kicker">Latency</dt>
            <dd class="mono" class:monitor__stat-value--none={!app.connected || app.stats.latency === null}>
              {latencyLabel}
            </dd>
          </div>
        </dl>

        <div class="monitor__actions">
          {#if disabledReason}
            <p class="monitor__blocked">{disabledReason}</p>
          {:else}
            <span class="monitor__shortcut mono">Ctrl / Cmd + Enter</span>
          {/if}
          <button
            type="button"
            class="monitor__power"
            class:monitor__power--live={isLive}
            disabled={buttonDisabled}
            title={disabledReason ?? undefined}
            onclick={() => void toggleConnection()}
          >
            <Icon name="power" size={16} />
            <span>{buttonLabel}</span>
          </button>
        </div>
      </div>

      <details class="monitor__details">
        <summary>Connection details</summary>
        <div class="monitor__details-body">
          <dl class="kv">
            <div class="kv__row">
              <dt>Proxy address</dt>
              <dd>
                <span class="row">
                  <span class="mono break">{proxyAddress}</span>
                  <button
                    type="button"
                    class="icon-btn"
                    aria-label="Copy proxy address"
                    title="Copy proxy address"
                    disabled={mixedPort === null}
                    onclick={() => void copy("Proxy address", proxyAddress)}
                  >
                    <Icon name="copy" size={12} />
                  </button>
                </span>
              </dd>
            </div>
            <div class="kv__row">
              <dt>Desktop mechanism</dt>
              <dd class="break">{mechanism}</dd>
            </div>
            {#if developerMode}
              <div class="kv__row">
                <dt>Engine PID</dt>
                <dd class="mono">{app.engine.pid ?? "—"}</dd>
              </div>
              <div class="kv__row">
                <dt>Core version</dt>
                <dd class="mono">{coreVersion ?? "not reported"}</dd>
              </div>
              <div class="kv__row">
                <dt>Core path</dt>
                <dd class="mono break">{corePath ?? "not reported"}</dd>
              </div>
              <div class="kv__row">
                <dt>Clash API port</dt>
                <dd class="mono">{app.settings?.apiPort ?? "—"}</dd>
              </div>
              <div class="kv__row">
                <dt>Profile ID</dt>
                <dd class="mono">{profileIdLabel}</dd>
              </div>
            {/if}
          </dl>

          {#if developerMode}
            <div class="monitor__env">
              <div class="monitor__env-head">
                <span class="kicker">Proxy environment</span>
                <button
                  type="button"
                  class="btn btn--sm btn--quiet"
                  disabled={mixedPort === null}
                  onclick={() => void copy("Environment block", envBlock)}
                >
                  <Icon name="copy" size={11} /> Copy env
                </button>
              </div>
              {#if mixedPort === null}
                <p class="small muted">Proxy port unavailable.</p>
              {:else}
                <pre class="code monitor__code">{envBlock}</pre>
              {/if}
            </div>
          {/if}
        </div>
      </details>
    </aside>

    <section class="workspace" aria-label="Outbound workspace">
      <OutboundBrowser />
    </section>
  </div>
{/if}

{#if importOpen}
  <ProfileImport onclose={() => (importOpen = false)} onimported={(profile) => void onImported(profile)} />
{/if}

<style>
  .overview-head {
    padding-bottom: 10px;
    margin-bottom: var(--space-4);
    align-items: center;
  }

  .overview-head .page-title {
    font-size: var(--fs-xl);
  }

  .overview-head__status {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .overview-head__word {
    font-size: var(--fs-2xs);
    letter-spacing: 0.14em;
  }

  .overview-grid {
    display: grid;
    grid-template-columns: minmax(272px, 25%) minmax(0, 1fr);
    gap: var(--space-5);
    align-items: start;
    min-width: 0;
  }

  .monitor {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .monitor__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 10px var(--space-4);
    border-bottom: 1px solid var(--line);
  }

  .monitor__state {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }

  .monitor__word {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    font-weight: 500;
    letter-spacing: 0.16em;
    white-space: nowrap;
  }

  .monitor__body {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-4);
    min-width: 0;
  }

  .monitor__body :global(.profile-menu) {
    width: 100%;
  }

  .monitor__field {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .monitor__hint {
    font-size: var(--fs-2xs);
    color: var(--muted);
    line-height: 1.45;
  }

  .monitor__warn {
    font-size: var(--fs-2xs);
    color: var(--warning);
    line-height: 1.45;
  }

  .monitor__error {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--error);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .monitor__rates {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
    padding-top: var(--space-3);
    border-top: 1px solid var(--line);
    min-width: 0;
  }

  .monitor__rate {
    display: grid;
    gap: 2px;
    align-content: start;
    min-width: 0;
  }

  .monitor__rate-value {
    font-size: var(--fs-xl);
    letter-spacing: -0.02em;
    line-height: 1.1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .monitor__stats {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3) var(--space-4);
    padding-top: var(--space-3);
    border-top: 1px solid var(--line);
    min-width: 0;
  }

  .monitor__stat {
    display: grid;
    gap: 1px;
    min-width: 0;
  }

  .monitor__stat dd {
    font-size: var(--fs-xs);
    overflow-wrap: anywhere;
  }

  .monitor__stat-value--none {
    color: var(--faint);
  }

  .monitor__actions {
    display: grid;
    gap: 6px;
    padding-top: var(--space-3);
    border-top: 1px solid var(--line);
    min-width: 0;
  }

  .monitor__shortcut {
    font-size: var(--fs-2xs);
    color: var(--faint);
    letter-spacing: 0.04em;
  }

  .monitor__blocked {
    color: var(--warning);
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    line-height: 1.45;
  }

  .monitor__power {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    width: 100%;
    height: 42px;
    padding: 0 12px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    font-weight: 500;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    clip-path: polygon(0 0, calc(100% - 9px) 0, 100% 9px, 100% 100%, 9px 100%, 0 calc(100% - 9px));
    transition: background-color var(--fast) var(--ease), color var(--fast) var(--ease),
      border-color var(--fast) var(--ease);
  }

  .monitor__power:hover:not(:disabled) {
    border-color: var(--text);
  }

  .monitor__power--live {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-ink);
  }

  .monitor__power--live:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 88%, var(--text));
    border-color: var(--accent);
  }

  .monitor__power:disabled {
    opacity: 0.45;
  }

  .monitor__details {
    border-top: 1px solid var(--line);
  }

  .monitor__details summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px var(--space-4);
    cursor: pointer;
    list-style: none;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
    user-select: none;
  }

  .monitor__details summary::-webkit-details-marker {
    display: none;
  }

  .monitor__details summary::after {
    content: "+";
    margin-left: auto;
    color: var(--faint);
  }

  .monitor__details[open] summary::after {
    content: "−";
  }

  .monitor__details summary:hover {
    color: var(--text);
  }

  .monitor__details-body {
    display: grid;
    gap: var(--space-4);
    padding: 0 var(--space-4) var(--space-4);
    min-width: 0;
  }

  .kv {
    display: grid;
    gap: 0;
    min-width: 0;
  }

  .kv__row {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 2px;
    padding: 7px 0;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  .kv__row:last-child {
    border-bottom: 0;
    padding-bottom: 0;
  }

  .kv__row dt {
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  .kv__row dd {
    font-size: var(--fs-xs);
    min-width: 0;
  }

  .kv__row dd .mono {
    font-size: var(--fs-xs);
  }

  .monitor__env {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .monitor__env-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .monitor__code {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: var(--fs-2xs);
  }

  .workspace {
    height: clamp(480px, calc(100vh - 205px), 900px);
    min-height: 380px;
    min-width: 0;
  }

  .session-error {
    margin-bottom: var(--space-4);
    align-items: center;
  }

  .session-error .mono {
    overflow-wrap: anywhere;
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

  @media (max-width: 1100px) {
    .overview-grid {
      grid-template-columns: minmax(0, 1fr);
    }

    .workspace {
      height: clamp(460px, 72vh, 780px);
    }
  }

  @media (max-width: 560px) {
    .monitor__rates {
      grid-template-columns: minmax(0, 1fr);
    }

    .monitor__stats {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
