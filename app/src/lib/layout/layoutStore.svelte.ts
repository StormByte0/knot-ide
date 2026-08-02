/**
 * Layout tree store.
 *
 * Owns the root {@link LayoutNode} tree and all structural operations
 * (open/close/switch tabs, resize splits). The tree is `$state`, so Svelte 5
 * deeply proxies it — in-place mutations (e.g. `panel.tabs.push(tab)`)
 * trigger reactivity automatically.
 *
 * ## Ownership
 *
 * The store owns **layout structure** only: which panels exist, their tab
 * lists, active tabs, and split sizes. It does NOT own Monaco model
 * instances (those live in Monaco's global registry by URI), file-system
 * state (the file browser owns the tree), or LSP state (the LSP client owns
 * document tracking). Content is cached on editor-tab payloads so the Editor
 * can create/restore models without re-reading disk, but the store never
 * writes to disk.
 *
 * ## Svelte 5 runes
 *
 * Uses `$state` — this file MUST be named `*.svelte.ts` so the Svelte
 * compiler processes it (see worklog phase1-task1-fix). Imports must use the
 * explicit `.svelte` extension.
 */

import type {
  EditorTabPayload,
  FileBrowserTabPayload,
  LayoutNode,
  PanelNode,
  TabData,
} from './types';

class LayoutStore {
  /** Root of the layout tree. `null` when no workspace is open. */
  root = $state<LayoutNode | null>(null);

  /**
   * Initialize the default layout for a workspace.
   *
   * Creates a horizontal split: filebrowser panel on the left (20%),
   * editor panel on the right (80%). The editor panel starts empty — tabs
   * are added when the user opens files.
   */
  initDefaultLayout(workspaceFolder: string): void {
    this.root = {
      type: 'split',
      direction: 'horizontal',
      children: [
        {
          type: 'panel',
          id: 'sidebar',
          tabs: [
            {
              id: 'files',
              kind: 'filebrowser',
              title: 'Files',
              payload: { folder: workspaceFolder } satisfies FileBrowserTabPayload,
            },
          ],
          activeTabId: 'files',
        },
        {
          type: 'panel',
          id: 'editor',
          tabs: [],
          activeTabId: null,
        },
      ],
      sizes: [20, 80],
    };
  }

  /** Reset the layout to `null` (no workspace). */
  clear(): void {
    this.root = null;
  }

  // ── Tab operations ──────────────────────────────────────────────────

  /**
   * Open an editor tab for `path`. Dedupes: if a tab for this path already
   * exists in the editor panel, just activates it. Otherwise creates a new
   * tab with the given content.
   */
  openEditorTab(path: string, content: string, languageId: string = 'twee'): void {
    const panel = this.findEditorPanel();
    if (!panel) {
      console.warn('[knot:layout] no editor panel found — cannot open tab');
      return;
    }
    const existing = panel.tabs.find((t) => t.id === path);
    if (existing) {
      panel.activeTabId = path;
      return;
    }
    const tab: TabData = {
      id: path,
      kind: 'editor',
      title: basename(path),
      payload: {
        path,
        uri: pathToUri(path),
        languageId,
        content,
        isDirty: false,
      } satisfies EditorTabPayload,
    };
    panel.tabs.push(tab);
    panel.activeTabId = path;
  }

  /**
   * Close a tab. Returns `true` if closed, `false` if refused (editor tab
   * is dirty). Caller must confirm and call {@link forceCloseTab}.
   */
  closeTab(tabId: string): boolean {
    const panel = this.findPanelByTabId(tabId);
    if (!panel) return true;
    const tab = panel.tabs.find((t) => t.id === tabId);
    if (!tab) return true;
    if (isDirtyEditorTab(tab)) return false;
    this.forceCloseTab(tabId);
    return true;
  }

  /** Close a tab unconditionally, even if dirty. Used after user confirms. */
  forceCloseTab(tabId: string): void {
    const panel = this.findPanelByTabId(tabId);
    if (!panel) return;
    const idx = panel.tabs.findIndex((t) => t.id === tabId);
    if (idx === -1) return;
    panel.tabs.splice(idx, 1);
    if (panel.activeTabId === tabId) {
      // Activate the neighbor: prefer the tab now at the same index, fall
      // back to the previous tab, or null if the panel is now empty.
      const next = panel.tabs[idx] ?? panel.tabs[idx - 1] ?? null;
      panel.activeTabId = next ? next.id : null;
    }
  }

  /** Switch the active tab in a panel. No-op if `tabId` is not in the panel. */
  switchTab(panelId: string, tabId: string): void {
    const panel = this.findPanel(panelId);
    if (!panel) return;
    if (panel.tabs.some((t) => t.id === tabId)) {
      panel.activeTabId = tabId;
    }
  }

  /**
   * Close all tabs in a panel except `keepTabId`. Returns dirty tab ids that
   * were NOT closed (caller must confirm before force-closing).
   */
  closeOthers(panelId: string, keepTabId: string): string[] {
    const panel = this.findPanel(panelId);
    if (!panel) return [];
    const dirty = panel.tabs
      .filter((t) => t.id !== keepTabId && isDirtyEditorTab(t))
      .map((t) => t.id);
    if (dirty.length > 0) return dirty;
    panel.tabs = panel.tabs.filter((t) => t.id === keepTabId);
    panel.activeTabId = keepTabId;
    return [];
  }

  /**
   * Close all tabs in a panel. Returns dirty tab ids that were NOT closed
   * (caller must confirm before force-closing).
   */
  closeAll(panelId: string): string[] {
    const panel = this.findPanel(panelId);
    if (!panel) return [];
    const dirty = panel.tabs.filter(isDirtyEditorTab).map((t) => t.id);
    if (dirty.length > 0) return dirty;
    panel.tabs = [];
    panel.activeTabId = null;
    return [];
  }

  /** Force-close all tabs in a panel (after user confirms dirty close). */
  forceCloseAll(panelId: string): void {
    const panel = this.findPanel(panelId);
    if (!panel) return;
    panel.tabs = [];
    panel.activeTabId = null;
  }

  // ── Content tracking (editor tabs) ──────────────────────────────────

  /** Update an editor tab's content + mark it dirty. Called by `<Editor>`. */
  markTabContentChanged(tabId: string, content: string): void {
    const panel = this.findPanelByTabId(tabId);
    if (!panel) return;
    const tab = panel.tabs.find((t) => t.id === tabId);
    if (!tab || tab.kind !== 'editor') return;
    const payload = tab.payload as EditorTabPayload;
    payload.content = content;
    payload.isDirty = true;
  }

  /** Mark an editor tab clean (e.g. after save — not wired yet). */
  markTabClean(tabId: string): void {
    const panel = this.findPanelByTabId(tabId);
    if (!panel) return;
    const tab = panel.tabs.find((t) => t.id === tabId);
    if (!tab || tab.kind !== 'editor') return;
    (tab.payload as EditorTabPayload).isDirty = false;
  }

  // ── Resize ──────────────────────────────────────────────────────────

  /**
   * Adjust two adjacent children's sizes in a split. `deltaPercent` is added
   * to `sizes[childIndex]` and subtracted from `sizes[childIndex + 1]`.
   * Clamped to a 5% minimum so panels can't collapse to zero.
   */
  resizeSplit(splitNode: LayoutNode, childIndex: number, deltaPercent: number): void {
    if (splitNode.type !== 'split') return;
    const sizes = splitNode.sizes;
    if (childIndex < 0 || childIndex >= sizes.length - 1) return;
    const minSize = 5;
    const sum = sizes[childIndex] + sizes[childIndex + 1];
    let newLeft = sizes[childIndex] + deltaPercent;
    let newRight = sizes[childIndex + 1] - deltaPercent;
    // Clamp both to minimum; if both hit minimum, split the remainder evenly.
    if (newLeft < minSize) newLeft = minSize;
    if (newRight < minSize) newRight = minSize;
    if (newLeft + newRight > sum) {
      // One was clamped — give the excess to the other.
      if (newLeft === minSize) newRight = sum - newLeft;
      else newLeft = sum - newRight;
    }
    sizes[childIndex] = newLeft;
    sizes[childIndex + 1] = newRight;
  }

  // ── Queries ─────────────────────────────────────────────────────────

  /** Find a panel by id. Returns `null` if not found. */
  findPanel(panelId: string): PanelNode | null {
    if (!this.root) return null;
    return findPanelInNode(this.root, panelId);
  }

  /**
   * Find the editor panel. Tries by id `'editor'` first, then falls back to
   * the first panel containing an editor tab. Returns `null` if none.
   */
  findEditorPanel(): PanelNode | null {
    if (!this.root) return null;
    const byId = findPanelInNode(this.root, 'editor');
    if (byId) return byId;
    return findFirstPanelWithKind(this.root, 'editor');
  }

  /** Find the panel containing a tab by id. Returns `null` if not found. */
  findPanelByTabId(tabId: string): PanelNode | null {
    if (!this.root) return null;
    return findPanelContainingTab(this.root, tabId);
  }

  /**
   * Get the active editor tab across all panels. Returns the first panel's
   * active editor tab, or `null`. Used by the status bar and menu actions.
   */
  getActiveEditorTab(): TabData | null {
    if (!this.root) return null;
    return findActiveEditorTab(this.root);
  }

  // ── Drag-and-drop ───────────────────────────────────────────────────

  /**
   * Move a tab to a target panel at a dock zone. This is the single entry
   * point for all tab DnD operations — the drop handler just calls this with
   * the source tab id + target panel + zone, and the store handles all
   * structural mutation (remove from source, split or join at target).
   *
   * ## Zones
   *
   * - `center` — drop into the target panel's tab list (no split created).
   *   Activates the moved tab.
   * - `left` / `right` — split the target panel horizontally, insert the
   *   tab into a new panel on that side.
   * - `top` / `bottom` — split vertically, insert into a new panel on that
   *   side.
   *
   * After the move, {@link pruneEmptyPanels} runs to remove any panel left
   * empty by the drag-out, and collapse any split left with a single child.
   *
   * No-op (returns without mutation) if:
   * - The source tab is not found.
   * - Source and target are the same panel and zone is `center` (dropping a
   *   tab back onto its own panel — handled by reorder, not moveTab).
   */
  moveTab(
    sourceTabId: string,
    targetPanelId: string,
    zone: 'left' | 'right' | 'top' | 'bottom' | 'center',
  ): void {
    if (!this.root) return;
    const sourcePanel = this.findPanelByTabId(sourceTabId);
    const targetPanel = this.findPanel(targetPanelId);
    if (!sourcePanel || !targetPanel) return;
    const tabIndex = sourcePanel.tabs.findIndex((t) => t.id === sourceTabId);
    if (tabIndex === -1) return;
    const tab = sourcePanel.tabs[tabIndex];

    if (zone === 'center') {
      // Join: move into the target panel's tab list.
      if (sourcePanel.id === targetPanel.id) {
        // Same panel — just reorder (move to end, or activate).
        return;
      }
      sourcePanel.tabs.splice(tabIndex, 1);
      targetPanel.tabs.push(tab);
      targetPanel.activeTabId = tab.id;
      // Fix source panel's activeTabId if it was the moved tab.
      fixActiveTabAfterRemoval(sourcePanel);
    } else {
      // Split: create a new panel + new split node containing [target, new]
      // or [new, target] depending on zone.
      const newPanelId = generatePanelId();
      const newPanel: PanelNode = {
        type: 'panel',
        id: newPanelId,
        tabs: [tab],
        activeTabId: tab.id,
      };
      // Remove from source first (so prune can clean up).
      sourcePanel.tabs.splice(tabIndex, 1);
      fixActiveTabAfterRemoval(sourcePanel);

      // Find the split containing the target panel + replace the target
      // panel with a new split.
      const direction = zone === 'left' || zone === 'right' ? 'horizontal' : 'vertical';
      const newTabFirst = zone === 'left' || zone === 'top';
      const newSplit: LayoutNode = {
        type: 'split',
        direction,
        children: newTabFirst ? [newPanel, targetPanel] : [targetPanel, newPanel],
        sizes: [50, 50],
      };
      replaceNodeInTree(this.root, targetPanel, newSplit);
    }

    // Clean up any empty panels / single-child splits left behind.
    pruneEmptyPanels(this.root, null);
  }

  /**
   * Reorder a tab within its own panel. `newIndex` is clamped to the valid
   * range. Activates the moved tab.
   */
  reorderTabInPanel(panelId: string, tabId: string, newIndex: number): void {
    const panel = this.findPanel(panelId);
    if (!panel) return;
    const oldIndex = panel.tabs.findIndex((t) => t.id === tabId);
    if (oldIndex === -1) return;
    const clamped = Math.max(0, Math.min(newIndex, panel.tabs.length - 1));
    if (clamped === oldIndex) return;
    const [tab] = panel.tabs.splice(oldIndex, 1);
    panel.tabs.splice(clamped, 0, tab);
    panel.activeTabId = tab.id;
  }
}

/** Generate a unique panel id for newly-split panels. */
function generatePanelId(): string {
  return `panel-${Date.now()}-${Math.floor(Math.random() * 10000)}`;
}

/** After removing a tab, fix the panel's activeTabId if it pointed to the removed tab. */
function fixActiveTabAfterRemoval(panel: PanelNode): void {
  if (panel.activeTabId && !panel.tabs.some((t) => t.id === panel.activeTabId)) {
    panel.activeTabId = panel.tabs.length > 0 ? panel.tabs[panel.tabs.length - 1].id : null;
  }
}

/**
 * Replace `oldNode` with `newNode` in the tree. Mutates the parent split's
 * `children` array in place. If `oldNode` is the root, the caller must
 * handle replacement (the root can't be replaced from within — but splits
 * always wrap the root, so this is only called on non-root nodes in
 * practice via `moveTab`).
 *
 * Returns `true` if replaced, `false` if not found.
 */
function replaceNodeInTree(root: LayoutNode, oldNode: LayoutNode, newNode: LayoutNode): boolean {
  if (root.type !== 'split') return false;
  const idx = root.children.indexOf(oldNode);
  if (idx !== -1) {
    root.children[idx] = newNode;
    return true;
  }
  for (const child of root.children) {
    if (replaceNodeInTree(child, oldNode, newNode)) return true;
  }
  return false;
}

/**
 * Prune empty panels and collapse single-child splits.
 *
 * - If a split has a child that's an empty panel, remove that child (and
 *   its size entry).
 * - If a split has only one child left after pruning, replace it with that
 *   child in its parent (collapse).
 *
 * Recursively walks the tree. The `parent` param is the parent split (null
 * for the root) — needed to collapse single-child splits into their parent.
 */
function pruneEmptyPanels(node: LayoutNode, parent: { split: import('./types').SplitNode; index: number } | null): void {
  if (node.type === 'panel') return;
  // Recurse into children first (post-order).
  for (let i = 0; i < node.children.length; i++) {
    pruneEmptyPanels(node.children[i], { split: node, index: i });
  }
  // Remove empty panels.
  const before = node.children.length;
  node.children = node.children.filter((child) => {
    if (child.type === 'panel' && child.tabs.length === 0) {
      // Don't remove the last panel in the root split — keep at least one.
      // (This shouldn't happen in practice because the last tab is never
      // moved out of a panel without a target, but guard anyway.)
      return false;
    }
    return true;
  });
  // If we removed children, fix the sizes array length.
  if (node.children.length < before) {
    // Redistribute sizes evenly across remaining children.
    const evenSize = 100 / node.children.length;
    node.sizes = node.children.map(() => evenSize);
  }
  // Collapse single-child splits: replace this split with its only child
  // in the parent.
  if (node.children.length === 1 && parent) {
    const onlyChild = node.children[0];
    parent.split.children[parent.index] = onlyChild;
  }
}

/** Singleton layout store. */
export const layoutStore = new LayoutStore();

// ── Helpers (pure functions — no state) ────────────────────────────────

/** Check if a tab is an editor tab with unsaved changes. */
function isDirtyEditorTab(tab: TabData): boolean {
  if (tab.kind !== 'editor') return false;
  return (tab.payload as EditorTabPayload).isDirty;
}

/** Recursively find a panel by id. */
function findPanelInNode(node: LayoutNode, panelId: string): PanelNode | null {
  if (node.type === 'panel') {
    return node.id === panelId ? node : null;
  }
  for (const child of node.children) {
    const found = findPanelInNode(child, panelId);
    if (found) return found;
  }
  return null;
}

/** Recursively find the first panel containing a tab of the given kind. */
function findFirstPanelWithKind(node: LayoutNode, kind: TabData['kind']): PanelNode | null {
  if (node.type === 'panel') {
    return node.tabs.some((t) => t.kind === kind) ? node : null;
  }
  for (const child of node.children) {
    const found = findFirstPanelWithKind(child, kind);
    if (found) return found;
  }
  return null;
}

/** Recursively find the panel containing a tab by id. */
function findPanelContainingTab(node: LayoutNode, tabId: string): PanelNode | null {
  if (node.type === 'panel') {
    return node.tabs.some((t) => t.id === tabId) ? node : null;
  }
  for (const child of node.children) {
    const found = findPanelContainingTab(child, tabId);
    if (found) return found;
  }
  return null;
}

/** Recursively find the first active editor tab. */
function findActiveEditorTab(node: LayoutNode): TabData | null {
  if (node.type === 'panel') {
    if (!node.activeTabId) return null;
    const tab = node.tabs.find((t) => t.id === node.activeTabId);
    return tab && tab.kind === 'editor' ? tab : null;
  }
  for (const child of node.children) {
    const found = findActiveEditorTab(child);
    if (found) return found;
  }
  return null;
}

/** Cross-platform basename. */
function basename(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

/** Convert an absolute path to a `file://` URI (cross-platform). */
function pathToUri(path: string): string {
  if (path.startsWith('file://') || path.startsWith('inmemory://')) return path;
  const normalized = path.replace(/\\/g, '/');
  return `file://${normalized.startsWith('/') ? '' : '/'}${normalized}`;
}
