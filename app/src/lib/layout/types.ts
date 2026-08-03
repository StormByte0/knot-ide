/**
 * Layout tree types.
 *
 * The layout is a recursive tree of splits and panels. Splits divide space
 * horizontally or vertically; panels hold tab groups. Every leaf is a panel.
 *
 * ## Invariants
 *
 * - Split `children.length === sizes.length` (one size per child).
 * - Split `sizes` are flex-grow ratios (not strict percentages, but
 *   conventionally sum to ~100 for readability).
 * - Panel `activeTabId` is either `null` (no tab active) or the id of a tab
 *   in `tabs`.
 * - Tab `id` is unique within the entire layout (used as Svelte `{#each}` key
 *   and for store lookups like `closeTab(id)`).
 *
 * ## Immutability
 *
 * Nodes are NOT immutable — the layout store mutates them in place (Svelte 5's
 * `$state` makes nested objects deeply reactive). The `type` field is the
 * only truly immutable field: a split never becomes a panel or vice versa.
 */

/** A node in the layout tree. Either a split (container) or a panel (leaf). */
export type LayoutNode = SplitNode | PanelNode;

/** A container that divides space between children in one direction. */
export interface SplitNode {
  type: 'split';
  /** Split direction. `horizontal` = children side-by-side; `vertical` = stacked. */
  direction: 'horizontal' | 'vertical';
  /** Child nodes. */
  children: LayoutNode[];
  /** Flex-grow ratios, one per child. Conventionally sum to ~100. */
  sizes: number[];
}

/** A leaf node containing a tab group. */
export interface PanelNode {
  type: 'panel';
  /** Stable panel id (e.g. `'sidebar'`, `'editor'`). Used by the store for lookups. */
  id: string;
  /** Tabs in this panel, left-to-right. */
  tabs: TabData[];
  /** Active tab id, or `null` if the panel is empty. */
  activeTabId: string | null;
}

/** Tab kind — determines which content component DockPanel renders. */
export type TabKind = 'editor' | 'filebrowser' | 'storymap' | 'build' | 'settings';

/** A tab in a panel. The payload varies by kind. */
export interface TabData {
  /** Unique id (for editor tabs: the file path; for filebrowser: `'files'`). */
  id: string;
  /** Determines the content component. */
  kind: TabKind;
  /** Display title (tab label). */
  title: string;
  /** Kind-specific data. Cast by the rendering component. See {@link EditorTabPayload}. */
  payload: unknown;
}

/** Payload for `kind: 'editor'` tabs. */
export interface EditorTabPayload {
  /** Absolute file path (same as tab id). */
  path: string;
  /** `file://` URI for Monaco model lookup. */
  uri: string;
  /** Monaco language id (e.g. `'twee'`). */
  languageId: string;
  /** Last-known file content (includes unsaved edits). */
  content: string;
  /** True when in-editor content differs from disk. */
  isDirty: boolean;
}

/** Payload for `kind: 'filebrowser'` tabs. */
export interface FileBrowserTabPayload {
  /** Workspace root path. */
  folder: string;
  /**
   * Absolute paths of directories that should start expanded when the
   * filebrowser mounts. Populated by the layout store when restoring from
   * `.knot/window-state.json`; updated by the filebrowser as the user
   * expands/collapses folders. `undefined` on a fresh workspace (no saved
   * state yet) — the filebrowser starts with everything collapsed.
   */
  expandedPaths?: string[];
}
