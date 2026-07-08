//! Format-agnostic type definitions shared across the core document model.
//!
//! These types were originally defined in `knot-formats/src/types.rs` but
//! were moved here because they are referenced by the core document model
//! (specifically [`crate::zoning::MacroBody`]) and `knot-core` must not
//! depend on `knot-formats`. `knot-formats` re-exports them from
//! `types.rs` for backward compatibility.

/// Whether a macro can have a body (content between open and close tags).
///
/// The tree builder uses this to determine how to handle an open macro tag
/// that has no matching close tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BodyRequirement {
    /// Always inline — no body, no close tag expected.
    /// Examples: `<<set>>`, `<<print>>`, `<<goto>>`, `<<run>>`, `<<unset>>`
    Never,

    /// Always block — body is required, close tag is expected.
    /// Unclosed blocks produce a diagnostic.
    /// Examples: `<<if>>`, `<<for>>`, `<<switch>>`, `<<widget>>`, `<<link>>`
    Required,

    /// Body is optional — close tag is allowed but not required.
    /// If a close tag is present, content between open/close becomes children.
    /// If no close tag, content until the next sibling macro becomes children.
    /// No "unclosed" diagnostic is produced.
    /// Examples: `<<case>>`, `<<default>>` (can use `<</case>>` or not)
    Optional,
}

/// The structural kind of a macro — determines its role in the macro tree.
///
/// This classification drives completion filtering, close-tag behavior,
/// and sub-macro scope enforcement. It is orthogonal to `BodyRequirement`:
///
/// | MacroKind     | body        | container/any_of | Examples                        |
/// |---------------|-------------|------------------|---------------------------------|
/// | Container     | Required    | None             | `if`, `for`, `link`, `widget`   |
/// | Inline        | Never       | None             | `set`, `goto`, `print`, `audio` |
/// | SubMacro      | Never       | Some             | `else`, `break`, `case`, `next` |
///
/// Container macros always need a closing tag. Inline macros never need one.
/// Sub-macros are only valid inside their parent container(s) — they are
/// filtered from top-level completions when the cursor is outside a valid
/// parent block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MacroKind {
    /// A macro that opens a closeable body section — always needs `<</name>>`.
    /// Examples: `<<if>>`, `<<for>>`, `<<link>>`, `<<button>>`, `<<widget>>`
    Container,

    /// A standalone macro that never has a body or close tag.
    /// Examples: `<<set>>`, `<<goto>>`, `<<print>>`, `<<audio>>`, `<<remove>>`
    Inline,

    /// A macro only valid inside a specific parent container.
    /// The `container` / `container_any_of` field on `MacroDef` specifies
    /// which parent(s) are valid.
    /// Examples: `<<else>>` (inside `<<if>>`), `<<break>>` (inside `<<for>>`),
    /// `<<case>>` (inside `<<switch>>`), `<<next>>` (inside `<<timed>>`)
    SubMacro,
}
