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
