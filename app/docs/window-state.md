# Window State — `.knot/window-state.json`

**Status:** Implemented (Phase 1 Task 8). Persists the custom pane-layout layer across app restarts.

## What it does

When the user closes Knot and reopens the same workspace, the layout is restored exactly as they left it: which panels exist, their tab order, split sizes, active tab per panel, and which folders are expanded in the file browser.

This is the **custom pane-layout layer** mentioned in `PLAN.md §6.1`. OS-level window geometry (window size, position, maximized state) is handled separately by `tauri-plugin-window-state` and stored in `<appData>/.window-state` (Tauri internal). This file is the in-app layout tree only.

## File location

`<workspace_root>/.knot/window-state.json`

Co-located with `<workspace_root>/.knot/config.json`. The whole `.knot/` directory is IDE-local state and should be gitignored by the project (not committed to version control).

## Schema

```json
{
  "version": 1,
  "workspaceFolder": "C:/projects/my-game",
  "layout": {
    "type": "split",
    "direction": "horizontal",
    "children": [
      {
        "type": "panel",
        "id": "sidebar",
        "tabs": [
          {
            "id": "files",
            "kind": "filebrowser",
            "title": "Files",
            "payload": {
              "folder": "C:/projects/my-game",
              "expandedPaths": [
                "C:/projects/my-game/src",
                "C:/projects/my-game/assets"
              ]
            }
          }
        ],
        "activeTabId": "files"
      },
      {
        "type": "panel",
        "id": "editor",
        "tabs": [
          {
            "id": "C:/projects/my-game/src/intro.twee",
            "kind": "editor",
            "title": "intro.twee",
            "payload": {
              "path": "C:/projects/my-game/src/intro.twee",
              "uri": "file:///C:/projects/my-game/src/intro.twee",
              "languageId": "twee",
              "content": ":: StoryTitle\nMy Game\n",
              "isDirty": false
            }
          }
        ],
        "activeTabId": "C:/projects/my-game/src/intro.twee"
      }
    ],
    "sizes": [20, 80]
  }
}
```

### Fields

| Field | Type | Description |
|---|---|---|
| `version` | `number` | Schema version. Currently `1`. If the schema changes in a backwards-incompatible way, this is bumped and a migrator is added. |
| `workspaceFolder` | `string` | The workspace root the layout belongs to. Used as a defensive check — if the file is somehow at the wrong workspace's path, the load is rejected. |
| `layout` | `LayoutNode` | The layout tree root. See `app/src/lib/layout/types.ts` for the full type definition. |

### LayoutNode

Discriminated union:

- **`split`** — container that divides space between children.
  - `direction`: `'horizontal'` (side-by-side) | `'vertical'` (stacked)
  - `children`: `LayoutNode[]`
  - `sizes`: `number[]` — flex-grow ratios, one per child. Conventionally sum to ~100.
- **`panel`** — leaf node containing a tab group.
  - `id`: `string` — stable panel id (e.g. `'sidebar'`, `'editor'`).
  - `tabs`: `TabData[]`
  - `activeTabId`: `string | null`

### TabData

- `id`: `string` — unique within the layout.
- `kind`: `'editor' | 'filebrowser' | 'storymap' | 'build' | 'settings'`
- `title`: `string` — tab label.
- `payload`: kind-specific. See `types.ts` for each payload interface.

#### Editor tab payload

| Field | Type | Description |
|---|---|---|
| `path` | `string` | Absolute file path (same as tab id). |
| `uri` | `string` | `file://` URI for Monaco model lookup. |
| `languageId` | `string` | Monaco language id (e.g. `'twee'`). |
| `content` | `string` | Last-known file content (includes unsaved edits). |
| `isDirty` | `boolean` | True when content differs from disk. |

#### Filebrowser tab payload

| Field | Type | Description |
|---|---|---|
| `folder` | `string` | Workspace root path. |
| `expandedPaths` | `string[]` (optional) | Absolute paths of directories to start expanded. Absent on a fresh workspace (everything starts collapsed). |

## Persistence lifecycle

### Save

- **Trigger:** any structural layout change (tab open/close, panel drag/split/resize, expand/collapse in file browser).
- **Mechanism:** `App.svelte` has a `$effect` that watches structural fields of `layoutStore.root`. On change, a 500ms debounced save is scheduled.
- **On window close:** `beforeunload` handler flushes any pending save immediately (cancels the debounce timer + writes synchronously).
- **Excluded from save trigger:** editor-tab `content` and `isDirty` changes (those fire on every keystroke; persisting on every keystroke would be wasteful). Content is included in the saved JSON, but only written to disk on structural changes or window close.

### Load

- **Trigger:** workspace open (after the user picks a folder).
- **Mechanism:** `layoutStore.loadSavedState(workspaceFolder)` calls the Rust `load_window_state` command, which returns the raw JSON. The frontend `deserializeLayout` validates the shape, version, and workspace match, then hydrates the store.
- **Fallback:** if the file doesn't exist (first open), is corrupt, or fails validation, the default layout is used (filebrowser | editor split, filebrowser on the left at 20%).
- **Path validation:** the backend `load_window_state` command validates the workspace root against the app's tracked root (via the shared `workspace.rs` helper). The frontend also checks `workspaceFolder` matches — defensive double-check.

### Restore behavior

- **Layout tree:** restored verbatim. Panel ids, tab order, split sizes, active tab per panel.
- **Editor tabs:** restored with cached content. The Monaco model is created from the cached content; the LSP `didOpen` is sent when the Editor component mounts. The cached content may be stale if the file changed on disk while the app was closed — a future phase can add an async disk re-read on restore.
- **File browser expanded folders:** restored by walking the saved `expandedPaths` array (sorted shallowest-first so ancestors expand before descendants). Paths that no longer exist are silently skipped.

## Versioning & migration

The `version` field gates load. The current schema is `1`.

If a future Phase introduces a v2 schema:

1. Bump `WINDOW_STATE_VERSION` in `app/src/lib/layout/windowState.ts`.
2. Add a `migrateV1toV2(old: WindowStateV1): WindowStateV2` function.
3. In `deserializeLayout`, branch on the parsed version: v1 → migrate → return v2; v2 → return as-is; unknown → reject.
4. Old v1 files remain loadable until the migrator is removed (which should be never, or only after a major version bump with explicit user communication).

## Backend commands

| Command | Purpose |
|---|---|
| `load_window_state(workspaceRoot)` | Returns `Option<String>` — `None` if no file, `Some(json)` if it exists. Validates workspace root. |
| `save_window_state(workspaceRoot, json)` | Writes the JSON string to `.knot/window-state.json`. Creates `.knot/` if missing. Validates workspace root. |

Both commands are thin file-IO wrappers — they do not parse or validate the JSON structure. All structural validation is frontend-side (`windowState.ts`).

## What is NOT persisted (out of scope for Phase 1 Task 8)

- **OS window geometry** (size, position, maximized) — handled by `tauri-plugin-window-state`.
- **Child window state** — detached tab windows are transient; they're created on-demand and not restored on next launch.
- **Editor cursor position / scroll position** — the Editor reads cursor from Monaco on mount (always starts at 1:1). Future phase can add per-tab cursor persistence.
- **File browser scroll position** — the tree always starts scrolled to top.
- **File browser selection** — no row is selected until the user clicks one.
- **LSP server state** — the supervisor tracks open documents in-memory; the LSP `didOpen` flow runs fresh on each launch.

These may be added in later phases; track them in `ROADMAP.md` if needed.
