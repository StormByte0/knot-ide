<script lang="ts">
  /**
   * Editor tab strip.
   *
   * Pure presentation + event forwarding. Reads tab state from
   * {@link editorStore} and renders one tab per open file. Click switches,
   * middle-click or X closes, right-click opens a context menu (Close / Close
   * Others / Close All).
   *
   * Owns no state beyond transient UI (which tab is hovered, context-menu
   * position). Dirty tabs show a dot instead of the close X until hovered,
   * matching VS Code's behavior.
   *
   * ## Close-on-dirty
   *
   * Closing a dirty tab is a two-step dance: the store's `closeTab` refuses
   * to close dirty tabs, so this component prompts via {@link ConfirmDialog}
   * first, then calls `forceClose` on confirm.
   */

  import { editorStore, type EditorTab } from './editorStore.svelte';
  import ConfirmDialog from '$lib/filebrowser/ConfirmDialog.svelte';

  /** Pending close-dirty confirmation. `null` when no dialog is open. */
  let pendingClose = $state<
    | { kind: 'single'; tab: EditorTab }
    | { kind: 'others'; keep: EditorTab; dirty: string[] }
    | { kind: 'all'; dirty: string[] }
    | null
  >(null);

  /** Context menu state. `null` when closed. */
  let ctxMenu = $state<{ x: number; y: number; tab: EditorTab } | null>(null);

  /** Tab id currently being hovered (for showing the X on dirty tabs). */
  let hoveredId = $state<string | null>(null);

  // Reactive reads from the store.
  let tabs = $derived(editorStore.tabs);
  let activeTabId = $derived(editorStore.activeTabId);

  /** Activate a tab on click. */
  function handleClick(tab: EditorTab): void {
    editorStore.switchTo(tab.id);
  }

  /** Middle-click closes a tab (browser convention). */
  function handleAuxClick(tab: EditorTab, e: MouseEvent): void {
    if (e.button === 1) {
      // Middle-click.
      e.preventDefault();
      handleClose(tab);
    }
  }

  /** X button click. */
  function handleClose(tab: EditorTab): void {
    if (!editorStore.closeTab(tab.id)) {
      // Refused — tab is dirty. Prompt.
      pendingClose = { kind: 'single', tab };
    }
  }

  /** Right-click → context menu. */
  function handleContextMenu(tab: EditorTab, e: MouseEvent): void {
    e.preventDefault();
    e.stopPropagation();
    // Activate the tab so the menu actions target the visible tab.
    editorStore.switchTo(tab.id);
    ctxMenu = { x: e.clientX, y: e.clientY, tab };
  }

  /** Close Others: store returns dirty ids that refused; prompt if any. */
  function handleCloseOthers(tab: EditorTab): void {
    const dirty = editorStore.closeOthers(tab.id);
    if (dirty.length > 0) {
      pendingClose = { kind: 'others', keep: tab, dirty };
    }
  }

  /** Close All: store returns dirty ids that refused; prompt if any. */
  function handleCloseAll(): void {
    const dirty = editorStore.closeAll();
    if (dirty.length > 0) {
      pendingClose = { kind: 'all', dirty };
    }
  }

  /** Confirm the pending dirty-close dialog. */
  function confirmPendingClose(): void {
    if (!pendingClose) return;
    if (pendingClose.kind === 'single') {
      editorStore.forceClose(pendingClose.tab.id);
    } else if (pendingClose.kind === 'others') {
      // Force-close every tab except the kept one.
      for (const id of editorStore.tabs.map((t) => t.id)) {
        if (id !== pendingClose.keep.id) editorStore.forceClose(id);
      }
    } else if (pendingClose.kind === 'all') {
      for (const id of editorStore.tabs.map((t) => t.id)) {
        editorStore.forceClose(id);
      }
    }
    pendingClose = null;
  }

  /** Cancel the pending dirty-close dialog. */
  function cancelPendingClose(): void {
    pendingClose = null;
  }

  /** Close the context menu on any window click. */
  function closeContextMenu(): void {
    ctxMenu = null;
  }

  /** Build the message for the confirm dialog based on pending kind. */
  function confirmMessage(): string {
    if (!pendingClose) return '';
    if (pendingClose.kind === 'single') {
      return `“${pendingClose.tab.name}” has unsaved changes. Close anyway? Your edits will be lost.`;
    }
    const count = pendingClose.kind === 'all' ? pendingClose.dirty.length : pendingClose.dirty.length;
    const noun = count === 1 ? 'tab has' : 'tabs have';
    return `${count} ${noun} unsaved changes. Close anyway? Your edits will be lost.`;
  }
</script>

<svelte:window onclick={closeContextMenu} />

{#if tabs.length === 0}
  <div class="tab-strip-empty"></div>
{:else}
  <div class="tab-strip" role="tablist">
    {#each tabs as tab (tab.id)}
      <button
        type="button"
        role="tab"
        class="tab"
        class:active={tab.id === activeTabId}
        class:dirty={tab.isDirty}
        title={tab.path}
        onclick={() => handleClick(tab)}
        onauxclick={(e) => handleAuxClick(tab, e)}
        oncontextmenu={(e) => handleContextMenu(tab, e)}
        onmouseenter={() => (hoveredId = tab.id)}
        onmouseleave={() => (hoveredId = null)}
      >
        <span class="tab-name">{tab.name}</span>
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          class="tab-close"
          onclick={(e) => { e.stopPropagation(); handleClose(tab); }}
          role="button"
          tabindex="-1"
          aria-label="Close tab"
        >
          {#if tab.isDirty && hoveredId !== tab.id}
            <span class="dirty-dot" aria-hidden="true"></span>
          {:else}
            ×
          {/if}
        </span>
      </button>
    {/each}
  </div>
{/if}

{#if ctxMenu}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div
    class="ctx-menu"
    style="left: {ctxMenu.x}px; top: {ctxMenu.y}px"
    role="menu"
    onclick={(e) => e.stopPropagation()}
  >
    <button type="button" role="menuitem" onclick={() => { handleClose(ctxMenu!.tab); closeContextMenu(); }}>
      Close
    </button>
    <button type="button" role="menuitem" onclick={() => { handleCloseOthers(ctxMenu!.tab); closeContextMenu(); }}>
      Close Others
    </button>
    <button type="button" role="menuitem" onclick={() => { handleCloseAll(); closeContextMenu(); }}>
      Close All
    </button>
  </div>
{/if}

{#if pendingClose}
  <ConfirmDialog
    title="Close unsaved tab?"
    message={confirmMessage()}
    confirmLabel="Close Anyway"
    cancelLabel="Cancel"
    danger={true}
    onConfirm={confirmPendingClose}
    onCancel={cancelPendingClose}
  />
{/if}

<style>
  .tab-strip {
    display: flex;
    align-items: stretch;
    height: 36px;
    background: #252526;
    border-bottom: 1px solid #1e1e1e;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
  }

  .tab-strip-empty {
    height: 36px;
    background: #252526;
    border-bottom: 1px solid #1e1e1e;
    flex-shrink: 0;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    min-width: 120px;
    max-width: 240px;
    background: #2d2d2d;
    color: #969696;
    border: none;
    border-right: 1px solid #1e1e1e;
    cursor: pointer;
    font-size: 13px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    white-space: nowrap;
    position: relative;
  }

  .tab:hover {
    background: #343434;
    color: #cccccc;
  }

  .tab.active {
    background: #1e1e1e;
    color: #ffffff;
  }

  .tab.active::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 1px;
    background: #007acc;
  }

  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
  }

  .tab-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 3px;
    font-size: 16px;
    line-height: 1;
    color: #888;
    cursor: pointer;
    flex-shrink: 0;
  }

  .tab-close:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
  }

  .dirty-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #cccccc;
    display: inline-block;
  }

  .ctx-menu {
    position: fixed;
    z-index: 1000;
    background: #252526;
    border: 1px solid #454545;
    border-radius: 4px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
    padding: 4px 0;
    min-width: 160px;
    display: flex;
    flex-direction: column;
  }

  .ctx-menu button {
    background: none;
    border: none;
    color: #cccccc;
    text-align: left;
    padding: 6px 16px;
    font-size: 13px;
    cursor: pointer;
    font-family: inherit;
  }

  .ctx-menu button:hover {
    background: #094771;
    color: #fff;
  }
</style>
