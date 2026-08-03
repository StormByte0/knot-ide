/**
 * Context menu item builder for the file browser.
 *
 * Pure function — takes a `TreeNode | null` and returns the menu entries
 * appropriate for that context. Extracted from `FileBrowser.svelte`
 * (PLAN.md §13.9 cleanup) to reduce the component's size + centralize the
 * menu structure (easier to audit + modify).
 *
 * ## Menu structure
 *
 * - File node: Open, Cut, Copy, [Paste], Copy Path, Copy Relative Path, Rename, Delete
 * - Directory node: New File, New Folder, Cut, Copy, [Paste], Copy Path, Rename, Delete
 * - Empty space: New File, New Folder, [Paste], Refresh
 *
 * `[Paste]` only appears when the clipboard has something to paste.
 */

import type { MenuEntry } from './ContextMenu.svelte';
import type { Clipboard, TreeNode } from './types';

/**
 * Build the context menu entries for a given node (or null for empty space).
 *
 * @param node The node under the cursor, or `null` if right-clicking empty space.
 * @param clipboard The current clipboard state. `null` = empty (no Paste item).
 * @returns Ordered list of menu entries (items + separators).
 */
export function buildContextMenuItems(node: TreeNode | null, clipboard: Clipboard): MenuEntry[] {
  const items: MenuEntry[] = [];

  if (node && !node.isDirectory) {
    // File context menu
    items.push({ id: 'open', label: 'Open', icon: '📂' });
    items.push({ separator: true });
    items.push({ id: 'cut', label: 'Cut', icon: '✂' });
    items.push({ id: 'copy', label: 'Copy', icon: '📋' });
    if (clipboard) items.push({ id: 'paste', label: 'Paste', icon: '📥' });
    items.push({ separator: true });
    items.push({ id: 'copy-path', label: 'Copy Path', icon: '🔗' });
    items.push({ id: 'copy-relative-path', label: 'Copy Relative Path', icon: '📎' });
    items.push({ separator: true });
    items.push({ id: 'rename', label: 'Rename…', icon: '✏' });
    items.push({ id: 'delete', label: 'Delete…', icon: '🗑', danger: true });
  } else if (node && node.isDirectory) {
    // Directory context menu
    items.push({ id: 'new-file', label: 'New File…', icon: '📄' });
    items.push({ id: 'new-folder', label: 'New Folder…', icon: '📁' });
    items.push({ separator: true });
    items.push({ id: 'cut', label: 'Cut', icon: '✂' });
    items.push({ id: 'copy', label: 'Copy', icon: '📋' });
    if (clipboard) items.push({ id: 'paste', label: 'Paste', icon: '📥' });
    items.push({ separator: true });
    items.push({ id: 'copy-path', label: 'Copy Path', icon: '🔗' });
    items.push({ separator: true });
    items.push({ id: 'rename', label: 'Rename…', icon: '✏' });
    items.push({ id: 'delete', label: 'Delete…', icon: '🗑', danger: true });
  } else {
    // Empty space context menu
    items.push({ id: 'new-file', label: 'New File…', icon: '📄' });
    items.push({ id: 'new-folder', label: 'New Folder…', icon: '📁' });
    if (clipboard) {
      items.push({ separator: true });
      items.push({ id: 'paste', label: 'Paste', icon: '📥' });
    }
    items.push({ separator: true });
    items.push({ id: 'refresh', label: 'Refresh', icon: '↻' });
  }

  return items;
}
