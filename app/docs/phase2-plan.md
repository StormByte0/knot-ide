# Phase 2 — Editor Layer Plan

**Status:** Implemented. Editor layer deliverable from PLAN.md §8: "TextMate grammars, language config, decorations, multi-format support."

**Overarching plan:** `PLAN.md` (project root, §8). This document tracks Phase 2 execution.

---

## 1. Goals

Provide syntax highlighting for Twee files using the LSP server's semantic token provider as the sole source of truth. No TextMate grammars — the server produces rich semantic tokens (30 token types, 19 modifiers) that cover everything a grammar would handle, plus dynamic information grammars can't (deprecated macros, block-depth coloring, custom widgets, variable definitions vs reads).

## 2. Architecture decision — semantic tokens only, no TextMate

**Decision:** Drop TextMate grammars entirely. Use the LSP server's semantic token provider as the single source of truth for syntax highlighting.

**Rationale:**
- The LSP server (`crates/formats/src/sugarcube/lsp/token_builder.rs`) already produces comprehensive semantic tokens covering passage headers, links, macros, variables, embedded JS/CSS regions, block markup, and more.
- Semantic tokens provide *dynamic* information that TextMate grammars can't: deprecated macros (strikethrough), block-depth coloring (rainbow delimiters), variable definitions vs reads (bold vs italic), custom widgets.
- Using both would cause double-coloring conflicts — the grammar's static tokens would clash with the server's semantic tokens.
- Semantic tokens are the cleaner architecture: one source of truth, consistent coloring, no regex maintenance.

**Trade-off:** Files show no highlighting until the LSP indexes them (~100-500ms after open). For SugarCube (the only format with a real LSP), this is acceptable. The other 3 formats (Harlowe, Chapbook, Snowman) are stubs — they get plain text until their LSPs are implemented in a future phase.

## 3. Implementation

### 3.1 Single language ID

All `.tw`/`.twee` files use the `twee` language regardless of story format. The LSP server detects the format from the workspace config, not from the language ID. This avoids the complexity of multiple language IDs + grammar switching.

### 3.2 Language configuration

A single inline language configuration in `monaco-init.ts` provides:
- Line comment: `::%`
- Brackets: `[[ ]]`, `<< >>`, `{}`, `()`
- Auto-closing pairs: same as brackets plus `"` and `'`

SugarCube's `<<` `>>` macro delimiters are included since SugarCube is the only format with a real LSP. The other 3 formats are stubs and don't need format-specific bracket config yet.

### 3.3 Semantic token colors

The theme's `semanticTokenColors` map (in `themes.ts`) maps the LSP's 30 token types + modifier combinations to colors. 47 rules covering:
- Passage structure: `passageHeader`, `passageName`, `specialPassageHeader`, `specialPassage`, `tag` (+ `twineCore`/`storyFormat` modifiers)
- Code constructs: `macro` (+ `controlFlow`/`deprecated`), `function` (+ `definition`/`deprecated`), `variable` (+ `definition`/`readonly`), `keyword`, `string`, `number`, `boolean`, `operator`, `comment`
- Object model: `namespace`, `property` (+ `definition`)
- Narrative content: `prose`, `inlineStyle`, `textFormat`
- Macro delimiters: `macroDelimiter` (+ `controlFlow`/`deprecated`/`blockDepth1`-`blockDepth6`)
- Block markup: `heading`, `horizontalRule`, `listMarker`, `blockquote`, `blockquoteBlock`, `table`, `codeBlock`, `inlineCode`

`applyTheme.ts` passes `semanticTokenColors` to Monaco's `defineTheme` — the theme service override processes it and applies the colors to the LSP's semantic token stream.

### 3.4 Format support

- **SugarCube:** Full LSP support (semantic tokens, completion, hover, diagnostics, go-to-definition, formatting). The primary supported format.
- **Harlowe / Chapbook / Snowman:** Stubs. The LSP server has basic parsers for these formats but no semantic token builders — they emit a small subset of tokens inline (passage headers, variables, macros). Full support lands in a future phase per `ROADMAP.md`.

### 3.5 Project settings store

A reactive `projectSettingsStore.svelte.ts` holds the workspace's story format + build config (loaded from `.knot/config.json`). The Settings dialog reads/writes this store directly. While the story format no longer drives grammar switching (single `twee` language), the store is still used for:
- Build configuration (output dir, format, Tweego flags) — Phase 7
- Future LSP feature selection (when Harlowe/Chapbook/Snowman get real LSPs)

## 4. What was reverted

The initial Phase 2 implementation used TextMate grammars (5 grammars recovered from git history, 5 language IDs, grammar switching based on story format). This was reverted per the user's decision to use semantic tokens exclusively. The reverted files:
- `app/src/lib/editor/grammars/` (5 `.tmLanguage.json` files) — deleted
- `app/src/lib/editor/language-configs/` (5 `.language-configuration.json` files) — deleted
- `app/src/lib/editor/formatRegistry.ts` — deleted
- `monaco-init.ts` grammar registration — removed
- `Editor.svelte` format-based `setModelLanguage` + `$effect` — removed
- `client.ts` multi-language `documentSelector` — reverted to single `twee`

## 5. Testing checklist

- [ ] `svelte-check` reports 0 errors
- [ ] `vite build` succeeds
- [ ] Open a SugarCube project → semantic tokens color the code (passage headers, macros, variables, links) once the LSP indexes
- [ ] Harlowe/Chapbook/Snowman files open as plain text (no LSP semantic tokens) — acceptable for stubs
- [ ] Theme switching applies semantic token colors correctly
