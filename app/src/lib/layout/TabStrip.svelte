<script lang="ts">
  /**
   * Generic tab strip for a dock panel.
   *
   * Replaces the Task 2 `EditorTabs.svelte` — this version is kind-agnostic:
   * it renders tabs for any panel (editor, filebrowser, storymap, etc.).
   * Dirty-dot and close-on-dirty behavior applies only to editor tabs
   * (checked via `tab.kind === 'editor'` + payload cast).
   *
   * Reads panel state from {@link layoutStore} by `panelId`. All mutations
   * go through the store — this component owns no state beyond transient UI
   * (hover, context menu position, pending close-dirty dialog).
   */

  import { layoutStore } from './layoutStore.svelte';
  import { dragStore } from './dragStore.svelte';
  import type { EditorTabPayload, TabData } from './types';
  import ConfirmDialog from '$lib/filebrowser/ConfirmDialog.svelte';
  import { detachTab } from '$lib/windows/windowManager';

  interface Props {
    /** The panel whose tabs to render. */
    panelId: string;
  }

  let { panelId }: Props = $props();

  /** Pending close-dirty confirmation. `null` when no dialog is open. */
  let pendingClose = $state<
    | { kind: 'single'; tab: TabData }
    | { kind: 'others'; keep: TabData; dirty: string[] }
    | { kind: 'all'; dirty: string[] }
    | null
  >(null);

  /** Context menu state. `null` when closed. */
  let ctxMenu = $state<{ x: number; y: number; tab: TabData } | null>(null);

  /** Tab id currently hovered (for showing the X on dirty tabs). */
  let hoveredId = $state<string | null>(null);

  // Reactive reads from the store.
  let panel = $derived(layoutStore.findPanel(panelId));
  let tabs = $derived(panel?.tabs ?? []);
  let activeTabId = $derived(panel?.activeTabId ?? null);

  /** Check if a tab is a dirty editor tab. */
  function isDirty(tab: TabData): boolean {
    return tab.kind === 'editor' && (tab.payload as EditorTabPayload).isDirty;
  }

  /** Activate a tab on click. */
  function handleClick(tab: TabData): void {
    layoutStore.switchTab(panelId, tab.id);
  }

  /** Middle-click closes a tab (browser convention). */
  function handleAuxClick(tab: TabData, e: MouseEvent): void {
    if (e.button === 1) {
      e.preventDefault();
      handleClose(tab);
    }
  }

  /** X / middle-click close — prompts if dirty. */
  function handleClose(tab: TabData): void {
    if (!layoutStore.closeTab(tab.id)) {
      pendingClose = { kind: 'single', tab };
    }
  }

  /** Right-click → context menu. */
  function handleContextMenu(tab: TabData, e: MouseEvent): void {
    e.preventDefault();
    e.stopPropagation();
    layoutStore.switchTab(panelId, tab.id);
    ctxMenu = { x: e.clientX, y: e.clientY, tab };
  }

  /** Close Others — store returns dirty ids that refused; prompt if any. */
  function handleCloseOthers(tab: TabData): void {
    const dirty = layoutStore.closeOthers(panelId, tab.id);
    if (dirty.length > 0) {
      pendingClose = { kind: 'others', keep: tab, dirty };
    }
  }

  /** Close All — store returns dirty ids that refused; prompt if any. */
  function handleCloseAll(): void {
    const dirty = layoutStore.closeAll(panelId);
    if (dirty.length > 0) {
      pendingClose = { kind: 'all', dirty };
    }
  }

  /** Confirm the pending dirty-close dialog. */
  function confirmPendingClose(): void {
    if (!pendingClose) return;
    if (pendingClose.kind === 'single') {
      layoutStore.forceCloseTab(pendingClose.tab.id);
    } else if (pendingClose.kind === 'others') {
      // Force-close every tab except the kept one.
      const keepId = pendingClose.keep.id;
      const currentTabs = panel?.tabs.map((t) => t.id) ?? [];
      for (const id of currentTabs) {
        if (id !== keepId) layoutStore.forceCloseTab(id);
      }
    } else if (pendingClose.kind === 'all') {
      layoutStore.forceCloseAll(panelId);
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
      return `“${pendingClose.tab.title}” has unsaved changes. Close anyway? Your edits will be lost.`;
    }
    const count = pendingClose.dirty.length;
    const noun = count === 1 ? 'tab has' : 'tabs have';
    return `${count} ${noun} unsaved changes. Close anyway? Your edits will be lost.`;
  }

  /** Start a tab drag. Sets the drag session in the store. */
  function handleDragStart(tab: TabData, e: DragEvent): void {
    if (!e.dataTransfer) return;
    e.dataTransfer.effectAllowed = 'move';
    // Set a minimal payload — the store is the real source of truth.
    e.dataTransfer.setData('text/plain', tab.id);
    dragStore.startDrag(tab.id, panelId);
  }

  /** End a tab drag. Always called (success or cancel). Clears the session. */
  function handleDragEnd(): void {
    dragStore.endDrag();
  }

  /** Send a tab to a new OS window (detach). */
  async function handleSendToNewWindow(tab: TabData): Promise<void> {
    await detachTab(tab.id);
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
        class:dirty={isDirty(tab)}
        class:dragging={dragStore.session?.sourceTabId === tab.id}
        title={tab.kind === 'editor' ? (tab.payload as EditorTabPayload).path : tab.title}
        draggable="true"
        onclick={() => handleClick(tab)}
        onauxclick={(e) => handleAuxClick(tab, e)}
        oncontextmenu={(e) => handleContextMenu(tab, e)}
        onmouseenter={() => (hoveredId = tab.id)}
        onmouseleave={() => (hoveredId = null)}
        ondragstart={(e) => handleDragStart(tab, e)}
        ondragend={handleDragEnd}
      >
        <span class="tab-name">{tab.title}</span>
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          class="tab-close"
          onclick={(e) => { e.stopPropagation(); handleClose(tab); }}
          role="button"
          tabindex="-1"
          aria-label="Close tab"
        >
          {#if isDirty(tab) && hoveredId !== tab.id}
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
    <div class="ctx-separator"></div>
    <button type="button" role="menuitem" onclick={() => { handleSendToNewWindow(ctxMenu!.tab); closeContextMenu(); }}>
      Send to New Window
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
    flex-shrink: 0;
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
    min-width: 100px;
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

  .tab.dragging {
    opacity: 0.4;
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

  .ctx-separator {
    height: 1px;
    background: #3c3c3c;
    margin: 4px 0;
  }
</style>
