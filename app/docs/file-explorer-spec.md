# File Explorer — Feature Specification

**Status:** Design. Implementation follows this spec.

## 1. Purpose

The file explorer is the primary workspace navigation surface. It shows the
project's directory tree and lets the author open, create, rename, move, and
delete files and folders. It is the Tauri-app equivalent of VS Code's Explorer
panel.

## 2. Visual structure

```
┌─ Explorer ──────────────────────── ↻ ─ [+] ┐
│ 📁 src                                    │
│   📁 passages                              │
│     📄 start.twee            (active)      │
│     📄 forest.twee                         │
│     📄 castle.twee                         │
│   📁 assets                                │
│     📁 images                              │
│       🖼 hero.png                          │
│       🖼 bg.jpg                            │
│     🎵 intro.ogg                           │
│   📄 00-meta.twee                          │
│   📄 style.css                             │
│ 📄 StoryData.json                          │
│ 📄 .knot/config.json                       │
└────────────────────────────────────────────┘
```

- **Tree, not flat list.** Directories expand/collapse. Files are leaves.
- **All files shown**, not just `.twee`. Assets (images, audio, fonts), CSS,
  JS, JSON, config files — everything in the workspace.
- **File-type icons** (emoji for the spike; SVG icons in Phase 8 polish).
- **Indentation** by depth, with subtle guide lines.
- **Active file** highlighted.
- **Context menu** on right-click (per §4 below).
- **Toolbar** with refresh + new file/folder buttons.

## 3. Core features

### 3.1 Tree display

- Recursive directory tree rooted at the workspace folder.
- Directories shown first (sorted alphabetically), then files (sorted
  alphabetically).
- Directories are **collapsible** — click to toggle expand/collapse.
- Expand state persists in `.knot/window-state.json` (Phase 1; for the spike,
  in-memory only).
- **No depth limit** (remove the depth-5 cap from the current implementation).
- **Hidden files** (starting with `.`) are shown — `.knot/config.json` is a
  real file users need to see and edit.
- **Skipped directories:** `node_modules`, `target`, `.git` only. Everything
  else is shown.

### 3.2 File operations

| Operation | Trigger | Behavior |
|---|---|---|
| **Open file** | Single-click or Enter | Opens in editor. Active file highlighted. |
| **Open folder** | Click folder name | Toggles expand/collapse. Does NOT open in editor. |
| **New file** | Toolbar button or context menu → "New File" | Prompts for name. Creates in the selected directory (or workspace root if none selected). Opens in editor after creation. |
| **New folder** | Toolbar button or context menu → "New Folder" | Prompts for name. Creates in the selected directory. Expands parent. |
| **Rename** | F2, double-click name, or context menu → "Rename" | Inline edit of the name. Updates all references via LSP `workspace/willRenameFiles` + `workspace/didRenameFiles`. |
| **Delete** | Delete key or context menu → "Delete" | Confirm dialog. Moves to trash (not permanent delete). For files, notifies LSP via `workspace/didDeleteFiles`. |
| **Duplicate** | Context menu → "Duplicate" | Copies file with `-copy` suffix. Opens copy in editor. |
| **Refresh** | Toolbar ↻ button or F5 | Re-scans the workspace. Preserves expand state. |

### 3.3 Drag and drop (Phase 1 — not in spike)

- Drag file → drop on folder → moves file, updates references.
- Drag file → drop on editor → opens file.
- **Not in the spike** — deferred to Phase 1.

### 3.4 Multi-select (Phase 1 — not in spike)

- Ctrl+click for additive select.
- Shift+click for range select.
- Bulk delete/rename.
- **Not in the spike.**

## 4. Context menu

Right-click on a file or folder (or the background) shows a native-feeling
context menu:

**On a file:**
- Open
- —
- New File… (in this file's directory)
- New Folder… (in this file's directory)
- —
- Rename… (F2)
- Duplicate
- Copy Path
- —
- Delete… (Del)

**On a folder:**
- New File… (in this folder)
- New Folder… (in this folder)
- —
- Rename… (F2)
- Copy Path
- —
- Delete… (Del)

**On empty space (background):**
- New File… (in workspace root)
- New Folder… (in workspace root)
- —
 Refresh

## 5. File-type detection

Icons by extension:

| Type | Extensions | Icon |
|---|---|---|
| Twee | `.tw`, `.twee` | 📄 (blue — passage file) |
| JavaScript | `.js`, `.mjs` | 📜 |
| Stylesheet | `.css` | 🎨 |
| JSON | `.json` | ⚙ |
| Image | `.png`, `.jpg`, `.jpeg`, `.gif`, `.svg`, `.webp` | 🖼 |
| Audio | `.ogg`, `.mp3`, `.wav`, `.flac` | 🎵 |
| Font | `.ttf`, `.otf`, `.woff`, `.woff2` | 🔤 |
| Markdown | `.md`, `.txt` | 📝 |
| Config | `.knot/*` (inside `.knot/` dir) | 🔧 |
| Default | anything else | 📄 |

## 6. Keyboard shortcuts

| Key | Action |
|---|---|
| `↑` / `↓` | Navigate up/down |
| `←` | Collapse folder (if expanded) or move to parent |
| `→` | Expand folder (if collapsed) or move to first child |
| `Enter` | Open file / toggle folder |
| `F2` | Rename selected |
| `Delete` | Delete selected |
| `F5` | Refresh tree |
| `Ctrl+N` | New file (in selected dir) |
| `Ctrl+Shift+N` | New folder (in selected dir) |

## 7. Backend commands (Tauri)

The frontend calls these Tauri commands (implemented in
`app/src-tauri/src/fs_ops.rs`):

```rust
// Read directory entries (path → Vec<DirEntry> with full paths)
#[tauri::command]
async fn list_dir(path: String) -> Result<Vec<FileEntry>, String>

// Create a new file (empty) — returns the path
#[tauri::command]
async fn create_file(path: String) -> Result<String, String>

// Create a new directory — returns the path
#[tauri::command]
async fn create_dir(path: String) -> Result<String, String>

// Rename/move a file or directory — returns the new path
#[tauri::command]
async fn rename(old_path: String, new_path: String) -> Result<String, String>

// Delete a file or directory (move to trash, not permanent)
#[tauri::command]
async fn delete(path: String) -> Result<(), String>

// Copy a file — returns the new path
#[tauri::command]
async fn copy_file(src: String, dest: String) -> Result<String, String>
```

**Why custom commands instead of `tauri-plugin-fs`?**

- `tauri-plugin-fs`'s frontend API is fine for simple reads, but file
  mutations (rename, delete, create) benefit from backend-side validation
  (path safety, conflict detection, LSP notification).
- The backend can emit `workspace/file-changed` events after mutations so
  the file browser auto-refreshes without a manual `refresh()`.
- Trash (not permanent delete) requires platform-specific calls
  (`trash` crate) — easier from Rust than from JS.

## 8. LSP integration

File mutations must notify `knot-server` so it stays in sync:

| Operation | LSP notification |
|---|---|
| Create file | `workspace/didCreateFiles` |
| Rename file | `workspace/willRenameFiles` (before) + `workspace/didRenameFiles` (after) |
| Delete file | `workspace/didDeleteFiles` |

The frontend doesn't send these directly — the Rust backend sends them
after the filesystem operation succeeds. (The LanguageClient's
`workspaceFolder` middleware can also handle this, but backend-side is
simpler for the spike.)

For rename, the server can compute and apply reference rewrites
(`<<include "OldPassage">>` → `<<include "NewPassage">>`) via the
`workspace/willRenameFiles` request, which returns `WorkspaceEdit` with
text edits.

## 9. Error handling

- **Path outside workspace** — rejected with error toast. The explorer
  only operates on files inside the workspace root.
- **Name collision** — rejected with error toast ("A file with this name
  already exists").
- **Permission denied** — rejected with error toast showing the OS error.
- **File in use** (can't delete/rename) — rejected with error toast
  suggesting to close the file first.

## 10. Persistence (Phase 1)

- **Expand state:** which folders are expanded, persisted to
  `.knot/window-state.json`. Restored on app launch.
- **Scroll position:** vertical scroll of the explorer, restored on launch.
- **Selected file:** last selected file, restored on launch (if it still
  exists).

Not in the spike — in-memory only.

## 11. Spike scope (what to build now)

**In the spike:**
- Tree display with expand/collapse
- All files shown (not just `.twee`)
- File-type icons
- Open file on click
- New file / New folder (prompt-based, no inline edit yet)
- Rename (prompt-based)
- Delete (with confirm dialog)
- Refresh button
- Copy path
- Backend commands in `fs_ops.rs`
- Auto-refresh on file mutations (via backend events)

**Deferred to Phase 1:**
- Drag and drop
- Multi-select
- Inline rename (instead of prompt)
- Expand-state persistence
- Native context menu (spike uses HTML context menu)
- Keyboard navigation

## 12. Component structure

```
app/src/lib/filebrowser/
├── FileBrowser.svelte       # Main component — tree, toolbar, context menu
├── FileTree.svelte          # Recursive tree renderer (renders FileNode per entry)
├── FileNode.svelte          # Single file/folder row (icon, name, expand toggle)
├── ContextMenu.svelte       # Right-click context menu
├── PromptDialog.svelte      # Generic prompt dialog (for new file/folder/rename)
├── ConfirmDialog.svelte     # Generic confirm dialog (for delete)
├── icons.ts                 # File-type → icon mapping
└── types.ts                 # FileEntry, TreeNode, etc.
```

**Backend:**
```
app/src-tauri/src/
├── fs_ops.rs                # Tauri commands: list_dir, create_file, etc.
└── lib.rs                   # registers fs_ops commands
```
