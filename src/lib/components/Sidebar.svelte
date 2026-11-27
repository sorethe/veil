<script lang="ts">
  import { formatBytes, formatSpeed, statusWord } from "../format";
  import { app, type View } from "../state.svelte";
  import type { IconName } from "../icons";
  import Icon from "./Icon.svelte";
  import Mark from "./Mark.svelte";
  import StatusGlyph from "./StatusGlyph.svelte";

  const nav: { id: View; label: string; icon: IconName; tag?: string }[] = [
    { id: "overview", label: "Overview", icon: "overview" },
    { id: "membrane", label: "Membrane", icon: "membrane", tag: "lab" },
    { id: "profiles", label: "Profiles", icon: "profiles" },
    { id: "routing", label: "Routing", icon: "routing" },
    { id: "outbounds", label: "Outbounds", icon: "outbounds" },
    { id: "connections", label: "Connections", icon: "connections" },
    { id: "logs", label: "Logs", icon: "logs" },
    { id: "diagnostics", label: "Diagnostics", icon: "diagnostics" },
  ];

  const activeProfile = $derived(
    app.engine.profileId === null
      ? null
      : (app.profiles.find((profile) => String(profile.id) === String(app.engine.profileId)) ??
          null),
  );
  const profileName = $derived(
    app.connected
      ? (app.engine.profileName ?? activeProfile?.name ?? "Not reported")
      : (app.selectedProfile?.name ?? activeProfile?.name ?? "No profile"),
  );
  const profileCaption = $derived(app.connected ? "Active profile" : "Next session");
  const engineInfo = $derived(app.engineInfo);
  const coreHealth = $derived(
    !app.core
      ? "Core unknown"
      : !app.core.ok
        ? "Core unavailable"
        : !engineInfo
          ? "Engine unknown"
          : !engineInfo.capabilities.clashApi
            ? "Clash API missing"
            : "Core ready",
  );
  const coreDotClass = $derived(
    coreHealth === "Core ready"
      ? "core__dot--ok"
      : coreHealth === "Core unavailable"
        ? "core__dot--bad"
        : "core__dot--warn",
  );
  const sessionLabel = $derived(
    app.manualSession
      ? "LIVE · MANUAL"
      : `${statusWord(app.engine.status)}${
          app.engine.mode ? ` · ${app.engine.mode === "tun" ? "TUN" : "SYSTEM"}` : ""
        }`,
  );
  const downSpeed = $derived(app.connected ? formatSpeed(app.stats.downSpeed) : "—");
  const upSpeed = $derived(app.connected ? formatSpeed(app.stats.upSpeed) : "—");
  const sessionTotal = $derived(
    app.connected ? `${formatBytes(app.stats.down)} ↓ · ${formatBytes(app.stats.up)} ↑` : "—",
  );
  const trafficTitle = $derived(
    app.connected
      ? `Session totals: ${formatBytes(app.stats.down)} down, ${formatBytes(app.stats.up)} up`
      : "No live session",
  );
</script>

<aside class="sidebar" aria-label="Application">
  <button
    type="button"
    class="brand"
    onclick={() => app.setView("overview")}
    aria-label="Veil overview"
    title="Veil overview"
  >
    <span class="brand__mark"><Mark size={23} /></span>
    <span class="brand__text">
      <span class="brand__name">Veil</span>
      <span class="brand__sub">Network layer</span>
    </span>
  </button>

  <nav class="nav" aria-label="Primary">
    <ul>
      {#each nav as item, index (item.id)}
        <li>
          <button
            type="button"
            class="nav__item"
            class:nav__item--active={app.view === item.id}
            aria-current={app.view === item.id ? "page" : undefined}
            aria-label={item.tag ? `${item.label} — experimental prototype` : item.label}
            title={item.label}
            onclick={() => app.setView(item.id)}
          >
            <span class="nav__index" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
            <Icon name={item.icon} size={16} />
            <span class="nav__label">{item.label}</span>
            {#if item.tag}
              <span class="nav__tag" aria-hidden="true">{item.tag}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="sidebar__foot">
    <div class="profile-block">
      <div class="profile-block__head">
        <span class="kicker">{profileCaption}</span>
        <StatusGlyph status={app.engine.status} size={9} />
      </div>
      <p class="profile-block__name" title={profileName}>{profileName}</p>
      <p class="profile-block__state st-{app.engine.status}">{sessionLabel}</p>
    </div>

    <dl
      class="traffic"
      class:traffic--idle={!app.connected}
      aria-label="Session traffic"
      title={trafficTitle}
    >
      <div class="traffic__row">
        <dt>Down</dt>
        <dd class="num">{downSpeed}</dd>
      </div>
      <div class="traffic__row">
        <dt>Up</dt>
        <dd class="num">{upSpeed}</dd>
      </div>
      <div class="traffic__row traffic__row--total">
        <dt>Session</dt>
        <dd class="num" title={trafficTitle}>{sessionTotal}</dd>
      </div>
    </dl>

    <button
      type="button"
      class="core"
      aria-label={`${coreHealth}. Open Diagnostics.`}
      title={`${coreHealth} · Diagnostics`}
      onclick={() => app.setView("diagnostics")}
    >
      <span class="core__dot {coreDotClass}" aria-hidden="true"></span>
      <span class="core__label">{coreHealth}</span>
    </button>

    {#if app.version}
      <p class="foot-version mono" title="Veil version">v{app.version}</p>
    {/if}

    <div class="foot-actions">
      <button
        type="button"
        class="foot-link"
        class:foot-link--active={app.view === "settings"}
        aria-current={app.view === "settings" ? "page" : undefined}
        aria-label="Settings"
        title="Settings"
        onclick={() => app.setView("settings")}
      >
        <Icon name="settings" size={15} />
        <span class="foot-link__label">Settings</span>
      </button>
      <button
        type="button"
        class="foot-link foot-link--icon"
        aria-label={app.resolvedTheme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
        title={app.resolvedTheme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
        onclick={() => void app.cycleTheme()}
      >
        <Icon name={app.resolvedTheme === "dark" ? "sun" : "moon"} size={15} />
      </button>
    </div>
  </div>
</aside>

<style>
  .sidebar {
    grid-column: 1;
    grid-row: 1;
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100vh;
    height: 100dvh;
    min-height: 0;
    overflow-y: auto;
    background: var(--surface);
    border-right: 1px solid var(--line);
    z-index: 20;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: var(--space-5) var(--space-4) var(--space-4);
    text-align: left;
    color: var(--text);
  }

  .brand__mark {
    display: flex;
    color: var(--text);
  }

  .brand__text {
    display: grid;
    gap: 1px;
    min-width: 0;
  }

  .brand__name {
    font-family: var(--font-display);
    font-size: 18px;
    font-weight: 600;
    letter-spacing: 0.02em;
    line-height: 1;
  }

  .brand__sub {
    font-family: var(--font-mono);
    font-size: 8px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--faint);
  }

  .nav {
    flex: 1 1 auto;
    padding: var(--space-2) 8px;
  }

  .nav ul {
    display: grid;
    gap: 2px;
  }

  .nav__item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    color: var(--muted);
    border-radius: var(--radius);
    text-align: left;
    transition:
      color var(--fast) var(--ease),
      background-color var(--fast) var(--ease);
  }

  .nav__item:hover:not(.nav__item--active) {
    color: var(--text);
    background: var(--row-hover);
  }

  .nav__index {
    width: 16px;
    flex: none;
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--faint);
  }

  .nav__label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-sm);
    font-weight: 450;
  }

  .nav__tag {
    flex: none;
    margin-left: auto;
    padding: 1px 4px;
    border: 1px solid var(--line);
    border-radius: 3px;
    font-family: var(--font-mono);
    font-size: 8px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--faint);
    line-height: 1.35;
  }

  .nav__item--active .nav__tag {
    color: var(--muted);
    border-color: var(--line-strong);
  }

  .nav__item--active {
    color: var(--text);
    background: var(--raised);
  }

  .nav__item--active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 2px;
    border-radius: 1px;
    background: var(--accent);
  }

  .nav__item--active .nav__index {
    color: var(--muted);
  }

  .sidebar__foot {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4) var(--space-4);
    border-top: 1px solid var(--line);
  }

  .profile-block {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .profile-block__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .profile-block__name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-sm);
    color: var(--text);
  }

  .profile-block__state {
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.1em;
  }

  .traffic {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 8px 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }

  .traffic__row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }

  .traffic dt {
    flex: none;
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--faint);
  }

  .traffic dd {
    overflow: hidden;
    color: var(--text);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .traffic__row--total {
    padding-top: 2px;
  }

  .traffic__row--total dt {
    color: var(--muted);
  }

  .traffic__row--total dd {
    color: var(--muted);
    font-size: 9px;
  }

  .traffic--idle dd {
    color: var(--faint);
  }

  .core {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 2px 0;
    text-align: left;
  }

  .core:hover .core__label {
    color: var(--text);
  }

  .core__dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--faint);
  }

  .core__dot--ok {
    background: var(--accent);
  }

  .core__dot--warn {
    background: var(--warning);
  }

  .core__dot--bad {
    background: var(--error);
  }

  .core__label {
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.06em;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .foot-version {
    margin-top: -4px;
    font-size: 9px;
    letter-spacing: 0.08em;
    color: var(--faint);
  }

  .foot-actions {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 30px;
    gap: 4px;
  }

  .foot-link {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 30px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    color: var(--muted);
    font-size: var(--fs-xs);
    text-align: left;
    transition:
      color var(--fast) var(--ease),
      background-color var(--fast) var(--ease),
      border-color var(--fast) var(--ease);
  }

  .foot-link:hover {
    color: var(--text);
    background: var(--raised);
    border-color: var(--line);
  }

  .foot-link--active {
    color: var(--text);
    background: var(--raised);
    border-color: var(--line);
  }

  .foot-link--icon {
    justify-content: center;
    padding: 0;
  }

  .foot-link__label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (max-width: 1080px) {
    .sidebar {
      align-items: center;
    }

    .brand {
      justify-content: center;
      padding: var(--space-4) 0;
    }

    .brand__text,
    .nav__index,
    .nav__label,
    .nav__tag,
    .profile-block,
    .traffic,
    .foot-version,
    .core__label,
    .foot-link__label {
      display: none;
    }

    .nav {
      width: 100%;
      padding: var(--space-2) 8px;
    }

    .nav__item {
      justify-content: center;
      gap: 0;
      padding: 10px 0;
    }

    .nav__item--active::before {
      left: -8px;
    }

    .sidebar__foot {
      width: 100%;
      gap: var(--space-2);
      padding: var(--space-3) 8px;
      justify-items: center;
    }

    .core {
      justify-content: center;
      padding: 4px 0 0;
    }

    .foot-actions {
      grid-template-columns: 1fr;
      width: 100%;
    }

    .foot-link {
      justify-content: center;
      padding: 0;
    }
  }
</style>
