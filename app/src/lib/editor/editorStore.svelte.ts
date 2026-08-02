/**
 * Open-editor-tab store.
 *
 * Single source of truth for which files are open in the editor and which tab
 * is active. Drives {@link EditorTabs.svelte} (the tab strip) and the
 * `<Editor>` component (which binds its model to the active tab's URI +
 * content).
 *
 * ## Ownership
 *
 * The store owns **tab-list state only** — the open tabs, their order, the
 * active tab id, and per-tab dirty flag. It does NOT own Monaco model
 * instances (those live in `monaco-editor`'s global model registry, looked up
 * by URI) or file-system state (the file browser owns the tree). Content is
 * cached on the tab so the editor can create/restore a model without a
 * re-read, but the store never writes to disk.
 *
 * ## Svelte 5 runes
 *
 * Uses `$state` — this file MUST be named `*.svelte.ts` so the Svelte
 * compiler processes it (see worklog phase1-task1-fix). Imports must use the
 * explicit `.svelte` extension.
 */

/** One open editor tab. */
export interface EditorTab {
  /** Stable id (=== file path). Used as the Svelte `{#each}` key. */
  id: string;
  /** Absolute file path. Same as `id` — kept as a separate field for clarity at call sites. */
  path: string;
  /** Display name (basename, e.g. `Start.twee`). */
  name: string;
  /** `file://` URI for Monaco model lookup. */
  uri: string;
  /** Monaco language id (e.g. `'twee'`). */
  languageId: string;
  /** Last-known file content. Used to create/restore the Monaco model. */
  content: string;
  /** True when the in-editor content differs from the last-saved disk content. Save lands in a later task. */
  isDirty: boolean;
}

class EditorStore {
  /** Open tabs, left-to-right. */
  tabs = $state<EditorTab[]>([]);

  /** Active tab id (=== path), or `null` when no tab is open. */
  activeTabId = $state<string | null>(null);

  /** Reactive read: the active tab object, or `null`. */
  get activeTab(): EditorTab | null {
    if (!this.activeTabId) return null;
    return this.tabs.find((t) => t.id === this.activeTabId) ?? null;
  }

  /**
   * Open a tab for `path`. If a tab for this path already exists, switch to it
   * (no duplicate). Otherwise read the file from disk, create a tab, and
   * switch to it.
   *
   * @param path Absolute file path.
   * @param content File content (caller is responsible for reading — keeps
   *   the store free of Tauri `fs` imports, respecting CONVENTIONS §2.3).
   * @param languageId Monaco language id (default `'twee'`).
   */
  openTab(path: string, content: string, languageId: string = 'twee'): void {
    const existing = this.tabs.find((t) => t.id === path);
    if (existing) {
      // Tab already open — just activate it. Don't overwrite content: the
      // Monaco model may have unsaved edits.
      this.activeTabId = existing.id;
      return;
    }
    const tab: EditorTab = {
      id: path,
      path,
      name: basename(path),
      uri: pathToUri(path),
      languageId,
      content,
      isDirty: false,
    };
    this.tabs.push(tab);
    this.activeTabId = tab.id;
  }

  /** Switch the active tab. No-op if `id` is not open. */
  switchTo(id: string): void {
    if (this.tabs.some((t) => t.id === id)) {
      this.activeTabId = id;
    }
  }

  /**
   * Close a tab. Returns `true` if the tab was closed, `false` if the close
   * was cancelled (because the tab is dirty and the caller needs to confirm).
   *
   * The store does NOT prompt — that's a UI concern. Callers check
   * {@link isDirty} before calling, or call {@link forceClose} to skip the
   * check.
   */
  closeTab(id: string): boolean {
    const idx = this.tabs.findIndex((t) => t.id === id);
    if (idx === -1) return true;
    // If dirty, refuse — caller must confirm and call forceClose.
    if (this.tabs[idx].isDirty) return false;
    this.forceClose(id);
    return true;
  }

  /** Close a tab unconditionally, even if dirty. Used after user confirms. */
  forceClose(id: string): void {
    const idx = this.tabs.findIndex((t) => t.id === id);
    if (idx === -1) return;
    this.tabs.splice(idx, 1);
    if (this.activeTabId === id) {
      // Activate the neighbor: prefer the tab now at the same index (was
      // right-neighbor), fall back to the previous tab, or null if empty.
      const next = this.tabs[idx] ?? this.tabs[idx - 1] ?? null;
      this.activeTabId = next ? next.id : null;
    }
  }

  /** Close every tab. Returns the list of dirty tab ids that were NOT closed (caller confirms). */
  closeAll(): string[] {
    const dirty = this.tabs.filter((t) => t.isDirty).map((t) => t.id);
    if (dirty.length > 0) return dirty;
    this.tabs = [];
    this.activeTabId = null;
    return [];
  }

  /** Close all tabs except `id`. Returns dirty ids that were NOT closed. */
  closeOthers(id: string): string[] {
    const dirty = this.tabs.filter((t) => t.id !== id && t.isDirty).map((t) => t.id);
    if (dirty.length > 0) return dirty;
    this.tabs = this.tabs.filter((t) => t.id === id);
    this.activeTabId = id;
    return [];
  }

  /**
   * Mark a tab's content as changed (called by `<Editor>` on
   * `onDidChangeModelContent`). Updates the cached content + sets `isDirty`.
   */
  markContentChanged(id: string, content: string): void {
    const tab = this.tabs.find((t) => t.id === id);
    if (!tab) return;
    tab.content = content;
    tab.isDirty = true;
  }

  /**
   * Mark a tab clean (e.g. after save — not wired yet, but the API is here so
   * the save task doesn't need to touch the store's internals).
   */
  markClean(id: string): void {
    const tab = this.tabs.find((t) => t.id === id);
    if (!tab) return;
    tab.isDirty = false;
  }
}

/** Singleton editor store. Imported by App.svelte, EditorTabs.svelte, Editor.svelte. */
export const editorStore = new EditorStore();

/** Cross-platform basename: `D:\projects\my-game\Start.twee` → `Start.twee`. */
function basename(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

/**
 * Convert an absolute file path to a `file://` URI for Monaco.
 * Windows: `D:\path\file.twee` → `file:///D:/path/file.twee`
 * Unix:    `/path/file.twee`    → `file:///path/file.twee`
 */
function pathToUri(path: string): string {
  if (path.startsWith('file://') || path.startsWith('inmemory://')) return path;
  const normalized = path.replace(/\\/g, '/');
  return `file://${normalized.startsWith('/') ? '' : '/'}${normalized}`;
}
