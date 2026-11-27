<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    hint,
    error,
    ok,
    forId,
    children,
  }: {
    label?: string;
    hint?: string;
    error?: string | null;
    ok?: string | null;
    forId?: string;
    children: Snippet;
  } = $props();
</script>

<div class="field">
  {#if label}
    {#if forId}
      <label class="field__label" for={forId}>{label}</label>
    {:else}
      <span class="field__label">{label}</span>
    {/if}
  {/if}
  {@render children()}
  {#if hint}<p class="field__hint">{hint}</p>{/if}
  {#if error}<p class="field__error" role="alert">{error}</p>{/if}
  {#if ok}<p class="field__ok">{ok}</p>{/if}
</div>
