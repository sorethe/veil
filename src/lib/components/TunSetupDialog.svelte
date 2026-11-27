<script lang="ts">
  import { onMount } from "svelte";
  import { api, type TunSetup } from "../api";
  import { errorMessage } from "../format";
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let {
    intent = "enable",
    onclose,
    onready,
  }: {
    intent?: "enable" | "connect" | "inspect";
    onclose: () => void;
    onready: (setup: TunSetup) => void;
  } = $props();

  interface SetupRow {
    key: string;
    label: string;
    detail: string;
    ok: boolean;
  }

  let setup = $state<TunSetup | null>(null);
  let checking = $state(true);
  let installing = $state(false);
  let checkError = $state<string | null>(null);
  let installError = $state<string | null>(null);
  let installNotice = $state<string | null>(null);

  const rows = $derived(rowsFor(setup));
  const primaryLabel = $derived(
    setup?.ready
      ? intent === "inspect"
        ? "Done"
        : intent === "connect"
          ? "Enable TUN & connect"
          : "Enable TUN"
      : intent === "inspect"
        ? "Install helper"
        : intent === "connect"
          ? "Install helper & connect"
          : "Install helper & enable TUN",
  );
  const blockedReason = $derived(setup ? installBlockReason(setup) : null);
  const hasConflict = $derived((setup?.conflictingInterfaces.length ?? 0) > 0);

  function rowsFor(value: TunSetup | null): SetupRow[] {
    if (!value) return [];
    const rows: SetupRow[] = [];
    if (value.conflictingInterfaces.length) {
      const names = value.conflictingInterfaces.join(", ");
      const many = value.conflictingInterfaces.length > 1;
      rows.push({
        key: "conflict",
        label: "Other TUN interface",
        ok: false,
        detail: `${names} ${many ? "are" : "is"} active; TUN interfaces cannot share interface routes or DNS`,
      });
    }
    rows.push(
      {
        key: "helper",
        label: "Privileged helper",
        ok: value.helperInstalled,
        detail: value.helperInstalled
          ? `installed at ${value.helperPath ?? "/usr/bin/veil-helper"}`
          : value.helperFound
            ? `found at ${value.helperPath ?? "another location"}; not installed to /usr/bin/veil-helper`
            : "veil-helper was not found (expected /usr/bin/veil-helper)",
      },
      {
        key: "policy",
        label: "Polkit policy",
        ok: value.policyInstalled,
        detail: value.policyInstalled
          ? "dev.veil.helper policy is installed"
          : "dev.veil.helper policy is not installed",
      },
      {
        key: "pkexec",
        label: "pkexec / polkit",
        ok: value.pkexecAvailable,
        detail: value.pkexecAvailable ? "pkexec is available" : "pkexec is not installed",
      },
      {
        key: "tun",
        label: "/dev/net/tun",
        ok: value.tunDevice,
        detail: value.tunDevice ? "TUN device is present" : "/dev/net/tun is missing",
      },
      {
        key: "nft",
        label: "nftables",
        ok: value.nft,
        detail: value.nft ? "nft is available" : "nft is not installed",
      },
    );
    return rows;
  }

  function installBlockReason(value: TunSetup): string {
    if (value.conflictingInterfaces.length) {
      return "Another TUN interface is active. Veil cannot share interface routes or DNS with it; disable the other tunnel before using Veil TUN.";
    }
    if (value.helperInstalled && value.policyInstalled) {
      return "The helper and policy are installed, but TUN still has unmet prerequisites. Veil does not install system packages for you.";
    }
    if (!value.helperFound) {
      return "The veil-helper binary is missing, so there is nothing for Veil to install. Install Veil's helper package or use a packaged build.";
    }
    if (!value.pkexecAvailable) {
      return "pkexec is missing, so Veil cannot request admin approval. Install polkit first.";
    }
    return "The installer prerequisites are not met. Resolve the missing requirements above.";
  }

  onMount(() => {
    void check();
  });

  async function check(): Promise<void> {
    checking = true;
    checkError = null;
    try {
      setup = await api.getTunSetup();
      installError = null;
      installNotice = null;
    } catch (cause) {
      setup = null;
      checkError = errorMessage(cause);
      app.notify("error", "Could not read TUN setup", checkError);
    } finally {
      checking = false;
    }
  }

  async function install(): Promise<void> {
    if (installing || !setup?.canInstall) return;
    installing = true;
    installError = null;
    installNotice = null;
    try {
      const result = await api.installTunHelper();
      setup = result;
      if (result.ready) {
        app.notify(
          "success",
          "TUN helper installed",
          intent === "connect" ? "Enabling TUN and connecting." : "TUN is ready to use.",
        );
        onready(result);
        return;
      }
      const missing = result.missing.length ? result.missing.join("; ") : result.detail;
      installNotice = `The helper command finished, but TUN is still not ready. ${missing}`;
      if (result.helperInstalled) {
        app.notify("info", "Helper installed; TUN not ready yet", missing);
      } else {
        app.notify("error", "TUN is still not ready", missing);
      }
    } catch (cause) {
      installError = errorMessage(cause);
      app.notify("error", "TUN helper not installed", installError);
    } finally {
      installing = false;
    }
  }

  function finish(): void {
    if (!setup?.ready || installing) return;
    onready(setup);
  }

  function requestClose(): void {
    if (installing) return;
    onclose();
  }

  function openDiagnostics(): void {
    if (installing) return;
    app.setView("diagnostics");
    onclose();
  }
</script>

<Modal
  title="TUN setup"
  subtitle={intent === "connect"
    ? "Prepare interface routing, then connect."
    : intent === "inspect"
      ? "Check interface routing prerequisites."
      : "Prepare interface routing."}
  onclose={requestClose}
  width="580px"
>
  <div class="setup">
    <p class="setup__lede">
      TUN routes IPv4 and IPv6 at the interface. It needs a small privileged helper and a polkit
      policy. Veil installs them with your admin approval via pkexec. The GUI itself never runs as
      root.
    </p>

    {#if checking && !setup}
      <p class="setup__loading" role="status">Checking helper, polkit, and TUN prerequisites…</p>
    {/if}

    {#if checkError}
      <div class="banner banner--error" role="alert">
        <Icon name="error" size={15} />
        <div class="grow">
          <p class="banner__title">Could not read TUN setup</p>
          <p class="banner__body mono">{checkError}</p>
        </div>
        <button type="button" class="btn btn--sm" disabled={checking} onclick={() => void check()}>
          {checking ? "Checking" : "Retry"}
        </button>
      </div>
    {/if}

    {#if setup}
      <ul class="checks" aria-label="TUN prerequisites">
        {#each rows as row (row.key)}
          <li class="checks__row">
            <span class="checks__icon" class:checks__icon--ok={row.ok} aria-hidden="true">
              <Icon name={row.ok ? "check" : "warning"} size={13} />
            </span>
            <span class="checks__text">
              <span class="checks__label">{row.label}</span>
              <span class="checks__detail mono">{row.detail}</span>
            </span>
            <span class="tag" class:tag--accent={row.ok} class:tag--warning={!row.ok}>
              {row.ok ? "ready" : "missing"}
            </span>
          </li>
        {/each}
      </ul>

      {#if setup.conflictingInterfaces.length}
        <div class="banner banner--error" role="alert">
          <Icon name="error" size={15} />
          <div class="grow">
            <p class="banner__title">Another TUN interface is active</p>
            <p class="banner__body">
              {setup.conflictingInterfaces.join(", ")}
              {setup.conflictingInterfaces.length === 1 ? "is" : "are"} already using TUN. Veil cannot
              share interface routes or DNS with another tunnel, so TUN is unavailable while it runs.
              Disable or stop the other tunnel and retry, or switch this session to System proxy.
            </p>
          </div>
        </div>
      {:else if setup.ready}
        <div class="banner banner--accent" role="status">
          <Icon name="check" size={15} />
          <div class="grow">
            <p class="banner__title">TUN is ready</p>
            <p class="banner__body">{setup.detail}</p>
          </div>
        </div>
      {:else}
        {#if setup.missing.length}
          <div class="setup__missing">
            <span class="kicker">Missing</span>
            <ul class="setup__missing-list">
              {#each setup.missing as item (item)}
                <li>{item}</li>
              {/each}
            </ul>
          </div>
        {/if}

        {#if !setup.canInstall}
          <div class="banner banner--warning" role="alert">
            <Icon name="warning" size={15} />
            <div class="grow">
              <p class="banner__title">Cannot install automatically</p>
              <p class="banner__body">{blockedReason}</p>
            </div>
          </div>
        {:else if !setup.tunDevice || !setup.nft}
          <p class="setup__note">
            The helper can be installed, but TUN will stay unavailable until the host requirements
            above are resolved.
          </p>
        {/if}
      {/if}

      {#if installNotice}
        <p class="banner banner--warning setup__notice" role="status">{installNotice}</p>
      {/if}
    {/if}

    {#if installError}
      <p class="field__error" role="alert">{installError}</p>
    {/if}

    {#if installing}
      <p class="setup__progress" role="status" aria-live="polite">
        Waiting for admin approval. This can take a moment.
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <button type="button" class="btn btn--quiet" disabled={installing} onclick={requestClose}>
      {setup && !setup.ready && (!setup.canInstall || hasConflict) ? "Close" : "Cancel"}
    </button>
    {#if setup && hasConflict}
      <button
        type="button"
        class="btn"
        disabled={checking || installing}
        onclick={() => void check()}
      >
        <Icon name="refresh" size={12} />
        {checking ? "Checking" : "Retry check"}
      </button>
      <button type="button" class="btn" disabled={installing} onclick={openDiagnostics}>
        <Icon name="diagnostics" size={12} /> Diagnostics
      </button>
    {:else if setup && !setup.ready && !setup.canInstall}
      <button type="button" class="btn" disabled={installing} onclick={openDiagnostics}>
        <Icon name="diagnostics" size={12} /> Diagnostics
      </button>
    {:else if setup}
      <button
        type="button"
        class="btn btn--primary"
        disabled={installing || checking}
        onclick={() => (setup?.ready ? finish() : void install())}
      >
        {installing ? "Installing" : primaryLabel}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .setup {
    display: grid;
    gap: var(--space-4);
    min-width: 0;
  }

  .setup__lede {
    font-size: var(--fs-sm);
    color: var(--muted);
    line-height: 1.55;
    max-width: 62ch;
  }

  .setup__loading {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--faint);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .checks {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--line);
  }

  .checks__row {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr) auto;
    align-items: start;
    gap: var(--space-2);
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  .checks__icon {
    color: var(--warning);
    display: flex;
    align-items: center;
    height: 18px;
  }

  .checks__icon--ok {
    color: var(--accent);
  }

  .checks__text {
    display: grid;
    gap: 1px;
    min-width: 0;
  }

  .checks__label {
    font-size: var(--fs-sm);
  }

  .checks__detail {
    font-size: var(--fs-2xs);
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .setup__missing {
    display: grid;
    gap: 4px;
    padding: 9px 10px;
    border: 1px solid var(--line);
    border-left: 2px solid var(--warning);
    background: var(--field);
  }

  .setup__missing-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .setup__note,
  .setup__notice,
  .setup__progress {
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .setup__notice {
    padding: 9px 10px;
    border-left: 2px solid var(--warning);
    background: var(--surface);
  }

  .setup__progress {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--warning);
  }
</style>
