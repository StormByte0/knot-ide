<script lang="ts">
  /** Confirm dialog — for delete operations. */

  interface Props {
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    title,
    message,
    confirmLabel = 'Delete',
    cancelLabel = 'Cancel',
    danger = true,
    onConfirm,
    onCancel,
  }: Props = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      onConfirm();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="overlay"
  role="presentation"
  onclick={onCancel}
>
  <div
    class="dialog"
    role="alertdialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <h2 class="dialog-title">{title}</h2>
    <p class="dialog-message">{message}</p>
    <div class="dialog-actions">
      <button type="button" class="btn btn-cancel" onclick={onCancel}>{cancelLabel}</button>
      <button type="button" class="btn btn-confirm" class:danger onclick={onConfirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background: #252526;
    border: 1px solid #3c3c3c;
    border-radius: 6px;
    padding: 16px 20px;
    min-width: 400px;
    max-width: 500px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
  }

  .dialog-title {
    font-size: 14px;
    font-weight: 600;
    margin: 0 0 8px 0;
    color: #ffffff;
  }

  .dialog-message {
    font-size: 13px;
    color: #cccccc;
    margin: 0 0 16px 0;
    line-height: 1.4;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .btn {
    padding: 6px 14px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
  }

  .btn-cancel {
    background: #3c3c3c;
    color: #ccc;
  }

  .btn-cancel:hover {
    background: #4c4c4c;
  }

  .btn-confirm {
    background: #0e639c;
    color: white;
  }

  .btn-confirm:hover {
    background: #1177bb;
  }

  .btn-confirm.danger {
    background: #a1260d;
  }

  .btn-confirm.danger:hover {
    background: #c42b1a;
  }
</style>
