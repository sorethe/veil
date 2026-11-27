<script module lang="ts">
  const countCache = new Map<string, number | { error: string }>();
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { api, type Profile } from "../api";
  import EmptyState from "../components/EmptyState.svelte";
  import Field from "../components/Field.svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import ProfileEditor from "../components/ProfileEditor.svelte";
  import ProfileImport from "../components/ProfileImport.svelte";
  import { errorMessage, formatBytes, formatDateTime, formatRelative, hostOf } from "../format";
  import { app, type View } from "../state.svelte";

  let { navigate }: { navigate: (view: View) => void } = $props();

  let importOpen = $state(false);
  let editing = $state<Profile | null>(null);
  let duplicateOf = $state<Profile | null>(null);
  let duplicateName = $state("");
  let duplicateError = $state<string | null>(null);
  let duplicateBusy = $state(false);
  let deleting = $state<Profile | null>(null);
  let deleteError = $state<string | null>(null);
  let deleteBusy = $state(false);
  let refreshBusy = $state(false);
  let subscriptionBusy = $state<string | null>(null);
  let counts = $state<Record<string, number | "loading" | { error: string }>>({});
  let countGeneration = 0;

  const total = $derived(app.profiles.length);
  const failedWithRuntime = $derived(app.engine.status === "error" && app.engine.pid !== null);
  const autoUpdateNote =
    "Auto-update is scheduled by the backend. Per-profile scheduling cannot be changed from this build.";

  $effect(() => {
    const signature = app.profiles.map((profile) => `${profile.id}:${profile.updatedAt}`).join("|");
    if (!signature) return;
    untrack(() => {
      void computeCounts(app.profiles);
    });
  });

  async function computeCounts(profiles: Profile[]): Promise<void> {
    const generation = ++countGeneration;
    for (const profile of profiles) {
      const key = countKey(profile);
      if (counts[key] !== undefined) continue;
      const cached = countCache.get(key);
      if (cached !== undefined) {
        counts[key] = cached;
        continue;
      }
      counts[key] = "loading";
      try {
        const result = await api.validateContent(profile.content);
        if (generation !== countGeneration) return;
        const value = result.ok
          ? result.outbounds.length
          : { error: result.error ?? "The backend rejected this config." };
        counts[key] = value;
        countCache.set(key, value);
      } catch (cause) {
        if (generation !== countGeneration) return;
        const value = { error: errorMessage(cause) };
        counts[key] = value;
        countCache.set(key, value);
      }
    }
  }

  function countKey(profile: Profile): string {
    return `${String(profile.id)}:${profile.updatedAt}`;
  }

  function countLabel(profile: Profile): string {
    const state = counts[countKey(profile)];
    if (state === undefined || state === "loading") return "…";
    if (typeof state === "number") return String(state);
    return "—";
  }

  function countTitle(profile: Profile): string {
    const state = counts[countKey(profile)];
    if (state && typeof state === "object") return state.error;
    return "Outbound count derived from backend validation.";
  }

  function countFailed(profile: Profile): boolean {
    const state = counts[countKey(profile)];
    return state !== undefined && state !== "loading" && typeof state === "object";
  }

  function isCurrent(profile: Profile): boolean {
    return app.engine.profileId !== null && String(app.engine.profileId) === String(profile.id);
  }

  function connectDisabled(profile: Profile): boolean {
    return (
      app.busy ||
      app.connectBusy ||
      failedWithRuntime ||
      !app.clashApiReady ||
      (app.connected && !isCurrent(profile))
    );
  }

  function connectTitle(profile: Profile): string {
    if (failedWithRuntime) return "Stop the failed engine process before connecting.";
    if (!app.clashApiReady) return "The engine core lacks the Clash API required by Veil.";
    if (app.connected && !isCurrent(profile)) return "Disconnect before switching profiles.";
    if (app.busy || app.connectBusy) return "A session transition is in progress.";
    if (isCurrent(profile) && app.connected) return "Open the live session.";
    return "Connect with this profile.";
  }

  function sourceLabel(profile: Profile): string {
    if (profile.kind === "subscription") {
      return profile.subscriptionUrl ? hostOf(profile.subscriptionUrl) : profile.source || "subscription";
    }
    if (profile.kind === "links") return "share links";
    if (profile.source && profile.source !== "inline" && profile.source !== "manual") {
      return profile.source;
    }
    return "local config";
  }

  async function refreshList(): Promise<void> {
    refreshBusy = true;
    await app.refreshProfiles();
    refreshBusy = false;
  }

  async function connect(profile: Profile): Promise<void> {
    if (isCurrent(profile) && app.connected) {
      navigate("overview");
      return;
    }
    if (app.busy || app.connectBusy) return;
    const ok = await app.connect(profile.id, app.settings?.mode ?? "system");
    if (ok) navigate("overview");
  }

  async function refreshSubscription(profile: Profile): Promise<void> {
    if (subscriptionBusy) return;
    subscriptionBusy = String(profile.id);
    try {
      const updated = await api.refreshSubscription(profile.id);
      await app.refreshProfiles();
      app.notify("success", "Subscription refreshed", updated.name);
    } catch (error) {
      app.notify("error", "Subscription refresh failed", errorMessage(error));
    } finally {
      subscriptionBusy = null;
    }
  }

  async function exportAll(): Promise<void> {
    try {
      const path = await api.exportProfiles();
      app.notify("success", "Profiles exported", path);
    } catch (error) {
      app.notify("error", "Export failed", errorMessage(error));
    }
  }

  function startDuplicate(profile: Profile): void {
    duplicateOf = profile;
    duplicateName = `${profile.name} copy`;
    duplicateError = null;
  }

  async function confirmDuplicate(): Promise<void> {
    if (!duplicateOf || duplicateBusy) return;
    duplicateBusy = true;
    duplicateError = null;
    try {
      const created = await api.duplicateProfile(duplicateOf.id, duplicateName.trim());
      await app.refreshProfiles();
      app.notify("success", "Profile duplicated", created.name);
      duplicateOf = null;
    } catch (error) {
      duplicateError = errorMessage(error);
    } finally {
      duplicateBusy = false;
    }
  }

  async function confirmDelete(): Promise<void> {
    if (!deleting || deleteBusy) return;
    deleteBusy = true;
    deleteError = null;
    try {
      await api.deleteProfile(deleting.id);
      await app.refreshProfiles();
      app.notify("success", "Profile deleted", deleting.name);
      deleting = null;
    } catch (error) {
      deleteError = errorMessage(error);
    } finally {
      deleteBusy = false;
    }
  }

  async function onImported(): Promise<void> {
    importOpen = false;
    await app.refreshProfiles();
  }
</script>

<div class="page-head">
  <div class="page-head__text">
    <span class="kicker">Library</span>
    <h1 class="page-title">Profiles</h1>
    <p class="page-desc">
      Configurations are stored locally and validated by the engine before use. One profile is applied per
      session.
    </p>
  </div>
  <div class="head-actions">
    <button type="button" class="btn" disabled={!total} onclick={() => void exportAll()}>
      <Icon name="export" size={12} /> Export all
    </button>
    <button type="button" class="btn btn--primary" onclick={() => (importOpen = true)}>
      <Icon name="import" size={12} /> Import
    </button>
  </div>
</div>

{#if !app.clashApiReady && !app.connected}
  <div class="banner banner--warning profiles-banner" role="alert">
    <Icon name="warning" size={15} />
    <div class="grow">
      <p class="banner__title">Engine cannot be started</p>
      <p class="banner__body">
        {app.capabilities === null
          ? "The backend did not report engine capabilities."
          : "The core lacks the Clash API capability Veil needs for session health and telemetry."}
      </p>
    </div>
    <button type="button" class="btn btn--sm" onclick={() => navigate("diagnostics")}>Diagnostics</button>
  </div>
{/if}

{#if failedWithRuntime}
  <div class="banner banner--warning profiles-banner" role="alert">
    <Icon name="warning" size={15} />
    <div class="grow">
      <p class="banner__title">Engine failed but its process is still running</p>
      <p class="banner__body">
        {app.engine.error ?? "The backend reported an error while stopping the engine."}
        Stop the process before starting a new session.
      </p>
    </div>
    <button type="button" class="btn btn--sm" disabled={app.connectBusy} onclick={() => void app.disconnect()}>
      Stop engine
    </button>
  </div>
{/if}

{#if !total}
  <section class="panel">
    <EmptyState
      title="No profiles yet"
      description="Import a sing-box config, subscription, share links, or QR image to create the first profile."
      icon="profiles"
    >
      <button type="button" class="btn btn--primary" onclick={() => (importOpen = true)}>
        <Icon name="import" size={12} /> Import profile
      </button>
    </EmptyState>
  </section>
{:else}
  <section class="panel" aria-label="Profiles">
    <div class="panel__head">
      <span class="panel__title">Profiles · {total}</span>
      <div class="row">
        <button
          type="button"
          class="btn btn--sm btn--quiet"
          disabled={refreshBusy}
          onclick={() => void refreshList()}
        >
          <Icon name="refresh" size={12} /> {refreshBusy ? "Refreshing" : "Refresh"}
        </button>
      </div>
    </div>
    <div class="table-wrap">
      <table class="table profiles">
        <colgroup>
          <col style="width: 23%" />
          <col style="width: 17%" />
          <col style="width: 8%" />
          <col style="width: 14%" />
          <col style="width: 10%" />
          <col style="width: 9%" />
          <col style="width: 19%" />
        </colgroup>
        <thead>
          <tr>
            <th>Profile</th>
            <th>Source</th>
            <th class="ta-r">Out</th>
            <th>Updated</th>
            <th>State</th>
            <th>Auto</th>
            <th class="ta-r">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each app.profiles as profile (String(profile.id))}
            <tr>
              <td>
                <div class="profile-name">
                  <span class="profile-name__title" title={profile.name}>{profile.name}</span>
                  <span class="profile-name__id mono">#{String(profile.id)} · {profile.kind}</span>
                </div>
              </td>
              <td>
                <span class="mono detail truncated" title={sourceLabel(profile)}>
                  {sourceLabel(profile)}
                </span>
                {#if profile.lastError}
                  <span class="profile-error truncated" title={profile.lastError}>{profile.lastError}</span>
                {/if}
              </td>
              <td class="ta-r">
                <span
                  class="mono detail"
                  class:danger={countFailed(profile)}
                  title={countTitle(profile)}
                >
                  {countLabel(profile)}
                </span>
              </td>
              <td>
                <div class="updated">
                  <span class="detail" title={formatDateTime(profile.updatedAt)}>{formatRelative(profile.updatedAt)}</span>
                  <span class="meta-sub mono">{formatBytes(profile.size)}</span>
                </div>
              </td>
              <td>
                <div class="state-cell">
                  {#if isCurrent(profile)}
                    <span class="tag tag--accent">{app.connected ? "live" : "current"}</span>
                  {/if}
                  {#if profile.lastError}
                    <span class="tag tag--error" title={profile.lastError}>error</span>
                  {/if}
                  {#if !isCurrent(profile) && !profile.lastError}
                    <span class="faint small">—</span>
                  {/if}
                </div>
              </td>
              <td>
                {#if profile.kind === "subscription"}
                  <div class="auto-cell" title={autoUpdateNote}>
                    <span class="tag {profile.autoUpdate ? 'tag--accent' : ''}">
                      {profile.autoUpdate ? "on" : "off"}
                    </span>
                  </div>
                {:else}
                  <span class="faint small">—</span>
                {/if}
              </td>
              <td>
                <div class="row-actions">
                  <button
                    type="button"
                    class="icon-btn"
                    class:row-actions__live={isCurrent(profile) && app.connected}
                    aria-label={isCurrent(profile) && app.connected ? `Open ${profile.name}` : `Connect ${profile.name}`}
                    title={connectTitle(profile)}
                    disabled={connectDisabled(profile)}
                    onclick={() => void connect(profile)}
                  >
                    <Icon name="power" size={14} />
                  </button>
                  <button
                    type="button"
                    class="icon-btn"
                    aria-label={`Edit ${profile.name}`}
                    title="Edit profile"
                    onclick={() => (editing = profile)}
                  >
                    <Icon name="edit" size={14} />
                  </button>
                  <button
                    type="button"
                    class="icon-btn"
                    aria-label={`Duplicate ${profile.name}`}
                    title="Duplicate profile"
                    onclick={() => startDuplicate(profile)}
                  >
                    <Icon name="copy" size={14} />
                  </button>
                  {#if profile.kind === "subscription"}
                    <button
                      type="button"
                      class="icon-btn"
                      aria-label={`Refresh subscription ${profile.name}`}
                      title="Refresh subscription"
                      disabled={subscriptionBusy !== null}
                      onclick={() => void refreshSubscription(profile)}
                    >
                      <Icon name="refresh" size={14} />
                    </button>
                  {/if}
                  <button
                    type="button"
                    class="icon-btn icon-btn--danger"
                    aria-label={`Delete ${profile.name}`}
                    title="Delete profile"
                    onclick={() => {
                      deleting = profile;
                      deleteError = null;
                    }}
                  >
                    <Icon name="trash" size={14} />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <div class="panel__foot">
      <p class="small faint note">{autoUpdateNote}</p>
      {#if app.connected}
        <p class="small faint note">A live session is running; disconnect before switching profiles.</p>
      {/if}
    </div>
  </section>
{/if}

{#if importOpen}
  <ProfileImport onclose={() => (importOpen = false)} onimported={() => void onImported()} />
{/if}

{#if editing}
  <ProfileEditor
    profile={editing}
    onclose={() => (editing = null)}
    onsaved={() => {
      editing = null;
      void refreshList();
    }}
  />
{/if}

{#if duplicateOf}
  <Modal title="Duplicate profile" subtitle={duplicateOf.name} onclose={() => (duplicateOf = null)} width="440px">
    <div class="stack">
      <Field label="New name" forId="duplicate-name">
        <input
          id="duplicate-name"
          class="input"
          bind:value={duplicateName}
          onkeydown={(event) => {
            if (event.key === "Enter") void confirmDuplicate();
          }}
        />
      </Field>
      {#if duplicateError}<p class="modal-error" role="alert">{duplicateError}</p>{/if}
    </div>
    {#snippet footer()}
      <button type="button" class="btn" onclick={() => (duplicateOf = null)}>Cancel</button>
      <button
        type="button"
        class="btn btn--primary"
        disabled={duplicateBusy || !duplicateName.trim()}
        onclick={() => void confirmDuplicate()}
      >
        {duplicateBusy ? "Creating" : "Duplicate"}
      </button>
    {/snippet}
  </Modal>
{/if}

{#if deleting}
  <Modal
    title="Delete profile"
    subtitle={`${deleting.name} · #${String(deleting.id)}`}
    onclose={() => (deleting = null)}
    width="440px"
  >
    <div class="stack">
      <p class="small">
        This removes the stored config from Veil. A running session is not stopped automatically; disconnect
        first if this profile is live.
      </p>
      {#if deleteError}<p class="modal-error" role="alert">{deleteError}</p>{/if}
    </div>
    {#snippet footer()}
      <button type="button" class="btn" onclick={() => (deleting = null)}>Cancel</button>
      <button
        type="button"
        class="btn btn--danger"
        disabled={deleteBusy}
        onclick={() => void confirmDelete()}
      >
        {deleteBusy ? "Deleting" : "Delete profile"}
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  .profiles {
    table-layout: fixed;
    min-width: 720px;
  }

  .profiles-banner {
    margin-bottom: var(--space-4);
  }

  .profiles th,
  .profiles td {
    padding: 8px 10px;
  }

  .profiles th:last-child,
  .profiles td:last-child {
    text-align: right;
  }

  .profile-name {
    display: grid;
    gap: 1px;
    min-width: 0;
  }

  .profile-name__title {
    font-size: var(--fs-sm);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .profile-name__id {
    font-size: var(--fs-2xs);
    color: var(--faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail {
    font-size: var(--fs-xs);
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .detail.danger {
    color: var(--error);
  }

  .truncated {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .updated {
    display: grid;
    gap: 1px;
    min-width: 0;
  }

  .meta-sub {
    font-size: 9px;
    color: var(--faint);
  }

  .profile-error {
    display: block;
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--error);
    margin-top: 1px;
  }

  .state-cell {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    align-items: center;
    min-width: 0;
  }

  .auto-cell {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .row-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    justify-content: flex-end;
  }

  .row-actions :global(.icon-btn) {
    width: 24px;
    height: 24px;
  }

  .row-actions__live {
    color: var(--accent);
  }

  .note {
    max-width: 90ch;
  }

  .modal-error {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--error);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
</style>
