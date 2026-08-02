# File Explorer

**Status:** Phase 0 complete. All P0 and P1 features implemented and tested on Windows.

## What was built

A VS Code-style file explorer for the Knot IDE, running as a Svelte 5 component inside the Tauri webview. It talks to a Rust backend (`fs_ops.rs`, `watcher.rs`) via Tauri commands and events — no direct filesystem access from the frontend.

### Component structure

```
app/src/lib/filebrowser/
├── FileBrowser.svelte   # Orchestrator: tree state, editing, clipboard, DnD, keyboard nav, watcher
├── TreeRow.svelte        # Single row: icon, name, inline-edit input, drag handlers, indent guides
├── ContextMenu.svelte    # Right-click menu with viewport-edge clamping
├── icons.ts              # Extension → emoji icon mapping
└── types.ts              # TreeNode, EditState, Clipboard, FsChangedEvent
```

### Backend (Tauri commands)

| Command | Purpose |
|---|---|
| `list_dir(path)` | Non-recursive directory listing; dirs first, then files, alphabetical |
| `create_file(path)` | Create empty file; fails if exists |
| `create_dir(path)` | Create single directory; fails if exists |
| `create_dir_all(path)` | Create directory + all intermediate parents (for path-with-slashes) |
| `rename_path(old, new)` | Rename/move; fails if dest exists |
| `delete_path(path)` | Move to OS trash (cross-platform via `trash` crate) |
| `copy_file(src, dest)` | Copy file; appends `-copy` on name collision |
| `set_workspace_root(path)` | Set the root for path validation (all ops reject paths outside root) |
| `watch_workspace(rootPath)` | Start recursive FS watcher; emits `fs-changed` events |
| `stop_watching()` | Stop the watcher |

All commands validate that paths are inside the workspace root (canonicalized comparison). Dotfiles and `node_modules`/`target`/`dist`/`build` are hidden.

### Features implemented

**Tree display**
- Lazy-loading recursive tree (children fetched on expand)
- Flat-list rendering with stable node IDs (path-based) — preserves expansion/selection across refreshes
- Directories first, then files, alphabetical (case-insensitive)
- Indent guide lines (CSS `repeating-linear-gradient`, one line per depth level)
- File-type icons (emoji by extension)
- Active file highlight

**File operations**
- Open file on click (single-click opens; no preview tabs in Phase 0)
- New File / New Folder via inline input (no modal dialog)
- Rename via inline input (F2 or double-click)
- Delete with confirm dialog (moves to trash, not permanent)
- Copy Path / Copy Relative Path
- Cut / Copy / Paste clipboard (Ctrl+X / Ctrl+C / Ctrl+V)
- Path-with-slashes in New File/Folder: typing `sub1/sub2/file.twee` creates intermediate dirs + the file

**Drag and drop**
- Move (default) and copy (Ctrl held)
- Reject self-drop, cycle (folder into its own child), same-parent no-op
- File→parent bubble: dropping on a file targets its parent directory
- Auto-expand folder on hover (500ms delay)

**Keyboard navigation**
- ↑↓ navigate, → expand / move to first child, ← collapse / move to parent
- Enter open file / toggle folder
- F2 rename, Delete trash, Escape cancel edit/cut
- Ctrl+X / Ctrl+C / Ctrl+V cut/copy/paste

**Context menu**
- Right-click on file: Open, Cut, Copy, Paste, Copy Path, Copy Relative Path, Rename, Delete
- Right-click on folder: New File, New Folder, Cut, Copy, Paste, Copy Path, Rename, Delete
- Right-click on empty space: New File, New Folder, Paste, Refresh
- Viewport-edge clamping (menu shifts to stay visible near window bounds)

**Auto-reveal**
- When a file opens in the editor, the tree expands ancestor directories and scrolls to the node

**Auto-refresh (FS watcher)**
- `notify-debouncer-full` watches the workspace recursively
- Emits kind-aware events: `create` / `remove` / `rename` / `modify`
- Frontend refreshes the parent directory on `create`/`remove`/`rename`; skips `modify` (file content changes don't affect tree structure)
- Rename events include `oldPath` so both old and new parents refresh
- Expand state preserved across refreshes via `mergeChildren()` (matches by path, copies `expanded`/`loaded`/`children`)

### Resolved issues (Phase 0 testing)

| # | Issue | Fix |
|---|---|---|
| 1 | Context menu showed empty-space items on file right-click | `stopPropagation()` in TreeRow's `oncontextmenu` |
| 2 | New file at workspace root blocked (root not in `rootChildren`) | Special-case `parentId === folder` in `visibleNodes` |
| 3 | Watcher auto-refresh broken on Windows (path separator mismatch) | `parentDir()` preserves OS-native separators instead of normalizing to `/` |
| 4 | Drag showed "no-drop" cursor over indent area | Consolidated TreeRow into single element with all handlers on root div |
| 5 | Context menu clipped by window bounds | `$effect` in ContextMenu clamps position to viewport |
| 6 | HTML5 drag-and-drop not working at all | Set `dragDropEnabled: false` in `tauri.conf.json` (Tauri v2 intercepts DnD by default) |
| 7 | Tree collapsed on file delete | `mergeChildren()` preserves expand state across refreshes |
| 8 | Rows didn't stretch to full width | `box-sizing: border-box` on row; moved `overflow`/`text-overflow` to `.name` |

## What's not done (deferred to later phases)

### P2 — Nice to have (not Phase 0 critical)

- **Multi-select** (Ctrl+click, Shift+click) — bulk delete/rename
- **Type-to-search / fuzzy filter** — type to jump to matching files
- **Compact folders** — collapse single-child directory chains (VS Code "explorer.compactFolders")
- **Undo stack for file operations** — undo delete/rename/create
- **File nesting** — group related files (e.g. `file.twee` + `file.css` + `file.js`)
- **Drag out of explorer to OS** — drag a file from the tree to Windows Explorer

### Phase 1 items (not file-explorer-specific, but related)

- **Expand-state persistence** — save expanded folders to `.knot/window-state.json`, restore on launch
- **Scroll position persistence** — restore explorer scroll position on launch
- **Selected file persistence** — restore last selected file on launch
- **OS file drag-in** — drag files from Windows Explorer into the asset manager (requires re-enabling `dragDropEnabled` or using Tauri's DnD API; currently disabled for HTML5 DnD to work inside the tree)
- **Native context menu** — replace the HTML context menu with Tauri's native menu API (current HTML menu works but isn't OS-native)

## Architecture notes

### Why custom Tauri commands instead of `tauri-plugin-fs`?

`tauri-plugin-fs` is fine for simple reads, but file mutations (rename, delete, create) benefit from backend-side validation (path safety, conflict detection) and LSP notification. The backend emits `fs-changed` events after mutations so the tree auto-refreshes without a manual `refresh()`. Trash (not permanent delete) requires the `trash` crate — easier from Rust than JS.

### Why `notify-debouncer-full` instead of `notify-debouncer-mini`?

The mini debouncer deliberately discards `EventKind` (its `DebouncedEventKind` only has `Any`/`AnyContinuous`). The full debouncer preserves `EventKind` and pairs rename from/to events, which the file browser needs for kind-aware refresh (skip `modify`, refresh both parents on `rename`).

### Why `dragDropEnabled: false`?

Tauri v2's `dragDropEnabled` defaults to `true`, which intercepts drag events at the window level for OS file drops. This prevents HTML5 `dragstart`/`dragover`/`drop` from firing inside the webview. Setting it to `false` restores HTML5 DnD for the file tree. Trade-off: OS file drops are disabled until Phase 5 (Asset Manager), where we'll use Tauri's DnD API or a file picker dialog instead.
