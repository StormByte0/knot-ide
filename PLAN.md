# Knot — Tauri Migration Plan

**Status:** Active. Server fixes (#7 below) land on `main` first; the Tauri
spike starts in parallel once the toolchain is scaffolded.

**Last updated:** 2026-08-01

---

## 1. Goal

Migrate Knot from a VS Code extension to a standalone desktop application built on
Tauri 2, distributed as closed source (with permissively-licensed themes). The VS
Code extension has been removed from the tree; the Rust workspace (`crates/`) is
the only surviving code and is now the foundation for the desktop app.

The migration is also a redesign opportunity: the Story Map is being rebuilt as a
passage-metadata editor (not a logic editor), and an asset manager is being added
to address Tweego's lack of asset bundling.

## 2. Locked decisions

| Area | Decision |
|---|---|
| App shell | Tauri 2 |
| Editor | Monaco |
| Frontend framework | Svelte 5 |
| Graph renderer | svelte-flow (behind a `GraphRenderer` abstraction for future swap) |
| Server transport | `knot-server` as subprocess (not in-process) |
| VS Code extension | **Removed.** Tree purged; no maintenance burden. |
| License trajectory | Closed source. Themes may be released under a permissive license (MIT/Apache-2.0) separately. |
| Plugin/extension API | None. Not planned. |
| Workspace config | `.knot/config.json`. Auto-migrate from `.vscode/knot.json` on first open with backup. |
| Menu bar | Native (per-platform). "Check for Updates…" lives under the app menu (About on Windows/Linux). |
| Multi-window | Yes. Parent window owns child windows; panels are detachable and movable. |
| Crash reporting | Mandatory local anonymized report file (machine/env/error). Opt-in cloud upload deferred until website/server exists. |
| Auto-update | Tauri updater. "Check for Updates…" menu item. |
| Min OS | Windows 10 1903+, macOS 11+ (Big Sur), Ubuntu 22.04+ / Fedora 36+. Same as current. |
| Compiler | External prereq now: **Tweego** (not "Twine CLI" — that's the GUI project). In-house compiler is a future track. |
| Story Map scope | Passage metadata only (position, color, group, tags, display name). No link/logic editing. |
| Asset manager scope | MVP: ownership of `assets/` folder; loading a file copies it in under a managed name + relative path. |
| Asset reference syntax | Reuse existing SugarCube `[img[...]]` parser (`link_parser.rs:81-230`). Not a new proposal. |

## 3. Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Tauri Window                       │
│  ┌───────────────────────────────────────────────┐  │
│  │              Svelte 5 Frontend                 │  │
│  │  ┌──────────┐  ┌──────────────────────────┐   │  │
│  │  │  Monaco  │  │  App UI                   │   │  │
│  │  │  editor  │  │  - Story Map v2           │   │  │
│  │  │  panes   │  │  - Passage Inspector      │   │  │
│  │  │          │  │  - Variable Flow          │   │  │
│  │  │          │  │  - Asset Manager          │   │  │
│  │  │          │  │  - Build/Run panel        │   │  │
│  │  │          │  │  - File tree, palette     │   │  │
│  │  └────┬─────┘  └──────────┬───────────────┘   │  │
│  │       │  monaco-lc        │  invoke/listen    │  │
│  └───────┼───────────────────┼───────────────────┘  │
│          ▼                   ▼                      │
│  ┌───────────────────────────────────────────────┐  │
│  │            Tauri Rust Backend                  │  │
│  │  ┌────────────────┐  ┌──────────────────────┐ │  │
│  │  │ LSP bridge     │  │ Process supervisor   │ │  │
│  │  │ (JSON-RPC      │  │ - spawn knot-server  │ │  │
│  │  │  forwarding)   │  │ - stdin/stdout pipes │ │  │
│  │  │                │  │ - watchdog (ping)    │ │  │
│  │  │                │  │ - crash capture      │ │  │
│  │  │                │  │ - auto-restart       │ │  │
│  │  │                │  │ - state restore      │ │  │
│  │  └────────────────┘  └──────────────────────┘ │  │
│  │  ┌────────────────┐  ┌──────────────────────┐ │  │
│  │  │ File I/O +     │  │ Compiler runner      │ │  │
│  │  │ watchers       │  │ - detect Tweego      │ │  │
│  │  │                │  │ - execute build      │ │  │
│  │  │                │  │ - stream stdout      │ │  │
│  │  └────────────────┘  └──────────────────────┘ │  │
│  │  ┌────────────────┐                          │  │
│  │  │ Asset manager  │                          │  │
│  │  │ - manifest CRUD│                          │  │
│  │  │ - file copy-in│                          │  │
│  │  │ - fs watcher   │                          │  │
│  │  └────────────────┘                          │  │
│  └───────────────────────────────────────────────┘  │
│          │                                           │
│          ▼ (subprocess)                              │
│  ┌───────────────────────────────────────────────┐  │
│  │  knot-server (existing Rust LSP, unchanged)   │  │
│  └───────────────────────────────────────────────┘  │
│          │                                           │
│          ▼ (subprocess, external prereq)            │
│  ┌───────────────────────────────────────────────┐  │
│  │  Tweego compiler (external)                   │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

## 4. Story Map v2 spec

### 4.1 Scope

The Story Map is a **passage metadata editor and navigation surface**, not a logic
editor. Every action in Story Map modifies only metadata (position, color, group,
tags, display name) or opens a passage in the code editor.

Edges (links) are derived **read-only** from `[[link]]` syntax in passage source
and cannot be created, edited, or deleted from the Story Map. Logic editing stays
in the passage code.

### 4.2 Gesture contract (locked — Option A: industry standard)

This contract follows the Unity / Blender / Figma convention. **Left-drag on
empty space never pans** — it box-selects. Panning is on middle-drag (or
Space+left-drag), matching every major scene/graph editor.

| Input | Action |
|---|---|
| Left-click on node | Select node |
| Left-click + hold on node | Drag node (updates position metadata) |
| Shift + left-click on node | Add to selection |
| Alt + left-click on node | Remove from selection |
| Double left-click on node | Open passage in editor |
| Right-click on node | Context menu: rename, recolor, retag, group, delete, etc. |
| Left-click + drag on empty space | Box select (marquee) |
| Middle-click + drag | Pan viewport |
| Space + left-click + drag | Pan viewport (alternate) |
| Scroll | Zoom (cursor-centered) |

**Rationale:** the previous plan had a conflict ("left-drag on empty pans" +
"click and drag to box select" collide). Option A resolves it by never
overloading left-drag-on-empty with panning. Middle-drag pan is muscle memory
for users coming from Blender, Unity, Figma, Godot, and most 3D/DCC tools.

### 4.3 Edge interaction (read-only)

- Edges render as arrows from source to target passage
- Hover shows the link text (e.g. `[[Target]]`)
- Double-click on edge → opens source passage with cursor on the link
- No edge creation, deletion, or retargeting from Story Map

### 4.4 Features

- Multi-select via shift+click and marquee
- Group passages into regions/acts (visual grouping with color)
- Search/filter by name, tag, format
- Minimap with viewport indicator
- Layout algorithms: hierarchical, force-directed, manual (saved positions)
- Persisted positions in `.knot/storymap.json`

### 4.5 Format coverage

Story Map v2 ships for **all four formats** (SugarCube, Harlowe, Chapbook,
Snowman). Link extraction — the only thing Story Map needs from a format plugin
— is already implemented in all four. Editor intelligence features (completion,
hover, JS analysis, macro catalog, CSS parsing) remain SugarCube-only until the
other formats reach parity; that work is tracked separately in `ROADMAP.md` and
is **not** on the Story Map v2 critical path.

## 5. Asset manager spec (MVP)

### 5.1 Scope

The asset manager owns an `assets/` directory at the project root. Authors load
external files (images, audio, gifs, fonts) into the project; the asset manager
copies the file under a managed name and relative path inside `assets/`. Game
code references assets via that relative path. This addresses Tweego's lack of
asset bundling — authors currently have no managed way to bring external media
into a project.

**Out of scope for MVP:** thumbnailing, preview, tagging, search, bundling
formats beyond folder copy, conflict detection beyond filename collision. These
land in a later phase.

### 5.2 Workflow

1. Author drags an external file onto the asset browser (or uses "Import…")
2. Asset manager prompts for a managed name (default: filename without extension)
3. Asset manager copies the file to `assets/<managed-name>.<ext>`
4. Asset manager records the entry in `assets/manifest.json`
5. Author can copy the relative path (`assets/<managed-name>.<ext>`) or drag a
   reference into a passage

### 5.3 Manifest format

`assets/manifest.json`:

```json
{
  "version": 1,
  "assets": [
    {
      "id": "hero",
      "name": "hero",
      "path": "assets/hero.png",
      "type": "image",
      "source": "original-filename.png",
      "imported_at": "2026-07-31T14:00:00Z"
    }
  ]
}
```

### 5.4 Reference syntax

**Settled, not open.** The existing SugarCube parser at
`crates/formats/src/sugarcube/parser/link_parser.rs:81-230` already implements
`parse_image_link` for all four SugarCube image forms:

- `[img[src]]`
- `[img[Tooltip|src]]`
- `[img[src][Passage]]`
- `[img[Tooltip|src][Passage]]`

The AST node carries `kind: LinkKind::Image` with a separate `image_url` field
(`ast.rs:886`), so an asset-rewrite pass that scans for `image_url` values
matching a renamed asset is straightforward. Asset rename rewrites references
across all passages via `knot-core` library calls.

## 6. Window model

- App launches a parent window with native menu bar and system tray
- Each panel (editor tabs, story map, inspector, asset manager, console) is
  detachable and movable
- "Send to new window" or drag-out creates a child window
- Child windows are owned by the parent; closing parent closes all children
- Closing the last window exits the app (after confirm) — equivalent to "Quit"
- Window state (size, position, pane layout) persists in `.knot/window-state.json`
- Presets: "Editor", "Story Map", "Asset", "Debug" + user-savable custom presets

## 7. Server bug list

Bugs #1–#6 were the original pre-Tauri blocker list. **All six shipped on `main`**
before this revision — explicit `"Bug #N"` comments and regression tests are in
the tree. They are recorded here for history; no work remains on them.

| # | Bug | Status | Where it was fixed |
|---|---|---|---|
| 1 | Passage creation only recognizes `::[firstletter]` when no space between `::` and title | **Fixed** | `crates/formats/src/header.rs:81-119` + `sync.rs:970` header-stability check |
| 2 | No LSP restart on StoryData modification | **Fixed** | `crates/server/src/handlers/sync.rs:366-447` (`check_storydata_format_change`) |
| 3 | Passages after special passages marked invalid/plaintext if no space after `::` | **Fixed** | Same root cause as #1 |
| 4 | Workspace expands beyond source dir when unrelated twee file opened | **Fixed** | `crates/server/src/handlers/sync.rs:117` + `helpers/uri.rs:49` |
| 5 | Files persisting in cache after close/move → phantom passage conflicts | **Fixed** | `crates/server/src/handlers/sync.rs:1217` + `did_change_watched_files` DELETED |
| 6 | Very slow format recognition after restart LSP command | **Unverified** | No code trace; may be perceived only. Revisit if profiling shows a real cliff. |
| **7** | **Incremental parser glitch on new passage creation** | **Open — High** | `crates/server/src/handlers/sync.rs:962-986` (header-stability heuristic) |

### 7.1 Bug #7 — fix plan

The header-stability heuristic at `sync.rs:962-986` tries to detect newly-typed
`::` headers character-by-character during incremental parsing. This is
inherently fragile and is the source of the glitch.

**Fix:** change the incremental-vs-full reparse decision.

- **Header line edited → full file reparse.** Headers are rare edits; the cost
  is negligible and the correctness is unconditional.
- **Body edits → incremental parsing only.** Bodies are the hot path;
  incremental parsing is what matters there.

This eliminates the header-stability heuristic entirely. Land on `main` before
the Tauri spike — it's a server-side fix independent of the app shell.

## 8. Roadmap

| Phase | Duration | Deliverable |
|---|---|---|
| Server fix #7 | 2-3 days | Reparse decision fix on `main`; remove header-stability heuristic |
| 0. Spike | 1 wk | Tauri 2 + Monaco + Svelte 5 + `knot-server` subprocess PoC. Include crash-during-edit sequence spike. |
| 1. App shell | 2-3 wk | Multi-window manager, movable/dockable panes, file explorer, multi-tab Monaco, native menu bar (per-platform), system tray, themes, settings |
| 2. Editor layer | 1-2 wk | TextMate grammars, language config, decorations, multi-format support |
| 3. Process supervisor | 2 wk | Subprocess spawn/kill, watchdog ping, crash capture to local file (anonymized), auto-restart with backoff, state restore. Crash-during-edit handling per Phase 0 sequence diagram. |
| 4. Story Map v2 | 4-6 wk | svelte-flow metadata editor per §4. All four formats on day one (link extraction is complete). |
| 5. Asset manager (MVP) | 2-3 wk | Per §5. Manifest CRUD, copy-in, fs watcher, reference rewrite on rename. |
| 6. Other webviews v2 | 2-3 wk | Variable Flow, Profile View, Debug View in Svelte |
| 7. Build/Run system | 2 wk | Tweego detection, build config UI, output streaming, multi-target output architecture. **Introduce `Compiler` trait** — extract tweego behind it so the in-house compiler (Phase 10) is a swap, not a rewrite. |
| 8. Commands & polish | 1-2 wk | Command palette, status bar, shortcuts, crash recovery UI, "Check for Updates…" menu item |
| 9. Packaging | 1 wk | Tauri bundler (Win/Mac/Linux), code signing, auto-update channel |
| 10. In-house compiler | Future | Asset pipeline, gamedir output, per-OS executables. Separate track. Depends on Phase 7 `Compiler` trait. |

**Total to MVP: ~18-25 weeks solo.**

## 9. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| svelte-flow rigidity conflicting with custom renders (cytoscape failure mode) | Medium | Keep `GraphRenderer` abstraction layer from day one. Do not let svelte-flow own node component lifecycle — nodes are Svelte components, layout is a separate pluggable step. Spike in Phase 0. |
| Multi-window state persistence across OS window managers | Medium | Tauri window-state plugin for window size/position; custom layer for pane layout. |
| Tweego version drift | Medium | Detect version on build, warn if unsupported. |
| Crash report file growth in crash loops | Low | Cap at last 20 reports; rotate. |
| Subprocess supervisor race conditions (restart while user typing) | High | Sequence diagram for crash-during-edit case in Phase 0; test explicitly in Phase 3. State restore must snapshot in-flight edits. |
| WebKitGTK version variance on Linux | Medium | Bundle or pin minimum; document supported distros. |
| Story Map v2 scope creep | High | Hard scope freeze after §4 features. Anything else → `ROADMAP.md`. |
| Tweego coupling blocks in-house compiler | Medium | Phase 7 introduces `Compiler` trait; tweego becomes one implementation. Phase 10 is a swap, not a 900-line rewrite. |
| `.vscode/knot.json` → `.knot/config.json` migration breaks existing projects | Low | Auto-migrate on first open; write `.vscode/knot.json.bak` backup. 3 code sites to update (`lifecycle.rs:73`, `sync.rs:1265`, plus the new Tauri-side config loader). |

## 10. Open items

| Item | Default proposal | Status |
|---|---|---|
| `.vscode/knot.json` → `.knot/config.json` migration | Auto-migrate on first open, write backup of old file | **Approved** — implement in Phase 1 |
| Asset reference syntax | Reuse existing `[img[...]]` parser | **Closed** — already implemented |
| Telemetry / cloud crash reporting | Out of scope until website/server exists | Deferred |
| Layout manager library | Golden Layout or custom Svelte; spike in Phase 1 | Deferred |
| Tauri updater signing key infrastructure | Generate in Phase 9 | Deferred |
| Theme license (MIT vs Apache-2.0) | MIT, released as a separate repo post-MVP | Deferred |

---

**Next action:** Fix Bug #7 on `main` (reparse decision in `sync.rs`), then
scaffold the Tauri app shell in a new `app/` directory at the repo root.
