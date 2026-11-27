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

  let panel = $state<HTMLElement | null>(null);

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
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div class="overlay">
  <button type="button" class="overlay__scrim" aria-label="Close drawer" onclick={onclose}></button>
  <div
    class="drawer"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    bind:this={panel}
    style={`width:min(${width}, 100%)`}
  >
    <header class="drawer__head">
      <div class="drawer__heading">
        <h2 class="drawer__title">{title}</h2>
        {#if subtitle}<p class="drawer__subtitle">{subtitle}</p>{/if}
      </div>
      <button type="button" class="icon-btn" aria-label="Close" onclick={onclose}>
        <Icon name="close" size={14} />
      </button>
    </header>
    <div class="drawer__body">
      {@render children()}
    </div>
    {#if footer}
      <footer class="drawer__foot">
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
    justify-content: flex-end;
  }

  .overlay__scrim {
    position: absolute;
    inset: 0;
    background: var(--scrim);
    cursor: default;
  }

  .drawer {
    position: relative;
    z-index: 1;
    height: 100%;
    background: var(--surface);
    border-left: 1px solid var(--line-strong);
    display: flex;
    flex-direction: column;
    animation: drawer-in 160ms var(--ease);
    box-shadow: -12px 0 40px rgba(0, 0, 0, 0.16);
  }

  @keyframes drawer-in {
    from {
      transform: translateX(12px);
      opacity: 0;
    }
  }

  .drawer__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-4);
    border-bottom: 1px solid var(--line);
  }

  .drawer__heading {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .drawer__title {
    font-size: var(--fs-lg);
    font-weight: 500;
  }

  .drawer__subtitle {
    font-family: var(--font-mono);
    font-size: var(--fs-2xs);
    color: var(--muted);
    letter-spacing: 0.06em;
  }

  .drawer__body {
    padding: var(--space-4);
    overflow: auto;
    flex: 1;
    min-height: 0;
  }

  .drawer__foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--line);
    flex-wrap: wrap;
  }
</style>
