<script lang="ts">
  import { iconMarkup, type IconName } from "../icons";

  let {
    name,
    size = 16,
    strokeWidth = 1.6,
    label,
  }: {
    name: IconName;
    size?: number;
    strokeWidth?: number;
    label?: string;
  } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="square"
  stroke-linejoin="miter"
  role={label ? "img" : undefined}
  aria-hidden={label ? undefined : "true"}
>
  {#if label}<title>{label}</title>{/if}
  <!-- eslint-disable-next-line svelte/no-at-html-tags -- iconMarkup only returns constant SVG strings from the local icons map -->
  {@html iconMarkup(name)}
</svg>
