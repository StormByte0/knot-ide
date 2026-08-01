<script lang="ts">
  /** Right-click context menu for the file browser. */

  export interface MenuItem {
    id: string;
    label: string;
    icon?: string;
    danger?: boolean;
    separator?: false;
  }

  export interface MenuSeparator {
    separator: true;
    id?: string;
  }

  export type MenuEntry = MenuItem | MenuSeparator;

  interface Props {
    x: number;
    y: number;
    items: MenuEntry[];
    onAction: (id: string) => void;
    onClose: () => void;
  }

  let { x, y, items, onAction, onClose }: Props = $props();

  function handleClick(id: string) {
    onAction(id);
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Click-catcher behind the menu -->
<div class="context-backdrop" role="button" tabindex="-1" aria-label="Close menu" onclick={onClose} onkeydown={(e) => { if (e.key === 'Escape') onClose(); }} oncontextmenu={(e) => { e.preventDefault(); onClose(); }}></div>

<div
  class="context-menu"
  style="left: {x}px; top: {y}px;"
  role="menu"
>
  {#each items as item}
    {#if 'separator' in item && item.separator}
      <div class="menu-separator" role="separator"></div>
    {:else}
      <button
        class="menu-item"
        class:danger={item.danger}
        onclick={() => handleClick(item.id)}
        role="menuitem"
      >
        {#if item.icon}<span class="menu-icon">{item.icon}</span>{/if}
        <span class="menu-label">{item.label}</span>
      </button>
    {/if}
  {/each}
</div>

<style>
  .context-backdrop {
    position: fixed;
    inset: 0;
    z-index: 999;
  }

  .context-menu {
    position: fixed;
    background: #252526;
    border: 1px solid #454545;
    border-radius: 4px;
    padding: 4px 0;
    min-width: 180px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
    z-index: 1000;
  }

  .menu-separator {
    height: 1px;
    background: #454545;
    margin: 4px 0;
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: #cccccc;
    padding: 6px 12px;
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
  }

  .menu-item:hover {
    background: #094771;
    color: #ffffff;
  }

  .menu-item.danger {
    color: #f48771;
  }

  .menu-item.danger:hover {
    background: #5a1d1d;
    color: #f48771;
  }

  .menu-icon {
    width: 16px;
    text-align: center;
    font-size: 12px;
  }

  .menu-label {
    flex: 1;
  }
</style>
