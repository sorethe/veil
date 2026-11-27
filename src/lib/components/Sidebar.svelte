<script lang="ts">
  import { statusWord } from "../format";
  import { app, type View } from "../state.svelte";
  import type { IconName } from "../icons";
  import Icon from "./Icon.svelte";
  import Mark from "./Mark.svelte";
  import StatusGlyph from "./StatusGlyph.svelte";

  const nav: { id: View; label: string; icon: IconName }[] = [
    { id: "overview", label: "Overview", icon: "overview" },
    { id: "profiles", label: "Profiles", icon: "profiles" },
    { id: "routing", label: "Routing", icon: "routing" },
    { id: "outbounds", label: "Outbounds", icon: "outbounds" },
    { id: "connections", label: "Connections", icon: "connections" },
    { id: "logs", label: "Logs", icon: "logs" },
    { id: "diagnostics", label: "Diagnostics", icon: "diagnostics" },
  ];

  const activeProfile = $derived(
    app.profiles.find(
      (profile) =>
        app.engine.profileId !== null && String(profile.id) === String(app.engine.profileId),
    ) ?? null,
  );
  const profileName = $derived(
    app.engine.profileName ??
      activeProfile?.name ??
      (app.profiles.length ? "No active session" : "No profile"),
  );
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
</script>

<aside class="sidebar" aria-label="Application">
  <button
    type="button"
    class="brand"
    onclick={() => app.setView("overview")}
    aria-label="Veil overview"
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
            aria-label={item.label}
            title={item.label}
            onclick={() => app.setView(item.id)}
          >
            <span class="nav__index" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
            <Icon name={item.icon} size={16} />
            <span class="nav__label">{item.label}</span>
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="sidebar__foot">
    <div class="profile-block">
      <div class="profile-block__head">
        <span class="kicker">Active profile</span>
        <StatusGlyph status={app.engine.status} size={9} />
      </div>
      <p class="profile-block__name" title={profileName}>{profileName}</p>
      <p class="profile-block__state {`st-${app.engine.status}`}">
        {statusWord(app.engine.status)}{#if app.engine.mode}
          · {app.engine.mode === "tun" ? "TUN" : "SYSTEM"}{/if}
      </p>
    </div>

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

    <div class="foot-actions">
      <button
        type="button"
        class="foot-link"
        class:foot-link--active={app.view === "settings"}
        aria-label="Settings"
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
    position: fixed;
    inset: 0 auto 0 0;
    width: var(--sidebar-w);
    background: var(--surface);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    z-index: 20;
    overflow-y: auto;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: var(--space-5) var(--space-4) var(--space-4);
    text-align: left;
    color: var(--text);
    width: 100%;
  }

  .brand__mark {
    color: var(--text);
    display: flex;
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
    padding: var(--space-3) 8px;
    flex: 1 1 auto;
  }

  .nav ul {
    display: grid;
    gap: 1px;
  }

  .nav__item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 8px 8px 10px;
    color: var(--muted);
    border-radius: 1px;
    text-align: left;
    transition:
      color var(--fast) var(--ease),
      background-color var(--fast) var(--ease);
  }

  .nav__item:hover:not(.nav__item--active) {
    color: var(--text);
    background: transparent;
  }

  .nav__index {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--faint);
    width: 16px;
    flex: none;
  }

  .nav__label {
    font-size: var(--fs-sm);
    font-weight: 450;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .nav__item--active {
    color: var(--text);
    background: var(--raised);
  }

  .nav__item--active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 5px;
    bottom: 5px;
    width: 2px;
    background: var(--accent);
  }

  .nav__item--active .nav__index {
    color: var(--accent);
  }

  .sidebar__foot {
    border-top: 1px solid var(--line);
    padding: var(--space-3) var(--space-4) var(--space-4);
    display: grid;
    gap: var(--space-3);
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
    font-size: var(--fs-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .profile-block__state {
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.1em;
  }

  .core {
    display: flex;
    align-items: center;
    gap: 7px;
    text-align: left;
    padding: 6px 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  .core:hover .core__label {
    color: var(--text);
  }

  .core__dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--faint);
    flex: none;
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
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.06em;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
    color: var(--muted);
    border: 1px solid transparent;
    border-radius: var(--radius);
    font-size: var(--fs-xs);
    text-align: left;
  }

  .foot-link:hover {
    color: var(--text);
    background: var(--raised);
    border-color: var(--line);
  }

  .foot-link--active {
    color: var(--text);
  }

  .foot-link--icon {
    justify-content: center;
    padding: 0;
  }

  .foot-link__label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @media (max-width: 1080px) {
    .sidebar {
      width: 64px;
      align-items: center;
    }

    .brand {
      padding: var(--space-4) 0;
      justify-content: center;
    }

    .brand__text,
    .nav__index,
    .nav__label,
    .profile-block,
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
      padding: 10px 0;
      gap: 0;
    }

    .nav__item--active::before {
      left: -8px;
    }

    .sidebar__foot {
      width: 100%;
      padding: var(--space-3) 8px var(--space-3);
      justify-items: center;
    }

    .core {
      border-top: 0;
      padding: 4px 0 0;
    }

    .core__dot {
      margin-top: 0;
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
