<script lang="ts">
  import { formatDelay, statusWord } from "../format";
  import { outboundStore } from "../outbounds.svelte";
  import { app } from "../state.svelte";
  import StatusGlyph from "./StatusGlyph.svelte";

  const profileName = $derived(
    app.connected
      ? (app.engine.profileName ?? "not reported")
      : (app.selectedProfile?.name ??
          (app.profiles.length ? "No profile selected" : "No profiles")),
  );
  const profileLabel = $derived(app.connected ? "Session" : "Next");
  const activeGroup = $derived(outboundStore.groups.find((group) => group.now !== null) ?? null);
  const activeRoute = $derived(app.connected ? (activeGroup?.now ?? null) : null);
  const latency = $derived(
    app.connected && app.stats.latency !== null ? formatDelay(app.stats.latency) : "not measured",
  );
  const feedback = $derived.by(() => {
    if (app.engine.status === "error") {
      return app.engine.error ?? "Engine reported a fault without detail.";
    }
    if (app.manualSession) return "Manual session · apps must use the mixed proxy.";
    if (app.engine.status === "starting") return "Opening the network layer.";
    if (app.engine.status === "stopping") return "Closing the session.";
    if (app.connected) return "Session live.";
    if (!app.profiles.length) return "No profiles yet · add one to begin.";
    if (!app.selectedProfile) return "No profile selected for the next session.";
    return "Idle · ready for the next session.";
  });
</script>

<footer class="statusbar" aria-label="Connection status">
  <span class="statusbar__state st-{app.engine.status}" aria-live="polite" aria-atomic="true">
    <StatusGlyph status={app.engine.status} size={10} />
    <span>{statusWord(app.engine.status)}</span>
  </span>

  <span class="statusbar__sep" aria-hidden="true"></span>
  <span class="statusbar__item">
    <span class="statusbar__label">{profileLabel}</span>
    <span class="statusbar__value truncate" title={profileName}>{profileName}</span>
  </span>

  {#if app.connected}
    <span class="statusbar__sep" aria-hidden="true"></span>
    <span class="statusbar__item statusbar__route">
      <span class="statusbar__label">Route</span>
      <span
        class="statusbar__value truncate"
        title={activeGroup
          ? `${activeGroup.tag} → ${activeRoute ?? "not reported"}`
          : "No selector group reported by the engine."}
      >
        {activeRoute ?? "not reported"}
      </span>
    </span>
  {/if}

  <span class="statusbar__sep" aria-hidden="true"></span>
  <span class="statusbar__item">
    <span class="statusbar__label">Latency</span>
    <span class="statusbar__value num">{latency}</span>
  </span>

  <span
    class="statusbar__feedback truncate"
    class:statusbar__feedback--error={app.engine.status === "error"}
    class:statusbar__feedback--warning={app.manualSession && app.engine.status !== "error"}
    title={feedback}
  >
    {feedback}
  </span>
</footer>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: var(--status-h);
    min-width: 0;
    padding: 0 var(--space-4);
    border-top: 1px solid var(--line);
    background: var(--panel);
    overflow: hidden;
  }

  .statusbar__state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.12em;
  }

  .statusbar__sep {
    width: 1px;
    height: 12px;
    flex: none;
    background: var(--line);
  }

  .statusbar__item {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 240px;
    flex: 0 1 auto;
  }

  .statusbar__label {
    flex: none;
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--faint);
  }

  .statusbar__value {
    min-width: 0;
    font-size: var(--fs-xs);
    color: var(--text);
  }

  .statusbar__route {
    max-width: 200px;
  }

  .statusbar__feedback {
    min-width: 0;
    margin-left: auto;
    text-align: right;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .statusbar__feedback--error {
    color: var(--error);
  }

  .statusbar__feedback--warning {
    color: var(--warning);
  }

  @media (max-width: 1080px) {
    .statusbar {
      gap: 8px;
    }
  }

  @media (max-width: 900px) {
    .statusbar__route {
      display: none;
    }
  }
</style>
