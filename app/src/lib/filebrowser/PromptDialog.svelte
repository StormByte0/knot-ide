<script lang="ts">
  /** Generic prompt dialog — for new file, new folder, rename. */

  import { onMount } from 'svelte';

  interface Props {
    title: string;
    label: string;
    defaultValue?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    onConfirm: (value: string) => void;
    onCancel: () => void;
  }

  let {
    title,
    label,
    defaultValue = '',
    confirmLabel = 'OK',
    cancelLabel = 'Cancel',
    onConfirm,
    onCancel,
  }: Props = $props();

  let value = $state('');
  let inputEl: HTMLInputElement;

  onMount(() => {
    value = defaultValue;
    if (inputEl) {
      inputEl.focus();
      inputEl.select();
    }
  });

  function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (value.trim()) {
      onConfirm(value.trim());
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Overlay: click on the background closes the dialog.
     stopPropagation on the dialog prevents clicks inside it from closing. -->
<div
  class="overlay"
  role="presentation"
  onclick={onCancel}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <h2 class="dialog-title">{title}</h2>
    <form onsubmit={handleSubmit}>
      <label class="dialog-label">
        {label}
        <input
          bind:this={inputEl}
          bind:value
          type="text"
          class="dialog-input"
          autocomplete="off"
          spellcheck="false"
        />
      </label>
      <div class="dialog-actions">
        <button type="button" class="btn btn-cancel" onclick={onCancel}>{cancelLabel}</button>
        <button type="submit" class="btn btn-confirm" disabled={!value.trim()}>{confirmLabel}</button>
      </div>
    </form>
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
    margin: 0 0 12px 0;
    color: #ffffff;
  }

  .dialog-label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: #aaa;
  }

  .dialog-input {
    background: #1e1e1e;
    border: 1px solid #3c3c3c;
    color: #fff;
    padding: 8px;
    border-radius: 4px;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .dialog-input:focus {
    border-color: #007acc;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
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

  .btn-confirm:hover:not(:disabled) {
    background: #1177bb;
  }

  .btn-confirm:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
