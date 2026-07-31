# Knot — Conventions

Project-wide rules for task delivery and code modularity. These are enforced on
every change.

---

## 1. Task delivery protocol

Every task that modifies files in this repo must end with two artifacts:

### 1.1 Modified-files zip

Create a zip archive of **every file modified or created** during the task,
preserving the directory hierarchy **relative to the repo root** (`knot-ide/`).

- **Location:** `/home/z/my-project/download/`
- **Naming:** `knot-ide-<YYYY-MM-DD>-<short-task-slug>.zip`
  - Example: `knot-ide-2026-08-01-plan-revision-and-vscode-purge.zip`
- **Internal paths:** relative to repo root. A file at
  `knot-ide/scripts/build.sh` is stored in the zip as `scripts/build.sh`.
  Do **not** include the `knot-ide/` prefix inside the zip.
- **Contents:** only files that were modified or created in this task. Do not
  bundle unrelated files, build artifacts, or the entire repo.
- **Method:** use the `zip` command from the repo root with explicit file
  arguments — never `zip -r .` (that captures everything).

Example:
```bash
cd /home/z/my-project/knot-ide
zip /home/z/my-project/download/knot-ide-<date>-<slug>.zip \
  PLAN.md \
  ARCHITECTURE.md \
  scripts/build.sh \
  scripts/dev.sh \
  scripts/package.sh
```

### 1.2 Removed-files list

List every file removed during the task, with paths **relative to the repo
root** (same convention as the zip internals — no `knot-ide/` prefix).

Output the list in the task summary message, one path per line, under a
clear heading. Do not zip deletions — a text list is sufficient.

Example:
```
Removed files:
  extensions/vscode/src/extension.ts
  extensions/vscode/package.json
  media/supporters/.gitkeep
```

### 1.3 When to apply

This protocol applies to **every task that touches files in `knot-ide/`**,
including:
- Code changes (Rust, TypeScript, Svelte, scripts)
- Documentation changes (`.md` files)
- Config changes (`Cargo.toml`, `package.json`, etc.)
- File deletions or moves

It does **not** apply to:
- Read-only research / exploration tasks
- Conversation-only answers (no file changes)
- Changes outside `knot-ide/` (e.g. scripts under `/home/z/my-project/scripts/`)

### 1.4 Source of truth

Use `git status --short` to determine the modified and removed file set. The
zip and the removed-files list together must account for every change `git
status` reports.

---

## 2. Code modularity rules

Knot has a strict separation-of-concerns policy. Logic bleed and spaghetti code
are treated as bugs. The rules below are mandatory.

### 2.1 Crate boundaries (Rust workspace)

The three crates have fixed responsibilities. **Never move logic across these
boundaries** to "just make it work."

| Crate | Owns | Does NOT own |
|---|---|---|
| `knot-core` | Format-agnostic data model (`Workspace`, `Document`, `Passage`, `Block`, `Graph`, `Analysis`, `Editing`), config loading (JSON parse only), workspace graph operations | LSP types, client communication, format-specific syntax, file I/O beyond config read |
| `knot-formats` | Per-format parsers, semantic tokens, macro catalogs, link extraction, format-specific special passages, CSS/JS analysis pipelines | LSP wire types, workspace state mutation (produces data for core, doesn't own workspace) |
| `knot-server` | LSP request/response handling, client communication, subprocess orchestration (Tweego, future in-house compiler), file watching, build pipeline | Parsing logic, data model definitions, format-specific syntax rules |

Rules:
- `knot-core` must not depend on `knot-formats` or `knot-server`.
- `knot-formats` may depend on `knot-core` but not `knot-server`.
- `knot-server` may depend on both.
- No circular dependencies at any level.
- If you need a type from another crate, re-export it through the crate's
  `lib.rs` — do not reach into internal modules from outside.

### 2.2 Format plugin boundaries

Each format plugin (`sugarcube/`, `harlowe/`, `chapbook/`, `snowman/`) owns its
syntax completely.

- **No SugarCube-specific logic in `knot-server` or `knot-core`.** If the
  server needs to know something format-specific, it asks the `FormatPlugin`
  trait.
- **No cross-format imports.** `harlowe/mod.rs` must not import from
  `sugarcube/`. Shared utilities go in `knot-formats` root or `twine_core.rs`.
- **Each plugin produces the same `Document`/`Passage`/`Block` types** defined
  in `knot-core`. Plugins do not define their own passage types.
- The `FormatPlugin` trait is the only contract. Adding a method to it requires
  implementing it in all four plugins — do not add default stubs that silently
  no-op.

### 2.3 Module structure (within a crate)

Each module has a single responsibility. The existing structure is the
template:

- `crates/formats/src/sugarcube/parser/` — parsing only (AST construction)
- `crates/formats/src/sugarcube/macros/` — macro catalog and lookup (data)
- `crates/formats/src/sugarcube/registries/` — variable/function/template
  registries (analysis state)
- `crates/formats/src/sugarcube/graph/` — graph derivation from parsed passages
- `crates/formats/src/sugarcube/js/` — JavaScript analysis pipeline
- `crates/formats/src/sugarcube/lsp/` — LSP-specific token builders and
  pipeline logging

Rules:
- A module that does parsing must not do I/O.
- A module that does I/O must not do parsing.
- A module that owns data (registries, state) must not do rendering or wire
  protocol.
- Helpers (`helpers/` in `knot-server`) are pure functions — no state, no
  side effects beyond their arguments.

### 2.4 Tauri app boundaries (future, `app/`)

When the Tauri app is scaffolded, it follows the same principle:

- **Frontend (`app/src/`, Svelte 5)** — UI only. Never parses twee files, never
  calls Tweego directly, never touches the filesystem except through Tauri
  `invoke` commands.
- **Tauri backend (`app/src-tauri/`, Rust)** — process supervision, file I/O,
  LSP bridge, compiler runner, asset manager. Does not render UI.
- **`knot-server`** — unchanged, runs as subprocess.
- **`knot-core` / `knot-formats`** — reused by the Tauri backend for asset
  reference rewriting and any in-process logic that doesn't need the full LSP.

The frontend talks to the Tauri backend via `invoke` / `listen` only. The
Tauri backend talks to `knot-server` via LSP over stdin/stdout. No layer
skips.

### 2.5 Function and file size

- No file should exceed ~800 lines without a structural justification (e.g. a
  large catalog of static data). If a file grows past this, split it by
  responsibility.
- No function should exceed ~80 lines. Longer functions must be decomposed.
- No function should take more than ~6 parameters. If it does, group related
  params into a struct.
- A function does one thing. If its name needs "and", it does two things —
  split it.

### 2.6 Naming and visibility

- Public APIs (`pub`) must have doc comments.
- Internal helpers are `pub(crate)` or private. Do not `pub` everything.
- Module-private types stay private; expose only through accessor methods or
  the module's `mod.rs`.
- No `unsafe` without a safety comment explaining the invariant.

### 2.7 No hidden state

- No module-level `static mut`. If you need shared mutable state, use a
  struct field owned by `ServerState` (or the Tauri backend's app state).
- No global singletons beyond the process supervisor's single `knot-server`
  handle.
- Side effects (file writes, subprocess spawns, network) must be visible in
  the function signature — either via a typed handle or an explicit
  dependency injection. No "magic" filesystem or process access from deep
  inside a parser.

### 2.8 Review checklist

Before marking a change complete, verify:

- [ ] No logic crossed a crate boundary (§2.1)
- [ ] No format-specific logic leaked into server or core (§2.2)
- [ ] Each modified module still has a single responsibility (§2.3)
- [ ] No file exceeds ~800 lines without justification (§2.5)
- [ ] New public APIs have doc comments (§2.6)
- [ ] No new hidden state or globals (§2.7)
- [ ] `cargo check` passes (§2.1 — boundary violations usually fail to compile)
- [ ] Modified-files zip + removed-files list produced (§1)
