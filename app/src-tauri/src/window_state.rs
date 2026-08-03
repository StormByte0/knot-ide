//! Window-state persistence — `.knot/window-state.json` at the workspace root.
//!
//! Stores the custom pane-layout layer: which panels exist, tab order, split
//! sizes, active tabs, and file-browser expanded folders. OS-level window
//! geometry (size, position) is handled by `tauri-plugin-window-state`; this
//! module handles the in-app layout tree only.
//!
//! ## Ownership (CONVENTIONS §2.3)
//!
//! This module does ONLY file I/O. It receives/sends raw JSON strings. The
//! frontend owns parsing, validation, and structure. This mirrors `config.rs`
//! and `settings.rs` — the backend is a thin file-IO boundary, never touching
//! the layout tree's shape.
//!
//! ## File location
//!
//! `<workspace_root>/.knot/window-state.json` — co-located with
//! `<workspace_root>/.knot/config.json` so the whole `.knot/` directory can be
//! gitignored as IDE-local state.

use std::path::{Path, PathBuf};
use tauri::State;
use tokio::fs;

use crate::fs_ops::WorkspaceRoot;
use crate::workspace::validate_workspace_root;

/// Path to `.knot/window-state.json` relative to the workspace root.
fn window_state_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".knot").join("window-state.json")
}

/// Load the saved window state for a workspace.
///
/// Returns `Ok(None)` when the file doesn't exist yet (first open of a fresh
/// workspace — the frontend will use the default layout). Returns
/// `Ok(Some(json))` with the raw JSON string when the file exists. The
/// frontend parses the JSON; this command does no structural validation.
#[tauri::command]
pub async fn load_window_state(
    workspace_root: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<Option<String>, String> {
    let root = lookup_workspace_root(&workspace_root, &state)?;
    let path = window_state_path(&root);
    match fs::read_to_string(&path).await {
        Ok(content) => {
            tracing::debug!("loaded window state from {}", path.display());
            Ok(Some(content))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(
                "no window state at {} — frontend will use defaults",
                path.display()
            );
            Ok(None)
        }
        Err(e) => Err(format!("failed to read window state: {e}")),
    }
}

/// Save window state JSON to `.knot/window-state.json`.
///
/// Creates the `.knot/` directory if it doesn't exist. The frontend is
/// responsible for serializing the layout tree; this command just writes the
/// string verbatim.
#[tauri::command]
pub async fn save_window_state(
    workspace_root: String,
    json: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<(), String> {
    let root = lookup_workspace_root(&workspace_root, &state)?;
    let path = window_state_path(&root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create .knot directory: {e}"))?;
    }
    fs::write(&path, &json)
        .await
        .map_err(|e| format!("failed to write window state: {e}"))?;
    Ok(())
}

/// Lock the tracked workspace root state + validate the frontend-provided
/// path against it. Returns the canonicalized workspace root on success.
///
/// This is the same lookup pattern used by `config.rs` — locked here to keep
/// each Tauri command's body short. The actual comparison lives in
/// `workspace::validate_workspace_root` (pure function, no Tauri state).
fn lookup_workspace_root(
    workspace_root: &str,
    state: &State<'_, WorkspaceRoot>,
) -> Result<PathBuf, String> {
    let tracked = state.0.lock().unwrap().clone().ok_or("workspace not set")?;
    validate_workspace_root(workspace_root, &tracked)
}
