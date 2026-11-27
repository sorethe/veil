<script lang="ts">
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";

  const tone: Record<string, string> = {
    error: "toast--error",
    success: "toast--success",
    info: "toast--info",
  };
</script>

<div class="toasts" aria-live="polite" aria-atomic="false">
  {#each app.toasts as toast (toast.id)}
    <div class="toast {tone[toast.kind] ?? tone.info}" role="status">
      <span class="toast__mark" aria-hidden="true">
        <Icon
          name={toast.kind === "error" ? "error" : toast.kind === "success" ? "check" : "info"}
          size={14}
        />
      </span>
      <div class="toast__text">
        <p class="toast__title">{toast.title}</p>
        {#if toast.detail}<p class="toast__detail">{toast.detail}</p>{/if}
      </div>
      <button
        type="button"
        class="icon-btn toast__close"
        aria-label="Dismiss"
        onclick={() => app.dismiss(toast.id)}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 80;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: min(380px, calc(100vw - 32px));
    pointer-events: none;
  }

  .toast {
    pointer-events: auto;
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: 10px 10px 10px 12px;
    background: var(--raised);
    border: 1px solid var(--line-strong);
    border-left-width: 2px;
    border-radius: var(--radius);
    animation: toast-in 160ms var(--ease);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }

  .toast--error {
    border-left-color: var(--error);
  }

  .toast--success {
    border-left-color: var(--accent);
  }

  .toast--info {
    border-left-color: var(--line-strong);
  }

  .toast__mark {
    margin-top: 1px;
  }

  .toast--error .toast__mark {
    color: var(--error);
  }

  .toast--success .toast__mark {
    color: var(--accent);
  }

  .toast--info .toast__mark {
    color: var(--muted);
  }

  .toast__text {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }

  .toast__title {
    font-size: var(--fs-sm);
    font-weight: 500;
  }

  .toast__detail {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--muted);
    overflow-wrap: anywhere;
    line-height: 1.45;
    white-space: pre-wrap;
  }

  .toast__close {
    width: 22px;
    height: 22px;
    margin: -2px -2px 0 0;
  }
</style>
