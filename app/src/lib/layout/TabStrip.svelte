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

  /**
   * Current reorder drop target during an in-strip drag. `null` when no
   * reorder is pending. `before` is `true` if the dragged tab will be
   * inserted before `tabId`, `false` if after. Drives the `drop-before` /
   * `drop-after` CSS classes for the visual indicator.
   */
  let reorderTarget = $state<{ tabId: string; before: boolean } | null>(null);

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

  /**
   * Handle dragover on a tab — compute whether to insert before or after
   * based on pointer X relative to the tab's horizontal center. Stops
   * propagation so DockPanel doesn't also process this as a split-zone drop
   * (the tab strip owns in-strip reordering; DockPanel owns split-zone drops
   * on the panel content area below the strip).
   *
   * Sets a visual indicator (drop-before / drop-after class) via the
   * `reorderTarget` state so the user sees where the tab will land.
   */
  function handleTabDragOver(tab: TabData, e: DragEvent): void {
    if (!dragStore.isActive) return;
    // Only accept drops from the same panel (cross-panel drops go through
    // DockPanel's zone logic). This keeps reordering simple — cross-panel
    // moves use the split/center zones instead.
    if (dragStore.session?.sourcePanelId !== panelId) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const isAfter = e.clientX > rect.left + rect.width / 2;
    reorderTarget = { tabId: tab.id, before: !isAfter };
  }

  /** Handle dragleave on a tab — clear the reorder indicator if it was set. */
  function handleTabDragLeave(tab: TabData): void {
    if (reorderTarget?.tabId === tab.id) reorderTarget = null;
  }

  /**
   * Handle drop on a tab — compute the insertion index + call
   * `layoutStore.reorderTabInPanel`. Stops propagation so DockPanel doesn't
   * also process it as a split-zone drop.
   *
   * If the source tab is dropped on itself, this is a no-op (the store
   * clamps to the same index). If dropped on a neighbor, the tab moves
   * before or after the target based on the pointer position.
   */
  function handleTabDrop(tab: TabData, e: DragEvent): void {
    if (!dragStore.isActive || !dragStore.session) return;
    e.preventDefault();
    e.stopPropagation();
    const sourceTabId = dragStore.session.sourceTabId;
    if (dragStore.session.sourcePanelId !== panelId) return; // cross-panel → DockPanel handles
    // Compute the target index: insert before `tab` if `before`, else after.
    const targetIndex = tabs.findIndex((t) => t.id === tab.id);
    if (targetIndex === -1) return;
    const insertAt = reorderTarget?.before ? targetIndex : targetIndex + 1;
    // Adjust for the source tab being removed before insertion: if the
    // source is before the target index, the target shifts down by one.
    const sourceIndex = tabs.findIndex((t) => t.id === sourceTabId);
    const adjustedIndex = sourceIndex !== -1 && sourceIndex < insertAt
      ? insertAt - 1
      : insertAt;
    layoutStore.reorderTabInPanel(panelId, sourceTabId, adjustedIndex);
    reorderTarget = null;
  }

  /** End a tab drag. Always called (success or cancel). Clears the session. */
  function handleDragEnd(): void {
    dragStore.endDrag();
    reorderTarget = null;
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
        class:drop-before={reorderTarget?.tabId === tab.id && reorderTarget.before}
        class:drop-after={reorderTarget?.tabId === tab.id && !reorderTarget.before}
        title={tab.kind === 'editor' ? (tab.payload as EditorTabPayload).path : tab.title}
        draggable="true"
        onclick={() => handleClick(tab)}
        onauxclick={(e) => handleAuxClick(tab, e)}
        oncontextmenu={(e) => handleContextMenu(tab, e)}
        onmouseenter={() => (hoveredId = tab.id)}
        onmouseleave={() => { hoveredId = null; handleTabDragLeave(tab); }}
        ondragstart={(e) => handleDragStart(tab, e)}
        ondragover={(e) => handleTabDragOver(tab, e)}
        ondragleave={() => handleTabDragLeave(tab)}
        ondrop={(e) => handleTabDrop(tab, e)}
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
    background: var(--bg-tab-strip);
    border-bottom: 1px solid var(--border-subtle);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
    flex-shrink: 0;
  }

  .tab-strip-empty {
    height: 36px;
    background: var(--bg-tab-strip);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    min-width: 100px;
    max-width: 240px;
    background: var(--bg-tab);
    color: var(--fg-tab);
    border: none;
    border-right: 1px solid var(--border-subtle);
    cursor: pointer;
    font-size: 13px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    white-space: nowrap;
    position: relative;
  }

  .tab:hover {
    background: var(--bg-hover);
    color: var(--fg-default);
  }

  .tab.active {
    background: var(--bg-tab-active);
    color: var(--fg-tab-active);
  }

  .tab.dragging {
    opacity: 0.4;
  }

  /* Reorder drop indicators — a 2px accent-colored bar on the side where the
     tab will be inserted. Only shows during an active in-strip drag. */
  .tab.drop-before::before,
  .tab.drop-after::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--accent);
    z-index: 2;
    pointer-events: none;
  }

  .tab.drop-before::before {
    left: 0;
  }

  .tab.drop-after::before {
    right: 0;
  }

  .tab.active::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--accent);
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
    color: var(--fg-muted);
    cursor: pointer;
    flex-shrink: 0;
  }

  .tab-close:hover {
    background: var(--bg-hover);
    color: var(--fg-default);
  }

  .dirty-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--fg-default);
    display: inline-block;
  }

  .ctx-menu {
    position: fixed;
    z-index: 1000;
    background: var(--bg-context-menu);
    border: 1px solid var(--border-default);
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
    color: var(--fg-context-menu);
    text-align: left;
    padding: 6px 16px;
    font-size: 13px;
    cursor: pointer;
    font-family: inherit;
  }

  .ctx-menu button:hover {
    background: var(--bg-active-selection);
    color: var(--fg-default);
  }

  .ctx-separator {
    height: 1px;
    background: var(--border-default);
    margin: 4px 0;
  }
</style>
