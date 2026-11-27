<script lang="ts">
  import type { EngineStatus } from "../api";

  let { status, size = 12 }: { status: EngineStatus; size?: number } = $props();
</script>

<svg
  class="glyph {`st-${status}`}"
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.4"
  aria-hidden="true"
>
  {#if status === "connected"}
    <path d="M2.5 2.5h11v11h-4l-2.5-2.5h-4.5z" fill="currentColor" stroke="none" />
  {:else if status === "starting"}
    <path d="M8 1.5 14.5 8 8 14.5 1.5 8z" />
    <path d="M8 5v6" />
  {:else if status === "stopping"}
    <path d="M2.5 2.5h11v11h-11z" />
    <path d="m2.5 13.5 11-11" />
  {:else if status === "error"}
    <path d="M2.5 2.5h11v11h-11z" />
    <path d="m5 5 6 6M11 5l-6 6" />
  {:else}
    <path d="M2.5 2.5h11v11h-11z" />
  {/if}
</svg>

<style>
  .glyph {
    flex: none;
  }
</style>
