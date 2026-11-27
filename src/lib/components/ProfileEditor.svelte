<script lang="ts">
  import { api, type Profile, type Validation } from "../api";
  import { errorMessage, formatBytes, formatDateTime } from "../format";
  import { app } from "../state.svelte";
  import Drawer from "./Drawer.svelte";
  import Field from "./Field.svelte";
  import Icon from "./Icon.svelte";

  let {
    profile,
    onclose,
    onsaved,
  }: { profile: Profile; onclose: () => void; onsaved: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(profile.name);
  // svelte-ignore state_referenced_locally
  let content = $state(profile.content);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let validating = $state(false);
  let validation = $state<Validation | null>(null);

  async function validate() {
    validating = true;
    validation = null;
    try {
      validation = await api.validateContent(content);
    } catch (cause) {
      validation = { ok: false, error: errorMessage(cause), outbounds: [] };
    } finally {
      validating = false;
    }
  }

  async function save() {
    if (saving) return;
    error = null;
    saving = true;
    try {
      await api.updateProfile(profile.id, name.trim() || profile.name, content);
      await app.refreshProfiles();
      app.notify("success", "Profile saved", name.trim() || profile.name);
      onsaved();
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<Drawer title="Edit profile" subtitle={profile.kind} {onclose} width="640px">
  <div class="editor">
    <Field label="Name" forId="edit-name">
      <input id="edit-name" class="input" bind:value={name} />
    </Field>

    <Field
      label="Config JSON"
      hint={`${formatBytes(profile.size)} on disk · updated ${formatDateTime(profile.updatedAt)}`}
      forId="edit-content"
    >
      <textarea
        id="edit-content"
        class="textarea editor__json"
        spellcheck="false"
        bind:value={content}
        oninput={() => (validation = null)}
      ></textarea>
    </Field>

    <div class="row row--wrap">
      <button type="button" class="btn btn--sm" disabled={validating || !content.trim()} onclick={() => void validate()}>
        {validating ? "Validating" : "Validate"}
      </button>
      {#if validation?.ok}
        <span class="editor__ok">
          <Icon name="check" size={12} /> Valid · {validation.outbounds.length}
          {validation.outbounds.length === 1 ? "outbound" : "outbounds"}
        </span>
      {/if}
    </div>

    {#if validation && !validation.ok}
      <p class="editor__error" role="alert">{validation.error ?? "Validation failed."}</p>
    {/if}

    {#if validation?.ok && validation.outbounds.length}
      <p class="editor__outbounds mono">{validation.outbounds.join("  ·  ")}</p>
    {/if}

    {#if error}
      <p class="editor__error" role="alert">{error}</p>
    {/if}
  </div>

  {#snippet footer()}
    <button type="button" class="btn" onclick={onclose}>Cancel</button>
    <button type="button" class="btn btn--primary" disabled={saving || !content.trim()} onclick={() => void save()}>
      {saving ? "Saving" : "Save profile"}
    </button>
  {/snippet}
</Drawer>

<style>
  .editor {
    display: grid;
    gap: var(--space-4);
  }

  .editor__json {
    min-height: 320px;
    height: 46vh;
    resize: vertical;
  }

  .editor__error {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--error);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .editor__ok {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--fs-xs);
    color: var(--accent);
  }

  .editor__outbounds {
    font-size: var(--fs-2xs);
    color: var(--muted);
    overflow-wrap: anywhere;
  }
</style>
