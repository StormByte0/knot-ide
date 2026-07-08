//! Twee document formatting helpers.

use lsp_types::*;

use super::position::utf16_len;

/// Format a Twee document: normalize headers, trim trailing whitespace,
/// ensure blank lines between passages.
pub(crate) fn format_twee_text(text: &str) -> Vec<TextEdit> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return vec![];
    }

    let mut edits = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        // Trim trailing whitespace
        let trimmed_end = line.trim_end();
        if trimmed_end.len() != line.len() {
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: i as u32,
                        character: utf16_len(trimmed_end),
                    },
                    end: Position {
                        line: i as u32,
                        character: utf16_len(line),
                    },
                },
                new_text: String::new(),
            });
        }

        // Normalize passage header spacing: ensure exactly one space after "::"
        if let Some(rest) = line.strip_prefix("::")
            && rest.starts_with(|c: char| c != ' ' && c != '[' && c != '\t')
            && !rest.is_empty()
        {
            // Missing space after "::", add one.
            // "::" is always 2 UTF-16 code units (ASCII), so character=2 is correct.
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: i as u32,
                        character: 2,
                    },
                    end: Position {
                        line: i as u32,
                        character: 2,
                    },
                },
                new_text: " ".to_string(),
            });
        }
    }

    // Ensure blank lines between passages — done as a full replacement if needed
    let mut formatted_lines: Vec<String> = Vec::new();
    let mut prev_was_blank = true; // start with blank to avoid blank line at top

    for line in &lines {
        if line.starts_with("::") {
            if !prev_was_blank && !formatted_lines.is_empty() {
                formatted_lines.push(String::new());
            }
            formatted_lines.push(line.trim_end().to_string());
            prev_was_blank = false;
        } else {
            let trimmed = line.trim_end().to_string();
            prev_was_blank = trimmed.is_empty();
            formatted_lines.push(trimmed);
        }
    }

    let formatted_text = formatted_lines.join("\n");
    let original_text = text.to_string();

    if formatted_text != original_text {
        // Return a single edit replacing the entire document
        let line_count = lines.len() as u32;
        let last_line_utf16_len = lines.last().map(|l| utf16_len(l)).unwrap_or(0);
        vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: line_count.saturating_sub(1),
                    character: last_line_utf16_len,
                },
            },
            new_text: formatted_text,
        }]
    } else {
        edits
    }
}

/// Format a Twee document using the zone-aware formatter for passages (Phase 10).
///
/// This is the preferred formatting path for formats that support zone-based
/// formatting (SugarCube). For each passage, it calls
/// `plugin.format_passage(body_text, &passage.zones)`. If the plugin returns
/// `None` (formatter refused or format doesn't support it), the passage body
/// is left as-is.
///
/// Non-passage content (headers, blank lines between passages) is normalized
/// using the same logic as `format_twee_text`.
///
/// Returns a single TextEdit replacing the entire document, or an empty vec
/// if no changes were made.
pub(crate) fn format_twee_text_with_zones(
    text: &str,
    doc: &knot_core::Document,
    plugin: &dyn knot_formats::plugin::FormatPlugin,
) -> Vec<TextEdit> {
    // Build the formatted output by walking the document text and replacing
    // each passage body with the zone-formatted version.
    //
    // Inter-passage blank lines are normalized to exactly ONE blank line
    // between passages. This ensures idempotency: formatting twice produces
    // the same output (no growing blank lines).
    let mut formatted = String::with_capacity(text.len() + 256);
    let mut cursor: usize = 0;
    let mut first_passage = true;

    for passage in &doc.passages {
        let passage_start = passage.passage_offset;
        let passage_end = passage_start + passage.span.len();

        // Emit inter-passage content (between cursor and this passage's start).
        // Normalize: collapse all whitespace between passages to a single blank line.
        if passage_start > cursor {
            let between = &text[cursor..passage_start];
            if !first_passage {
                // Exactly one blank line between passages.
                formatted.push('\n');
            }
            // If there's non-whitespace content between passages (e.g., comments),
            // preserve it but trim surrounding whitespace.
            let trimmed = between.trim();
            if !trimmed.is_empty() {
                formatted.push_str(trimmed);
                formatted.push('\n');
            }
        }

        // Find the end of the passage header line.
        let header_end = text[passage_start..]
            .find('\n')
            .map(|n| passage_start + n + 1)
            .unwrap_or(text.len());

        // Emit the header line (trimmed of trailing whitespace).
        let header_line = &text[passage_start..header_end];
        formatted.push_str(header_line.trim_end());
        formatted.push('\n');

        // Extract ONLY this passage's body text (from header_end to passage_end).
        // Previously we passed &text[passage_start..] which included ALL subsequent
        // passages — causing the formatter to see text from other passages.
        let body_text = &text[header_end.min(passage_end)..passage_end];

        // Special passages with RAW content (StoryData JSON, [script] JS,
        // [stylesheet] CSS) have empty zone maps — skip the formatter for them.
        // But "Start" and other CoreNamed/FormatNamed passages are normal
        // gameplay passages with SugarCube body text — they SHOULD be formatted.
        // The distinction: only skip if the passage has a special_def whose
        // behavior is ScriptInjection or StyleInjection (raw content). Start,
        // StoryInit, PassageHeader, etc. are special by name but have normal
        // SugarCube bodies.
        let is_raw_special = passage.special_def.as_ref().is_some_and(|def| {
            matches!(
                def.behavior,
                knot_core::passage::SpecialPassageBehavior::ScriptInjection
                    | knot_core::passage::SpecialPassageBehavior::StyleInjection
            )
        }) || passage.name == "StoryData"
            || passage.name == "StoryTitle";

        let formatted_body = if is_raw_special {
            None
        } else {
            // The formatter expects body_text where offsets align with zone spans.
            // Zone spans are passage-relative (0 = `::`). We pass the full passage
            // text (from `::`) so offsets align, but only up to passage_end.
            let full_passage_text = &text[passage_start..passage_end];
            plugin.format_passage(full_passage_text, &passage.zones)
        };

        if let Some(ref body) = formatted_body {
            // The formatter already handles trailing newlines. Push the body
            // and ensure exactly one newline separates it from the next passage.
            let body_trimmed = body.trim_end_matches('\n');
            if !body_trimmed.is_empty() {
                formatted.push_str(body_trimmed);
                formatted.push('\n');
            }
        } else {
            // Formatter refused or special passage — emit body as-is.
            let body_trimmed = body_text.trim_end();
            if !body_trimmed.is_empty() {
                formatted.push_str(body_trimmed);
                formatted.push('\n');
            }
        }

        cursor = passage_end;
        first_passage = false;
    }

    // Emit any trailing content after the last passage.
    if cursor < text.len() {
        let trailing = text[cursor..].trim();
        if !trailing.is_empty() {
            formatted.push('\n');
            formatted.push_str(trailing);
            formatted.push('\n');
        }
    }

    // Trim trailing whitespace from the final output.
    let formatted = formatted.trim_end().to_string();
    let original = text.trim_end();

    if formatted != original {
        let lines: Vec<&str> = text.lines().collect();
        let line_count = lines.len().max(1) as u32;
        let last_line_utf16_len = lines.last().map(|l| utf16_len(l)).unwrap_or(0);
        vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: line_count.saturating_sub(1),
                    character: last_line_utf16_len,
                },
            },
            new_text: formatted,
        }]
    } else {
        vec![]
    }
}
