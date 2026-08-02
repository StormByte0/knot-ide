<script lang="ts">
  /**
   * Dock panel — a leaf node in the layout tree.
   *
   * Renders a {@link TabStrip} (tab bar) and the active tab's content
   * component. Maps tab `kind` to the appropriate Svelte component:
   *
   * - `editor` → `Editor.svelte`
   * - `filebrowser` → `FileBrowser.svelte`
   * - `storymap` / `build` / `settings` → placeholder (future phases)
   *
   * ## Drag-and-drop target
   *
   * This panel is a drop target for tab drags. On `dragover`, it computes the
   * dock zone from the pointer position (left/right/top/bottom/center) and
   * updates the drag store. On `drop`, it calls `layoutStore.moveTab` with
   * the source tab id + this panel's id + the active zone. The
   * {@link DropOverlay} renders the zone highlights during drag.
   *
   * ## Context
   *
   * The `openFile` callback is provided via Svelte context by `App.svelte`,
   * so `FileBrowser` can trigger editor-tab opens without prop-drilling
   * through the recursive layout tree.
   *
   * ## Active-file highlighting
   *
   * `FileBrowser.currentFile` reads from {@link statusStore.activeFile},
   * which the `Editor` component updates on mount/tab-swap. This decouples
   * `FileBrowser` from the layout store.
   */

  import { getContext } from 'svelte';
  import TabStrip from './TabStrip.svelte';
  import DropOverlay from './DropOverlay.svelte';
  import { layoutStore } from './layoutStore.svelte';
  import { dragStore, type DropZone } from './dragStore.svelte';
  import { statusStore } from '$lib/statusbar/statusStore.svelte';
  import type { EditorTabPayload, FileBrowserTabPayload, TabData } from './types';
  import Editor from '$lib/editor/Editor.svelte';
  import FileBrowser from '$lib/filebrowser/FileBrowser.svelte';

  interface Props {
    /** The panel id — used to look up the panel in the layout store. */
    panelId: string;
  }

  let { panelId }: Props = $props();

  /** App-level callback for opening files from the file browser. */
  const openFile = getContext<(path: string) => void>('openFile');

  // Reactive reads from the store.
  let panel = $derived(layoutStore.findPanel(panelId));
  let activeTab = $derived(
    panel?.activeTabId
      ? (panel.tabs.find((t) => t.id === panel.activeTabId) ?? null)
      : null,
  );

  // For FileBrowser's currentFile highlighting — reads from statusStore
  // (Editor pushes the active file path there).
  let activeFilePath = $derived(statusStore.activeFile);

  // Whether a drag is in progress AND this panel is the current target.
  // Drives whether DropOverlay renders + which zone is active.
  let isDropTarget = $derived(
    dragStore.isActive && dragStore.session?.targetPanelId === panelId,
  );
  let activeZone = $derived(dragStore.session?.currentZone ?? null);

  /**
   * Compute the dock zone from the pointer position relative to this panel's
   * content area. The center 50% × 50% is the "center" (join) zone; the
   * outer halves are left/right/top/bottom based on which edge is closest.
   */
  function computeZone(e: DragEvent, rect: DOMRect): DropZone {
    const x = (e.clientX - rect.left) / rect.width; // 0..1
    const y = (e.clientY - rect.top) / rect.height; // 0..1
    // Center box: 0.25..0.75 in both axes.
    const inCenterX = x >= 0.25 && x <= 0.75;
    const inCenterY = y >= 0.25 && y <= 0.75;
    if (inCenterX && inCenterY) return 'center';
    // Otherwise, pick the edge with the largest distance from center.
    const dx = Math.abs(x - 0.5);
    const dy = Math.abs(y - 0.5);
    if (dx > dy) {
      return x < 0.5 ? 'left' : 'right';
    }
    return y < 0.5 ? 'top' : 'bottom';
  }

  /** Handle dragover — compute zone, update drag store, allow drop. */
  function handleDragOver(e: DragEvent): void {
    if (!dragStore.isActive) return;
    // Only accept drags from a different panel, OR same panel for reorder
    // (reorder is handled by the TabStrip, not here — but we still allow
    // the drop so the browser doesn't show the "no-drop" cursor).
    const currentTarget = e.currentTarget as HTMLElement;
    const rect = currentTarget.getBoundingClientRect();
    const zone = computeZone(e, rect);
    e.preventDefault(); // Allow drop.
    e.dataTransfer!.dropEffect = 'move';
    dragStore.setTarget(panelId, zone);
  }

  /** Handle dragleave — clear this panel as the target if it was set. */
  function handleDragLeave(e: DragEvent): void {
    if (!dragStore.isActive) return;
    // Only clear if leaving the panel entirely (not entering a child).
    // relatedTarget is null when leaving the window.
    const related = e.relatedTarget as Node | null;
    const currentTarget = e.currentTarget as HTMLElement;
    if (!related || !currentTarget.contains(related)) {
      if (dragStore.session?.targetPanelId === panelId) {
        dragStore.clearTarget();
      }
    }
  }

  /** Handle drop — call layoutStore.moveTab with the source + zone. */
  function handleDrop(e: DragEvent): void {
    if (!dragStore.isActive || !dragStore.session) return;
    e.preventDefault();
    const { sourceTabId, currentZone } = dragStore.session;
    if (!currentZone) return;
    // Don't drop onto the source panel at center (no-op — same panel).
    if (dragStore.session.sourcePanelId === panelId && currentZone === 'center') return;
    layoutStore.moveTab(sourceTabId, panelId, currentZone);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="dock-panel"
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
  <TabStrip {panelId} />
  <div class="panel-content">
    {#if activeTab}
      {@render renderContent(activeTab)}
    {:else}
      <div class="empty-panel">No tab open</div>
    {/if}
    {#if isDropTarget}
      <DropOverlay activeZone={activeZone} />
    {/if}
  </div>
</div>

{#snippet renderContent(tab: TabData)}
  {#if tab.kind === 'editor'}
    {@const payload = tab.payload as EditorTabPayload}
    <Editor
      tabId={tab.id}
      uri={payload.uri}
      content={payload.content}
      language={payload.languageId}
    />
  {:else if tab.kind === 'filebrowser'}
    {@const payload = tab.payload as FileBrowserTabPayload}
    <FileBrowser
      folder={payload.folder}
      currentFile={activeFilePath}
      onSelect={openFile ?? (() => console.warn('[knot:layout] no openFile context'))}
    />
  {:else}
    <div class="placeholder">
      <p>{tab.kind} panel</p>
      <p class="placeholder-hint">Not yet implemented — coming in a later phase.</p>
    </div>
  {/if}
{/snippet}

<style>
  .dock-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--bg-editor);
  }

  .panel-content {
    flex: 1;
    position: relative;
    overflow: hidden;
    background: var(--bg-editor);
  }

  .empty-panel {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--fg-muted);
    font-size: 14px;
    user-select: none;
  }

  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--fg-muted);
    font-size: 14px;
    gap: 4px;
    user-select: none;
  }

  .placeholder-hint {
    font-size: 12px;
    color: var(--fg-muted);
  }
</style>
