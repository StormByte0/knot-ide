/** Shared types for the file browser. */

export interface FileEntry {
  /** Full absolute path. */
  path: string;
  /** File or directory name (last path component). */
  name: string;
  /** True if this is a directory. */
  isDirectory: boolean;
  /** True if this is a regular file. */
  isFile: boolean;
}

export interface TreeNode extends FileEntry {
  /** Stable identity (=== path) used to preserve expansion/selection across refreshes. */
  id: string;
  /** Child nodes. Empty for files, or for directories that haven't been loaded yet. */
  children: TreeNode[];
  /** Whether this directory is expanded in the UI. */
  expanded: boolean;
  /** Whether children have been fetched from the backend. */
  loaded: boolean;
  /** True if children are currently being fetched (for loading indicator). */
  loading: boolean;
  /** Depth in the tree (0 = workspace root's children). */
  depth: number;
}

/**
 * Inline editing state — drives the `<input>` rendered in place of a row's name.
 *
 * Discriminated union so the renderer can branch on `editState.type` without
 * narrowing on optional fields. No closures stored — only plain data.
 */
export type EditState =
  | { type: 'new-file'; parentPath: string; parentId: string; tempId: string }
  | { type: 'new-folder'; parentPath: string; parentId: string; tempId: string }
  | { type: 'rename'; node: TreeNode };

/** Clipboard state for cut/copy/paste. `null` = clipboard empty. */
export type Clipboard = { operation: 'copy' | 'cut'; paths: string[] } | null;

/**
 * Payload emitted by the backend via the `fs-changed` Tauri event.
 * Produced by `watcher.rs` — see that file for the `EventKind` → `kind` mapping.
 */
export interface FsChangedEvent {
  /** `"create"` | `"remove"` | `"rename"` | `"modify"` */
  kind: string;
  /** Full path of the changed file/dir. For `"rename"`, this is the NEW path. */
  path: string;
  /** For `"rename"` only: the previous path that was renamed away. */
  oldPath?: string;
}
