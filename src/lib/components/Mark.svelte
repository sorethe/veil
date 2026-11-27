<script lang="ts">
  let { size = 22, label }: { size?: number; label?: string } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 32 32"
  fill="none"
  stroke="currentColor"
  stroke-width="1.35"
  stroke-linejoin="miter"
  role={label ? "img" : undefined}
  aria-hidden={label ? undefined : "true"}
>
  {#if label}<title>{label}</title>{/if}
  <path d="M16 2.6 27.9 9.3v13.4L16 29.4 4.1 22.7V9.3Z" />
  <path d="M16 8.6 22.6 12.3v7.4L16 23.4 9.4 19.7v-7.4Z" stroke-width="1.05" opacity="0.8" />
  <path
    d="M16 8.6 9.4 12.3M22.6 19.7 16 23.4M4.1 9.3l5.3 3M27.9 22.7l-5.3-3"
    stroke-width="1.05"
    opacity="0.55"
  />
  <path d="M16 13.5v5" stroke-width="1.6" />
</svg>
