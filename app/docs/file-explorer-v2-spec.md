# File Explorer v2 — Implementation Spec

Based on VS Code's Explorer architecture, scoped to a Twine IDE.

## What changes from current implementation

| Area | Current (broken) | Target (VS Code-style) |
|---|---|---|
| Tree rendering | Flat list, deep-mutation reactivity issues | Flat list with stable node IDs (preserves expand/selection across refresh) |
| New File/Folder/Rename | Modal dialog, closures in $state | **Inline input box** in the tree row (no modal) |
| Refresh | Full rootChildren reload (collapses everything) | Targeted: reload only the affected parent dir |
| Selection | Single, implicit | Explicit `selectedNode` + `focusedNode`, click-to-select |
| Auto-reveal | None | When a file opens in the editor, expand ancestors + scroll to it |
| Auto-refresh | Manual refresh only | Tauri FS watcher → targeted parent refresh |
| Drag and drop | Basic move only | Move + copy (Ctrl), reject self/cycle, file→parent bubble, auto-expand on hover |
| Keyboard | None | Arrow keys, Enter, F2, Delete, Ctrl+X/C/V, Escape |
| Context menu | Has most items | Add Cut/Copy/Paste, validate item visibility per node type |
| File creation target | `getTargetDir(null)` falls back to root | Uses focused/selected node (folder→inside, file→parent, none→root) |

## Architecture

### State model (Svelte 5 stores, not $state-with-closures)

```ts
// All state is plain data — no function references.
interface TreeNode {
  id: string;              // = path (stable identity for expansion/selection preservation)
  path: string;
  name: string;
  isDirectory: boolean;
  children: TreeNode[];
  expanded: boolean;
  loaded: boolean;         // children have been fetched
  loading: boolean;        // children being fetched
  depth: number;
}

// Editing state — inline input box
type EditState =
  | { type: 'new-file'; parentPath: string; parentId: string }
  | { type: 'new-folder'; parentPath: string; parentId: string }
  | { type: 'rename'; node: TreeNode };

// Clipboard
type Clipboard =
  | { operation: 'copy' | 'cut'; paths: string[] }
  | null;
```

### Backend (Tauri commands)

Already implemented in `fs_ops.rs`:
- `list_dir(path)` → `FileEntry[]`
- `create_file(path)`, `create_dir(path)`
- `rename_path(oldPath, newPath)` (also used for move)
- `delete_path(path)` (trash)
- `copy_file(src, dest)`
- `set_workspace_root(path)`

**New backend command needed:**
- `watch_workspace(rootPath)` — starts a recursive file watcher, emits `fs-changed` events with `{ kind: 'create'|'delete'|'rename', path, oldPath? }`

### Component structure

```
app/src/lib/filebrowser/
├── FileBrowser.svelte      # Main: tree, toolbar, context menu, editing state
├── TreeRow.svelte          # Single row: icon, name, inline input when editing
├── ContextMenu.svelte      # (existing, reused)
├── icons.ts                # (existing, reused)
└── types.ts                # TreeNode, EditState, Clipboard types
```

## Feature list (priority order)

### P0 — Must have for functional completeness

1. **Lazy loading** — children fetched on expand (✅ already works)
2. **Inline New File / New Folder / Rename** — replace modal dialogs with an inline `<input>` in the tree row. This is the biggest UX fix.
3. **Extension-aware selection** — when renaming `file.twee`, select just `file`, not `.twee`. F2 cycles: name → whole → extension.
4. **Targeted refresh** — after create/delete/rename, reload only the parent dir (✅ already works, keep it)
5. **Selection state** — explicit `selectedNode` for context-aware operations (✅ already works, keep it)
6. **Context menu** — full item set, validated per node type:
   - File: Open, Cut, Copy, Paste (if clipboard), Copy Path, Copy Relative Path, Rename, Delete
   - Folder: New File, New Folder, Cut, Copy, Paste, Copy Path, Rename, Delete
   - Empty: New File, New Folder, Paste, Refresh
7. **Cut/Copy/Paste** — clipboard state in the browser, paste calls `copy_file` or `rename_path` to the target dir
8. **Keyboard navigation** — ↑↓ navigate, → expand, ← collapse, Enter open, F2 rename, Delete trash, Ctrl+X/C/V cut/copy/paste, Escape cancel edit/cut
9. **Drag and drop** — move (default), copy (Ctrl held), reject self/cycle/same-parent, file target bubbles to parent, auto-expand folder on hover
10. **Auto-reveal** — when `currentFile` changes (file opened in editor), expand ancestor chain + scroll to the node

### P1 — Should have

11. **Indent guides** — vertical lines at each nesting depth
12. **Auto-refresh via FS watcher** — Tauri backend watches the workspace, emits events, frontend does targeted refresh
13. **Path-with-slashes in New File** — typing `subfolder/file.twee` creates intermediate dirs + the file
14. **Active file highlight** — the node matching the open editor file gets distinct styling (✅ already have `selected` class, keep it)
15. **Double-click to open** — single click selects (preview), double-click pins (✅ for our case: single click opens, since we don't have preview tabs yet)

### P2 — Nice to have (defer)

- Multi-select (Ctrl+click, Shift+click)
- Type-to-search / fuzzy filter
- Compact folders
- Undo stack for file operations
- File nesting
- Drag out of explorer to OS

## Inline editing UX (the key change)

When the user triggers New File / New Folder / Rename:

1. A **temporary placeholder node** is inserted into the tree (for New File/Folder) or the existing node enters edit mode (for Rename)
2. The row renders an `<input>` instead of the name label
3. The input is auto-focused, with selection set to the filename (not extension)
4. **Enter** confirms → calls the backend → targeted refresh
5. **Escape** cancels → removes placeholder / exits edit mode
6. **Blur** confirms (with validation)
7. **Live validation**: empty name, name collision, invalid characters → red error text below the input

```
Before:                    During edit:
📁 src                     📁 src
  📄 start.twee              📄 [start.twee____]  ← inline input
  📄 forest.twee             📄 forest.twee
```

## Implementation plan

1. Add `watch_workspace` Tauri command (backend, `fs_ops.rs` or new `watcher.rs`)
2. Rewrite `FileBrowser.svelte`:
   - Add `editState` (discriminated union, no closures)
   - Add `clipboard` state
   - Add `focusedId` for keyboard nav
   - Inline input rendering in the `{#each}` loop
   - Keyboard event handlers on the tree container
   - Drag-and-drop with copy support
3. Create `TreeRow.svelte` — extracted for clarity (single row with edit mode)
4. Wire auto-reveal: when `currentFile` prop changes, find the node, expand ancestors, scroll into view
5. Wire FS watcher: listen for `fs-changed` events → targeted `refreshDir`
