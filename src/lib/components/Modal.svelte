<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    title,
    subtitle,
    onclose,
    children,
    footer,
    width = "560px",
  }: {
    title: string;
    subtitle?: string;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
    width?: string;
  } = $props();

  let panel = $state<HTMLDivElement | null>(null);

  onMount(() => {
    panel?.focus();
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  });

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onclose();
      return;
    }
    if (event.key !== "Tab" || !panel) return;
    const nodes = Array.from(
      panel.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((node) => node.offsetParent !== null);
    if (!nodes.length) return;
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div class="overlay">
  <button type="button" class="overlay__scrim" aria-label="Close dialog" onclick={onclose}></button>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    bind:this={panel}
    style={`width:min(${width}, 100%)`}
  >
    <header class="modal__head">
      <div class="modal__heading">
        <h2 class="modal__title">{title}</h2>
        {#if subtitle}<p class="modal__subtitle">{subtitle}</p>{/if}
      </div>
      <button type="button" class="icon-btn" aria-label="Close" onclick={onclose}>
        <Icon name="close" size={14} />
      </button>
    </header>
    <div class="modal__body">
      {@render children()}
    </div>
    {#if footer}
      <footer class="modal__foot">
        {@render footer()}
      </footer>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 7vh var(--space-4) var(--space-4);
    overflow: auto;
  }

  .overlay__scrim {
    position: fixed;
    inset: 0;
    background: var(--scrim);
    cursor: default;
  }

  .modal {
    position: relative;
    z-index: 1;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    display: flex;
    flex-direction: column;
    max-height: 86vh;
    animation: modal-in var(--fast) var(--ease);
  }

  @keyframes modal-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  .modal__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-4);
    border-bottom: 1px solid var(--line);
  }

  .modal__heading {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .modal__title {
    font-size: var(--fs-lg);
    font-weight: 500;
  }

  .modal__subtitle {
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .modal__body {
    padding: var(--space-4);
    overflow: auto;
    min-height: 0;
  }

  .modal__foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--line);
    flex-wrap: wrap;
  }
</style>
