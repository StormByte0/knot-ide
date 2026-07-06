//! Edit impact classification — gate for the incremental parse path.
//!
//! When the editor sends a `textDocument/didChange` notification, the server
//! must decide whether the edit is confined to a single passage (and thus
//! eligible for incremental single-passage re-parse) or whether it crosses
//! passage boundaries (in which case we fall back to a full-file re-parse).
//!
//! This module is a pure function — no I/O, no state mutation. It takes the
//! pre-edit `Document`, the pre-edit source text, and the LSP content change
//! events, and returns an [`EditImpact``].
//!
//! ## Decision table
//!
//! | Condition                                              | Result              |
//! |--------------------------------------------------------|---------------------|
//! | Any change has `range == None`                         | `WholeDocument`     |
//! | Empty `changes` vec                                    | `WholeDocument`     |
//! | `doc_before.passages` is empty                         | `WholeDocument`     |
//! | Any single change spans two passages (start vs end)    | `BoundaryCrossing`  |
//! | Any change's inserted text contains a `:: ` line start | `BoundaryCrossing`  |
//! | Any change's removed text contains a `:: ` line start  | `BoundaryCrossing`  |
//! | Multiple changes touch multiple distinct passages      | `BoundaryCrossing`  |
//! | Otherwise (all changes within one passage)             | `WithinPassage`     |
//!
//! ## `:: ` detection
//!
//! Twine 3 requires `::` at column 0 of a line to start a passage header.
//! Leading whitespace is NOT allowed. We strip a trailing `\r` (for CRLF
//! files) before checking, so `::\r\n` and `:: Name\r\n` are detected.
//!
//! ## Passage-relative range
//!
//! When the result is `WithinPassage`, the `in_passage_range` is the union
//! (bounding box) of all per-change byte ranges, expressed in passage-relative
//! coordinates (0 = the `::` of the containing passage's header). The actual
//! per-passage re-parse will see the post-edit passage text anyway, so we
//! don't need precise multi-range tracking — the bounding box is enough for
//! M2's offset fix-up logic.

use knot_core::Document;
use std::ops::Range;
use tower_lsp::lsp_types::TextDocumentContentChangeEvent;

/// The impact of a `didChange` edit batch on the document's passage structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditImpact {
    /// The edit falls entirely within one passage's body or header.
    ///
    /// `in_passage_range` is passage-relative (0 = start of the `:: ` header).
    /// For multi-change batches confined to the same passage, this is the
    /// bounding box of all per-change ranges.
    WithinPassage {
        /// The name of the passage the edit falls within.
        passage_name: String,
        /// Passage-relative byte range (bounding box of all changes).
        in_passage_range: Range<usize>,
    },

    /// The edit touches or creates a `:: ` header line, or spans multiple
    /// passages. The caller must fall back to a full-file re-parse.
    BoundaryCrossing,

    /// The edit is a full-text replacement (any change with `range == None`),
    /// or no snapshot is available, or the document had no passages to begin
    /// with. The caller must perform a full-file re-parse.
    WholeDocument,
}

/// Classify the impact of a `didChange` edit batch.
///
/// See the [module docs](self) for the full decision table.
///
/// ## Arguments
///
/// - `doc_before`: the `Document` state prior to the edits. Used to look up
///   passage offsets and spans.
/// - `text_before`: the full source text prior to the edits. Used to (a)
///   convert LSP line/char positions to byte offsets via the existing
///   `lsp_range_to_byte_range` helper, and (b) extract the *removed* text
///   so we can detect a deleted `:: ` header line.
/// - `changes`: the LSP content change events from the `didChange`
///   notification.
pub fn classify_edit(
    doc_before: &Document,
    text_before: &str,
    changes: &[TextDocumentContentChangeEvent],
) -> EditImpact {
    // Defensive: empty changes vec → WholeDocument (shouldn't happen in practice).
    if changes.is_empty() {
        return EditImpact::WholeDocument;
    }

    // If the document has no passages, we can't do incremental work — there's
    // nothing to splice into. This also covers the empty-file case.
    if doc_before.passages.is_empty() {
        return EditImpact::WholeDocument;
    }

    // Step 1: any change with `range == None` → full-text replacement.
    // Per LSP spec, a None range means the new text is the full document.
    if changes.iter().any(|c| c.range.is_none()) {
        return EditImpact::WholeDocument;
    }

    // Step 2: convert each change to a byte range + check passage containment.
    //
    // We collect (passage_index, passage_relative_range) for each change.
    // If any change spans two passages, or any change's inserted/removed text
    // contains a `:: ` line start, we return BoundaryCrossing immediately.
    let mut touched_passages: Vec<(usize, Range<usize>)> = Vec::with_capacity(changes.len());

    for change in changes {
        // `range` is Some (we checked above). Convert LSP line/char to byte range.
        let lsp_range = change.range.expect("checked above");
        let byte_range = super::position::lsp_range_to_byte_range(text_before, &lsp_range);

        // Find the passage containing byte_range.start.
        let start_passage_idx = match find_passage_index(doc_before, byte_range.start) {
            Some(idx) => idx,
            None => return EditImpact::WholeDocument, // before first passage — shouldn't happen
        };

        // Find the passage containing byte_range.end. Note: `end` is exclusive,
        // so a range ending exactly at a passage boundary belongs to the
        // passage before it. We handle this by clamping: if end is at the very
        // start of the next passage, treat it as belonging to the previous one.
        let end_passage_idx = match find_passage_index_for_end(doc_before, byte_range.end) {
            Some(idx) => idx,
            None => return EditImpact::WholeDocument, // past EOF — shouldn't happen
        };

        // Different passages → boundary crossing.
        if start_passage_idx != end_passage_idx {
            return EditImpact::BoundaryCrossing;
        }

        // Check the inserted text for a `:: ` line start.
        if contains_passage_header_line(&change.text) {
            return EditImpact::BoundaryCrossing;
        }

        // Check the removed text for a `:: ` line start.
        let removed = &text_before[byte_range.clone()];
        if contains_passage_header_line(removed) {
            return EditImpact::BoundaryCrossing;
        }

        // Compute the passage-relative range.
        let passage = &doc_before.passages[start_passage_idx];
        let rel_start = byte_range
            .start
            .saturating_sub(passage.passage_offset);
        let rel_end = byte_range
            .end
            .saturating_sub(passage.passage_offset);
        touched_passages.push((start_passage_idx, rel_start..rel_end));
    }

    // Step 3: all changes must touch the same passage.
    let first_passage_idx = touched_passages[0].0;
    if touched_passages.iter().any(|(idx, _)| *idx != first_passage_idx) {
        return EditImpact::BoundaryCrossing;
    }

    // Step 4: merge per-change ranges into a bounding box.
    let passage = &doc_before.passages[first_passage_idx];
    let merged_start = touched_passages
        .iter()
        .map(|(_, r)| r.start)
        .min()
        .unwrap_or(0);
    let merged_end = touched_passages
        .iter()
        .map(|(_, r)| r.end)
        .max()
        .unwrap_or(0);

    EditImpact::WithinPassage {
        passage_name: passage.name.clone(),
        in_passage_range: merged_start..merged_end,
    }
}

/// Find the index of the passage whose span contains the given document-absolute
/// byte offset.
///
/// A passage's span is `[passage_offset, passage_offset + span.len())` in
/// document-absolute coordinates. The last passage's span is extended to EOF
/// (its `span.end` may be shorter than the actual document length if the
/// file has trailing content not part of any passage).
fn find_passage_index(doc: &Document, abs_offset: usize) -> Option<usize> {
    for (i, p) in doc.passages.iter().enumerate() {
        let start = p.passage_offset;
        let end = p.passage_offset + p.span.len();
        if abs_offset >= start && abs_offset < end {
            return Some(i);
        }
    }
    // If the offset is past the last passage's recorded span end but still
    // within the document (trailing content / EOF edit), assign it to the
    // last passage. This is the "edit at EOF" case from the plan.
    if let Some(last) = doc.passages.last() {
        let last_end = last.passage_offset + last.span.len();
        if abs_offset >= last_end {
            return Some(doc.passages.len() - 1);
        }
    }
    None
}

/// Find the passage index for the *exclusive* end of a byte range.
///
/// LSP ranges are half-open: `[start, end)`. An edit whose `end` falls exactly
/// at a passage boundary (e.g. deleting the trailing newline of passage A
/// before passage B's header) belongs to passage A, not passage B.
fn find_passage_index_for_end(doc: &Document, abs_end: usize) -> Option<usize> {
    // If end falls exactly at a passage start, assign it to the previous passage.
    for (i, p) in doc.passages.iter().enumerate() {
        if abs_end == p.passage_offset && i > 0 {
            return Some(i - 1);
        }
    }
    // Otherwise use the same containment logic as find_passage_index.
    find_passage_index(doc, abs_end)
}

/// Check whether any line in `text` starts with `::` at column 0.
///
/// Twine 3 requires the `::` prefix to be at the start of the line (no
/// leading whitespace). We strip a trailing `\r` for CRLF safety, so
/// `::\r\n` and `:: Name\r\n` are detected as passage headers.
///
/// An empty `text` (no insertion, or deletion of zero bytes) returns `false`.
fn contains_passage_header_line(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    // `str::lines` splits on `\n` and strips a trailing `\r` from each line.
    // The first line is included even if `text` doesn't start with `\n`.
    // The final `\n` (if any) does NOT produce an empty trailing line.
    text.lines().any(|line| line.starts_with("::"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use knot_core::passage::{Passage, StoryFormat};
    use tower_lsp::lsp_types::{Position, Range as LspRange, TextDocumentContentChangeEvent};
    use url::Url;

    /// Build a Document with the given passages, computing `passage_offset`
    /// from the order they appear in the source text.
    fn doc_with_passages(text: &str, passage_specs: &[(&str, Range<usize>)]) -> Document {
        let mut doc = Document::new(Url::parse("file:///test.tw").unwrap(), StoryFormat::Core);
        for (name, span) in passage_specs {
            let mut p = Passage::new((*name).to_string(), span.clone());
            p.passage_offset = span.start;
            doc.passages.push(p);
        }
        let _ = text; // kept for API symmetry; not stored on Document
        doc
    }

    /// Build a TextDocumentContentChangeEvent with the given LSP line range
    /// (start_line, start_char)..(end_line, end_char) and replacement text.
    fn change(
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
        text: &str,
    ) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(LspRange {
                start: Position::new(start_line, start_char),
                end: Position::new(end_line, end_char),
            }),
            range_length: None,
            text: text.to_string(),
        }
    }

    /// Build a full-text-replacement change (range == None).
    fn change_full(text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Test cases from the plan
    // -----------------------------------------------------------------------

    #[test]
    fn edit_within_passage_body() {
        // Two passages; edit a body line of passage A.
        // Text: ":: A\nHello world\n:: B\nGoodbye\n"
        //   ":: A\n"        = 5 bytes  (0..5)
        //   "Hello world\n" = 12 bytes (5..17)
        //   ":: B\n"        = 5 bytes  (17..22)
        //   "Goodbye\n"     = 9 bytes  (22..31)
        let text = ":: A\nHello world\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..17), ("B", 17..31)],
        );
        // Replace "world" (line 1, chars 6..11) with "there".
        // Line 1 starts at byte 5. Chars 6..11 = bytes 11..16. rel = 11..16.
        let changes = vec![change(1, 6, 1, 11, "there")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "A".to_string(),
                in_passage_range: 11..16,
            }
        );
    }

    #[test]
    fn edit_within_passage_header_line() {
        // Edit the header line itself (e.g. user typing in the passage name).
        // Text: ":: A\nHello\n:: B\nGoodbye\n"
        //   ":: A\n"    = 5 bytes (0..5)
        //   "Hello\n"   = 6 bytes (5..11)
        //   ":: B\n"    = 5 bytes (11..16)
        //   "Goodbye\n" = 9 bytes (16..25)
        let text = ":: A\nHello\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..11), ("B", 11..25)],
        );
        // Replace "A" (line 0, chars 3..4) with "AA". Byte range 3..4. rel = 3..4.
        let changes = vec![change(0, 3, 0, 4, "AA")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "A".to_string(),
                in_passage_range: 3..4,
            }
        );
    }

    #[test]
    fn edit_spanning_two_passages_is_boundary_crossing() {
        // Edit spans the boundary between passage A and passage B.
        // Text: ":: A\nHello\n:: B\nGoodbye\n" (25 bytes total).
        //   A span = 0..11, B span = 11..25.
        let text = ":: A\nHello\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..11), ("B", 11..25)],
        );
        // Replace from line 1 char 0 ("Hello" start, byte 5) to line 2 char 4
        // (":: B" end, byte 15) — crosses the passage boundary at byte 11.
        let changes = vec![change(1, 0, 2, 4, "")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::BoundaryCrossing);
    }

    #[test]
    fn edit_at_byte_zero_within_first_passage() {
        // Edit at the very start of the file — still within passage 1's header.
        // Text: ":: A\nHello\n" = 11 bytes. Span A = 0..11.
        let text = ":: A\nHello\n";
        let doc = doc_with_passages(text, &[("A", 0..11)]);
        // Insert "x" before the existing ":: A" (line 0, chars 0..0).
        // Not ":: " so no boundary crossing.
        let changes = vec![change(0, 0, 0, 0, "x")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "A".to_string(),
                in_passage_range: 0..0,
            }
        );
    }

    #[test]
    fn edit_at_eof_within_last_passage() {
        // Edit past the last passage's recorded span end (trailing content /
        // EOF edit) — should still be assigned to the last passage.
        // Text: ":: A\nHello\n" = 11 bytes. Line 2 char 0 = byte 11 (past trailing \n).
        let text = ":: A\nHello\n";
        let doc = doc_with_passages(text, &[("A", 0..11)]);
        // Insert at EOF (line 2, char 0 — byte offset 11).
        let changes = vec![change(2, 0, 2, 0, "more")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "A".to_string(),
                in_passage_range: 11..11, // past span end, but clamped to last passage
            }
        );
    }

    #[test]
    fn edit_inserting_passage_header_mid_body_is_boundary_crossing() {
        // Edit inserts ":: NewPassage\n" in the middle of passage A's body.
        // Text: ":: A\nHello world\n:: B\nGoodbye\n"
        //   A span = 0..17, B span = 17..31.
        let text = ":: A\nHello world\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..17), ("B", 17..31)],
        );
        // Insert ":: NewPassage\n" at line 1, char 5 (mid "Hello world", byte 10).
        let changes = vec![change(1, 5, 1, 5, ":: NewPassage\n")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::BoundaryCrossing);
    }

    #[test]
    fn edit_deleting_passage_header_prefix_is_boundary_crossing() {
        // Edit deletes the ":: " prefix of passage B's header.
        // Text: ":: A\nHello\n:: B\nGoodbye\n"
        //   A span = 0..11, B span = 11..25.
        let text = ":: A\nHello\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..11), ("B", 11..25)],
        );
        // Delete ":: " from line 2 (chars 0..3). Line 2 starts at byte 11.
        // Byte range 11..14. The removed text is ":: " which starts with "::".
        let changes = vec![change(2, 0, 2, 3, "")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::BoundaryCrossing);
    }

    #[test]
    fn multi_cursor_edit_across_passages_is_boundary_crossing() {
        // Two changes in two different passages.
        // Text: ":: A\nHello\n:: B\nGoodbye\n"
        //   A span = 0..11, B span = 11..25.
        let text = ":: A\nHello\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..11), ("B", 11..25)],
        );
        let changes = vec![
            change(1, 0, 1, 5, "Hi"),     // passage A (byte 5..10)
            change(3, 0, 3, 7, "Bye"),    // passage B (byte 16..23)
        ];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::BoundaryCrossing);
    }

    #[test]
    fn multi_cursor_edit_within_same_passage_is_within_passage() {
        // Two changes both within passage A.
        // Text: ":: A\nHello world\n:: B\nGoodbye\n"
        //   A span = 0..17, B span = 17..31.
        let text = ":: A\nHello world\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..17), ("B", 17..31)],
        );
        // Line 1 starts at byte 5.
        // "Hello" is chars 0..5 = bytes 5..10.
        // "world" is chars 6..11 = bytes 11..16.
        let changes = vec![
            change(1, 0, 1, 5, "Hi"),     // "Hello" → "Hi" (bytes 5..10)
            change(1, 6, 1, 11, "there"), // "world" → "there" (bytes 11..16)
        ];
        let impact = classify_edit(&doc, text, &changes);
        // Bounding box: rel_start = min(5, 11) = 5, rel_end = max(10, 16) = 16.
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "A".to_string(),
                in_passage_range: 5..16,
            }
        );
    }

    #[test]
    fn change_with_none_range_is_whole_document() {
        // A single change with range == None → WholeDocument.
        // Text: ":: A\nHello\n" = 11 bytes. Span A = 0..11.
        let text = ":: A\nHello\n";
        let doc = doc_with_passages(text, &[("A", 0..11)]);
        let changes = vec![change_full(":: A\nGoodbye\n")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::WholeDocument);
    }

    #[test]
    fn any_change_with_none_range_is_whole_document() {
        // Even if other changes have ranges, a single None-range change
        // forces WholeDocument.
        // Text: ":: A\nHello\n:: B\nGoodbye\n"
        //   A span = 0..11, B span = 11..25.
        let text = ":: A\nHello\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..11), ("B", 11..25)],
        );
        let changes = vec![
            change(1, 0, 1, 5, "Hi"),
            change_full(":: A\nGoodbye\n"),
        ];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::WholeDocument);
    }

    #[test]
    fn crlf_file_passage_header_detection() {
        // CRLF line endings: ":: A\r\nHello\r\n:: B\r\nGoodbye\r\n"
        // `str::lines()` strips the trailing `\r`, so ":: A\r\n" → ":: A"
        // starts with "::" → passage header.
        // Byte layout (CRLF adds 1 byte per line vs LF):
        //   ":: A\r\n"     = 6 bytes (0..6)
        //   "Hello\r\n"    = 7 bytes (6..13)
        //   ":: B\r\n"     = 6 bytes (13..19)
        //   "Goodbye\r\n" = 10 bytes (19..29)
        let text = ":: A\r\nHello\r\n:: B\r\nGoodbye\r\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..13), ("B", 13..29)],
        );
        // Insert ":: NewPassage\r\n" at line 1 char 0 (byte 6).
        let changes = vec![change(1, 0, 1, 0, ":: NewPassage\r\n")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::BoundaryCrossing);
    }

    #[test]
    fn empty_changes_vec_is_whole_document() {
        // Defensive: empty changes vec → WholeDocument.
        // Text: ":: A\nHello\n" = 11 bytes. Span A = 0..11.
        let text = ":: A\nHello\n";
        let doc = doc_with_passages(text, &[("A", 0..11)]);
        let changes: Vec<TextDocumentContentChangeEvent> = vec![];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::WholeDocument);
    }

    #[test]
    fn document_with_zero_passages_is_whole_document() {
        // Empty file (no passages) → WholeDocument.
        let text = "";
        let doc = doc_with_passages(text, &[]);
        let changes = vec![change(0, 0, 0, 0, ":: A\nHello\n")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::WholeDocument);
    }

    // -----------------------------------------------------------------------
    // Additional edge-case tests
    // -----------------------------------------------------------------------

    #[test]
    fn insert_text_with_only_double_colon_not_at_line_start_is_within_passage() {
        // "::" inside a body line (not at column 0) is NOT a passage header.
        // Text: ":: A\nHello\n" = 11 bytes. Span A = 0..11.
        // Edit at line 1 char 5 = byte 10 (past "Hello", at the \n).
        // Insert "x:: y\n" — `.lines()` yields ["x:: y"] (single line). "x:: y"
        // does NOT start with "::" at column 0. Result: WithinPassage.
        let text = ":: A\nHello\n";
        let doc = doc_with_passages(text, &[("A", 0..11)]);
        let changes = vec![change(1, 5, 1, 5, "x:: y\n")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "A".to_string(),
                in_passage_range: 10..10,
            }
        );
    }

    #[test]
    fn insert_text_with_newline_then_double_colon_is_boundary_crossing() {
        // Insert "\n:: y" — second line starts with "::".
        let text = ":: A\nHello\n";
        let doc = doc_with_passages(text, &[("A", 0..9)]);
        let changes = vec![change(1, 5, 1, 5, "\n:: y")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(impact, EditImpact::BoundaryCrossing);
    }

    #[test]
    fn edit_exactly_at_passage_boundary_belongs_to_previous_passage() {
        // Edit ends exactly at the start of passage B's ":: " header.
        // Per LSP semantics, `end` is exclusive, so this belongs to A.
        // Text: ":: A\nHello\n:: B\nGoodbye\n"
        //   ":: A\n"   = 5 bytes (0..5)
        //   "Hello\n"  = 6 bytes (5..11)
        //   ":: B\n"   = 5 bytes (11..16)
        //   "Goodbye\n"= 9 bytes (16..25)
        let text = ":: A\nHello\n:: B\nGoodbye\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..11), ("B", 11..25)],
        );
        // Replace from line 1 char 0 (byte 5) to line 2 char 0 (byte 11,
        // exactly at B's passage_offset). End is exclusive → belongs to A.
        let changes = vec![change(1, 0, 2, 0, "Hi\n")];
        let impact = classify_edit(&doc, text, &changes);
        // Byte range 5..11, both in A. rel = 5..11.
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "A".to_string(),
                in_passage_range: 5..11,
            }
        );
    }

    #[test]
    fn contains_passage_header_line_helper() {
        // Direct unit tests for the helper.
        assert!(!contains_passage_header_line(""));
        assert!(contains_passage_header_line(":: A"));
        assert!(contains_passage_header_line(":: A\n"));
        assert!(contains_passage_header_line(":: A\r\n"));
        assert!(contains_passage_header_line("body\n:: A\n"));
        assert!(!contains_passage_header_line("body"));
        assert!(!contains_passage_header_line("x:: A"));
        assert!(!contains_passage_header_line("  :: A")); // leading whitespace → not a header
        assert!(contains_passage_header_line("\n:: A")); // empty first line, then header
        assert!(contains_passage_header_line("::\n")); // bare `::` with no name
    }

    #[test]
    fn insert_empty_string_in_body_is_within_passage() {
        // A pure deletion (empty inserted text) within a passage body.
        // Text: ":: A\nHello world\n" = 17 bytes. Span A = 0..17.
        //   ":: A\n"        = 5 bytes (0..5)
        //   "Hello world\n" = 12 bytes (5..17)
        let text = ":: A\nHello world\n";
        let doc = doc_with_passages(text, &[("A", 0..17)]);
        // Delete "Hello " (line 1, chars 0..6) = bytes 5..11. rel = 5..11.
        let changes = vec![change(1, 0, 1, 6, "")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "A".to_string(),
                in_passage_range: 5..11,
            }
        );
    }

    #[test]
    fn three_passages_edit_in_middle_one() {
        // Three passages; edit passage B in the middle.
        // Text: ":: A\nA body\n:: B\nB body\n:: C\nC body\n"
        //   ":: A\n"   = 5 bytes (0..5)
        //   "A body\n" = 7 bytes (5..12)
        //   ":: B\n"   = 5 bytes (12..17)
        //   "B body\n" = 7 bytes (17..24)
        //   ":: C\n"   = 5 bytes (24..29)
        //   "C body\n" = 7 bytes (29..36)
        let text = ":: A\nA body\n:: B\nB body\n:: C\nC body\n";
        let doc = doc_with_passages(
            text,
            &[("A", 0..12), ("B", 12..24), ("C", 24..36)],
        );
        // Replace "B body" (line 3, chars 0..6) with "B edited".
        // Line 3 starts at byte 17. Chars 0..6 = bytes 17..23.
        // Passage B offset = 12. rel_start = 17 - 12 = 5. rel_end = 23 - 12 = 11.
        let changes = vec![change(3, 0, 3, 6, "B edited")];
        let impact = classify_edit(&doc, text, &changes);
        assert_eq!(
            impact,
            EditImpact::WithinPassage {
                passage_name: "B".to_string(),
                in_passage_range: 5..11,
            }
        );
    }
}
