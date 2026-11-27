<script lang="ts">
  import { onMount } from "svelte";
  import BottomStatus from "./lib/components/BottomStatus.svelte";
  import Mark from "./lib/components/Mark.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ToastHost from "./lib/components/ToastHost.svelte";
  import TopBar from "./lib/components/TopBar.svelte";
  import { statusWord } from "./lib/format";
  import { outboundStore } from "./lib/outbounds.svelte";
  import { app, type View } from "./lib/state.svelte";
  import Connections from "./lib/views/Connections.svelte";
  import Diagnostics from "./lib/views/Diagnostics.svelte";
  import Logs from "./lib/views/Logs.svelte";
  import Membrane from "./lib/views/Membrane.svelte";
  import Outbounds from "./lib/views/Outbounds.svelte";
  import Overview from "./lib/views/Overview.svelte";
  import Profiles from "./lib/views/Profiles.svelte";
  import Routing from "./lib/views/Routing.svelte";
  import Settings from "./lib/views/Settings.svelte";

  const titles: Record<View, string> = {
    overview: "Overview",
    membrane: "Membrane",
    profiles: "Profiles",
    routing: "Routing",
    outbounds: "Outbounds",
    connections: "Connections",
    logs: "Logs",
    diagnostics: "Diagnostics",
    settings: "Settings",
  };

  let scroller = $state<HTMLDivElement | null>(null);

  onMount(() => {
    void app.init();
    return () => {
      void app.dispose();
    };
  });

  $effect(() => {
    outboundStore.syncEngine(app.engine.status, app.engine.startedAt);
  });

  $effect(() => {
    void app.view;
    scroller?.scrollTo({ top: 0, behavior: "auto" });
  });

  $effect(() => {
    const theme = app.resolvedTheme;
    document.documentElement.dataset.theme = theme;
    document.documentElement.classList.toggle("dark", theme === "dark");
    try {
      localStorage.setItem("veil.theme.resolved", theme);
    } catch {
      /* first-paint cache is optional */
    }
  });

  $effect(() => {
    document.title = `Veil — ${titles[app.view]}`;
  });

  function navigate(view: View): void {
    app.setView(view);
    window.scrollTo({ top: 0, behavior: "auto" });
  }
</script>

{#if app.booting}
  <div class="boot" role="status" aria-live="polite">
    <div class="boot__mark"><Mark size={46} /></div>
    <span class="kicker">Veil</span>
    <h1 class="boot__title">Opening the network layer</h1>
    <p class="boot__text">Reading local profiles, core status, and desktop capabilities.</p>
    <div class="boot__rule" aria-hidden="true"></div>
    <p class="boot__mono mono">loading · local only</p>
  </div>
{:else if app.bootError}
  <div class="boot boot--error" role="alert">
    <div class="boot__mark"><Mark size={46} /></div>
    <span class="kicker">Startup failure</span>
    <h1 class="boot__title">Veil could not reach its backend</h1>
    <p class="boot__text">
      The window loaded, but the initial bootstrap command failed. No state was changed.
    </p>
    <pre class="code boot__error">{app.bootError}</pre>
    <div class="boot__actions">
      <button type="button" class="btn btn--primary" onclick={() => void app.retryBoot()}>
        Retry bootstrap
      </button>
    </div>
    <p class="boot__mono mono">The backend must be running inside the Tauri shell.</p>
  </div>
{:else}
  <div class="app-shell">
    <Sidebar />
    <main class="main">
      <TopBar />
      <div class="main-scroll" bind:this={scroller}>
        <div class="main-inner" class:main-inner--overview={app.view === "overview"}>
          {#if app.view === "overview"}
            <Overview {navigate} />
          {:else if app.view === "membrane"}
            <Membrane {navigate} />
          {:else if app.view === "profiles"}
            <Profiles {navigate} />
          {:else if app.view === "routing"}
            <Routing />
          {:else if app.view === "outbounds"}
            <Outbounds {navigate} />
          {:else if app.view === "connections"}
            <Connections {navigate} />
          {:else if app.view === "logs"}
            <Logs />
          {:else if app.view === "diagnostics"}
            <Diagnostics />
          {:else}
            <Settings />
          {/if}
        </div>
      </div>
      <BottomStatus />
    </main>
  </div>
  <p class="visually-hidden-live" aria-live="polite">
    Connection state: {statusWord(app.engine.status)}
  </p>
  <ToastHost />
{/if}

<style>
  .boot {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-6);
    text-align: center;
  }

  .boot__mark {
    color: var(--muted);
    margin-bottom: var(--space-3);
  }

  .boot__title {
    font-size: var(--fs-xl);
    max-width: 30ch;
  }

  .boot__text {
    color: var(--muted);
    font-size: var(--fs-sm);
    max-width: 52ch;
    margin-top: var(--space-1);
  }

  .boot__rule {
    width: 120px;
    height: 1px;
    background: var(--line-strong);
    margin: var(--space-4) 0 var(--space-2);
  }

  .boot__mono {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--faint);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .boot--error .boot__mark {
    color: var(--error);
  }

  .boot__error {
    max-width: 620px;
    width: 100%;
    text-align: left;
    margin: var(--space-3) 0 var(--space-2);
    white-space: pre-wrap;
    color: var(--error);
  }

  .boot__actions {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
