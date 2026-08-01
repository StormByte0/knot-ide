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
| Server transport | `knot-server` as subprocess (not in-process), bundled as a Tauri sidecar |
| Sidecar bundling | `knot-server` and Tweego binaries bundled inside the app via Tauri `bundle.externalBin` with target-triple suffixes. Users do not install Rust or Tweego manually. |
| VS Code extension | **Removed.** Tree purged; no maintenance burden. |
| License trajectory | Closed source. Themes may be released under a permissive license (MIT/Apache-2.0) separately. |
| Plugin/extension API | None. Not planned. |
| Workspace config | `.knot/config.json`. Auto-migrate from `.vscode/knot.json` on first open with backup. |
| Menu bar | Native (per-platform). "Check for Updates…" lives under the app menu (About on Windows/Linux). |
| Multi-window | Parent window (primary workspace + native menu + app-level status bar) owns child windows. Any panel ("deck") can detach into a child window for multi-monitor use. Closing parent closes all children. |
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

### 6.1 Parent / child deck model

- App launches a **parent window** containing: native menu bar, app-level
  status bar (project name, `knot-server` health, Tweego version, build status,
  update indicator), and the primary dockable workspace.
- The parent workspace hosts **decks** (panels): editor tabs, Story Map,
  Passage Inspector, Asset Manager, Variable Flow, Build/Run console, file tree.
  Decks are dockable and rearrangeable within the parent.
- Any deck can **detach** into a child window via "Send to New Window" or
  drag-out. Detached decks are owned by the parent and support multi-monitor
  (drag to any screen).
- Child windows are owned by the parent; closing parent closes all children.
- Closing the last window exits the app (after confirm) — equivalent to "Quit".
- Window state (size, position, pane layout) persists in `.knot/window-state.json`
  via `tauri-plugin-window-state` (OS-level geometry) + a custom pane-layout
  layer (which decks are open, dock positions, split sizes).
- Presets: "Editor", "Story Map", "Asset", "Debug" + user-savable custom presets.

### 6.2 Monaco `initialize()`-once constraint

`@codingame/monaco-vscode-api`'s `initialize()` runs once per JS context. Each
Tauri window is a separate JS context. Implications:

- **Parent window**: runs `initialize()` once at startup. Editor decks docked
  in the parent use this single init. ✓
- **Detached non-editor decks** (Story Map, Asset Manager, Build/Run, Variable
  Flow): do not need Monaco. No init cost. ✓
- **Detached editor deck**: runs its own `initialize()` in the child window's
  JS context. Cost is ~50-100ms + ~30MB memory. Acceptable because detached
  editor windows are infrequent (typically one at a time, power-user scenario).

This is the agreed model for MVP. If heavy multi-editor-window use emerges
post-MVP, revisit a shared-worker or postMessage bridge.

### 6.3 Inter-window communication

- Tauri `emitTo(label, event, payload)` targets a specific child window.
- Tauri `emit(event, payload)` broadcasts to all windows.
- Each window listens via `listen(event, cb)`.
- The Rust backend is the authoritative state owner; windows query/mutate state
  via `invoke` commands rather than peer-to-peer window messaging.
- The parent's status bar reflects the focused child window's context (e.g.,
  active passage name, cursor position) via events emitted by the focused deck.

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
| Monaco + Vite production build breaks (works in dev, fails in `tauri build`) | High | Phase 0 spike validates a minimal `tauri build` with bare Monaco before building features. Use manual `?worker` imports + `MonacoEnvironment.getWorker`; do NOT use `vite-plugin-monaco-editor` (conflicts with `monaco-vscode-api`). |
| `tauri-plugin-shell` breaks LSP framing (splits stdout on newlines) | High | Do NOT route LSP stdout through the shell plugin. Use `tokio::process::Command` directly in the Rust backend with manual `Content-Length` frame parsing. Documented bug: plugins-workspace#1632. |
| Detached editor window `initialize()` cost on multi-monitor use | Low | Acceptable per §6.2. Monitor memory usage if users spawn many editor windows; revisit shared-worker bridge if it becomes a problem. |
| WebKitGTK (Linux) Monaco perf on very large files | Medium | WebKitGTK 2.52+ is substantially improved. Test on Ubuntu 22.04 in Phase 0. Disable minimap + bracket-pair colorization for files >5MB. |
| Auto-updater signing key loss | High | Generate keys once, store private key in a secret manager (not in the repo). Lose the key = can never update existing installs. |

## 11. Integration research findings (Phase 0 basis)

Researched August 2026. Full findings in the task worklog.

### 11.1 Frontend stack (locked)

| Concern | Package | Version |
|---|---|---|
| Editor | `monaco-editor` | ~0.56 |
| VS Code services in Monaco | `@codingame/monaco-vscode-api` | matching |
| LSP client | `monaco-languageclient` | ^10.7 |
| Graph renderer | `@xyflow/svelte` | ^1.x |
| Framework | Svelte 5 + Vite 5/6 | latest |
| Tauri APIs | `@tauri-apps/api`, `@tauri-apps/cli` | ^2 |
| Tauri plugins | dialog, fs, updater, window-state, store, global-shortcut | v2 |

### 11.2 CSP (in `tauri.conf.json`)

```
default-src 'self';
script-src 'self' 'wasm-unsafe-eval';
worker-src 'self' blob:;
connect-src ipc: http://ipc.localhost
```

`'wasm-unsafe-eval'` is required for oniguruma WASM (TextMate tokenization).
Monaco itself needs neither `eval` nor inline scripts.

### 11.3 Monaco worker setup

Manual `?worker` imports + `MonacoEnvironment.getWorker` map. Do NOT use
`vite-plugin-monaco-editor` — it conflicts with `monaco-vscode-api` and causes
"Could not resolve editor.worker" build failures.

### 11.4 TextMate grammars

`@codingame/monaco-vscode-api` runs existing `.tmLanguage.json` files unmodified
via `vscode-textmate` + oniguruma WASM. Register the 5 grammars (Twee, SugarCube,
Harlowe, Chapbook, Snowman) as VS Code extension contributions via the extensions
service override. The grammars need to be recreated (they were in the purged
VS Code extension) — the SugarCube parser's tokenizer rules in
`crates/formats/src/sugarcube/lsp/token_builder.rs` are the source of truth.

### 11.5 LSP transport (custom, ~150 LOC frontend)

`monaco-languageclient` v10 supports custom `MessageTransports`.

- **Writer** (`TauriIpcWriter`): serializes JSON-RPC, calls
  `invoke('lsp_send', { payload })` → Rust writes to `knot-server` stdin with
  `Content-Length` framing.
- **Reader** (`TauriIpcReader`): listens to `lsp-message` Tauri events emitted
  by the Rust backend (which frames `knot-server` stdout), dispatches to
  `vscode-jsonrpc` reader.

Maintainer-confirmed (TypeFox discussion #583). No off-the-shelf Tauri+LSP
template exists; the transport shim is greenfield.

### 11.6 Subprocess supervisor (Rust backend, ~400 LOC)

- Spawn `knot-server` with `tokio::process::Command` (NOT `tauri-plugin-shell` —
  it breaks LSP framing by splitting stdout on newlines).
- Async task reads stdout, parses `Content-Length` frames,
  `app.emit("lsp-message", body)`.
- `#[tauri::command] fn lsp_send(payload: String)` writes framed messages to stdin.
- **Watchdog:** LSP `$/ping` every 10s; timeout → kill + restart.
- **Crash capture:** on child exit, capture last N stdout lines + exit code →
  anonymized report to `<appData>/crash-reports/<timestamp>.json`.
- **Auto-restart:** exponential backoff (2s → 4s → 8s → 16s cap), max 5 retries.
- **State restore:** track open documents + cursor positions + in-flight edits in
  `tauri::State`; on restart, re-`initialize` → re-`didOpen` all docs → replay
  pending edits.

### 11.7 Sidecar bundling

`knot-server` and Tweego binaries bundled via Tauri's `bundle.externalBin` with
`target-triple` suffixes (e.g., `knot-server-x86_64-unknown-linux-gnu`,
`knot-server-aarch64-apple-darwin`). At runtime, resolve via
`app.path().resolve("knot-server", BaseDirectory::Executable)`. Build pipeline
produces per-target binaries; the Tauri bundler picks the right one per platform.

### 11.8 Native menu bar

Tauri 2 `Menu` / `Submenu` API. macOS: screen-top, items must be grouped under
submenus. Windows/Linux: in-window. Structure:

- **App** (macOS) / **File** (Win/Linux): New Project, Open…, Recent, Quit
- **Edit**: Undo, Redo, Cut, Copy, Paste, Find…
- **View**: Toggle Story Map, Toggle Asset Manager, Theme, Zoom…
- **Build**: Build, Play, Watch toggle
- **Help**: About, **Check for Updates…**, Documentation

### 11.9 Auto-updater

`tauri-plugin-updater` + `tauri signer` CLI. `tauri-action` GitHub Action
builds Win/Mac/Linux matrix, uploads to GitHub Release, auto-generates
`latest.json`. "Check for Updates…" menu item calls `check()` →
`downloadAndInstall()` → `relaunch()`.

## 12. Open items

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
scaffold the Tauri app shell in a new `app/` directory and run the Phase 0 spike
(validate Monaco+Vite+Tauri production build, LSP round-trip over Tauri IPC,
crash-during-edit sequence diagram).
