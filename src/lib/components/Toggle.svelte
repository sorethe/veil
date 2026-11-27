<script lang="ts">
  let {
    checked,
    onchange,
    disabled = false,
    label,
    describedBy,
  }: {
    checked: boolean;
    onchange: (next: boolean) => void;
    disabled?: boolean;
    label: string;
    describedBy?: string;
  } = $props();
</script>

<button
  type="button"
  class="toggle"
  class:toggle--on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  aria-describedby={describedBy}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="toggle__track" aria-hidden="true"><span class="toggle__thumb"></span></span>
</button>

<style>
  .toggle {
    flex: none;
    display: inline-flex;
    align-items: center;
    padding: 2px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    color: var(--muted);
  }

  .toggle:disabled {
    opacity: 0.4;
  }

  .toggle__track {
    display: block;
    width: 34px;
    height: 18px;
    border: 1px solid var(--line-strong);
    background: var(--field);
    border-radius: var(--radius);
    padding: 2px;
    transition: background-color var(--fast) var(--ease), border-color var(--fast) var(--ease);
  }

  .toggle__thumb {
    display: block;
    width: 12px;
    height: 12px;
    background: var(--muted);
    border-radius: 1px;
    transform: translateX(0);
    transition: transform var(--fast) var(--ease), background-color var(--fast) var(--ease);
  }

  .toggle--on .toggle__track {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-color: var(--accent);
  }

  .toggle--on .toggle__thumb {
    background: var(--accent);
    transform: translateX(16px);
  }
</style>
