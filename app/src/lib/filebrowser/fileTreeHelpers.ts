/**
 * Pure tree helpers for the file browser.
 *
 * Extracted from `FileBrowser.svelte` (PLAN.md §13.9 cleanup) to bring it
 * under the 800-line limit. These functions don't touch component state,
 * Svelte runes, or Tauri — they're pure transformations on `TreeNode[]`.
 *
 * ## Why pure (CONVENTIONS §2.3)
 *
 * Tree-walking logic (find, flatten, merge, path resolution) is reusable
 * across the file browser's many operations: refresh, auto-reveal, drag-drop
 * validation, expand-state restore. Keeping it pure means:
 * - Testable without mounting a Svelte component
 * - No hidden state or side effects
 * - Clear inputs/outputs (tree in, result out)
 */

import type { FileEntry, TreeNode } from './types';

/**
 * Extract the parent directory of a path. Falls back to `folder` (the
 * workspace root) if the path has no separators.
 *
 * Does NOT normalize separators — the result must match OS-native paths
 * returned by the backend (`list_dir`, watcher events) for comparisons
 * like `dirPath === folder` and `findNode(...)` to work on Windows.
 */
export function parentDir(path: string, folder: string): string {
  const lastSep = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  if (lastSep < 0) return folder;
  return path.substring(0, lastSep) || folder;
}

/** Create a `TreeNode` from a backend `FileEntry` at the given depth. */
export function makeNode(entry: FileEntry, depth: number): TreeNode {
  return {
    id: entry.path,
    path: entry.path,
    name: entry.name,
    isDirectory: entry.isDirectory,
    isFile: entry.isFile,
    children: [],
    expanded: false,
    loaded: false,
    loading: false,
    depth,
  };
}

/**
 * Merge fresh children with existing ones, preserving expand state.
 *
 * When the tree refreshes (after create/delete/rename/watcher event), we
 * fetch a new set of child nodes from the backend. Naively replacing the
 * array would collapse every expanded directory. Instead, for each new
 * child that matches an existing child by path, we copy over `expanded`,
 * `loaded`, `loading`, and `children` — so the subtree stays intact.
 * New nodes (no match) start collapsed; deleted nodes (no new entry)
 * simply don't appear in the result.
 */
export function mergeChildren(oldChildren: TreeNode[], newChildren: TreeNode[]): TreeNode[] {
  return newChildren.map((newChild) => {
    const oldChild = oldChildren.find((c) => c.path === newChild.path);
    if (oldChild && oldChild.isDirectory) {
      newChild.expanded = oldChild.expanded;
      newChild.loaded = oldChild.loaded;
      newChild.loading = oldChild.loading;
      newChild.children = oldChild.children;
    }
    return newChild;
  });
}

/** Recursively find a node by path. Returns `null` if not found. */
export function findNode(nodes: TreeNode[], path: string): TreeNode | null {
  for (const node of nodes) {
    if (node.path === path) return node;
    if (node.isDirectory && node.children.length > 0) {
      const found = findNode(node.children, path);
      if (found) return found;
    }
  }
  return null;
}

/** Flatten the tree into a visible list (depth-first, respecting `expanded`). */
export function flatten(nodes: TreeNode[], result: TreeNode[] = []): TreeNode[] {
  for (const node of nodes) {
    result.push(node);
    if (node.isDirectory && node.expanded && node.children.length > 0) {
      flatten(node.children, result);
    }
  }
  return result;
}

/**
 * Walk the tree and return absolute paths of all expanded directories.
 * Used to persist expand state to `.knot/window-state.json`.
 */
export function collectExpandedPaths(nodes: TreeNode[]): string[] {
  const result: string[] = [];
  function walk(list: TreeNode[]): void {
    for (const node of list) {
      if (node.isDirectory && node.expanded) {
        result.push(node.path);
      }
      if (node.isDirectory && node.children.length > 0) {
        walk(node.children);
      }
    }
  }
  walk(nodes);
  return result;
}

/**
 * Get the target directory for an operation given a node (or null).
 * - Directory node → its own path
 * - File node → its parent directory
 * - Null → `folder` (workspace root)
 */
export function getTargetDir(node: TreeNode | null, folder: string): string {
  if (!node) return folder;
  if (node.isDirectory) return node.path;
  // Parent = path minus the trailing name component.
  const parent = node.path.substring(0, node.path.length - node.name.length);
  return parent.replace(/[/\\]+$/, '');
}

/**
 * Validate a name typed in the inline edit input.
 *
 * Accepts paths with slashes (e.g. `subfolder/deep/file.twee`) for the
 * new-file and new-folder cases — intermediate directories are created
 * automatically. Rejects:
 * - empty names
 * - absolute paths (leading `/` or `C:\`)
 * - parent traversal (`..` segments)
 * - backslashes in the path (typed `\` is normalized to `/` so the user
 *   can type either separator on Windows)
 *
 * Returns the cleaned name, or throws with a user-facing error message.
 */
export function validateEditName(rawName: string): string {
  const name = rawName.trim().replace(/\\/g, '/');
  if (!name) throw new Error('Name cannot be empty');
  if (name.startsWith('/')) throw new Error('Absolute paths are not allowed');
  // Reject `C:\` style absolute paths (drive letter + colon).
  if (/^[a-zA-Z]:/.test(name)) throw new Error('Absolute paths are not allowed');
  const segments = name.split('/');
  for (const seg of segments) {
    if (seg === '..') throw new Error('Parent traversal (..) is not allowed');
  }
  return name;
}

/** Check if a node is an ancestor of another node (for cycle detection in DnD). */
export function isAncestor(ancestor: TreeNode, descendant: TreeNode): boolean {
  return descendant.path.startsWith(ancestor.path);
}

/**
 * Extract the name (last path component) from an absolute path.
 * Cross-platform — handles both `/` and `\`.
 */
export function basename(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}
