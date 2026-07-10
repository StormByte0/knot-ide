//! SugarCube passage formatter (Phase 10 — the primary payoff of the zoning engine).
//!
//! Produces properly-indented, normalized SugarCube code from a passage's
//! [`ZoneMap`]. The formatter is the consumer that justifies the entire
//! zoning effort: it converts the byte-covering leaf classification into
//! indentation, line breaks, and whitespace normalization.
//!
//! ## Algorithm
//!
//! The formatter iterates `zones.iter_leaves()` in source order. For each
//! leaf, it emits the leaf's text (sliced from the original body) with
//! appropriate indentation:
//!
//! - **Prose / Markup**: emit text, indenting each line to the current depth.
//!   Internal newlines are preserved; blank lines are kept (not collapsed).
//! - **MacroTag (Open)**: if the macro has a body (block macro), put the open
//!   tag on its own line and increment the indent for subsequent body content.
//!   Inline macros (`<<set>>`, `<<print>>`) stay inline with surrounding prose.
//! - **MacroTag (Close)**: decrement indent, emit `<</name>>` on its own line.
//! - **MacroTag (Expression)**: `<<=>>expr>>` / `<<->>expr>>` — keep inline,
//!   normalize internal whitespace.
//! - **Raw**: emit as-is (JS/CSS sub-formatters are a follow-up).
//! - **Error**: refuse to format — return `None`. The user fixes the error
//!   first; formatting broken input would produce misleading output.
//!
//! ## Idempotency
//!
//! The formatter is idempotent: `format_passage(format_passage(text)) ==
//! format_passage(text)`. This is verified by a property test.
//!
//! ## Coordinate system
//!
//! All zone spans are passage-relative (0 = passage head `::`). The formatter
//! receives the body text (the content after the `:: Name` header line) and
//! the zone map. The caller is responsible for slicing the body text out of
//! the full document and for re-assembling the formatted body with the header.
//!
//! ## Out of scope
//!
//! - JS sub-formatter (oxc-formatter / dprint) — `Raw { Js }` is emitted as-is.
//! - CSS sub-formatter — `Raw { Css }` is emitted as-is.
//! - Configuration (indent style, line width) — uses tabs and sensible defaults.

use std::ops::Range;

use knot_core::zoning::{LeafKind, LeafZone, TagPart, ZoneMap};

/// The indentation unit: one tab per depth level.
const INDENT_UNIT: &str = "\t";

/// Format a SugarCube passage body.
///
/// Returns `None` if the zone map contains any `Error` leaves (the formatter
/// refuses to format broken input — the user fixes the error first). Returns
/// `Some(formatted_text)` otherwise.
///
/// `body_text` is the passage body (content after the `:: Name` header line).
/// `zones` is the zone map built from the same body (spans are passage-relative,
/// matching the body text starting at offset 0 — i.e., `body_offset_in_passage`
/// has already been applied by the caller, OR the zones were built with
/// `body_offset_in_passage = 0`).
///
/// **Note on offsets**: The zone map's spans are passage-relative (0 = passage
/// head `::`). The body text starts after the header line. If the caller passes
/// the body text starting at the first byte after the header newline, and the
/// zones were built with `body_offset_in_passage` matching that offset, then
/// the zone spans are already relative to the body text start. This is the
/// convention used by `ClassifiedPassage.body_text` and the production parse
/// pipeline. The formatter slices `body_text[leaf.span.start..leaf.span.end]`
/// directly, so the caller must ensure the body text and zone spans are
/// aligned.
pub fn format_passage(body_text: &str, zones: &ZoneMap) -> Option<String> {
    // Refuse to format if there are any Error leaves — the user should fix
    // errors first. Formatting broken input would produce misleading output.
    for leaf in zones.iter_leaves() {
        if matches!(leaf.kind, LeafKind::Error { .. }) {
            return None;
        }
    }

    // Collect leaves into a Vec for lookahead. We need to detect empty-body
    // macros (open tag immediately followed by its matching close tag with no
    // content between them) so we can keep them inline instead of putting the
    // close tag on a new indented line.
    let leaves: Vec<&LeafZone> = zones.iter_leaves().collect();

    let mut out = String::with_capacity(body_text.len() + 256);
    let mut at_line_start = true;
    // When true, skip whitespace-only leaves until we hit the matching close
    // tag. Set after emitting a forced-inline open tag to suppress the
    // newline/whitespace between `<<link>>` and `<</link>>`.
    let mut skipping_whitespace_for_empty_body: Option<String> = None;
    // When true, the next Prose leaf should have its leading spaces stripped
    // (because a markup marker leaf just emitted a single space separator).
    let mut strip_next_prose_leading_spaces = false;

    for (i, leaf) in leaves.iter().enumerate() {
        // Compute the indentation depth for this leaf.
        //
        // The zone map's `MacroBody.depth` is the depth of the BODY itself
        // (0 at top level). A leaf INSIDE that body (whether prose, markup,
        // or a nested macro tag) should be indented one level deeper than
        // the body, so we use `body.depth + 1`.
        //
        // **Exception**: SubMacros (`<<else>>`, `<<case>>`, `<<default>>`)
        // are structural markers that should be at the PARENT's depth (the
        // body's depth), not one level deeper. They open on their own line
        // at the parent's indentation per the formatter spec.
        //
        // For leaves at the top level (body_idx=None), depth is 0.
        //
        // Example: `<<if $x>>\nhello\n<<else>>\nno\n<</if>>` at top level:
        // - `<<if $x>>` open tag: body_idx=None → depth 0 ✓
        // - `hello` prose: body_idx=Some(0), bodies[0].depth=0 → depth 1 ✓
        // - `<<else>>` SubMacro: body_idx=Some(0), but it's a SubMacro → depth 0 (parent's level) ✓
        // - `no` prose: body_idx=Some(0), bodies[0].depth=0 → depth 1 ✓
        // - `<</if>>` close tag: body_idx=None → depth 0 ✓
        let current_depth: u32 = match leaf.body_idx {
            None => 0,
            Some(idx) => {
                // Compute the effective depth by walking up the parent chain.
                // If this body is spurious (unknown/inline macro), the content
                // should be indented at the NEAREST REAL parent's depth, not 0.
                let effective = zones.effective_depth(idx);

                // SubMacros with `body: Never` (like <<else>>, <<break>>,
                // <<continue>>) are structural markers that go at the parent
                // body's depth (not +1). They don't open their own body.
                let is_never_submacro = matches!(
                    &leaf.kind,
                    LeafKind::MacroTag {
                        macro_kind: Some(knot_core::types::MacroKind::SubMacro),
                        body_requirement: Some(knot_core::types::BodyRequirement::Never),
                        ..
                    }
                );
                if is_never_submacro {
                    effective.saturating_sub(1)
                } else {
                    effective
                }
            }
        };

        let raw_text = slice_span(body_text, &leaf.span);

        // If the previous leaf was a markup marker that inserted a space,
        // strip leading spaces from this Prose leaf so we get exactly one
        // space between the marker and the content.
        let text = if strip_next_prose_leading_spaces && matches!(leaf.kind, LeafKind::Prose { .. }) {
            strip_next_prose_leading_spaces = false;
            raw_text.trim_start_matches(' ')
        } else {
            strip_next_prose_leading_spaces = false;
            raw_text
        };

        // Detect empty-body macros: if this is an open tag for a block macro
        // and the next non-whitespace leaf is its matching close tag (no
        // content between them), keep BOTH the open and close tags inline.
        // This avoids unnecessary newlines between `<<link>><</link>>`.
        //
        // For open tags: scan forward to see if the body is empty.
        // For close tags: check if the previous leaf was the matching open
        // tag (which was already flagged as force_inline).
        let force_inline = is_empty_body_open(leaf, &leaves, i, body_text)
            || is_matching_close_of_empty_body(leaf, &leaves, i, body_text);

        // If we're skipping whitespace (inside an empty body) and this leaf
        // is the matching close tag, stop skipping and emit the close tag.
        // If it's a whitespace-only Prose/Markup leaf, skip it entirely.
        if let Some(ref open_name) = skipping_whitespace_for_empty_body {
            if let LeafKind::MacroTag {
                part: TagPart::Close,
                macro_name,
                ..
            } = &leaf.kind
            {
                if macro_name == open_name {
                    // This is the matching close tag — stop skipping, emit it.
                    skipping_whitespace_for_empty_body = None;
                }
            } else {
                // Not a close tag. If it's a whitespace-only Prose/Markup
                // leaf, skip it. Otherwise, stop skipping (shouldn't happen
                // — is_empty_body_open already verified no content).
                let is_ws_only = matches!(&leaf.kind, LeafKind::Prose { .. } | LeafKind::Markup(_))
                    && text.chars().all(|c| c == '\t' || c == ' ' || c == '\n' || c == '\r');
                if is_ws_only {
                    continue;
                }
                // Non-whitespace content — stop skipping (defensive).
                skipping_whitespace_for_empty_body = None;
            }
        }

        // After emitting a forced-inline open tag, start skipping whitespace
        // until we hit the matching close tag.
        let was_force_inline_open = force_inline
            && matches!(&leaf.kind, LeafKind::MacroTag { part: TagPart::Open, .. });

        emit_leaf(
            &mut out,
            text,
            &leaf.kind,
            current_depth,
            &mut at_line_start,
            force_inline,
        );

        // After emitting a forced-inline open tag, start skipping whitespace
        // leaves until we hit the matching close tag. This collapses
        // `<<link>>\n<</link>>` to `<<link>><</link>>`.
        if was_force_inline_open
            && let LeafKind::MacroTag { macro_name, .. } = &leaf.kind {
                skipping_whitespace_for_empty_body = Some(macro_name.clone());
            }

        // Zone-specific: after emitting a markup marker leaf (Heading, ListItem,
        // Blockquote), normalize the space between the marker and the following
        // Prose content. The zone builder splits `!heading` into Markup(`!`) +
        // Prose(`heading`), and `!!  heading` into Markup(`!!`) + Prose(`  heading`).
        // We want exactly one space: `! heading` and `!! heading`.
        if let LeafKind::Markup(markup_kind) = &leaf.kind
            && matches!(
                markup_kind,
                knot_core::zoning::MarkupKind::Heading
                    | knot_core::zoning::MarkupKind::ListItem
                    | knot_core::zoning::MarkupKind::Blockquote
            ) {
                // Peek at the next leaf. If it's Prose, we need to ensure exactly
                // one space between the marker and the content. The Prose leaf
                // may start with 0, 1, or more spaces — we normalize to exactly 1.
                // We do this by pushing a space now and setting a flag that tells
                // the next emit_text to strip leading spaces.
                if let Some(next) = leaves.get(i + 1)
                    && matches!(next.kind, LeafKind::Prose { .. }) {
                        let next_text = slice_span(body_text, &next.span);
                        // Only insert a space if the prose doesn't start with a newline
                        // (newlines mean the content is on the next line, no space needed).
                        if !next_text.starts_with('\n') && !next_text.is_empty() {
                            out.push(' ');
                            // Mark that the next Prose leaf should have leading
                            // spaces stripped (handled in the next iteration).
                            strip_next_prose_leading_spaces = true;
                        }
                    }
            }
    }

    // Trim trailing whitespace from the final output (no trailing newline
    // artifacts). The formatter preserves internal blank lines but should
    // not add a trailing one.
    trim_trailing_newlines(&mut out);

    Some(out)
}

/// Extension method on `ZoneMap` to get the depth of a body by index.
///
/// This is a free function (not a method on `ZoneMap`) because `ZoneMap`
/// lives in `knot-core` and we don't want to add formatter-specific helpers
/// there. The formatter accesses `bodies` via `iter_bodies()` and looks up
/// depth by matching the body's span.
trait ZoneMapExt {
    /// Compute the effective depth for a leaf inside body `idx`.
    ///
    /// If the body is spurious (inline/unknown macro), walk up the parent
    /// chain until we find a REAL body (or reach top level). This ensures
    /// content inside spurious bodies is indented at the correct depth
    /// relative to the nearest real enclosing block macro.
    ///
    /// Example: `<<link>>` (real, depth 0) → `<<adjustStat>>` (spurious,
    /// depth 1) → content. The content should be at depth 1 (inside link),
    /// NOT depth 0. Without this walk, the spurious `<<adjustStat>>` body
    /// would mask the real `<<link>>` parent, causing wrong indentation.
    fn effective_depth(&self, idx: usize) -> u32;
}

impl ZoneMapExt for ZoneMap {
    fn effective_depth(&self, idx: usize) -> u32 {
        let mut current_idx = idx;
        loop {
            let Some(body) = self.iter_bodies().nth(current_idx) else {
                return 0;
            };
            let is_spurious = match body.body_requirement {
                Some(knot_core::types::BodyRequirement::Never) => true,
                None => true,
                Some(knot_core::types::BodyRequirement::Optional) => false,
                Some(knot_core::types::BodyRequirement::Required) => false,
            };
            if !is_spurious {
                // Real body — content is one level deeper.
                return body.depth + 1;
            }
            // Spurious body — walk up to the parent.
            match body.parent_body {
                Some(parent_idx) => current_idx = parent_idx,
                None => return 0,
            }
        }
    }
}

/// Slice a span from the body text, clamping to avoid panics on malformed input.
fn slice_span<'a>(body: &'a str, span: &Range<usize>) -> &'a str {
    let start = span.start.min(body.len());
    let end = span.end.max(start).min(body.len());
    &body[start..end]
}

/// Check if this leaf is an open tag for a block macro whose body is empty
/// (no non-whitespace content between the open and close tags). When true,
/// the formatter forces inline emission for both the open and close tags,
/// avoiding unnecessary newlines and indentation between `<<link>><</link>>`.
///
/// This scans forward from the open tag, skipping whitespace-only Prose/Markup
/// leaves (a bare `\n` between `<<link>>` and `<</link>>` is NOT content — it's
/// just a newline that should be collapsed). If we find the matching close tag
/// before any non-whitespace content, the body is empty.
fn is_empty_body_open(leaf: &LeafZone, leaves: &[&LeafZone], i: usize, body_text: &str) -> bool {
    // Must be an open tag.
    let LeafKind::MacroTag {
        part: TagPart::Open,
        macro_name: open_name,
        macro_kind,
        body_requirement,
        ..
    } = &leaf.kind
    else {
        return false;
    };

    // Only block macros qualify (Container or Required/Optional body).
    // Inline macros (Never body) are already inline — no need to force.
    use knot_core::types::{BodyRequirement, MacroKind};
    let is_block = matches!(macro_kind, Some(MacroKind::Container | MacroKind::SubMacro))
        || matches!(body_requirement, Some(BodyRequirement::Required | BodyRequirement::Optional));
    if !is_block {
        return false;
    }

    // Scan forward through subsequent leaves. Skip whitespace-only Prose/Markup
    // leaves (a bare newline is whitespace, not content). If we find the
    // matching close tag before any non-whitespace content, the body is empty.
    let mut j = i + 1;
    while let Some(next) = leaves.get(j) {
        match &next.kind {
            LeafKind::MacroTag {
                part: TagPart::Close,
                macro_name,
                ..
            } => {
                return macro_name == open_name;
            }
            LeafKind::Prose { .. } | LeafKind::Markup(_) => {
                // Check the actual text content. If it's all whitespace
                // (newlines, tabs, spaces), skip it — it's not real content,
                // just formatting noise between the tags.
                let text = slice_span(body_text, &next.span);
                if text.chars().any(|c| c != '\t' && c != ' ' && c != '\n' && c != '\r') {
                    // Non-whitespace content found — body is not empty.
                    return false;
                }
                // Whitespace-only — skip and continue scanning.
                j += 1;
            }
            _ => {
                // Any other leaf kind (Raw, Error, another MacroTag) means
                // the body has content.
                return false;
            }
        }
    }

    // Reached the end without finding a close tag — not empty (unclosed).
    false
}

/// Check if this leaf is a close tag whose matching open tag is the
/// preceding leaf (possibly with whitespace-only leaves between them).
/// When true, the formatter forces inline emission for the close tag,
/// keeping `<<link>><</link>>` together.
fn is_matching_close_of_empty_body(
    leaf: &LeafZone,
    leaves: &[&LeafZone],
    i: usize,
    body_text: &str,
) -> bool {
    // Must be a close tag.
    let LeafKind::MacroTag {
        part: TagPart::Close,
        macro_name: close_name,
        ..
    } = &leaf.kind
    else {
        return false;
    };

    // Scan backward through preceding leaves. Skip whitespace-only Prose/Markup
    // leaves. If we find the matching open tag before any non-whitespace
    // content, this close tag is part of an empty body.
    let mut j = i;
    while let Some(prev_idx) = j.checked_sub(1) {
        j = prev_idx;
        let Some(prev) = leaves.get(prev_idx) else {
            return false;
        };
        match &prev.kind {
            LeafKind::MacroTag {
                part: TagPart::Open,
                macro_name,
                ..
            } => {
                return macro_name == close_name;
            }
            LeafKind::Prose { .. } | LeafKind::Markup(_) => {
                let text = slice_span(body_text, &prev.span);
                if text.chars().any(|c| c != '\t' && c != ' ' && c != '\n' && c != '\r') {
                    // Non-whitespace content — not an empty body.
                    return false;
                }
                // Whitespace-only — skip and continue scanning backward.
            }
            _ => {
                return false;
            }
        }
    }

    false
}

/// Emit a single leaf's text into the output buffer.
///
/// `force_inline` — when true, block-macro open/close tags are emitted inline
/// (no newline + indent). Used for empty-body macros like `<<link>><</link>>`.
fn emit_leaf(
    out: &mut String,
    text: &str,
    kind: &LeafKind,
    depth: u32,
    at_line_start: &mut bool,
    force_inline: bool,
) {
    match kind {
        LeafKind::Prose { .. } => {
            emit_text(out, text, depth, at_line_start);
        }
        LeafKind::Markup(markup_kind) => {
            // Zone-specific formatting: SugarCube markup gets normalized
            // (e.g., `!heading` → `! heading`, `*item` → `* item`).
            // Other markup is emitted as prose.
            emit_markup(out, text, *markup_kind, depth, at_line_start);
        }
        LeafKind::MacroTag { part, macro_kind, body_requirement, .. } => {
            match part {
                TagPart::Open => {
                    emit_macro_open(
                        out,
                        text,
                        depth,
                        at_line_start,
                        *macro_kind,
                        *body_requirement,
                        force_inline,
                    );
                }
                TagPart::Close => {
                    emit_macro_close(out, text, depth, at_line_start, force_inline);
                }
                TagPart::Expression => {
                    emit_macro_expression(out, text, depth, at_line_start);
                }
            }
        }
        LeafKind::Raw { .. } => {
            // Emit raw content as-is, indented to the current depth at line
            // starts. We don't normalize internal whitespace — a JS/CSS
            // sub-formatter would do that. We DO indent each line of the raw
            // block to the current depth so the raw block sits at the right
            // indentation level relative to its enclosing macro.
            emit_text(out, text, depth, at_line_start);
        }
        LeafKind::Error { .. } => {
            // Should be unreachable — format_passage() returns None early if
            // any Error leaves exist. But handle defensively: emit raw.
            emit_text(out, text, depth, at_line_start);
        }
    }
}

/// Emit markup text with SugarCube-specific normalization.
///
/// Formatting rules vary by markup kind:
/// - **Heading** (`!`, `!!`, etc.): ensure exactly one space after the `!`s.
///   `!heading` → `! heading`, `!!  heading` → `!! heading`.
/// - **ListItem** (`*`, `**`, `#`, `##`): ensure exactly one space after the marker.
///   `*item` → `* item`, `*  item` → `* item`.
/// - **Blockquote** (`>`): ensure exactly one space after `>`.
///   `>quote` → `> quote`.
/// - Other markup: emit as prose (no special normalization).
fn emit_markup(
    out: &mut String,
    text: &str,
    markup_kind: knot_core::zoning::MarkupKind,
    depth: u32,
    at_line_start: &mut bool,
) {
    use knot_core::zoning::MarkupKind;

    match markup_kind {
        MarkupKind::Heading | MarkupKind::ListItem | MarkupKind::Blockquote => {
            // Normalize the marker: ensure exactly one space after the marker
            // prefix (!, !!, *, **, #, ##, >, >>).
            let normalized = normalize_markup_marker(text, markup_kind);
            emit_text(out, &normalized, depth, at_line_start);
        }
        _ => {
            // Other markup (InlineStyle, TextFormat, Link, Comment, CodeBlock,
            // InlineCode, Verbatim, Table, HorizontalRule) — emit as prose.
            emit_text(out, text, depth, at_line_start);
        }
    }
}

/// Normalize a markup marker line: ensure exactly one space after the marker.
///
/// - `!heading` → `! heading`
/// - `!!  heading` → `!! heading`
/// - `*item` → `* item`
/// - `>quote` → `> quote`
///
/// Only normalizes the FIRST line if it starts with a marker. Subsequent lines
/// (after newlines) are left as-is — they may be continuation lines that don't
/// have a marker.
fn normalize_markup_marker(text: &str, kind: knot_core::zoning::MarkupKind) -> String {
    use knot_core::zoning::MarkupKind;

    // Determine the marker characters for this markup kind.
    let marker_chars: &[char] = match kind {
        MarkupKind::Heading => &['!'],
        MarkupKind::ListItem => &['*', '#'],
        MarkupKind::Blockquote => &['>'],
        _ => return text.to_string(),
    };

    // Split into lines, normalize only the first line (which has the marker).
    let mut lines = text.split_inclusive('\n');
    let first = lines.next();

    let mut result = String::with_capacity(text.len());

    if let Some(first_line) = first {
        let is_newline_terminated = first_line.ends_with('\n');
        let content = if is_newline_terminated {
            &first_line[..first_line.len() - 1]
        } else {
            first_line
        };

        // Count leading marker chars.
        let marker_count = content
            .chars()
            .take_while(|c| marker_chars.contains(c))
            .count();

        if marker_count > 0 {
            // Extract the marker and the rest.
            let marker = &content[..marker_count];
            let rest = content[marker_count..].trim_start_matches(' ');
            // Reassemble: marker + " " + rest (if rest is non-empty).
            if rest.is_empty() {
                result.push_str(marker);
            } else {
                result.push_str(marker);
                result.push(' ');
                result.push_str(rest);
            }
        } else {
            // No marker found (shouldn't happen for well-formed markup, but
            // handle defensively) — emit as-is.
            result.push_str(content);
        }

        if is_newline_terminated {
            result.push('\n');
        }
    }

    // Emit remaining lines as-is.
    for line in lines {
        result.push_str(line);
    }

    result
}

/// Emit prose/markup text, indenting each line to `depth`.
///
/// Formatting rules (tighter inside macro bodies):
/// - **Inside macro bodies (depth > 0)**: blank lines are suppressed entirely.
///   The body content is packed tightly — no blank lines between open tag and
///   content, no blank lines between content items, no blank lines before the
///   close tag. This produces neat, compact macro bodies.
/// - **At top level (depth == 0)**: blank lines are preserved but collapsed
///   to at most one consecutive blank line. This preserves visual separation
///   between top-level elements without creating random gaps.
///
/// Normalizes leading whitespace on each line (strips existing indentation,
/// then re-applies the formatter's indentation) — this ensures idempotency.
fn emit_text(out: &mut String, text: &str, depth: u32, at_line_start: &mut bool) {
    if text.is_empty() {
        return;
    }

    // Normalize \r\n → \n (and lone \r → \n) so all newline handling below
    // only deals with \n. This follows the same CRLF-awareness pattern used
    // throughout the codebase (see twine_core.rs, snowman/mod.rs,
    // chapbook/mod.rs, sugarcube/lexer.rs). Without this, \r characters
    // survive the leading-newline trim and blank-line detection on Windows
    // (CRLF) files, producing extra tab-only lines inside macro bodies.
    let owned_normalized;
    let mut text = if text.contains('\r') {
        owned_normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        owned_normalized.as_str()
    } else {
        text
    };

    // Trim leading newlines if we're already at a line start.
    // - Inside macro bodies (depth > 0): trim ALL leading newlines — no blank
    //   lines between open tag and content.
    // - At top level (depth == 0): trim all but one newline — preserve ONE
    //   blank line between top-level elements (visual separation).
    if *at_line_start {
        if depth > 0 {
            // Inside a macro body: remove ALL leading newlines.
            while text.starts_with('\n') {
                text = &text[1..];
            }
        } else {
            // Top level: preserve exactly one blank line (one newline after
            // the current line's newline = one blank line). Remove extras.
            // The previous element already ended with \n, so:
            // - 0 leading newlines → content follows immediately (no blank line)
            // - 1+ leading newlines → keep one (one blank line), remove extras
            if text.starts_with('\n') {
                // Keep one newline (one blank line), remove the rest.
                text = &text[1..];
                while text.starts_with('\n') {
                    text = &text[1..];
                }
                // Emit the blank line now.
                out.push('\n');
            }
        }
    }

    if text.is_empty() {
        return;
    }

    // Inside macro bodies, trim trailing whitespace-only lines (the blank
    // lines before a close tag). We strip trailing whitespace+newlines and
    // re-attach a single newline so the content is properly terminated.
    let owned_text;
    if depth > 0 && text.ends_with('\n') {
        let trimmed = text.trim_end_matches(['\t', ' ', '\n']);
        if !trimmed.is_empty() && trimmed != text {
            owned_text = format!("{}\n", trimmed);
            text = &owned_text;
        }
    }

    let indent = indent_str(depth);
    let mut first = true;
    let mut last_was_blank = false;
    for line in text.split_inclusive('\n') {
        let is_newline_terminated = line.ends_with('\n');
        let line_content = if is_newline_terminated {
            &line[..line.len() - 1]
        } else {
            line
        };

        // Determine if this line starts at a line boundary (after a newline).
        let at_boundary = if first { *at_line_start } else { true };

        let normalized = if at_boundary {
            line_content.trim_start_matches(['\t', ' '])
        } else {
            line_content
        };
        let is_blank = at_boundary && normalized.is_empty();

        if is_blank {
            // Blank line handling:
            // - Inside macro bodies (depth > 0): suppress entirely.
            // - At top level: allow at most one consecutive blank line.
            if depth > 0 {
                // Skip blank lines inside macro bodies.
                last_was_blank = true;
                first = false;
                continue;
            }
            if last_was_blank {
                // Already had a blank line — skip consecutive blanks.
                first = false;
                continue;
            }
            last_was_blank = true;
            // Emit the blank line (just the newline).
            if is_newline_terminated {
                out.push('\n');
            }
            first = false;
            continue;
        }

        last_was_blank = false;

        if !first || *at_line_start {
            out.push_str(&indent);
            out.push_str(normalized);
        } else {
            out.push_str(normalized);
        }

        if is_newline_terminated {
            out.push('\n');
        }

        first = false;
    }

    // If the entire text was whitespace-only (after trimming), we're still
    // at a line start.
    let all_whitespace = text.chars().all(|c| c == '\t' || c == ' ' || c == '\n');
    if all_whitespace && *at_line_start {
        return;
    }

    let trailing_newline = text.ends_with('\n')
        || text.rfind('\n').is_some_and(|pos| {
            text[pos + 1..].chars().all(|c| c == '\t' || c == ' ')
        });
    *at_line_start = trailing_newline;
}

/// Emit a macro open tag.
///
/// For block macros (those with a body) and SubMacros (`<<else>>`, `<<case>>`,
/// `<<default>>`), the open tag goes on its own line. For inline macros (no
/// body), the tag stays inline with surrounding prose.
///
/// `force_inline` — when true, emit inline even for block macros (used for
/// empty-body macros like `<<link>><</link>>`).
fn emit_macro_open(
    out: &mut String,
    text: &str,
    depth: u32,
    at_line_start: &mut bool,
    macro_kind: Option<knot_core::types::MacroKind>,
    body_requirement: Option<knot_core::types::BodyRequirement>,
    force_inline: bool,
) {
    use knot_core::types::{BodyRequirement, MacroKind};

    // A macro goes on its own line if it's a Container (has a body), a
    // SubMacro (like <<else>>, <<case>> — these are structural markers that
    // should be on their own line at the parent's depth), or has a
    // Required/Optional body requirement.
    //
    // **Exception**: if `body_requirement` is `Some(Never)`, the macro is
    // inline (e.g., a custom widget registered without `container`). Even if
    // the parser paired it with a close tag (because `lookup_body_requirement`
    // defaults to `Optional` for unknown macros during tree building), the
    // zone builder looks up the real `body_requirement` from the registry —
    // and `Never` means "no body, stay inline."
    let is_own_line = !force_inline
        && !matches!(body_requirement, Some(BodyRequirement::Never))
        && (matches!(macro_kind, Some(MacroKind::Container | MacroKind::SubMacro))
            || matches!(body_requirement, Some(BodyRequirement::Required | BodyRequirement::Optional)));

    if is_own_line {
        // Block/SubMacro open — put on its own line.
        if !*at_line_start {
            out.push('\n');
        }
        out.push_str(&indent_str(depth));
        out.push_str(&normalize_macro_tag(text));
        out.push('\n');
        *at_line_start = true;
    } else {
        // Inline macro — keep inline. Indent if at line start.
        if *at_line_start {
            out.push_str(&indent_str(depth));
        }
        out.push_str(&normalize_macro_tag(text));
        *at_line_start = false;
    }
}

/// Emit a macro close tag (`<</name>>`).
///
/// Close tags go on their own line, at the same depth as the open tag.
/// The close tag emits a trailing newline so subsequent content starts on
/// a fresh line.
///
/// `force_inline` — when true, emit inline (no newline + indent). Used for
/// empty-body macros where the close tag immediately follows the open tag.
fn emit_macro_close(
    out: &mut String,
    text: &str,
    depth: u32,
    at_line_start: &mut bool,
    force_inline: bool,
) {
    if force_inline {
        // Empty-body macro — emit close tag right after the open tag, inline.
        out.push_str(&normalize_macro_tag(text));
        *at_line_start = false;
        return;
    }

    if !*at_line_start {
        out.push('\n');
    }
    out.push_str(&indent_str(depth));
    out.push_str(&normalize_macro_tag(text));
    out.push('\n');
    *at_line_start = true;
}

/// Emit an expression macro (`<<=>>expr>>` / `<<->>expr>>`).
///
/// Expression macros stay inline with surrounding prose.
fn emit_macro_expression(out: &mut String, text: &str, depth: u32, at_line_start: &mut bool) {
    if *at_line_start {
        out.push_str(&indent_str(depth));
    }
    out.push_str(&normalize_macro_tag(text));
    *at_line_start = false;
}

/// Normalize a macro tag: trim ends and collapse multiple internal spaces
/// to a single space.
///
/// `<<adjustStat "stress"  9>>` → `<<adjustStat "stress" 9>>`
///
/// **Important**: spaces inside string literals are preserved. We track
/// single/double quote state to avoid collapsing spaces inside strings.
fn normalize_macro_tag(text: &str) -> String {
    let trimmed = text.trim();
    let mut result = String::with_capacity(trimmed.len());
    let mut prev_was_space = false;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for c in trimmed.chars() {
        match c {
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                prev_was_space = false;
                result.push(c);
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                prev_was_space = false;
                result.push(c);
            }
            ' ' | '\t' if !in_single_quote && !in_double_quote => {
                if !prev_was_space {
                    result.push(' ');
                }
                prev_was_space = true;
            }
            _ => {
                prev_was_space = false;
                result.push(c);
            }
        }
    }

    result
}

/// Produce the indentation string for a given depth.
fn indent_str(depth: u32) -> String {
    INDENT_UNIT.repeat(depth as usize)
}

/// Trim trailing newlines from the output buffer.
fn trim_trailing_newlines(s: &mut String) {
    while s.ends_with('\n') {
        s.pop();
    }
    // Also trim trailing whitespace (tabs/spaces on the last line).
    while s.ends_with(' ') || s.ends_with('\t') {
        s.pop();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sugarcube::ast::ParseMode;
    use crate::sugarcube::parser::parse_passage_body;
    use crate::sugarcube::registries::CustomMacroRegistry;
    use crate::zoning::build_from_ast;

    /// Helper: parse a body, build zones, and format.
    /// Returns None if the formatter refused (Error leaves present).
    fn format_body(body: &str) -> Option<String> {
        let ast = parse_passage_body(body, 0, ParseMode::Normal);
        let zones = build_from_ast(&ast.nodes, 0, &CustomMacroRegistry::new());
        format_passage(body, &zones)
    }

    /// Helper: parse, format, and assert the output matches a snapshot.
    /// Uses simple string comparison (not insta) to avoid snapshot file
    /// management in this phase — the formatted output is deterministic.
    fn assert_format_eq(body: &str, expected: &str) {
        let formatted = format_body(body).unwrap_or_else(|| {
            panic!("formatter refused to format (Error leaves present) for body: {:?}", body)
        });
        assert_eq!(
            formatted, expected,
            "formatter output mismatch for body:\n{:?}\n--- got ---\n{}\n--- expected ---\n{}",
            body, formatted, expected
        );
    }

    /// Pure prose is passed through with no indentation changes.
    #[test]
    fn test_pure_prose() {
        assert_format_eq("Hello world.", "Hello world.");
    }

    /// A single inline macro stays inline.
    #[test]
    fn test_inline_macro() {
        assert_format_eq("<<set $x to 1>>", "<<set $x to 1>>");
    }

    /// A block macro gets its body indented.
    #[test]
    fn test_block_macro_indent() {
        assert_format_eq(
            "<<if $x>>\nhello\n<</if>>",
            "<<if $x>>\n\thello\n<</if>>",
        );
    }

    /// Nested block macros indent at each level.
    #[test]
    fn test_nested_block_macros() {
        assert_format_eq(
            "<<if $x>>\n<<link \"Go\">>\nclick\n<</link>>\n<</if>>",
            "<<if $x>>\n\t<<link \"Go\">>\n\t\tclick\n\t<</link>>\n<</if>>",
        );
    }

    /// `<<else>>` is a SubMacro — opens on its own line at the parent's depth.
    #[test]
    fn test_if_else() {
        assert_format_eq(
            "<<if $x>>\nyes\n<<else>>\nno\n<</if>>",
            "<<if $x>>\n\tyes\n<<else>>\n\tno\n<</if>>",
        );
    }

    /// `<<switch>>` / `<<case>>` indentation.
    #[test]
    fn test_switch_case() {
        assert_format_eq(
            "<<switch $x>>\n<<case 1>>\none\n<<default>>\nother\n<</switch>>",
            "<<switch $x>>\n\t<<case 1>>\n\t\tone\n\t<<default>>\n\t\tother\n<</switch>>",
        );
    }

    /// `<<script>>` body is emitted as-is (no JS sub-formatter yet).
    #[test]
    fn test_script_raw_body() {
        let body = "<<script>>\nvar x = 1;\n<</script>>";
        let formatted = format_body(body).expect("formatter should accept <<script>>");
        // The script body should be indented one level. We don't normalize JS.
        assert!(
            formatted.contains("<<script>>"),
            "formatted output should contain <<script>>: {:?}",
            formatted
        );
        assert!(
            formatted.contains("<</script>>"),
            "formatted output should contain <</script>>: {:?}",
            formatted
        );
        assert!(
            formatted.contains("var x = 1;"),
            "formatted output should preserve JS content: {:?}",
            formatted
        );
    }

    /// Inline macros in prose stay inline.
    #[test]
    fn test_inline_macro_in_prose() {
        let formatted = format_body("You have <<print $gold>> gold.").unwrap();
        assert!(
            formatted.contains("You have <<print $gold>> gold."),
            "inline macro should stay inline: {:?}",
            formatted
        );
    }

    /// The formatter is idempotent: format(format(x)) == format(x).
    #[test]
    fn test_idempotent_simple() {
        let body = "<<if $x>>\nhello\n<</if>>";
        let once = format_body(body).unwrap();
        let twice = format_body(&once).unwrap();
        assert_eq!(once, twice, "formatter is not idempotent:\nonce: {:?}\ntwice: {:?}", once, twice);
    }

    /// Idempotency on a more complex example.
    #[test]
    fn test_idempotent_complex() {
        let body = "<<if $x>>\n<<link \"Go\">>\nclick here\n<</link>>\n<<else>>\nno link\n<</if>>";
        let once = format_body(body).unwrap();
        let twice = format_body(&once).unwrap();
        assert_eq!(once, twice, "formatter is not idempotent on complex input");
    }

    /// The formatter refuses to format passages with Error leaves.
    #[test]
    fn test_refuses_error_input() {
        // An unclosed block macro produces an Error leaf? Let's check.
        // Actually, unclosed macros produce a MacroBody with unclosed=true,
        // not an Error leaf. Error leaves come from orphan close tags and
        // parse errors. Let's use an orphan close tag.
        let body = "<</if>>";
        let result = format_body(body);
        // An orphan close tag produces an Error leaf (OrphanClose kind).
        // The formatter should refuse.
        assert!(
            result.is_none(),
            "formatter should refuse to format input with Error leaves, got: {:?}",
            result
        );
    }

    /// Multiple blank lines between top-level block macros are collapsed to
    /// a single newline (no blank line). This keeps the output tight.
    #[test]
    fn test_collapse_blank_lines_between_blocks() {
        let body = "<<if $x>>\nhello\n<</if>>\n\n\n\n<<if $y>>\nworld\n<</if>>";
        let formatted = format_body(body).unwrap();
        // Should NOT contain multiple consecutive blank lines.
        assert!(
            !formatted.contains("\n\n\n"),
            "multiple blank lines should be collapsed: {:?}",
            formatted
        );
    }

    /// Markup (headings, lists) is preserved with indentation.
    #[test]
    fn test_markup_preserved() {
        let body = "! Heading\n* list item";
        let formatted = format_body(body).unwrap();
        assert!(
            formatted.contains("! Heading"),
            "heading should be preserved: {:?}",
            formatted
        );
        assert!(
            formatted.contains("* list item"),
            "list item should be preserved: {:?}",
            formatted
        );
    }

    // ===================================================================
    // Phase 10 bugfix tests — empty-body macros, close-tag indentation
    // ===================================================================

    /// An empty-body block macro (`<<link>><</link>>`) stays inline — no
    /// newline or indentation between the open and close tags.
    #[test]
    fn test_empty_body_macro_stays_inline() {
        assert_format_eq("<<link>><</link>>", "<<link>><</link>>");
    }

    /// An empty-body macro with arguments stays inline.
    #[test]
    fn test_empty_body_macro_with_args_stays_inline() {
        assert_format_eq(
            "<<link \"Go\">><</link>>",
            "<<link \"Go\">><</link>>",
        );
    }

    /// An empty-body macro in prose stays inline with the surrounding text.
    #[test]
    fn test_empty_body_macro_in_prose() {
        let body = "Click <<link \"here\">><</link>> to continue.";
        let formatted = format_body(body).unwrap();
        assert!(
            formatted.contains("Click <<link \"here\">><</link>> to continue."),
            "empty-body macro should stay inline in prose: {:?}",
            formatted
        );
    }

    /// A block macro with content gets the close tag at the open tag's depth
    /// (one less than the body content). This verifies the -1 indentation.
    #[test]
    fn test_close_tag_indentation_is_minus_one() {
        // <<if>> at depth 0, body content at depth 1, <</if>> at depth 0.
        assert_format_eq(
            "<<if $x>>\nhello\n<</if>>",
            "<<if $x>>\n\thello\n<</if>>",
        );
    }

    /// Nested block macros: close tags dedent correctly at each level.
    #[test]
    fn test_nested_close_tag_indentation() {
        // <<if>> depth 0, <<link>> depth 1, body depth 2,
        // <</link>> depth 1, <</if>> depth 0.
        assert_format_eq(
            "<<if $x>>\n<<link \"Go\">>\nclick\n<</link>>\n<</if>>",
            "<<if $x>>\n\t<<link \"Go\">>\n\t\tclick\n\t<</link>>\n<</if>>",
        );
    }

    /// Empty-body macro followed by content: the empty macro is inline,
    /// then the next block macro starts on a new line.
    #[test]
    fn test_empty_body_macro_then_block_macro() {
        let body = "<<link \"Go\">><</link>>\n<<if $x>>\ntext\n<</if>>";
        let formatted = format_body(body).unwrap();
        assert!(
            formatted.starts_with("<<link \"Go\">><</link>>"),
            "empty-body macro should be at start, inline: {:?}",
            formatted
        );
        assert!(
            formatted.contains("<<if $x>>"),
            "if macro should follow on new line: {:?}",
            formatted
        );
    }

    /// Empty `<<if>>` with immediate `<</if>>` stays inline.
    #[test]
    fn test_empty_if_stays_inline() {
        assert_format_eq("<<if $x>><</if>>", "<<if $x>><</if>>");
    }

    /// Idempotency with empty-body macros.
    #[test]
    fn test_idempotent_empty_body() {
        let body = "<<link \"Go\">><</link>>";
        let once = format_body(body).unwrap();
        let twice = format_body(&once).unwrap();
        assert_eq!(
            once, twice,
            "formatter is not idempotent on empty-body macro:\nonce: {:?}\ntwice: {:?}",
            once, twice
        );
    }

    // ===================================================================
    // Spacing normalization tests — existing indentation is consumed
    // ===================================================================

    /// 2-space indented body content is normalized to tab indentation.
    #[test]
    fn test_consumes_2space_indent() {
        assert_format_eq(
            "<<if $x>>\n  hello\n<</if>>",
            "<<if $x>>\n\thello\n<</if>>",
        );
    }

    /// 4-space indented body content is normalized to tab indentation.
    #[test]
    fn test_consumes_4space_indent() {
        assert_format_eq(
            "<<if $x>>\n    hello\n<</if>>",
            "<<if $x>>\n\thello\n<</if>>",
        );
    }

    /// Mixed tabs and spaces are normalized to tab indentation.
    #[test]
    fn test_consumes_mixed_indent() {
        assert_format_eq(
            "<<if $x>>\n  \thello\n<</if>>",
            "<<if $x>>\n\thello\n<</if>>",
        );
    }

    /// Nested macros with 2-space indentation are normalized to tabs at each level.
    #[test]
    fn test_consumes_nested_2space_indent() {
        assert_format_eq(
            "<<if $x>>\n  <<link \"Go\">>\n    click\n  <</link>>\n<</if>>",
            "<<if $x>>\n\t<<link \"Go\">>\n\t\tclick\n\t<</link>>\n<</if>>",
        );
    }

    /// An indented close tag is dedented to the correct depth.
    #[test]
    fn test_consumes_indented_close_tag() {
        assert_format_eq(
            "<<if $x>>\nhello\n  <</if>>",
            "<<if $x>>\n\thello\n<</if>>",
        );
    }

    /// Leading whitespace on top-level prose is stripped (it's indentation, not content).
    #[test]
    fn test_strips_leading_whitespace_on_prose() {
        assert_format_eq("  indented prose", "indented prose");
    }

    /// Leading whitespace before a top-level inline macro is stripped.
    #[test]
    fn test_strips_leading_whitespace_before_macro() {
        assert_format_eq("  <<set $x to 1>>", "<<set $x to 1>>");
    }

    /// Internal spaces (after non-whitespace) are preserved — only LEADING
    /// whitespace (indentation) is stripped.
    #[test]
    fn test_preserves_internal_spaces() {
        let formatted = format_body("! Heading with spaces").unwrap();
        assert_eq!(formatted, "! Heading with spaces");
    }

    /// Idempotency holds regardless of input indentation style.
    #[test]
    fn test_idempotent_with_space_indent() {
        let body = "<<if $x>>\n  <<link \"Go\">>\n    click\n  <</link>>\n<</if>>";
        let once = format_body(body).unwrap();
        let twice = format_body(&once).unwrap();
        assert_eq!(
            once, twice,
            "formatter is not idempotent on space-indented input:\nonce: {:?}\ntwice: {:?}",
            once, twice
        );
    }

    /// A newline-only body is NOT content — `<<link>>\n<</link>>` collapses
    /// to `<<link>><</link>>` (inline, no newline between tags).
    #[test]
    fn test_newline_only_body_collapses() {
        assert_format_eq("<<link>>\n<</link>>", "<<link>><</link>>");
    }

    /// A whitespace-only body (spaces + newline) also collapses to inline.
    #[test]
    fn test_whitespace_only_body_collapses() {
        assert_format_eq("<<link>>  \n  <</link>>", "<<link>><</link>>");
    }

    /// A newline-only body with args collapses to inline.
    #[test]
    fn test_newline_only_body_with_args_collapses() {
        assert_format_eq(
            "<<link \"Go\">>\n<</link>>",
            "<<link \"Go\">><</link>>",
        );
    }

    // ===================================================================
    // Markup normalization tests — zone-specific formatting rules
    // ===================================================================

    /// Heading without space after `!` gets normalized: `!heading` → `! heading`.
    #[test]
    fn test_heading_marker_normalized() {
        assert_format_eq("!heading", "! heading");
    }

    /// Heading with extra spaces gets normalized: `!!  heading` → `!! heading`.
    #[test]
    fn test_heading_extra_spaces_normalized() {
        assert_format_eq("!!  heading", "!! heading");
    }

    /// List item without space after `*` gets normalized: `*item` → `* item`.
    #[test]
    fn test_list_item_marker_normalized() {
        assert_format_eq("*item", "* item");
    }

    /// List item with `#` marker gets normalized: `#item` → `# item`.
    #[test]
    fn test_numbered_list_marker_normalized() {
        assert_format_eq("#item", "# item");
    }

    /// List item with extra spaces gets normalized: `*  item` → `* item`.
    #[test]
    fn test_list_item_extra_spaces_normalized() {
        assert_format_eq("*  item", "* item");
    }

    /// Blockquote without space after `>` gets normalized: `>quote` → `> quote`.
    #[test]
    fn test_blockquote_marker_normalized() {
        assert_format_eq(">quote", "> quote");
    }

    /// Heading inside a macro body is normalized with indentation.
    #[test]
    fn test_heading_in_macro_body() {
        assert_format_eq(
            "<<if $x>>\n!heading\n<</if>>",
            "<<if $x>>\n\t! heading\n<</if>>",
        );
    }

    /// Idempotency with markup normalization.
    #[test]
    fn test_idempotent_with_markup() {
        let body = "!heading\n*item\n>quote";
        let once = format_body(body).unwrap();
        let twice = format_body(&once).unwrap();
        assert_eq!(
            once, twice,
            "formatter is not idempotent on markup input:\nonce: {:?}\ntwice: {:?}",
            once, twice
        );
    }

    /// Unknown/custom macros (like `<<questLog>>`, `<<warn>>`) without close
    /// tags should NOT cause following prose to be indented.
    #[test]
    fn test_unknown_macro_no_indent() {
        let body = "<<questLog \"first-day\" \"desc\">>\n\n\tThe work is repetitive.";
        let formatted = format_body(body).unwrap();
        assert!(
            !formatted.contains("\tThe work"),
            "prose after unknown macro should NOT be indented: {:?}",
            formatted
        );
    }

    /// Unknown macro followed by a `<<link>>` — the link should be at depth 0.
    #[test]
    fn test_unknown_macro_then_link_no_extra_indent() {
        let body = "<<warn>>\n\n\t<<link \"Go\" \"Start\">><</link>>";
        let formatted = format_body(body).unwrap();
        assert!(
            !formatted.contains("\t<<link"),
            "link should NOT be tab-indented after unknown macro: {:?}",
            formatted
        );
    }

    /// Blank lines between top-level block macros are preserved as exactly
    /// one blank line, and the formatter is idempotent.
    #[test]
    fn test_blank_between_macros_idempotent() {
        let body = "<<if $x>>\n\thello\n<</if>>\n\n\n<<if $y>>\n\tworld\n<</if>>";
        let f1 = format_body(body).unwrap();
        let f2 = format_body(&f1).unwrap();
        let f3 = format_body(&f2).unwrap();
        assert_eq!(
            f1, f2,
            "not idempotent: f1 != f2\nf1: {:?}\nf2: {:?}",
            f1, f2
        );
        assert_eq!(f2, f3, "still growing: f2 != f3");
        // Should have exactly one blank line between the blocks.
        assert!(
            f1.contains("<</if>>\n\n<<if $y>>"),
            "should have one blank line between blocks: {:?}",
            f1
        );
    }



}



