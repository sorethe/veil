<script lang="ts">
  import { api, type Profile, type Validation } from "../api";
  import { errorMessage } from "../format";
  import { app } from "../state.svelte";
  import Field from "./Field.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let { onclose, onimported }: { onclose: () => void; onimported: (profile: Profile) => void } =
    $props();

  type Tab = "paste" | "file" | "subscription" | "links" | "qr";

  const tabs: { id: Tab; label: string }[] = [
    { id: "paste", label: "Paste JSON" },
    { id: "file", label: "File" },
    { id: "subscription", label: "Subscription" },
    { id: "links", label: "Share links" },
    { id: "qr", label: "QR image" },
  ];

  let tab = $state<Tab>("paste");
  let name = $state("");
  let content = $state("");
  let url = $state("");
  let links = $state("");
  let imageBase64 = $state<string | null>(null);
  let imageName = $state("");
  let fileName = $state("");
  let dragging = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let validating = $state(false);
  let validation = $state<Validation | null>(null);
  let fileInput = $state<HTMLInputElement | null>(null);
  let qrInput = $state<HTMLInputElement | null>(null);

  const canSubmit = $derived(
    tab === "paste"
      ? content.trim().length > 0
      : tab === "file"
        ? content.trim().length > 0
        : tab === "subscription"
          ? url.trim().length > 0
          : tab === "links"
            ? links.trim().length > 0
            : imageBase64 !== null,
  );

  const disabledReason = $derived(
    canSubmit
      ? null
      : tab === "paste"
        ? "Paste a config first"
        : tab === "file"
          ? "Choose a config file first"
          : tab === "subscription"
            ? "Enter a subscription URL"
            : tab === "links"
              ? "Paste at least one share link"
              : "Choose a QR image first",
  );

  function switchTab(next: Tab) {
    tab = next;
    error = null;
    validation = null;
  }

  function fallbackName(fallback: string): string {
    const trimmed = name.trim();
    if (trimmed) return trimmed;
    if (fileName) return fileName.replace(/\.[^.]+$/, "");
    return fallback;
  }

  async function validate() {
    const text = tab === "links" ? links : content;
    if (!text.trim()) {
      validation = { ok: false, error: "Nothing to validate.", outbounds: [] };
      return;
    }
    validating = true;
    validation = null;
    try {
      validation = await api.validateContent(text);
    } catch (cause) {
      validation = { ok: false, error: errorMessage(cause), outbounds: [] };
    } finally {
      validating = false;
    }
  }

  async function readProfileFile(file: File) {
    error = null;
    validation = null;
    fileName = file.name;
    if (!name.trim()) name = file.name.replace(/\.[^.]+$/, "");
    try {
      const text = await file.text();
      content = text;
      if (!text.trim()) error = "That file is empty.";
    } catch (cause) {
      error = `Could not read ${file.name}: ${errorMessage(cause)}`;
    }
  }

  function readQrFile(file: File) {
    error = null;
    validation = null;
    imageName = file.name;
    if (!name.trim()) name = file.name.replace(/\.[^.]+$/, "");
    const reader = new FileReader();
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      const comma = result.indexOf(",");
      imageBase64 = comma >= 0 ? result.slice(comma + 1) : result;
      if (!imageBase64) error = "Could not read image data from that file.";
    };
    reader.onerror = () => {
      error = `Could not read ${file.name}.`;
    };
    reader.readAsDataURL(file);
  }

  function onDrop(event: DragEvent) {
    event.preventDefault();
    dragging = false;
    const file = event.dataTransfer?.files?.[0];
    if (!file) return;
    if (tab === "qr") readQrFile(file);
    else void readProfileFile(file);
  }

  async function pasteFromClipboard() {
    error = null;
    try {
      const text = await api.readClipboard();
      if (!text.trim()) {
        error = "The clipboard is empty.";
        return;
      }
      if (tab === "links") links = text;
      else content = text;
      validation = null;
    } catch (cause) {
      error = errorMessage(cause);
    }
  }

  async function submit() {
    if (!canSubmit || busy) return;
    busy = true;
    error = null;
    try {
      let profile: Profile;
      if (tab === "paste") {
        profile = await api.importJson(fallbackName("Pasted profile"), content);
      } else if (tab === "file") {
        profile = await api.importJson(
          fallbackName("Imported file"),
          content,
          fileName || undefined,
        );
      } else if (tab === "subscription") {
        profile = await api.importSubscription(url.trim(), name.trim() || undefined);
      } else if (tab === "links") {
        profile = await api.importShareLinks(fallbackName("Share links"), links);
      } else {
        if (!imageBase64) return;
        profile = await api.importQr(imageBase64, name.trim() || undefined);
      }
      app.notify("success", "Profile imported", profile.name);
      onimported(profile);
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      busy = false;
    }
  }
</script>

<Modal
  title="Import profile"
  subtitle="A profile is a sing-box config, subscription, or a set of share links."
  {onclose}
  width="640px"
>
  <div class="import">
    <div class="tabs" role="tablist" aria-label="Import source">
      {#each tabs as item (item.id)}
        <button
          type="button"
          role="tab"
          id={`tab-${item.id}`}
          aria-selected={tab === item.id}
          aria-controls="import-panel"
          class="tabs__item"
          class:tab--active={tab === item.id}
          onclick={() => switchTab(item.id)}
        >
          {item.label}
        </button>
      {/each}
    </div>

    <div id="import-panel" role="tabpanel" aria-labelledby={`tab-${tab}`} class="import__panel">
      {#if tab === "paste"}
        <Field
          label="Config JSON"
          hint="Paste a complete sing-box configuration."
          forId="import-json"
        >
          <div class="row row--wrap">
            <textarea
              id="import-json"
              class="textarea"
              spellcheck="false"
              placeholder={'{\n  "outbounds": [ ... ]\n}'}
              bind:value={content}
              oninput={() => (validation = null)}></textarea>
          </div>
        </Field>
        <div class="row row--wrap">
          <button type="button" class="btn btn--sm" onclick={() => void pasteFromClipboard()}>
            <Icon name="clipboard" size={12} /> Read clipboard
          </button>
          <button
            type="button"
            class="btn btn--sm"
            disabled={validating || !content.trim()}
            onclick={() => void validate()}
          >
            {validating ? "Validating" : "Validate"}
          </button>
        </div>
      {:else if tab === "file"}
        <div
          class="dropzone"
          class:dropzone--drag={dragging}
          role="group"
          aria-label="Config file drop zone"
          ondragover={(event) => {
            event.preventDefault();
            dragging = true;
          }}
          ondragleave={() => (dragging = false)}
          ondrop={onDrop}
        >
          <Icon name="file" size={20} />
          <p class="dropzone__title">{fileName || "Drop a config file here"}</p>
          <p class="dropzone__hint">JSON read as text, kept local until imported.</p>
          <button type="button" class="btn btn--sm" onclick={() => fileInput?.click()}>
            Choose file
          </button>
          <input
            type="file"
            class="sr-only"
            accept=".json,application/json,text/plain"
            bind:this={fileInput}
            onchange={(event) => {
              const file = event.currentTarget.files?.[0];
              if (file) void readProfileFile(file);
              event.currentTarget.value = "";
            }}
          />
        </div>
        {#if fileName}
          <p class="import__meta">
            <span class="mono">{fileName}</span> · {content.length.toLocaleString()} characters read
          </p>
        {/if}
        <div class="row row--wrap">
          <button
            type="button"
            class="btn btn--sm"
            disabled={validating || !content.trim()}
            onclick={() => void validate()}
          >
            {validating ? "Validating" : "Validate"}
          </button>
        </div>
      {:else if tab === "subscription"}
        <Field
          label="Subscription URL"
          hint="The backend fetches the URL. No credentials are displayed after import."
          forId="import-url"
        >
          <input
            id="import-url"
            class="input input--mono"
            type="url"
            spellcheck="false"
            placeholder="https://example.com/subscription"
            bind:value={url}
          />
        </Field>
      {:else if tab === "links"}
        <Field
          label="Share links"
          hint="One or more vmess://, vless://, trojan://, ss://, or hy2:// links."
          forId="import-links"
        >
          <textarea
            id="import-links"
            class="textarea"
            spellcheck="false"
            placeholder="vless://…"
            bind:value={links}
            oninput={() => (validation = null)}></textarea>
        </Field>
        <div class="row row--wrap">
          <button type="button" class="btn btn--sm" onclick={() => void pasteFromClipboard()}>
            <Icon name="clipboard" size={12} /> Read clipboard
          </button>
          <button
            type="button"
            class="btn btn--sm"
            disabled={validating || !links.trim()}
            onclick={() => void validate()}
          >
            {validating ? "Validating" : "Validate"}
          </button>
        </div>
      {:else}
        <div
          class="dropzone"
          class:dropzone--drag={dragging}
          role="group"
          aria-label="QR image drop zone"
          ondragover={(event) => {
            event.preventDefault();
            dragging = true;
          }}
          ondragleave={() => (dragging = false)}
          ondrop={onDrop}
        >
          <Icon name="qr" size={20} />
          <p class="dropzone__title">{imageName || "Drop a QR image here"}</p>
          <p class="dropzone__hint">PNG or JPEG. The backend decodes it locally.</p>
          <button type="button" class="btn btn--sm" onclick={() => qrInput?.click()}>
            Choose image
          </button>
          <input
            type="file"
            class="sr-only"
            accept="image/*"
            bind:this={qrInput}
            onchange={(event) => {
              const file = event.currentTarget.files?.[0];
              if (file) readQrFile(file);
              event.currentTarget.value = "";
            }}
          />
        </div>
      {/if}

      <Field
        label="Profile name"
        hint="Optional; derived from the file or source when left blank."
        forId="import-name"
      >
        <input id="import-name" class="input" bind:value={name} placeholder="Name" />
      </Field>

      {#if validation}
        {#if validation.ok}
          <p class="import__ok">
            <Icon name="check" size={12} />
            Valid config · {validation.outbounds.length}
            {validation.outbounds.length === 1 ? "outbound" : "outbounds"}
          </p>
          {#if validation.outbounds.length}
            <p class="import__outbounds mono">{validation.outbounds.join("  ·  ")}</p>
          {/if}
        {:else}
          <p class="import__error" role="alert">
            {validation.error ?? "The backend rejected this config."}
          </p>
        {/if}
      {/if}

      {#if error}
        <p class="import__error" role="alert">{error}</p>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <button type="button" class="btn" onclick={onclose}>Cancel</button>
    <button
      type="button"
      class="btn btn--primary"
      disabled={!canSubmit || busy}
      title={disabledReason ?? undefined}
      onclick={() => void submit()}
    >
      {busy ? "Importing" : "Import profile"}
    </button>
  {/snippet}
</Modal>

<style>
  .import {
    display: grid;
    gap: var(--space-4);
  }

  .tabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--line);
    overflow-x: auto;
  }

  .tabs__item {
    padding: 8px 10px;
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    white-space: nowrap;
  }

  .tabs__item:hover {
    color: var(--text);
  }

  .tab--active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }

  .import__panel {
    display: grid;
    gap: var(--space-3);
    min-height: 220px;
  }

  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: var(--space-5);
    border: 1px dashed var(--line-strong);
    background: var(--field);
    text-align: center;
    color: var(--muted);
  }

  .dropzone--drag {
    border-color: var(--accent);
    background: var(--raised);
    color: var(--text);
  }

  .dropzone__title {
    font-size: var(--fs-sm);
    color: var(--text);
    overflow-wrap: anywhere;
  }

  .dropzone__hint {
    font-size: var(--fs-2xs);
    color: var(--faint);
    margin-bottom: 4px;
  }

  .import__meta {
    font-size: var(--fs-xs);
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .import__error {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--error);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .import__ok {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--accent);
  }

  .import__outbounds {
    font-size: var(--fs-2xs);
    color: var(--muted);
    overflow-wrap: anywhere;
  }
</style>
