<script lang="ts">
  /** Right-click context menu for the file browser.
   *
   *  Positions itself at `(x, y)` but clamps to the viewport so the menu
   *  never gets clipped by window edges. Uses a `$effect` to measure the
   *  rendered menu element after mount and adjust the position in-place.
   */

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

  /** The rendered menu element — bound via `bind:this`. */
  let menuEl: HTMLDivElement;

  /**
   * After the menu renders, measure it and clamp the position so the menu
   * stays fully visible within the viewport. Runs whenever `x`, `y`, or
   * `items` change (different items = different menu height).
   */
  $effect(() => {
    // Read props to register reactive dependencies.
    const _x = x;
    const _y = y;
    const _items = items;
    if (!menuEl) return;

    const rect = menuEl.getBoundingClientRect();
    const margin = 4;
    let clampedX = _x;
    let clampedY = _y;

    if (clampedX + rect.width > window.innerWidth - margin) {
      clampedX = Math.max(margin, window.innerWidth - rect.width - margin);
    }
    if (clampedY + rect.height > window.innerHeight - margin) {
      clampedY = Math.max(margin, window.innerHeight - rect.height - margin);
    }

    menuEl.style.left = `${clampedX}px`;
    menuEl.style.top = `${clampedY}px`;
  });

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
  bind:this={menuEl}
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
    background: var(--bg-context-menu);
    border: 1px solid var(--border-default);
    border-radius: 4px;
    padding: 4px 0;
    min-width: 180px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
    z-index: 1000;
  }

  .menu-separator {
    height: 1px;
    background: var(--border-default);
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
    color: var(--fg-context-menu);
    padding: 6px 12px;
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
  }

  .menu-item:hover {
    background: var(--bg-active-selection);
    color: var(--fg-default);
  }

  .menu-item.danger {
    color: var(--danger);
  }

  .menu-item.danger:hover {
    background: color-mix(in srgb, var(--danger) 25%, transparent);
    color: var(--danger);
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
