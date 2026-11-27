<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, type Mode, type Settings, type ThemeMode, type TunSetup } from "../api";
  import Icon from "../components/Icon.svelte";
  import Segmented from "../components/Segmented.svelte";
  import Toggle from "../components/Toggle.svelte";
  import TunSetupDialog from "../components/TunSetupDialog.svelte";
  import { errorMessage, modeLabel } from "../format";
  import { app } from "../state.svelte";

  const initial = untrack(() => app.settings);

  let theme = $state<ThemeMode>(initial?.theme ?? "dark");
  let mode = $state<Mode>(initial?.mode ?? "system");
  let mixedPort = $state(String(initial?.mixedPort ?? ""));
  let apiPort = $state(String(initial?.apiPort ?? ""));
  let corePath = $state(initial?.corePath ?? "");
  let autoRedirect = $state(initial?.autoRedirect ?? false);
  let autostart = $state(initial?.autostart ?? false);
  let connectOnLaunch = $state(initial?.connectOnLaunch ?? false);
  let notifications = $state(initial?.notifications ?? true);
  let minimizeToTray = $state(initial?.minimizeToTray ?? false);
  let logLevel = $state(initial?.logLevel ?? "info");
  let intervalHours = $state(String(initial?.subscriptionIntervalHours ?? ""));
  let developerMode = $state(initial?.developerMode === true);
  let errors = $state<Record<string, string | null>>({});
  let detecting = $state(false);
  let coreNote = $state<string | null>(null);
  let setupOpen = $state(false);
  let setupForMode = $state(false);
  let tunSetup = $state<TunSetup | null>(null);
  let tunSetupError = $state<string | null>(null);
  let setupChecking = $state(false);
  let setupSeq = 0;
  let modeSeq = 0;
  let lastSignature = "";

  const trayAvailable = $derived(app.platform?.trayAvailable ?? false);
  const capabilities = $derived(app.capabilities);
  const tunAvailable = $derived(app.tunReady);
  const autoRedirectAvailable = $derived(app.autoRedirectReady);
  const coreVersion = $derived(app.engineInfo?.version ?? app.core?.version ?? null);
  const tunReason = $derived(
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
  const autoRedirectHint = $derived(
    capabilities === null
      ? "Engine capabilities were not reported by the backend."
      : !capabilities.autoRedirect
        ? "Unavailable: the backend reports no TUN + nftables capability on this host."
        : mode === "tun"
          ? "Only affects TUN sessions. Requires nftables and the privileged helper."
          : "Currently ineffective: the default mode is system proxy. Switch to TUN for redirect rules.",
  );
  const logLevels = $derived(
    Array.from(new Set(["trace", "debug", "info", "warn", "error", "fatal", "panic", logLevel])),
  );
  const tunSetupLabel = $derived(
    setupChecking
      ? "checking"
      : tunSetup
        ? tunSetup.ready
          ? "ready"
          : "required"
        : tunSetupError
          ? "unknown"
          : "not checked",
  );

  onMount(() => {
    void refreshTunSetup();
  });

  const themeOptions = [
    { value: "dark", label: "Dark" },
    { value: "light", label: "Light" },
    { value: "system", label: "System" },
  ];
  const modeOptions = $derived([
    { value: "system", label: "System proxy", hint: "Apps that honor desktop proxy settings" },
    {
      value: "tun",
      label: "TUN",
      hint: tunReason,
      disabled: !tunAvailable,
    },
  ]);

  $effect(() => {
    const settings = app.settings;
    if (!settings) return;
    const signature = JSON.stringify(settings);
    if (signature === lastSignature) return;
    lastSignature = signature;
    syncFromSettings();
  });

  function isTheme(value: string): value is ThemeMode {
    return value === "dark" || value === "light" || value === "system";
  }

  function isMode(value: string): value is Mode {
    return value === "system" || value === "tun";
  }

  function syncFromSettings(): void {
    const settings = app.settings;
    if (!settings) return;
    theme = settings.theme;
    mode = settings.mode;
    mixedPort = String(settings.mixedPort);
    apiPort = String(settings.apiPort);
    corePath = settings.corePath ?? "";
    autoRedirect = settings.autoRedirect;
    autostart = settings.autostart;
    connectOnLaunch = settings.connectOnLaunch;
    notifications = settings.notifications;
    minimizeToTray = settings.minimizeToTray;
    logLevel = settings.logLevel;
    intervalHours = String(settings.subscriptionIntervalHours);
    developerMode = settings.developerMode === true;
  }

  async function apply(
    field: string,
    patch: Partial<Settings>,
    success?: string,
  ): Promise<boolean> {
    errors[field] = null;
    const failure = await app.saveSettings(patch);
    if (failure) {
      errors[field] = failure;
      syncFromSettings();
      app.notify("error", "Setting not saved", failure);
      return false;
    }
    if (success) app.notify("success", success);
    return true;
  }

  function changeTheme(next: string): void {
    if (!isTheme(next)) return;
    theme = next;
    void apply("theme", { theme: next });
  }

  function changeMode(next: string): void {
    if (!isMode(next)) return;
    const seq = ++modeSeq;
    if (next === "system") {
      mode = next;
      void apply("mode", { mode: next });
      return;
    }
    if (!tunAvailable) {
      errors.mode = tunReason;
      return;
    }
    errors.mode = null;
    void requestTunMode(seq);
  }

  async function requestTunMode(seq: number): Promise<void> {
    const setup = await refreshTunSetup();
    if (seq !== modeSeq) return;
    if (setup?.ready) {
      mode = "tun";
      void apply("mode", { mode: "tun" });
      return;
    }
    setupForMode = true;
    setupOpen = true;
  }

  async function refreshTunSetup(): Promise<TunSetup | null> {
    const seq = ++setupSeq;
    setupChecking = true;
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
      if (seq === setupSeq) setupChecking = false;
    }
  }

  function openTunSetup(): void {
    setupForMode = false;
    setupOpen = true;
  }

  function closeTunSetup(): void {
    setupOpen = false;
    setupForMode = false;
    void refreshTunSetup();
  }

  async function onTunReady(setup: TunSetup): Promise<void> {
    tunSetup = setup;
    tunSetupError = null;
    const forMode = setupForMode;
    setupOpen = false;
    setupForMode = false;
    if (forMode) {
      mode = "tun";
      await apply("mode", { mode: "tun" }, "TUN mode saved");
    }
    void refreshTunSetup();
  }

  function commitPort(field: "mixedPort" | "apiPort"): void {
    const raw = (field === "mixedPort" ? mixedPort : apiPort).trim();
    const value = Number(raw);
    if (!/^\d+$/.test(raw) || !Number.isInteger(value) || value < 1 || value > 65535) {
      errors[field] = "Enter a whole port between 1 and 65535.";
      return;
    }
    if (app.settings?.[field] === value) {
      errors[field] = null;
      return;
    }
    void apply(
      field,
      field === "mixedPort" ? { mixedPort: value } : { apiPort: value },
      `${field === "mixedPort" ? "Mixed" : "API"} port saved`,
    );
  }

  function commitInterval(): void {
    const raw = intervalHours.trim();
    const value = Number(raw);
    if (!/^\d+$/.test(raw) || !Number.isInteger(value) || value < 0 || value > 720) {
      errors.intervalHours =
        "Enter whole hours between 0 and 720. Zero disables scheduled refreshes.";
      return;
    }
    if (app.settings?.subscriptionIntervalHours === value) {
      errors.intervalHours = null;
      return;
    }
    void apply(
      "intervalHours",
      { subscriptionIntervalHours: value },
      "Subscription interval saved",
    );
  }

  async function commitCorePath(): Promise<void> {
    const trimmed = corePath.trim();
    if ((app.settings?.corePath ?? "") === trimmed) {
      errors.corePath = null;
      return;
    }
    const saved = await apply("corePath", { corePath: trimmed || null }, "Core path saved");
    if (!saved) return;
    const bootstrapError = await app.reloadBootstrap();
    if (bootstrapError) errors.corePath = bootstrapError;
  }

  async function detectCore(): Promise<void> {
    if (detecting) return;
    detecting = true;
    errors.corePath = null;
    coreNote = null;
    try {
      const diagnostics = await api.runDiagnostics();
      if (!diagnostics.core.path) {
        errors.corePath = diagnostics.core.error ?? "No core binary was detected.";
        return;
      }
      corePath = diagnostics.core.path;
      const saved = await apply("corePath", { corePath: diagnostics.core.path });
      if (!saved) return;
      const bootstrapError = await app.reloadBootstrap();
      if (bootstrapError) {
        errors.corePath = bootstrapError;
        return;
      }
      coreNote = `Detected ${diagnostics.core.version ?? "a core binary"} at ${diagnostics.core.path}.`;
      app.notify("success", "Core detected", diagnostics.core.path);
    } catch (cause) {
      errors.corePath = errorMessage(cause);
    } finally {
      detecting = false;
    }
  }

  async function changeAutostart(next: boolean): Promise<void> {
    autostart = next;
    errors.autostart = null;
    const failure = await app.setAutostart(next);
    if (failure) {
      errors.autostart = failure;
      autostart = app.settings?.autostart ?? !next;
      app.notify("error", "Autostart not changed", failure);
    } else {
      app.notify("success", next ? "Autostart enabled" : "Autostart disabled");
    }
  }

  function changeAutoRedirect(next: boolean): void {
    if (next && !autoRedirectAvailable) {
      errors.autoRedirect = autoRedirectHint;
      return;
    }
    autoRedirect = next;
    void apply("autoRedirect", { autoRedirect: next });
  }

  function changeConnectOnLaunch(next: boolean): void {
    connectOnLaunch = next;
    void apply("connectOnLaunch", { connectOnLaunch: next });
  }

  function changeNotifications(next: boolean): void {
    notifications = next;
    void apply("notifications", { notifications: next });
  }

  function changeMinimizeToTray(next: boolean): void {
    if (next && !trayAvailable) {
      errors.minimizeToTray = "Unavailable: the backend reports no usable tray on this desktop.";
      return;
    }
    minimizeToTray = next;
    void apply("minimizeToTray", { minimizeToTray: next });
  }

  function changeLogLevel(next: string): void {
    logLevel = next;
    void apply("logLevel", { logLevel: next });
  }

  function changeDeveloperMode(next: boolean): void {
    developerMode = next;
    void apply("developerMode", { developerMode: next });
  }

  async function openConfigFolder(): Promise<void> {
    try {
      await api.openConfigDir();
      app.notify("success", "Config folder opened");
    } catch (cause) {
      app.notify("error", "Could not open config folder", errorMessage(cause));
    }
  }
</script>

<div class="page-head">
  <div class="page-head__text">
    <span class="kicker">System</span>
    <h1 class="page-title">Settings</h1>
    <p class="page-desc">
      Changes are written through the backend as soon as a control is committed. The sidebar theme
      toggle uses the same setting.
    </p>
  </div>
  <div class="head-actions">
    <span class="tag">applies immediately</span>
    <button type="button" class="btn" onclick={() => void openConfigFolder()}>
      <Icon name="folder" size={12} /> Config folder
    </button>
  </div>
</div>

<div class="settings">
  <section class="settings-section" aria-labelledby="settings-appearance">
    <div class="section-head">
      <h2 class="section-title" id="settings-appearance">Appearance</h2>
      <p class="section-desc">
        Dark is the default; the system option follows the desktop preference.
      </p>
    </div>
    <dl class="deflist">
      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Theme</span>
          <span class="setting__hint">Resolved theme is cached for the first paint only.</span>
        </dt>
        <dd class="setting__control">
          <Segmented value={theme} options={themeOptions} label="Theme" onchange={changeTheme} />
          {#if errors.theme}<p class="field__error" role="alert">{errors.theme}</p>{/if}
        </dd>
      </div>
    </dl>
  </section>

  <section class="settings-section" aria-labelledby="settings-session">
    <div class="section-head">
      <h2 class="section-title" id="settings-session">Session</h2>
      <p class="section-desc">
        Defaults for the next engine start. Ports and mode are read when a session begins.
      </p>
    </div>
    <dl class="deflist">
      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Default proxy mode</span>
          <span class="setting__hint">
            System proxy covers cooperative apps; TUN covers the interface.
          </span>
        </dt>
        <dd class="setting__control">
          <Segmented
            value={mode}
            options={modeOptions}
            label="Default proxy mode"
            onchange={changeMode}
          />
          {#if !tunAvailable}
            <p class="inline-warning">
              {tunReason}{#if mode === "tun"}
                Choose System proxy before connecting.{/if}
            </p>
          {/if}
          {#if errors.mode}<p class="field__error" role="alert">{errors.mode}</p>{/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Mixed port</span>
          <span class="setting__hint">
            HTTP and SOCKS5 share this loopback port. Applies to the next session.
          </span>
        </dt>
        <dd class="setting__control">
          <input
            class="input input--mono port"
            type="text"
            inputmode="numeric"
            autocomplete="off"
            bind:value={mixedPort}
            onchange={() => commitPort("mixedPort")}
            onkeydown={(event) => {
              if (event.key === "Enter") commitPort("mixedPort");
            }}
          />
          {#if errors.mixedPort}<p class="field__error" role="alert">{errors.mixedPort}</p>{/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Auto-redirect</span>
          <span class="setting__hint">{autoRedirectHint}</span>
        </dt>
        <dd class="setting__control">
          <div class="toggle-line">
            <Toggle
              checked={autoRedirect}
              label="Auto-redirect TUN traffic"
              disabled={!autoRedirectAvailable && !autoRedirect}
              onchange={changeAutoRedirect}
            />
            <span class="toggle-line__state mono">{autoRedirect ? "on" : "off"}</span>
          </div>
          {#if errors.autoRedirect}<p class="field__error" role="alert">
              {errors.autoRedirect}
            </p>{/if}
        </dd>
      </div>
    </dl>
  </section>

  <section class="settings-section" aria-labelledby="settings-startup">
    <div class="section-head">
      <h2 class="section-title" id="settings-startup">Startup & desktop</h2>
      <p class="section-desc">
        Autostart is requested from the backend and may be refused outside a packaged install.
      </p>
    </div>
    <dl class="deflist">
      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Start Veil on login</span>
          <span class="setting__hint">Installs or removes the desktop autostart entry.</span>
        </dt>
        <dd class="setting__control">
          <div class="toggle-line">
            <Toggle
              checked={autostart}
              label="Start Veil on login"
              onchange={(next) => void changeAutostart(next)}
            />
            <span class="toggle-line__state mono">{autostart ? "on" : "off"}</span>
          </div>
          {#if errors.autostart}<p class="field__error" role="alert">{errors.autostart}</p>{/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Connect on launch</span>
          <span class="setting__hint">
            Start the last active profile when the application boots.
          </span>
        </dt>
        <dd class="setting__control">
          <div class="toggle-line">
            <Toggle
              checked={connectOnLaunch}
              label="Connect on launch"
              onchange={changeConnectOnLaunch}
            />
            <span class="toggle-line__state mono">{connectOnLaunch ? "on" : "off"}</span>
          </div>
          {#if errors.connectOnLaunch}<p class="field__error" role="alert">
              {errors.connectOnLaunch}
            </p>{/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Minimize to tray</span>
          <span class="setting__hint">
            {trayAvailable
              ? "Hide the window instead of closing it when supported by the backend."
              : "Unavailable: the backend reports no usable tray on this desktop."}
          </span>
        </dt>
        <dd class="setting__control">
          <div class="toggle-line">
            <Toggle
              checked={minimizeToTray}
              label="Minimize to tray"
              disabled={!trayAvailable && !minimizeToTray}
              onchange={changeMinimizeToTray}
            />
            <span class="toggle-line__state mono">{minimizeToTray ? "on" : "off"}</span>
          </div>
          {#if errors.minimizeToTray}<p class="field__error" role="alert">
              {errors.minimizeToTray}
            </p>{/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Notifications</span>
          <span class="setting__hint">Allow desktop notices for engine events.</span>
        </dt>
        <dd class="setting__control">
          <div class="toggle-line">
            <Toggle checked={notifications} label="Notifications" onchange={changeNotifications} />
            <span class="toggle-line__state mono">{notifications ? "on" : "off"}</span>
          </div>
          {#if errors.notifications}<p class="field__error" role="alert">
              {errors.notifications}
            </p>{/if}
        </dd>
      </div>
    </dl>
  </section>

  <section class="settings-section" aria-labelledby="settings-advanced">
    <div class="section-head">
      <h2 class="section-title" id="settings-advanced">Advanced</h2>
      <p class="section-desc">
        Engine internals and technical visibility. Engine:
        <span class="mono">{app.engineInfo?.id ?? "—"}</span> · API
        <span class="mono">{app.engineInfo?.api ?? "—"}</span>. Detected core:
        <span class="mono">{coreVersion ?? "not reported"}</span>.
      </p>
      {#if !app.clashApiReady}
        <p class="inline-warning" role="alert">
          The backend reports no Clash API capability. Veil needs it for health checks, stats,
          connections, and outbounds; starting a session is disabled.
        </p>
      {/if}
    </div>
    <dl class="deflist">
      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Current session mode</span>
          <span class="setting__hint">
            The live engine mode, or the saved default while disconnected.
          </span>
        </dt>
        <dd class="setting__control">
          <span class="mono small">
            {app.connected && app.engine.mode
              ? modeLabel(app.engine.mode)
              : app.connected
                ? "connected · mode not reported"
                : `${modeLabel(mode)} · default`}
          </span>
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">TUN setup</span>
          <span class="setting__hint">
            TUN needs the privileged helper and polkit policy. Veil installs them with admin
            approval via pkexec; the GUI never runs as root.
          </span>
        </dt>
        <dd class="setting__control">
          <div class="row row--wrap">
            <button type="button" class="btn btn--sm" onclick={openTunSetup}>
              <Icon name="diagnostics" size={12} /> Set up TUN
            </button>
            <span
              class="tag"
              class:tag--accent={tunSetup?.ready === true}
              class:tag--warning={tunSetup?.ready === false || tunSetupError !== null}
            >
              {tunSetupLabel}
            </span>
          </div>
          {#if tunSetup && !tunSetup.ready}
            <p class="setting__hint">
              {#if tunSetup.missing.length}
                Missing: {tunSetup.missing.join(" · ")}
              {:else}
                {tunSetup.detail}
              {/if}
            </p>
          {:else if tunSetup?.ready}
            <p class="setting__hint">{tunSetup.detail}</p>
          {:else if tunSetupError}
            <p class="field__error" role="alert">{tunSetupError}</p>
          {/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">API port</span>
          <span class="setting__hint">
            Local Clash API used for stats, connections, and outbound switching.
          </span>
        </dt>
        <dd class="setting__control">
          <input
            class="input input--mono port"
            type="text"
            inputmode="numeric"
            autocomplete="off"
            bind:value={apiPort}
            onchange={() => commitPort("apiPort")}
            onkeydown={(event) => {
              if (event.key === "Enter") commitPort("apiPort");
            }}
          />
          {#if errors.apiPort}<p class="field__error" role="alert">{errors.apiPort}</p>{/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Core binary path</span>
          <span class="setting__hint">
            Leave blank to use the bundled or PATH-discovered sing-box.
          </span>
        </dt>
        <dd class="setting__control">
          <div class="input-row">
            <input
              class="input input--mono"
              bind:value={corePath}
              spellcheck="false"
              placeholder="Auto-detect"
              onkeydown={(event) => {
                if (event.key === "Enter") commitCorePath();
              }}
            />
            <button type="button" class="btn" onclick={commitCorePath}>Apply</button>
            <button
              type="button"
              class="btn"
              disabled={detecting}
              onclick={() => void detectCore()}
            >
              {detecting ? "Detecting" : "Detect"}
            </button>
          </div>
          {#if coreNote}<p class="field__ok" role="status">{coreNote}</p>{/if}
          {#if errors.corePath}<p class="field__error" role="alert">{errors.corePath}</p>{/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Log level</span>
          <span class="setting__hint">Engine verbosity. Applies to the next session.</span>
        </dt>
        <dd class="setting__control">
          <select
            class="select"
            aria-label="Log level"
            onchange={(event) => changeLogLevel(event.currentTarget.value)}
          >
            {#each logLevels as level (level)}
              <option value={level} selected={level === logLevel}>{level}</option>
            {/each}
          </select>
          {#if errors.logLevel}<p class="field__error" role="alert">{errors.logLevel}</p>{/if}
        </dd>
      </div>

      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Developer mode</span>
          <span class="setting__hint">
            Reveals engine PID, core version/path, Clash API port, and profile ID in Overview's
            Connection details. It never changes connection behavior.
          </span>
        </dt>
        <dd class="setting__control">
          <div class="toggle-line">
            <Toggle checked={developerMode} label="Developer mode" onchange={changeDeveloperMode} />
            <span class="toggle-line__state mono">{developerMode ? "on" : "off"}</span>
          </div>
          {#if errors.developerMode}<p class="field__error" role="alert">
              {errors.developerMode}
            </p>{/if}
        </dd>
      </div>
    </dl>
  </section>

  <section class="settings-section" aria-labelledby="settings-subscriptions">
    <div class="section-head">
      <h2 class="section-title" id="settings-subscriptions">Subscriptions</h2>
      <p class="section-desc">
        Only profiles marked for auto-update are refreshed by the backend scheduler.
      </p>
    </div>
    <dl class="deflist">
      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Refresh interval</span>
          <span class="setting__hint">
            Hours between scheduled refreshes. Zero disables the schedule.
          </span>
        </dt>
        <dd class="setting__control">
          <div class="input-with-unit">
            <input
              class="input input--mono port"
              type="text"
              inputmode="numeric"
              autocomplete="off"
              bind:value={intervalHours}
              onchange={commitInterval}
              onkeydown={(event) => {
                if (event.key === "Enter") commitInterval();
              }}
            />
            <span class="unit mono">h</span>
          </div>
          {#if errors.intervalHours}<p class="field__error" role="alert">
              {errors.intervalHours}
            </p>{/if}
        </dd>
      </div>
    </dl>
  </section>

  <section class="settings-section" aria-labelledby="settings-language">
    <div class="section-head">
      <h2 class="section-title" id="settings-language">Language</h2>
      <p class="section-desc">
        Only English is implemented today; additional languages are planned.
      </p>
    </div>
    <dl class="deflist">
      <div class="deflist__row">
        <dt class="setting">
          <span class="setting__label">Interface language</span>
          <span class="setting__hint">Disabled until translations ship.</span>
        </dt>
        <dd class="setting__control">
          <span class="mono small">English</span>
          <p class="setting__hint">
            Only English is implemented; additional languages are planned.
          </p>
        </dd>
      </div>
    </dl>
  </section>
</div>

<p class="settings-foot mono">
  Backend state is authoritative. A rejected change leaves the previous value in place and reports
  the error here.
</p>

{#if setupOpen}
  <TunSetupDialog
    intent={setupForMode ? "enable" : "inspect"}
    onclose={closeTunSetup}
    onready={(setup) => void onTunReady(setup)}
  />
{/if}

<style>
  .settings {
    max-width: 1100px;
  }

  .settings-section {
    padding: var(--space-5) 0;
    border-bottom: 1px solid var(--line);
  }

  .settings-section:first-child {
    padding-top: 0;
  }

  .section-head {
    display: grid;
    gap: 3px;
    margin-bottom: var(--space-2);
  }

  .section-title {
    font-size: var(--fs-lg);
    font-weight: 500;
  }

  .section-desc {
    font-size: var(--fs-xs);
    color: var(--muted);
    max-width: 70ch;
  }

  .settings-section .deflist__row {
    grid-template-columns: minmax(190px, 250px) minmax(0, 1fr);
  }

  .setting {
    display: grid;
    gap: 3px;
    align-content: start;
  }

  .setting__label {
    font-size: var(--fs-sm);
  }

  .setting__hint {
    font-size: var(--fs-xs);
    color: var(--muted);
    line-height: 1.45;
    max-width: 56ch;
  }

  .setting__control {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 5px;
    min-width: 0;
  }

  .setting__control .select,
  .setting__control .input-row {
    width: min(100%, 420px);
  }

  .port {
    width: 130px;
  }

  .toggle-line {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .toggle-line__state {
    font-size: var(--fs-2xs);
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .input-with-unit {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .inline-warning {
    font-size: var(--fs-2xs);
    color: var(--warning);
    line-height: 1.45;
    max-width: 56ch;
  }

  .unit {
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .settings-foot {
    margin-top: var(--space-4);
    font-size: var(--fs-2xs);
    color: var(--faint);
    max-width: 80ch;
  }

  @media (max-width: 1080px) {
    .settings-section .deflist__row {
      grid-template-columns: minmax(0, 1fr);
      gap: 6px;
    }

    .setting__hint {
      max-width: 68ch;
    }

    .setting__control .select,
    .setting__control .input-row {
      width: 100%;
    }

    .port {
      width: min(100%, 130px);
    }
  }
</style>
