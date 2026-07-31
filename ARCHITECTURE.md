# Knot — Architecture

This document describes how Knot is structured, what each component does, and
how the pieces fit together. It is written for developers and curious users
who want to understand the system under the hood.

> **Note (2026-08-01):** Knot is migrating from a VS Code extension to a
> standalone Tauri 2 desktop app. The VS Code extension has been removed from
> the tree. The Rust workspace (`crates/`) is the only surviving code and is
> now the foundation for the desktop app. The Tauri frontend (`app/`) and
> backend integration are not yet scaffolded — see `PLAN.md` for the migration
> roadmap. Sections describing the VS Code extension below are retained as
> historical context for the build pipeline and feature set that the Tauri
> app must reproduce; they are marked **[legacy]**.

---

## Overview

Knot is a language server and (soon) desktop IDE for Twine/Twee interactive
fiction projects. The project is modeled as a directed graph of passages
connected by links, which enables structural analysis (broken links,
unreachable passages, dead ends) that file-by-file tooling cannot provide.
Game loop detection is planned but not yet implemented — it requires
conditional-edge tracking that the current graph model does not support.

The project is split into two parts:

1. **Rust language server** (`crates/`) — a high-performance LSP server
   that parses twee files, builds the workspace graph, runs analysis, and
   handles all language features. Written in Rust for low latency and
   memory safety. **This is the only code currently in the tree.**

2. **Tauri desktop app** (`app/`, not yet scaffolded) — the future client
   that will own the UI: native menu bar, multi-window manager, movable
   dockable panes, Monaco editor, Story Map v2, Asset Manager, Build/Run
   panel, and a process supervisor for `knot-server`. Communicates with
   the server over LSP via subprocess stdin/stdout. See `PLAN.md` for the
   full migration roadmap.

The client never parses twee files directly. Every language feature goes
through the server via standard LSP requests and a small set of custom
`knot/*` requests.

---

## Rust Workspace

The server is organized as a Cargo workspace with three crates:

```
crates/
├── core/       — workspace model, graph, analysis, document editing
├── formats/    — format plugins (SugarCube, Harlowe, Chapbook, Snowman)
└── server/     — LSP server, request handlers, client communication
```

### `knot-core`

The foundation. Defines the format-agnostic data model that all format
plugins produce and all analysis runs against:

- **`Workspace`** — owns all documents, the passage graph, configuration
  (currently `.vscode/knot.json`, migrating to `.knot/config.json`), and
  resolved story metadata (format, version, IFID from StoryData). This is
  the central state that everything else reads from.
- **`Document`** — a single parsed `.twee` file. Contains passages,
  their tags, links, variable operations, and a `Rope`-backed text
  buffer for incremental editing.
- **`Passage`** — a single passage with its header, body blocks
  (text, macros, expressions, headings), links, variable operations,
  and classification (special, metadata, normal).
- **`Block`** — the content within a passage body: plain text, macro
  invocations, inline expressions, headings, or incomplete/malformed
  segments.
- **`Graph`** — a `petgraph` directed graph where nodes are passages
  and edges are links. Supports incremental surgery (add/remove
  passages and links without rebuilding the whole graph), reachability
  analysis (for dead-end and unreachable detection), and SCC
  computation (Tarjan's algorithm). The graph model has public mutation
  APIs (`add_edge`, `remove_edges_from`, `edge_weight_mut`) but these
  are only called from the parse pipeline — never from user input. Story
  Map v2 treats edges as derived read-only, enforced by convention.
- **`Analysis`** — runs the diagnostic passes over the workspace:
  broken links, unreachable passages, uninitialized variables, unused
  variables, redundant writes, duplicate passage names, empty passages,
  dead-end passages, invalid passage names, complex passages (too many
  outgoing links), large passages (exceeds word count threshold).
- **`Editing`** — incremental document update logic. Applies text
  changes to the `Rope` and figures out which passages were affected,
  so only those need re-parsing.

### `knot-formats`

The format plugin system. Each Twine story format (SugarCube, Harlowe,
Chapbook, Snowman) has its own parser that produces the format-agnostic
`Document`/`Passage`/`Block` types defined in `knot-core`.

- **`FormatPlugin` trait** — defines the contract every format must
  implement: parsing, semantic token generation, special passage
  classification, macro catalog access, completion providers, hover
  providers.
- **`FormatRegistry`** — routes requests to the right plugin based on
  the workspace's detected format (from StoryData).
- **SugarCube** (`crates/formats/src/sugarcube/`) — the most complete
  plugin. Uses a recursive descent parser backed by `oxc` (a
  JavaScript parser) for the `<<script>>` and `<<set>>` bodies. Has a
  static macro catalog (~1200 lines of data), special passage
  definitions, CSS parsing, and a full JS annotation pipeline that
  tracks variable reads/writes across SugarCube macros.
- **Harlowe, Chapbook, Snowman** — these have full `FormatPlugin` trait
  implementations and complete link extraction (`[[Target]]`,
  `[[Display->Target]]`, `[[Display|Target]]`), which is sufficient for
  Story Map v2 edge rendering across all four formats. However, they
  lack SugarCube's macro catalog, `oxc`-backed JS analysis pipeline,
  CSS parser, and LSP syntax-detect/token-builder modules. Editor
  intelligence features (completion, hover, JS validation) are
  SugarCube-only until the other formats reach parity. Bringing them
  to parity is planned — see `ROADMAP.md`.

The key architectural principle is **format ownership**: each plugin
owns its syntax, its special passages, its macros, and its semantic
tokens. The server never hardcodes SugarCube syntax — it asks the
plugin.

### `knot-server`

The LSP server built on `tower-lsp` + `tokio`. Handles all client
communication and delegates to `knot-core` and `knot-formats` for the
actual work.

- **`state.rs`** — `ServerState`, the server's mutable state. Holds
  the `Workspace`, the `FormatRegistry`, the language client handle,
  and the client's global storage path.
- **`handlers/`** — LSP request handlers, organized by concern:
  - `sync.rs` — `did_open`, `did_change`, `did_close`, file watching,
    workspace indexing. Contains the incremental-vs-full reparse
    decision logic (see Bug #7 in `PLAN.md`).
  - `completion.rs`, `hover.rs`, `navigation.rs`, `semantic.rs`,
    `structure.rs` — standard LSP features.
  - `build.rs` — the `knot/build` and `knot/play` custom requests.
    Resolves the tweego binary, story formats directory, source
    directory, output filename, and runs tweego. **Note:** this is
    ~891 lines of tweego-specific code with no `Compiler` trait
    abstraction. Phase 7 of the Tauri migration introduces a
    `Compiler` trait so the in-house compiler (Phase 10) is a swap,
    not a rewrite.
  - `profile.rs`, `passage_diagnostics.rs` — custom requests for the
    webview panels.
- **`lsp_ext.rs`** — definitions for all custom `knot/*` request and
  notification types.
- **`helpers/`** — shared utilities: compiler resolution (`which`/
  `where`), story formats directory discovery, indexing logic, URI
  conversion.

---

## [legacy] VS Code Extension

> The VS Code extension has been removed from the tree. The section below
> is retained as historical context for the build pipeline and feature set
> that the Tauri app must reproduce.

The former client was a VS Code extension written in TypeScript. It owned
all UI and orchestrated the server. Its responsibilities — status bar,
commands, webview panels (Story Map, Debug View, Profile View, Variable
Tracking), build orchestration, crash recovery — will be absorbed by the
Tauri app's Svelte frontend and Rust backend.

### [legacy] Build Pipeline

The build flow was the most complex orchestration in the extension:

1. **Source**: the workspace root is the source directory. Users put
   all game files (`.twee`, `.js`, `.css`, assets) directly in the
   workspace. Story formats live separately in the client-managed
   folder — this keeps the workspace purely game files and prevents
   `format.js` from being bundled as a passage.

2. **Tweego resolution**: `knot.build.tweegoPath` setting →
   `.vscode/knot.json` → PATH lookup → managed download.

3. **Story formats resolution**: `knot.build.storyformatsPath` setting
   → versioned managed cache (`<globalStorage>/storyformats/<id>@<ver>/`)
   → error with download hint.

4. **Output filename**: derived from the `StoryTitle` passage
   (sanitized), falling back to `index.html`. This matches Twine GUI
   behavior.

5. **Tweego invocation**: the server assembles args (`--start` if
   needed, `-l` for stats, `-o` for output, merged flags from settings
   + `.knot/config.json`, source path) and runs tweego with `cwd` set
   to the workspace root. `TWEEGO_PATH` env var is set when story
   formats are in the managed cache or a user-configured path.

6. **Output streaming**: tweego's stdout/stderr is streamed to the
   build output panel via `knot/buildOutput` notifications. The stats
   line (`Passages: N | Words: N`) is parsed and re-emitted as
   `Knot: Build stats — N passages, N words`.

### [legacy] Webview Panels

Four webview panels provided visual tooling. All four will be rebuilt in
Svelte as part of the Tauri migration:

- **Story Map** — an interactive directed graph of all passages,
  rendered with `@xyflow/react` + `dagre` for layout. Nodes are
  passages, edges are links. The Tauri migration rebuilds this as
  Story Map v2 using svelte-flow, scoped to passage-metadata editing
  only (see `PLAN.md` §4).
- **Passage Diagnostics** — shows detailed info about the passage
  under the cursor: links, variables, macros, complexity metrics.
- **Project Info** — workspace-level stats: passage count, word count,
  format, IFID, story format version.
- **Variable Tracking** — shows variable flow across passages:
  where each variable is set, read, and how it propagates.

---

## Format Detection

The workspace's story format is detected from the `StoryData`
passage's `format` field. When the server parses a `StoryData`
passage, it extracts the format name (e.g. "SugarCube"), version
(e.g. "2.37.0"), IFID, and start passage. This metadata is stored in
`Workspace.metadata` and drives:

- Which `FormatPlugin` handles parsing and language features
- Which story format the build pipeline downloads into the managed
  cache (if missing)
- The versioned managed cache path: `<globalStorage>/storyformats/sugarcube-2@2.37.0/`

If no `StoryData` passage exists, the server defaults to SugarCube
and notifies the client via `knot/formatDetected`. The client may
prompt the user to initialize a project.

---

## Configuration

Knot reads configuration from two sources, merged at build time:

1. **Client settings** (`knot.*` settings in the VS Code extension;
   will become app settings in the Tauri app) — the primary user-facing
   configuration.

2. **`.knot/config.json`** (migrating from `.vscode/knot.json`) —
   project-local configuration, checked into the repo. Supports
   `compiler_path`, `storyformats_path`, `build` (source_dir,
   output_dir, flags), `diagnostics` (severity overrides), `ignore`
   (glob patterns), `max_files`, `format` (override), `special_passages`
   (user-defined).

Client settings take priority over `.knot/config.json` for the same
field. Some fields (like `build.flags`) are merged — both sets apply.

**Migration:** the path string `.vscode/knot.json` appears in 3
code-bearing sites (`lifecycle.rs:73`, `sync.rs:1265`, plus the new
Tauri-side config loader). `load_config` in `workspace.rs:506` is
already format-agnostic (takes JSON text), so the migration is a
3-site path swap plus an auto-migrate shim that writes a `.bak`
backup on first open.

---

## [legacy] Build Output

> The Tauri app will reproduce this in its own build output panel.

When tweego runs, the server streams output to the client's build
output panel. The server prepends diagnostic lines showing the
resolution decisions:

```
Knot: Tweego binary: /home/user/.knot/tweego/tweego
Knot: Compiling source from: /home/user/project
Knot: Story formats: using managed cache at .../sugarcube-2@2.37.0
Knot: Story formats search path = .../sugarcube-2@2.37.0
<tweego stdout>
Knot: Build stats — 42 passages, 12345 words
```

This makes it easy to debug build failures — every resolution step
is visible.

---

## Platform Support

Knot supports Windows, macOS, and Linux on x64 and arm64. Min OS:
Windows 10 1903+, macOS 11+ (Big Sur), Ubuntu 22.04+ / Fedora 36+.

The Tauri app will bundle the `knot-server` binary for each platform.
Tweego is downloaded on first build into the app's global storage, so
users do not need to install it manually.

Path handling is cross-platform: the server uses `PathBuf` for all
path manipulation, `cfg!(windows)` for platform-specific behavior
(like the `.exe` suffix), and `to_file_path()`/`to_string_lossy()`
for URI conversion. The `force_relative` helper strips Windows drive
prefixes and leading separators so settings like `sourceDir` work
consistently across platforms.
