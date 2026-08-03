# Phase 1 — App Shell Plan

**Status:** In progress. Sequential task breakdown for the Phase 1 deliverable from PLAN.md §8: "Multi-window manager, movable/dockable panes, file explorer, multi-tab Monaco, native menu bar, themes, settings."

**Overarching plan:** `PLAN.md` (project root). This document tracks Phase 1 execution.

---

## 1. Goals

Transform the Phase 0 spike (single-file editor with a sidebar) into a real IDE shell that can:
- Open multiple files in tabs
- Dock and rearrange panels (editor, file tree, Story Map, Build/Run, etc.)
- Detach panels into separate windows (multi-monitor support)
- Persist layout and settings
- Apply themes
- Show meaningful status at the bottom

The graph view (Story Map v2, Phase 4) and code view are equally important — multi-window is high priority so both can be visible simultaneously on different monitors.

---

## 2. Already done (pulled forward from Phase 0)

| Item | Where |
|---|---|
| File explorer (complete) | `app/src/lib/filebrowser/` — see `file-explorer.md` |
| Native menu bar (File/Edit/View/Build/Help) | `app/src-tauri/src/menu.rs` |
| Single parent window + geometry persistence | `tauri-plugin-window-state` |
| Minimal status bar (LSP status only) | `app/src/App.svelte` footer |
| LSP crash supervisor | `app/src-tauri/src/lsp.rs` — see `phase0-supervisor.md` |

---

## 3. Architecture decisions (locked)

### 3.1 Layout manager — custom Svelte 5, not Golden Layout

**Decision:** Build a custom dock/split layout system in Svelte 5, not use Golden Layout or similar libraries.

**Rationale:**
- Golden Layout is jQuery-era, not Svelte-native, and fights Svelte 5's reactivity model
- The layout we need is simpler than full Golden Layout (no floating panels, no popouts-beyond-windows for now)
- Custom gives full control over the data model (critical for multi-window + persistence)
- Svelte 5 runes (`$state`, `$derived`, `$effect`) make a tree-based layout reactive without a library

**Scope of "mature" interactions:**
- Horizontal splits (left | right) with draggable resize handles
- Vertical splits (top / bottom) with draggable resize handles
- Tab groups (multiple tabs in one panel)
- Drag a tab to a dock zone (left/right/bottom/center of target) → creates a new split or joins existing
- Drag a tab out of the window → detaches to a new OS window
- Close panel automatically when its last tab closes
- Collapse/expand sidebar panels (click icon to toggle)
- Persist the entire layout tree to `.knot/window-state.json`

### 3.2 Multi-window — parent/child deck model (PLAN.md §6.1)

**Decision:** Implement the parent/child deck model from PLAN.md §6.1 in Phase 1. High priority — graph + code views need to be visible simultaneously.

**Implications:**
- Each Tauri window is a separate JS context → Monaco `initialize()` runs per window (acceptable per §6.2, ~50-100ms + ~30MB per detached editor)
- The Rust backend is the authoritative state owner; windows query/mutate via `invoke` (not peer-to-peer messaging)
- Inter-window comms via Tauri `emitTo(label, event, payload)` (targeted) and `emit(event, payload)` (broadcast)
- Child windows are owned by the parent; closing parent closes all children
- Detached non-editor decks (Story Map, Build/Run) don't need Monaco — no init cost

### 3.3 Settings — two-tier: editor-level + project-level

**Decision:** Separate settings into two tiers with clear ownership boundaries.

**Editor-level settings** (global, per-user):
- Stored in `<appData>/settings.json` (via `tauri-plugin-store`)
- Font family, font size, tab size, word wrap, minimap, bracket pair colorization
- Theme (light/dark/custom)
- Tweego executable path (detected on first launch, overridable)
- Window layout presets (saved pane arrangements)
- Keybindings (future)

**Project-level settings** (per-workspace):
- Stored in `.knot/config.json` at the workspace root
- Story format (SugarCube / Harlowe / Chapbook / Snowman)
- Build configuration (output format, Tweego flags, output directory)
- Include/exclude patterns (which files Tweego bundles)
- Story Map layout preference (hierarchical / force-directed / manual)
- Asset manager settings (managed name convention)

**Migration:** Auto-migrate `.vscode/knot.json` → `.knot/config.json` on first open, write `.vscode/knot.json.bak` backup (PLAN.md §12, approved).

### 3.4 No system tray

**Decision:** No system tray icon. The native menu bar + status bar cover all needed entry points. Closing the last window quits the app (with confirm).

### 3.5 Status bar — expand the existing minimal one

**Decision:** Keep and expand the existing status bar at the bottom of every window. Per PLAN.md §6.1, it shows:
- Project name (workspace root basename)
- `knot-server` health (idle / starting / ready / restarting / failed)
- Tweego version (or "not configured")
- Build status (idle / building / success / failed)
- Active file path + cursor position (line:col)
- Language mode (Twee / SugarCube / etc.)
- Update indicator (when `tauri-plugin-updater` is wired)

---

## 4. Component structure (target)

```
app/src/lib/
├── layout/                   # NEW — dock/split layout system
│   ├── types.ts              # LayoutNode, SplitDirection, DockZone, TabData
│   ├── LayoutRoot.svelte     # Root: owns the layout tree state, renders top-level splits
│   ├── SplitView.svelte      # Horizontal or vertical split with resize handle
│   ├── DockPanel.svelte      # A panel containing a tab group + content area
│   ├── TabStrip.svelte       # Tab bar for a DockPanel (drag source + context menu)
│   ├── ResizeHandle.svelte   # Draggable divider between splits
│   ├── DropOverlay.svelte    # Visual overlay showing dock zones during drag
│   ├── layoutStore.svelte.ts # Layout tree state + operations + persistence (Task 8)
│   ├── dragStore.svelte.ts   # Active drag session state
│   └── windowState.ts        # Serialize/deserialize .knot/window-state.json (Task 8)
├── windows/                  # NEW — multi-window manager
│   ├── windowManager.ts      # Create/focus/close child windows, track ownership
│   └── WindowHost.svelte     # Root component for child windows (owns a LayoutRoot)
│   # Note: `windowState.ts` was originally planned under windows/ but landed
│   # under layout/ (it serializes the layout tree, not window metadata).
├── statusbar/                # NEW — status bar components
│   ├── StatusBar.svelte      # Main status bar (assembles items)
│   ├── StatusItem.svelte     # Single status bar item (label + value + click handler)
│   └── statusStore.svelte.ts # Reactive store for status bar data
├── editor/                   # MODIFIED — multi-tab support
│   ├── Editor.svelte         # (existing) single editor instance
│   ├── monaco-init.ts        # (existing)
│   └── workers.ts            # (existing)
│   # Note: Task 2 originally planned `EditorTabs.svelte` + `editorStore.ts`.
│   # These were superseded by `TabStrip.svelte` + `layoutStore.svelte.ts`
│   # in Task 3 (kind-agnostic tab strip + unified layout store). The dead
│   # files were removed during the Phase 1 audit — see PLAN.md §13.2.
├── settings/                 # NEW — settings system
│   ├── types.ts              # EditorSettings, ProjectSettings interfaces
│   ├── editorSettings.svelte.ts  # Reactive store: load/save <appData>/settings.json
│   ├── projectSettings.ts    # Load/save .knot/config.json + migration (inlined)
│   └── SettingsDialog.svelte # Settings UI (modal)
│   # Note: `migrate.ts` was originally planned as a separate file; the
│   # migration logic was inlined into `projectSettings.ts` as
│   # `migrateVscodeConfig` — see PLAN.md §13.6 for the audit note.
├── themes/                   # NEW — theme system
│   ├── themeStore.svelte.ts  # Active theme, load/save preference
│   ├── themes.ts             # Theme definitions (knot-dark, knot-light)
│   └── applyTheme.ts         # Apply CSS variables + sync Monaco theme
├── filebrowser/              # (existing, unchanged)
├── lsp/                      # (existing, unchanged)
└── App.svelte                # MODIFIED — uses LayoutRoot instead of fixed layout
```

**Backend additions:**
```
app/src-tauri/src/
├── windows.rs                # NEW — Tauri window management commands
├── settings.rs               # NEW — settings file I/O commands
├── config.rs                 # NEW — .knot/config.json loader + migration
└── (existing files unchanged)
```

---

## 5. Sequential task breakdown

Tasks are ordered by dependency. Each task is self-contained and ends with a zip + removed-files list per CONVENTIONS §1.

### Task 1 — Status bar expansion
**Why first:** Foundational for all subsequent work — every window needs a status bar, and it's the simplest component to extract from the current `App.svelte`. Establishes the `statusStore` pattern that other tasks use.

**Scope:**
- Extract the status bar from `App.svelte` into `StatusBar.svelte`
- Create `statusStore.ts` — reactive store with: lspStatus, lspError, projectName, tweegoVersion, buildStatus, activeFile, cursorPosition, languageMode
- Add status items: project name, Tweego version (or "not configured"), cursor position (line:col), language mode
- Wire `App.svelte` to push LSP status into the store
- Wire `Editor.svelte` to push cursor position into the store

**Deliverable:** `app/src/lib/statusbar/` (3 files), modified `App.svelte`, modified `Editor.svelte`

---

### Task 2 — Multi-tab Monaco
**Why second:** The editor is the core of the IDE. Without tabs, opening a second file replaces the first. Every layout task depends on the editor being tab-capable.

**Scope:**
- `editorStore.ts` — open tabs state: `{ uri, name, content, isDirty, languageId }[]`, active tab URI
- `EditorTabs.svelte` — tab strip component: tab per open file, click to switch, middle-click or X to close, dirty indicator (dot), right-click context menu (close, close others, close all)
- Modify `App.svelte` to use `editorStore` instead of single `filePath`/`fileContent` state
- `Editor.svelte` stays mostly unchanged — it already swaps models by URI; the tab system just drives which URI is active
- Wire file browser `onSelect` → `editorStore.openTab(path)` instead of replacing single file state
- Dirty state: track on content change, clear on save (save not implemented yet — just track)
- Close tab: if dirty, confirm discard

**Deliverable:** `app/src/lib/editor/editorStore.ts`, `app/src/lib/editor/EditorTabs.svelte`, modified `App.svelte`, modified `Editor.svelte`

---

### Task 3 — Layout data model + core components
**Why third:** The layout system is the foundation for dockable panes and multi-window. Build the data model and core rendering first, then add interactions.

**Scope:**
- `types.ts` — `LayoutNode` tree:
  ```ts
  type LayoutNode =
    | { type: 'split'; direction: 'horizontal' | 'vertical'; children: LayoutNode[]; sizes: number[] }
    | { type: 'panel'; id: string; tabs: TabData[]; activeTabId: string | null }
  type TabData = { id: string; kind: 'editor' | 'filebrowser' | 'storymap' | 'build' | 'settings'; title: string; payload?: unknown }
  ```
- `LayoutRoot.svelte` — recursively renders the layout tree; owns the root `$state`
- `SplitView.svelte` — renders children side-by-side or stacked; manages child sizes (percentages)
- `DockPanel.svelte` — renders a `TabStrip` + the active tab's content component
- `ResizeHandle.svelte` — draggable divider; updates sibling sizes on drag
- `layoutStore.ts` — layout tree state + `openTab()`, `closeTab()`, `moveTab()`, `splitPanel()` operations + persistence to `.knot/window-state.json` (persistence wired in Task 7)
- Map tab `kind` to component: `editor` → `Editor.svelte`, `filebrowser` → `FileBrowser.svelte`, etc.

**Deliverable:** `app/src/lib/layout/` (7 files), modified `App.svelte` (uses `LayoutRoot` instead of hardcoded sidebar+editor)

---

### Task 4 — Drag-and-drop dock interactions
**Why fourth:** With the layout tree rendering, add the interactions that make panels dockable. This is the "mature layout" part.

**Scope:**
- `DropOverlay.svelte` — visual overlay showing 5 dock zones (left, right, top, bottom, center) when dragging a tab over a panel
- Drag a tab within its own panel → reorder tabs
- Drag a tab to another panel → join that panel's tab group
- Drag a tab to a dock zone → split the target panel in that direction, insert the tab
- Drag a tab to the window edge → dock to the edge of the root layout
- Auto-close empty panels after tab drag-out
- Resize handle drag → update split sizes (if not done in Task 3)

**Deliverable:** `app/src/lib/layout/DropOverlay.svelte`, modified `SplitView.svelte`/`DockPanel.svelte`/`TabStrip.svelte`

---

### Task 5 — Multi-window manager
**Why fifth:** Now that the layout system works within a window, extend it to detach panels into new OS windows. High priority per user — graph + code views need simultaneous visibility.

**Scope:**
- `app/src-tauri/src/windows.rs` — Tauri commands: `create_child_window(label, title, layoutJson)`, `close_child_window(label)`, `list_child_windows()`, `focus_window(label)`
- `windowManager.ts` — frontend API: `detachTab(tabId)` → serializes the tab + creates a child window with a single-panel layout containing that tab
- `WindowHost.svelte` — root component for child windows: owns a `LayoutRoot`, runs Monaco `initialize()` if the window contains an editor tab, listens for `focus-window` events
- Inter-window comms: `emitTo(label, 'tab-updated', payload)` when a tab's content changes in one window and needs to sync
- Child window lifecycle: tracked by Rust backend; closing parent closes all children
- `App.svelte` becomes the parent window's root (owns `LayoutRoot` + `StatusBar`)
- Menu bar: add "Send to New Window" action (right-click tab or menu item)

**Deliverable:** `app/src-tauri/src/windows.rs`, `app/src/lib/windows/` (3 files), modified `lib.rs` (register commands), modified `App.svelte`

---

### Task 6 — Settings system + migration
**Why sixth:** With the layout working, add settings. This is needed before themes (Task 7) and persistence (Task 8).

**Scope:**
- `app/src-tauri/src/settings.rs` — `load_editor_settings()`, `save_editor_settings(json)`, `load_project_settings(workspaceRoot)`, `save_project_settings(workspaceRoot, json)`
- `app/src-tauri/src/config.rs` — `.knot/config.json` schema + loader + `.vscode/knot.json` migration (write `.bak`, parse old format, write new)
- `types.ts` — `EditorSettings` + `ProjectSettings` interfaces
- `editorSettings.ts` — load/save `<appData>/settings.json` via `tauri-plugin-store`
- `projectSettings.ts` — load/save `.knot/config.json` via invoke
- `migrate.ts` — check for `.vscode/knot.json` on workspace open, migrate if found
- `SettingsDialog.svelte` — settings UI: two tabs (Editor / Project), form fields for each setting
- Tweego path detection: on first launch, scan PATH + common install locations; store in editor settings
- Wire editor settings → Monaco options (font size, tab size, word wrap, minimap, etc.) reactively

**Deliverable:** `app/src-tauri/src/settings.rs`, `app/src-tauri/src/config.rs`, `app/src/lib/settings/` (5 files), modified `lib.rs`

---

### Task 7 — Themes
**Why seventh:** Depends on settings (theme preference stored in editor settings).

**Scope:**
- `themes.ts` — theme definitions: `dark` (default, current colors), `light`, and a `custom` type that reads from a user-provided JSON
- `themeStore.ts` — active theme, `setTheme(name)`, load from editor settings on startup
- `applyTheme.ts` — apply CSS variables to `:root`, sync Monaco theme (`monaco.editor.setTheme()`)
- CSS variables for all current hardcoded colors in components (extract from `App.svelte`, `FileBrowser.svelte`, `TreeRow.svelte`, `ContextMenu.svelte`, etc.)
- Theme switcher in the View menu (already has "Theme" placeholder)
- Settings dialog: theme picker

**Deliverable:** `app/src/lib/themes/` (3 files), modified components (CSS variableization)

---

### Task 8 — Window-state persistence
**Why last:** Depends on layout (Task 3-4) and settings (Task 6). Persists the full window state across app restarts.

**Status:** Implemented.

**Scope:**
- Extend `layoutStore.ts` with save/load: serialize the layout tree to `.knot/window-state.json`
- Persist: layout tree (which panels, tab order, split sizes), open tabs, active tab per panel, expanded folders in file browser
- `tauri-plugin-window-state` already handles OS-level geometry (window size/position) — this is the custom pane-layout layer
- Load on app startup: restore layout before first paint (show loading state)
- Save on: tab open/close, panel drag/split/resize, window close (debounced)
- Migration: if `.knot/window-state.json` doesn't exist, use default layout (file browser left, editor center)

**Deliverable:** modified `layoutStore.ts`, modified `lib.rs` (config path resolution), new `.knot/window-state.json` schema doc

**What was implemented:**
- New backend module `app/src-tauri/src/window_state.rs` — `load_window_state` + `save_window_state` Tauri commands. Thin file-IO only (no JSON parsing).
- New backend module `app/src-tauri/src/workspace.rs` — shared pure `validate_workspace_root` helper. `config.rs` refactored to use it (deduplication).
- New frontend module `app/src/lib/layout/windowState.ts` — pure `serializeLayout` / `deserializeLayout` with version guard, workspace-match check, and structural shape validation.
- `layoutStore.svelte.ts` gained `loadSavedState(workspaceFolder)` and `saveState(workspaceFolder)` methods + `setFileBrowserExpandedPaths(tabId, paths)` for the filebrowser to write back expand state.
- `FileBrowser.svelte` accepts `initialExpandedPaths` + `onExpandedPathsChange` props; restores expanded folders on mount (sorted shallowest-first so ancestors load before descendants); notifies the parent on every toggle/expand/collapse.
- `DockPanel.svelte` wires the filebrowser tab's `expandedPaths` payload to the new props.
- `App.svelte` loads saved state on workspace open (replacing the unconditional `initDefaultLayout`), sets up a debounced (500ms) `$effect`-driven save on structural layout changes, and flushes the save on `beforeunload`.
- New schema doc `app/docs/window-state.md`.
- `FileBrowserTabPayload` extended with optional `expandedPaths: string[]`.

**Architecture notes:**
- The save `$effect` deliberately reads only structural fields (panel ids, tab lists, sizes, active tab ids, filebrowser `expandedPaths`). Editor-tab `content` and `isDirty` are excluded from the reactivity trigger so typing in the editor doesn't fire a save per keystroke. Content IS included in the saved JSON, but only written to disk on structural changes or window close.
- Errors during load are logged + swallowed — a corrupt state file falls back to the default layout instead of blocking the app.
- The `workspaceFolder` field in the JSON is a defensive double-check (the backend already validates the workspace root against the tracked root).

**Not persisted (deferred):** child window state, editor cursor position, file browser scroll position, file browser selection. See `app/docs/window-state.md` for the full "not persisted" list.

---

## 6. Dependencies graph

```
Task 1 (Status bar)
  └─> Task 2 (Multi-tab Monaco)
        └─> Task 3 (Layout model)
              ├─> Task 4 (Dock interactions)
              └─> Task 5 (Multi-window)
                    └─> Task 8 (Persistence)
Task 6 (Settings) ──> Task 7 (Themes)
                    └─> Task 8 (Persistence)
```

Tasks 1-5 are sequential (each builds on the previous). Tasks 6-7 can run in parallel with 3-5 if needed, but 8 depends on both 5 and 6.

---

## 7. Testing checklist (per task)

Each task must pass before moving to the next:

- [ ] `svelte-check` reports 0 errors
- [ ] `vite build` succeeds
- [ ] `cargo check` passes (for Rust changes)
- [ ] Manual test on Windows: the feature works as described
- [ ] No regressions in existing features (file explorer, LSP, editor)
- [ ] Zip + removed-files list produced per CONVENTIONS §1

---

## 8. References

- **PLAN.md §6** — Window model (parent/child deck model, Monaco init-once, inter-window comms)
- **PLAN.md §8** — Roadmap (Phase 1 deliverable)
- **PLAN.md §9** — Risk register (multi-window persistence, layout manager decision)
- **PLAN.md §12** — Open items (`.knot/config.json` migration approved, layout manager deferred to Phase 1)
- **VS Code source** — Reference for tab strip UX, dock zones, status bar items. Do NOT copy architecture (VS Code is way more complex than we need).
- **`app/docs/file-explorer.md`** — Current file explorer state
- **`app/docs/phase0-supervisor.md`** — LSP crash supervisor design
