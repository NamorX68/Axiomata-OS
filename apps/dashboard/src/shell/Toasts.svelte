<!-- Bottom-right stack of transient notifications from `core/toast.ts`. -->
<script lang="ts">
  import { dismissToast, toasts } from "../core/toast";
</script>

{#if $toasts.length > 0}
  <div class="toasts" role="status" aria-live="polite">
    {#each $toasts as t (t.id)}
      <div class="toast {t.kind}">
        <span>{t.message}</span>
        <button type="button" aria-label="Dismiss" onclick={() => dismissToast(t.id)}>×</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toasts {
    position: fixed;
    right: var(--ax-space-4);
    bottom: var(--ax-space-4);
    z-index: var(--ax-z-toast);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    max-width: min(480px, calc(100vw - 2 * var(--ax-space-4)));
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: var(--ax-space-3);
    padding: var(--ax-space-3) var(--ax-space-4);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border-strong);
    border-left: 3px solid var(--ax-accent);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
    font-size: var(--ax-font-size-sm);
    word-break: break-word;
  }
  .toast.warning {
    border-left-color: var(--ax-warning);
  }
  .toast.danger {
    border-left-color: var(--ax-danger);
  }

  .toast button {
    flex: 0 0 auto;
    width: 20px;
    height: 20px;
    padding: 0;
    line-height: 1;
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
  }
</style>
