<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import type { IconName } from "../icons";

  let {
    title,
    description,
    icon = "info",
    children,
  }: {
    title: string;
    description?: string;
    icon?: IconName;
    children?: Snippet;
  } = $props();
</script>

<div class="empty">
  <span class="empty__mark" aria-hidden="true"><Icon name={icon} size={20} /></span>
  <p class="empty__title">{title}</p>
  {#if description}<p class="empty__desc">{description}</p>{/if}
  {#if children}<div class="empty__actions">{@render children()}</div>{/if}
</div>

<style>
  .empty__mark {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border: 1px solid var(--line);
    color: var(--muted);
    margin-bottom: var(--space-2);
  }
</style>
