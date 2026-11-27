<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Diagnostics } from "../api";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import { copyText, errorMessage, formatClock, formatDateTime } from "../format";
  import { app } from "../state.svelte";

  let diagnostics = $state<Diagnostics | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let lastRun = $state<number | null>(null);
  let copying = $state(false);

  onMount(() => {
    void run();
  });

  function yesNo(value: boolean): string {
    return value ? "yes" : "no";
  }

  function triState(value: string | null): string {
    return value && value.trim() ? value : "—";
  }

  async function run(): Promise<void> {
    loading = true;
    error = null;
    try {
      diagnostics = await api.runDiagnostics();
      lastRun = Date.now();
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  function buildReport(data: Diagnostics): string {
    const lines: string[] = [];
    lines.push("Veil diagnostics");
    lines.push(`Generated: ${new Date().toISOString()}`);
    lines.push(`Veil version: ${app.version || "—"}`);
    lines.push(`Desktop: ${app.platform?.desktop ?? "—"} (${app.platform?.session ?? "—"})`);
    lines.push(`Engine status: ${app.engine.status}`);
    if (app.engine.error) lines.push(`Engine error: ${app.engine.error}`);
    lines.push("");
    lines.push(`Core path: ${data.core.path ?? "—"}`);
    lines.push(`Core version: ${data.core.version ?? "—"}`);
    lines.push(`Core ok: ${yesNo(data.core.ok)}`);
    if (data.core.error) lines.push(`Core error: ${data.core.error}`);
    lines.push(`Engine: ${app.engineInfo?.id ?? "—"} (${app.engineInfo?.api ?? "—"} api)`);
    lines.push("");
    lines.push("Engine capabilities");
    lines.push(`  clash api: ${yesNo(data.capabilities.clashApi)}`);
    lines.push(`  tun: ${yesNo(data.capabilities.tun)}`);
    lines.push(`  auto route: ${yesNo(data.capabilities.autoRoute)}`);
    lines.push(`  auto redirect: ${yesNo(data.capabilities.autoRedirect)}`);
    lines.push(`  nftables: ${yesNo(data.capabilities.nftables)}`);
    lines.push(`  ipv6: ${yesNo(data.capabilities.ipv6)}`);
    lines.push(`  dns hijack: ${yesNo(data.capabilities.dnsHijack)}`);
    lines.push(
      `  system proxy backends: ${
        data.capabilities.systemProxyBackends.length
          ? data.capabilities.systemProxyBackends.join(", ")
          : "none detected"
      }`,
    );
    lines.push("");
    lines.push("Permissions");
    lines.push(`  user: ${data.permissions.user}`);
    lines.push(`  root: ${yesNo(data.permissions.root)}`);
    lines.push(`  helper permission: ${yesNo(data.permissions.helper)}`);
    lines.push(`  polkit: ${yesNo(data.permissions.polkit)}`);
    lines.push(`  tun device: ${yesNo(data.permissions.tunDevice)}`);
    lines.push(`  nft: ${yesNo(data.permissions.nft)}`);
    lines.push("");
    lines.push(`Helper installed: ${yesNo(data.helper.installed)} (${data.helper.path ?? "—"})`);
    lines.push(`TUN present: ${yesNo(data.tun.present)} (${data.tun.name ?? "—"})`);
    lines.push(`DNS mode: ${data.dns.mode}`);
    lines.push(`DNS servers: ${data.dns.servers.length ? data.dns.servers.join(", ") : "—"}`);
    lines.push(`Tray: ${yesNo(data.tray.available)} — ${data.tray.detail}`);
    lines.push("");
    lines.push("Interfaces");
    for (const iface of data.interfaces) {
      lines.push(
        `  ${iface.name}${iface.up ? " up" : " down"}${iface.default ? " default" : ""} — ${
          iface.addresses.length ? iface.addresses.join(", ") : "no addresses"
        }`,
      );
    }
    lines.push("");
    lines.push("Errors");
    if (data.errors.length) {
      for (const item of data.errors) lines.push(`  ${item}`);
    } else {
      lines.push("  none");
    }
    return lines.join("\n");
  }

  async function copyReport(): Promise<void> {
    if (!diagnostics || copying) return;
    copying = true;
    const ok = await copyText(buildReport(diagnostics));
    copying = false;
    if (ok) app.notify("success", "Diagnostics copied");
    else app.notify("error", "Copy failed", "The webview clipboard is not available.");
  }
</script>

<div class="page-head">
  <div class="page-head__text">
    <span class="kicker">Diagnostics</span>
    <h1 class="page-title">System report</h1>
    <p class="page-desc">
      A read-only probe of the core binary, desktop permissions, interfaces, DNS, and tray support. Nothing
      here changes system state.
    </p>
  </div>
  <div class="head-actions">
    {#if lastRun}<span class="mono small faint">last run {formatClock(lastRun)}</span>{/if}
    <button type="button" class="btn" disabled={!diagnostics || copying} onclick={() => void copyReport()}>
      <Icon name="copy" size={12} /> {copying ? "Copying" : "Copy report"}
    </button>
    <button type="button" class="btn btn--primary" disabled={loading} onclick={() => void run()}>
      <Icon name="refresh" size={12} /> {loading ? "Probing" : "Re-run"}
    </button>
  </div>
</div>

{#if error}
  <div class="banner banner--error" role="alert">
    <Icon name="error" size={16} />
    <div class="grow">
      <p class="banner__title">Diagnostics failed</p>
      <p class="banner__body mono">{error}</p>
    </div>
    <button type="button" class="btn btn--sm" onclick={() => void run()}>Retry</button>
  </div>
{/if}

{#if loading && !diagnostics}
  <section class="panel">
    <div class="panel__body"><p class="muted small">Probing the core, permissions, and interfaces…</p></div>
  </section>
{:else if !diagnostics}
  <section class="panel">
    <EmptyState
      title="No system report"
      description="The backend did not return a diagnostics report. Re-run once the engine and core are available."
      icon="diagnostics"
    >
      <button type="button" class="btn btn--sm" onclick={() => void run()}>Run diagnostics</button>
    </EmptyState>
  </section>
{:else}
  {#if !diagnostics.core.ok}
    <div class="banner banner--error diag-banner">
      <Icon name="error" size={16} />
      <div class="grow">
        <p class="banner__title">Core unavailable</p>
        <p class="banner__body mono">
          {diagnostics.core.error ?? "The core binary was not found or did not report a version."}
        </p>
      </div>
    </div>
  {/if}

  <div class="diag-grid">
    <section class="panel" aria-label="Core">
      <div class="panel__head"><span class="panel__title">Core</span><span class="tag">{diagnostics.core.ok ? "ok" : "failed"}</span></div>
      <div class="panel__body">
        <dl class="deflist">
          <div class="deflist__row">
            <dt class="deflist__term">Engine</dt>
            <dd class="deflist__value mono small">
              {app.engineInfo?.id ?? "—"} · API {app.engineInfo?.api ?? "—"}
            </dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Binary path</dt>
            <dd class="deflist__value mono small break">{triState(diagnostics.core.path)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Version</dt>
            <dd class="deflist__value mono small">{triState(diagnostics.core.version)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Status</dt>
            <dd class="deflist__value mono small">{diagnostics.core.ok ? "responds normally" : "unavailable"}</dd>
          </div>
          {#if diagnostics.core.error}
            <div class="deflist__row">
              <dt class="deflist__term">Error</dt>
              <dd class="deflist__value mono small danger break">{diagnostics.core.error}</dd>
            </div>
          {/if}
        </dl>
      </div>
    </section>

    <section class="panel capabilities-panel" aria-label="Engine capabilities">
      <div class="panel__head">
        <span class="panel__title">Engine capabilities</span>
        <span class="tag">from backend detection</span>
      </div>
      <div class="panel__body">
        <dl class="deflist capability-list">
          <div class="deflist__row">
            <dt class="deflist__term">Clash API</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.capabilities.clashApi)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">TUN</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.capabilities.tun)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Auto-route</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.capabilities.autoRoute)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Auto-redirect</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.capabilities.autoRedirect)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">nftables</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.capabilities.nftables)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">IPv6</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.capabilities.ipv6)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">DNS hijack</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.capabilities.dnsHijack)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">System proxy backends</dt>
            <dd class="deflist__value">
              {#if diagnostics.capabilities.systemProxyBackends.length}
                <div class="backends">
                  {#each diagnostics.capabilities.systemProxyBackends as backend (backend)}
                    <span class="tag">{backend}</span>
                  {/each}
                </div>
              {:else}
                <span class="mono small faint">none detected</span>
              {/if}
            </dd>
          </div>
        </dl>
      </div>
    </section>

    <section class="panel" aria-label="Permissions">
      <div class="panel__head"><span class="panel__title">Permissions</span><span class="tag">user {diagnostics.permissions.user}</span></div>
      <div class="panel__body">
        <dl class="deflist">
          <div class="deflist__row">
            <dt class="deflist__term">Running as root</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.permissions.root)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Helper permission</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.permissions.helper)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Polkit available</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.permissions.polkit)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">TUN device access</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.permissions.tunDevice)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">nftables</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.permissions.nft)}</dd>
          </div>
        </dl>
      </div>
    </section>

    <section class="panel" aria-label="TUN and helper">
      <div class="panel__head"><span class="panel__title">TUN & helper</span></div>
      <div class="panel__body">
        <dl class="deflist">
          <div class="deflist__row">
            <dt class="deflist__term">TUN interface present</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.tun.present)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">TUN name</dt>
            <dd class="deflist__value mono small">{triState(diagnostics.tun.name)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Helper installed</dt>
            <dd class="deflist__value mono small">{yesNo(diagnostics.helper.installed)}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Helper path</dt>
            <dd class="deflist__value mono small break">{triState(diagnostics.helper.path)}</dd>
          </div>
        </dl>
      </div>
    </section>

    <section class="panel" aria-label="DNS">
      <div class="panel__head"><span class="panel__title">DNS</span><span class="tag">{diagnostics.dns.mode}</span></div>
      <div class="panel__body">
        {#if diagnostics.dns.servers.length}
          <ul class="server-list">
            {#each diagnostics.dns.servers as server (server)}
              <li class="mono small break">{server}</li>
            {/each}
          </ul>
        {:else}
          <p class="muted small">No DNS servers reported by the engine.</p>
        {/if}
      </div>
    </section>

    <section class="panel" aria-label="Tray">
      <div class="panel__head"><span class="panel__title">Tray</span><span class="tag">{diagnostics.tray.available ? "available" : "unavailable"}</span></div>
      <div class="panel__body">
        <p class="small break">{diagnostics.tray.detail || "No tray detail was reported."}</p>
        <p class="small faint tray-note">
          Minimize-to-tray is only offered in Settings when the backend reports a usable tray.
        </p>
      </div>
    </section>

    <section class="panel" aria-label="Platform">
      <div class="panel__head"><span class="panel__title">Desktop</span><span class="tag">platform</span></div>
      <div class="panel__body">
        <dl class="deflist">
          <div class="deflist__row">
            <dt class="deflist__term">Desktop</dt>
            <dd class="deflist__value mono small">{app.platform?.desktop ?? "—"}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Session</dt>
            <dd class="deflist__value mono small">{app.platform?.session ?? "—"}</dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">System proxy</dt>
            <dd class="deflist__value mono small">
              {app.platform?.systemProxySupported ? app.platform.systemProxyKind : "not available"}
            </dd>
          </div>
          <div class="deflist__row">
            <dt class="deflist__term">Veil version</dt>
            <dd class="deflist__value mono small">{app.version || "—"}</dd>
          </div>
        </dl>
      </div>
    </section>
  </div>

  <section class="panel" aria-label="Interfaces">
    <div class="panel__head">
      <span class="panel__title">Interfaces · {diagnostics.interfaces.length}</span>
      <span class="mono small faint">default routes are marked</span>
    </div>
    {#if diagnostics.interfaces.length}
      <div class="table-wrap">
        <table class="table">
          <thead>
            <tr>
              <th>Interface</th>
              <th>Addresses</th>
              <th>State</th>
              <th class="ta-r">Role</th>
            </tr>
          </thead>
          <tbody>
            {#each diagnostics.interfaces as iface (iface.name)}
              <tr>
                <td><span class="mono small">{iface.name}</span></td>
                <td>
                  {#if iface.addresses.length}
                    <div class="addresses">
                      {#each iface.addresses as address (address)}
                        <span class="mono small faint break">{address}</span>
                      {/each}
                    </div>
                  {:else}
                    <span class="faint small">no addresses</span>
                  {/if}
                </td>
                <td><span class="tag {iface.up ? 'tag--accent' : ''}">{iface.up ? "up" : "down"}</span></td>
                <td class="ta-r">
                  {#if iface.default}<span class="tag">default</span>{:else}<span class="faint small">—</span>{/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <EmptyState title="No interfaces reported" description="The backend returned an empty interface list." icon="diagnostics" />
    {/if}
  </section>

  <section class="panel errors-panel" aria-label="Recent errors">
    <div class="panel__head">
      <span class="panel__title">Recent errors · {diagnostics.errors.length}</span>
    </div>
    <div class="panel__body">
      {#if diagnostics.errors.length || app.engine.error || diagnostics.core.error}
        <ul class="error-list">
          {#each diagnostics.errors as item, index (index)}
            <li class="mono small break">{item}</li>
          {/each}
          {#if diagnostics.core.error}
            <li class="mono small break">core: {diagnostics.core.error}</li>
          {/if}
          {#if app.engine.error}
            <li class="mono small break">engine: {app.engine.error}</li>
          {/if}
        </ul>
      {:else}
        <p class="muted small">No errors reported.</p>
      {/if}
    </div>
  </section>

  <p class="diag-foot mono">
    Report generated {lastRun ? formatDateTime(lastRun) : "—"} · values are reported by the backend, not inferred.
  </p>
{/if}

<style>
  .diag-banner {
    margin-bottom: var(--space-4);
  }

  .diag-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }

  @media (max-width: 860px) {
    .diag-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .capabilities-panel {
    grid-column: 1 / -1;
    order: 1;
  }

  .capability-list {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    column-gap: var(--space-5);
  }

  .capability-list .deflist__row {
    grid-template-columns: minmax(120px, 180px) minmax(0, 1fr);
    padding: 7px 0;
  }

  @media (max-width: 760px) {
    .capability-list {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .backends {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .server-list {
    display: grid;
    gap: 4px;
  }

  .server-list li {
    padding: 4px 8px;
    border: 1px solid var(--line);
    background: var(--field);
  }

  .tray-note {
    margin-top: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--line);
  }

  .addresses {
    display: grid;
    gap: 2px;
  }

  .errors-panel {
    margin-bottom: var(--space-4);
  }

  .error-list {
    display: grid;
    gap: 6px;
  }

  .error-list li {
    padding: 6px 8px;
    border-left: 2px solid var(--error);
    background: var(--field);
    white-space: pre-wrap;
  }

  .diag-foot {
    font-size: var(--fs-2xs);
    color: var(--faint);
    margin-top: var(--space-3);
  }
</style>
