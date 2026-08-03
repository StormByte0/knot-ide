//! Shared workspace-root validation helper.
//!
//! Pure function — no Tauri state, no side effects. Used by `config.rs` and
//! `window_state.rs` (and any future module that needs to validate a
//! frontend-provided workspace root against the app's tracked root).
//!
//! ## Why a shared helper (CONVENTIONS §2.3)
//!
//! Before this module existed, `config.rs` had its own
//! `validate_workspace_root` that took `State<'_, WorkspaceRoot>` directly.
//! Duplicating that logic in `window_state.rs` would violate the "no spaghetti
//! code" rule. Extracting the pure core (canonicalize + compare) here keeps
//! each Tauri command thin: lock state → call this helper → proceed.

use std::path::{Path, PathBuf};

/// Validate that `input` matches the `tracked` workspace root.
///
/// Canonicalizes both paths (resolving `..`, symlinks, etc.) before comparing.
/// Returns the canonicalized input path on success, or an error string if:
/// - The tracked root cannot be canonicalized (filesystem error).
/// - The input path cannot be canonicalized (doesn't exist or invalid).
/// - The canonicalized paths differ (input escapes the workspace).
///
/// Note: for not-yet-existing paths (e.g. `create_file` on a new file), the
/// caller should canonicalize the parent and join the filename — this helper
/// requires the path to exist. `fs_ops::validate_in_workspace` handles that
/// case for filesystem mutations; this helper is for read/write of files
/// *inside* an already-open workspace.
pub fn validate_workspace_root(input: &str, tracked: &Path) -> Result<PathBuf, String> {
    let canon_tracked = tracked
        .canonicalize()
        .map_err(|e| format!("invalid tracked workspace root: {e}"))?;
    let input_path = Path::new(input);
    let canon_input = input_path
        .canonicalize()
        .map_err(|e| format!("invalid workspace root path: {e}"))?;
    if canon_input != canon_tracked {
        return Err(format!(
            "workspace root '{}' does not match the tracked workspace",
            input
        ));
    }
    Ok(canon_input)
}
