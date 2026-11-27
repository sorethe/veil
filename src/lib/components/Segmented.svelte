<script lang="ts">
  interface SegmentOption {
    value: string;
    label: string;
    hint?: string;
    disabled?: boolean;
  }

  let {
    value,
    options,
    onchange,
    disabled = false,
    label,
  }: {
    value: string;
    options: SegmentOption[];
    onchange: (next: string) => void;
    disabled?: boolean;
    label: string;
  } = $props();
</script>

<div class="segmented" role="radiogroup" aria-label={label} class:segmented--disabled={disabled}>
  {#each options as option (option.value)}
    <button
      type="button"
      class="segmented__item"
      class:segmented__item--active={value === option.value}
      role="radio"
      aria-checked={value === option.value}
      title={option.hint}
      disabled={disabled || option.disabled}
      onclick={() => {
        if (!disabled && !option.disabled && option.value !== value) onchange(option.value);
      }}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--field);
    padding: 2px;
    gap: 2px;
  }

  .segmented--disabled {
    opacity: 0.6;
  }

  .segmented__item {
    height: 26px;
    padding: 0 12px;
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    border: 1px solid transparent;
    border-radius: 1px;
    transition: color var(--fast) var(--ease), background-color var(--fast) var(--ease);
  }

  .segmented__item:hover:not(:disabled) {
    color: var(--text);
  }

  .segmented__item--active {
    color: var(--text);
    background: var(--raised);
    border-color: var(--line);
  }

  .segmented__item--active:hover:not(:disabled) {
    color: var(--text);
  }
</style>
