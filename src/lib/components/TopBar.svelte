<script lang="ts">
  import { modeLabel, statusWord } from "../format";
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import StatusGlyph from "./StatusGlyph.svelte";

  const selectorLock = $derived.by((): string | null => {
    if (app.connected) return "Disconnect the live session before switching profiles.";
    if (app.busy) return "A session transition is in progress.";
    if (app.connectBusy) return "A connection command is already running.";
    if (!app.profiles.length) return "No profiles available. Add one on the Profiles page.";
    return null;
  });
  const selectorCaption = $derived(app.connected ? "Active session" : "Next session");
  const selectorTitle = $derived(
    selectorLock ?? "Profile prepared for the next connect. It does not change a live session.",
  );
  const selectedValue = $derived(
    app.connected && app.engine.profileId !== null
      ? String(app.engine.profileId)
      : app.selectedProfile
        ? String(app.selectedProfile.id)
        : "",
  );
  const sessionProfileMissing = $derived(
    app.connected &&
      app.engine.profileId !== null &&
      app.engine.profileName !== null &&
      !app.profiles.some((profile) => String(profile.id) === String(app.engine.profileId)),
  );
  const mode = $derived(app.engine.mode ?? app.settings?.mode ?? null);
  const modeTitle = $derived(
    app.connected ? "Mode of the live session" : "Configured mode for the next session",
  );
  const themeLabel = $derived(
    app.resolvedTheme === "dark" ? "Switch to light theme" : "Switch to dark theme",
  );

  function selectProfile(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (!value) return;
    const id = Number(value);
    if (Number.isFinite(id)) app.setSelectedProfile(id);
  }
</script>

<header class="topbar">
  <div class="topbar__profile" title={selectorTitle}>
    <label class="topbar__caption" for="topbar-profile">{selectorCaption}</label>
    <select
      id="topbar-profile"
      class="select topbar__select"
      value={selectedValue}
      disabled={selectorLock !== null}
      title={selectorTitle}
      onchange={selectProfile}
    >
      <option value="" disabled selected={selectedValue === ""}>
        {app.profiles.length ? "Select a profile" : "No profiles"}
      </option>
      {#if sessionProfileMissing}
        <option
          value={String(app.engine.profileId)}
          selected={selectedValue === String(app.engine.profileId)}
        >
          {app.engine.profileName} · session
        </option>
      {/if}
      {#each app.profiles as profile (String(profile.id))}
        <option value={String(profile.id)} selected={selectedValue === String(profile.id)}>
          {profile.name}
        </option>
      {/each}
    </select>
  </div>

  <div class="topbar__session" role="group" aria-label="Session state">
    <span
      class="topbar__state st-{app.engine.status}"
      title={app.manualSession ? "Live, but the desktop proxy is manual-only." : undefined}
    >
      <StatusGlyph status={app.engine.status} size={11} />
      <span>{statusWord(app.engine.status)}</span>
    </span>
    {#if app.manualSession}
      <span class="tag tag--warning" title="Apps must use the mixed proxy manually.">manual</span>
    {/if}
    <span class="tag topbar__mode" title={modeTitle}>{modeLabel(mode)}</span>
    {#if app.connected}
      <span class="topbar__proxy" title="Mixed proxy bound by the live engine">
        <span class="topbar__proxy-label">Mixed</span>
        <span class="mono">
          {app.engine.mixedPort === null ? "not reported" : `127.0.0.1:${app.engine.mixedPort}`}
        </span>
      </span>
    {/if}
  </div>

  <div class="topbar__actions">
    <button
      type="button"
      class="btn btn--quiet btn--sm"
      title="Open diagnostics"
      onclick={() => app.setView("diagnostics")}
    >
      <Icon name="diagnostics" size={12} />
      <span class="topbar__action-label">Diagnostics</span>
    </button>
    <button
      type="button"
      class="icon-btn"
      aria-label={themeLabel}
      title={themeLabel}
      onclick={() => void app.cycleTheme()}
    >
      <Icon name={app.resolvedTheme === "dark" ? "sun" : "moon"} size={14} />
    </button>
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: var(--topbar-h);
    min-width: 0;
    padding: 0 var(--space-5);
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }

  .topbar__profile {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 0 1 auto;
  }

  .topbar__caption {
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--faint);
    white-space: nowrap;
  }

  .topbar__select {
    width: clamp(150px, 18vw, 220px);
    min-height: 28px;
    height: 28px;
    padding: 0 26px 0 8px;
    font-family: var(--font-body);
    font-size: var(--fs-xs);
    background-position:
      calc(100% - 14px) 12px,
      calc(100% - 10px) 12px;
  }

  .topbar__session {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
  }

  .topbar__state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    padding: 3px 7px;
    border: 1px solid var(--line);
    border-radius: var(--radius-pill);
    background: var(--panel);
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.12em;
    white-space: nowrap;
  }

  .topbar__mode {
    flex: none;
  }

  .topbar__proxy {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    white-space: nowrap;
  }

  .topbar__proxy-label {
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--faint);
  }

  .topbar__proxy .mono {
    font-size: var(--fs-2xs);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .topbar__actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
    margin-left: auto;
  }

  @media (max-width: 1080px) {
    .topbar {
      gap: var(--space-3);
      padding: 0 var(--space-4);
    }
  }

  @media (max-width: 900px) {
    .topbar__proxy {
      display: none;
    }
  }

  @media (max-width: 820px) {
    .topbar__caption,
    .topbar__action-label {
      display: none;
    }
  }
</style>
