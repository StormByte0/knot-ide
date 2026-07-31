# Knot — Tauri Migration Plan

**Status:** Shelved (2026-07-31). Active work has moved to the `fix/server-pre-tauri`
branch to resolve blocking server bugs first. This plan resumes once those land.

**Last updated:** 2026-07-31

---

## 1. Goal

Migrate Knot from a VS Code extension to a standalone desktop application built on
Tauri 2, with a long-term path to a closed-source / commercial release. The VS Code
target is being abandoned entirely.

The migration is also a redesign opportunity: the Story Map is being rebuilt as a
passage-metadata editor (not a logic editor), and an asset manager is being added
to address Tweego's lack of asset bundling.

## 2. Locked decisions

| Area | Decision |
|---|---|
| App shell | Tauri 2 |
| Editor | Monaco |
| Frontend framework | Svelte 5 |
| Graph renderer | svelte-flow (with abstraction layer for future swap) |
| Server transport | `knot-server` as subprocess (not in-process) |
| VS Code extension | Abandoned; not maintained alongside |
| License trajectory | Source-available now → closed / commercial post-MVP |
| Plugin/extension API | None for now |
| Workspace config | `.knot/config.json` (clean break from `.vscode/knot.json`) |
| Menu bar | Native (per-platform) |
| Multi-window | Yes — parent window owns child windows; panels detachable |
| Crash reporting | Mandatory local file (anonymized); optional cloud upload later |
| Auto-update | Tauri updater, "Check for Updates…" menu item |
| Min OS | Windows 10 1903+, macOS 11+, Ubuntu 22.04+ / Fedora 36+ |
| Compiler | External prereq now (Twine CLI); in-house compiler is a future track |
| Story Map scope | Passage metadata only (position, color, group, tags). No link/logic editing. |
| Asset manager scope | MVP: ownership of `assets/` folder; loading a file copies it in under a managed name + relative path. |

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
│  │  │ watchers       │  │ - detect Twine CLI   │ │  │
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
│  │  knot-server (existing, unchanged)            │  │
│  └───────────────────────────────────────────────┘  │
│          │                                           │
│          ▼ (subprocess, external prereq)            │
│  ┌───────────────────────────────────────────────┐  │
│  │  Twine compiler (external)                    │  │
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

### 4.2 Gesture contract

| Input | Action |
|---|---|
| Left-click on node | Select node |
| Left-click + hold on node | Drag node (updates position metadata) |
| Shift + left-click | Add to selection |
| Alt + left-click | Remove from selection |
| Double left-click on node | Open passage in editor |
| Right-click on node | Context menu: rename, recolor, retag, group, delete, etc. |
| Left-click + drag on empty | Box select (marquee) |
| Middle-click + drag | Pan viewport |
| Scroll | Zoom (cursor-centered) |

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

## 5. Asset manager spec (MVP)

### 5.1 Scope

The asset manager owns an `assets/` directory at the project root. Authors load
external files (images, audio, gifs, fonts) into the project; the asset manager
copies the file under a managed name and relative path inside `assets/`. Game
code references assets via that relative path.

**Out of scope for MVP:** thumbnailing, preview, tagging, search, bundling formats
beyond folder copy, conflict detection beyond filename collision. These land in a
later phase.

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

TBD — open item (section 10). Default proposal: use existing SugarCube syntax with
the managed relative path: `[img[assets/hero.png]]`. Asset rename rewrites
references across all passages via `knot-core` library calls.

## 6. Window model

- App launches a parent window with native menu bar and system tray
- Each panel (editor tabs, story map, inspector, asset manager, console) is
  detachable
- "Send to new window" or drag-out creates a child window
- Child windows are owned by the parent; closing parent closes all children
- Closing the last window exits the app (after confirm) — equivalent to "Quit"
- Window state (size, position, pane layout) persists in `.knot/window-state.json`
- Presets: "Editor", "Story Map", "Asset", "Debug" + user-savable custom presets

## 7. Server bug list (fix first, on `fix/server-pre-tauri`)

| # | Bug | Severity | Likely area |
|---|---|---|---|
| 1 | Passage creation only recognizes `::[firstletter]` when no space between `::` and title | High | `crates/formats/src/header.rs`, parser |
| 2 | No LSP restart on StoryData modification | High | `crates/server/src/handlers/sync.rs`, `lifecycle.rs` |
| 3 | Passages after special passages marked invalid/plaintext if no space after `::` | High | Same root cause as #1 |
| 4 | Workspace expands beyond source dir when unrelated twee file opened | High | `crates/server/src/handlers/workspace.rs` |
| 5 | Files persisting in cache after close/move → phantom passage conflicts | High | `crates/server/src/state.rs` |
| 6 | Very slow format recognition after restart LSP command | Medium | `crates/server/src/handlers/lifecycle.rs`, `crates/formats/src/format_meta.rs` |

**Fix order:** 1+3 (shared root cause) → 5 → 4 → 2 → 6.

## 8. Roadmap

| Phase | Duration | Deliverable |
|---|---|---|
| Server fixes (parallel track) | 1-2 wk | Fix bugs #1-6 on `fix/server-pre-tauri` branch |
| 0. Spike | 1 wk | Tauri 2 + Monaco + Svelte 5 + `knot-server` subprocess proof-of-concept |
| 1. App shell | 2-3 wk | Multi-window manager, dockable panes, file explorer, multi-tab Monaco, themes, settings |
| 2. Editor layer | 1-2 wk | TextMate grammars, language config, decorations, multi-format support |
| 3. Process supervisor | 1 wk | Subprocess spawn/kill, watchdog, crash capture, auto-restart, local crash report generator |
| 4. Story Map v2 | 4-6 wk | svelte-flow metadata editor per spec in section 4 |
| 5. Asset manager (MVP) | 2-3 wk | Per spec in section 5 |
| 6. Other webviews v2 | 2-3 wk | Variable Flow, Profile View, Debug View in Svelte |
| 7. Build/Run system | 1-2 wk | Twine CLI detection, build config UI, output streaming, multi-target output architecture |
| 8. Commands & polish | 1-2 wk | Command palette, status bar, shortcuts, crash recovery UI, Check for Updates |
| 9. Packaging | 1 wk | Tauri bundler (Win/Mac/Linux), code signing, auto-update channel |
| 10. In-house compiler | Future | Asset pipeline, gamedir output, per-OS executables. Separate track. |

**Total to MVP: ~17-24 weeks solo.**

## 9. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| svelte-flow middle-mouse pan may not be default | Medium | Override pan handler in Phase 4 spike |
| Multi-window state persistence is fiddly across OS window managers | Medium | Use Tauri window-state plugin; custom layer for pane layout |
| Twine CLI version drift | Medium | Detect version, warn if unsupported |
| Crash report file growth in crash loops | Low | Cap at last 20 reports |
| Subprocess supervisor race conditions (restart while user typing) | Medium | Sequence diagram for crash-during-edit case; test explicitly |
| WebKitGTK version variance on Linux | Medium | Bundle or pin minimum; document supported distros |
| Story Map v2 scope creep | High | Hard scope freeze after section 4 features. Anything else → ROADMAP.md |

## 10. Open items

| Item | Default proposal | Status |
|---|---|---|
| Asset reference syntax | SugarCube-style `[img[assets/hero.png]]` with managed paths | Needs sign-off |
| `.vscode/knot.json` → `.knot/config.json` migration | Auto-migrate on first open, write backup of old file | Needs sign-off |
| Telemetry / cloud crash reporting | Out of scope until website/server exists | Deferred |
| Layout manager library | Golden Layout or custom Svelte; spike in Phase 1 | Deferred |
| Tauri updater signing key infrastructure | Generate in Phase 9 | Deferred |

---

**Next action:** Switch to `fix/server-pre-tauri` branch and start with bugs #1 + #3
(shared root cause in header parser).
