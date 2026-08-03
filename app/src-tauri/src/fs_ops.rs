//! Filesystem operations for the file browser.
//!
//! All commands validate that paths are inside the workspace root to prevent
//! the UI from mutating files outside the project. Deletions move to the
//! OS trash (not permanent delete) so users can recover.

use std::path::{Path, PathBuf};
use serde::Serialize;
use tauri::State;
use tokio::fs;

/// App state holding the workspace root path.
pub struct WorkspaceRoot(pub std::sync::Mutex<Option<PathBuf>>);

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_directory: bool,
    pub is_file: bool,
}

/// Directories that are never shown in the file browser.
const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "dist", "build"];

/// Hidden entries (starting with `.`) are suppressed. Git-related files are
/// hidden too — too verbose for a Twine-focused editor. Authors who need
/// version control can use external Git tools.
fn should_show_dir(name: &str) -> bool {
    if SKIPPED_DIRS.contains(&name) { return false; }
    if name.starts_with('.') { return false; }
    true
}

fn should_show_file(name: &str) -> bool {
    if name.starts_with('.') { return false; }
    true
}

/// Validate that `path` is inside `workspace_root`. Returns the canonicalized
/// path on success, or an error string if the path escapes the workspace.
fn validate_in_workspace(path: &str, root: &Path) -> Result<PathBuf, String> {
    let target = Path::new(path);
    // Canonicalize both to resolve `..` and symlinks before comparing.
    let canon_root = root.canonicalize().map_err(|e| format!("invalid workspace root: {e}"))?;
    let canon_target = target
        .canonicalize()
        .or_else(|_| {
            // If the path doesn't exist yet (e.g. create_file), canonicalize
            // the parent and join the filename.
            if let Some(parent) = target.parent() {
                parent
                    .canonicalize()
                    .map(|p| p.join(target.file_name().unwrap_or_default()))
                    .map_err(|e| format!("invalid path: {e}"))
            } else {
                Err(format!("invalid path: {path}"))
            }
        })?;
    if !canon_target.starts_with(&canon_root) {
        return Err(format!(
            "path '{}' is outside the workspace",
            target.display()
        ));
    }
    Ok(canon_target)
}

/// List directory entries (non-recursive). Returns directories first
/// (alphabetical), then files (alphabetical).
#[tauri::command]
pub async fn list_dir(
    path: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<Vec<FileEntry>, String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let _ = validate_in_workspace(&path, &root)?;

    let mut entries: Vec<FileEntry> = Vec::new();
    let mut dir = match fs::read_dir(&path).await {
        Ok(d) => d,
        Err(e) => return Err(format!("failed to read dir '{}': {e}", path)),
    };

    while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Use metadata() instead of file_type() — more reliable on Windows
        // (file_type() can fail on certain directory types like junctions).
        let metadata = entry.metadata().await.map_err(|e| e.to_string())?;
        let is_dir = metadata.is_dir();
        let is_file = metadata.is_file();

        let show = if is_dir { should_show_dir(&name) } else { should_show_file(&name) };
        if !show { continue; }

        entries.push(FileEntry {
            path: entry.path().to_string_lossy().into_owned(),
            name,
            is_directory: is_dir,
            is_file,
        });
    }

    // Sort: directories first, then files; both alphabetical (case-insensitive).
    entries.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(entries)
}

/// Create a new empty file. Fails if it already exists.
#[tauri::command]
pub async fn create_file(
    path: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<String, String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let validated = validate_in_workspace(&path, &root)?;

    if validated.exists() {
        return Err(format!("file already exists: {}", validated.display()));
    }
    fs::write(&validated, "").await.map_err(|e| e.to_string())?;
    Ok(validated.to_string_lossy().into_owned())
}

/// Create a new directory. Fails if it already exists.
#[tauri::command]
pub async fn create_dir(
    path: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<String, String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let validated = validate_in_workspace(&path, &root)?;

    if validated.exists() {
        return Err(format!("directory already exists: {}", validated.display()));
    }
    fs::create_dir(&validated).await.map_err(|e| e.to_string())?;
    Ok(validated.to_string_lossy().into_owned())
}

/// Create a directory and all intermediate parent directories.
///
/// Used by the file browser's "New File" / "New Folder" dialogs when the
/// user types a path with slashes (e.g. `subfolder/deep/file.twee`). Unlike
/// `create_dir`, this does NOT fail if intermediate directories already
/// exist — only the final leaf must not exist as a non-directory.
#[tauri::command]
pub async fn create_dir_all(
    path: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<String, String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let validated = validate_in_workspace(&path, &root)?;

    // create_dir_all succeeds if the dir already exists (unlike create_dir).
    // We only reject if a non-directory file is blocking the path.
    if validated.exists() && !validated.is_dir() {
        return Err(format!(
            "a file already exists at this path: {}",
            validated.display()
        ));
    }
    fs::create_dir_all(&validated).await.map_err(|e| e.to_string())?;
    Ok(validated.to_string_lossy().into_owned())
}

/// Rename/move a file or directory. The new path must also be inside the
/// workspace. Fails if the destination already exists.
#[tauri::command]
pub async fn rename_path(
    old_path: String,
    new_path: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<String, String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let old_validated = validate_in_workspace(&old_path, &root)?;
    let new_validated = validate_in_workspace(&new_path, &root)?;

    if !old_validated.exists() {
        return Err(format!("source does not exist: {}", old_validated.display()));
    }
    if new_validated.exists() {
        return Err(format!("destination already exists: {}", new_validated.display()));
    }
    fs::rename(&old_validated, &new_validated).await.map_err(|e| e.to_string())?;
    Ok(new_validated.to_string_lossy().into_owned())
}

/// Delete a file or directory by moving it to the OS trash.
/// Uses the `trash` crate for cross-platform safe deletion.
#[tauri::command]
pub async fn delete_path(
    path: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<(), String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let validated = validate_in_workspace(&path, &root)?;

    if !validated.exists() {
        return Err(format!("path does not exist: {}", validated.display()));
    }

    // `trash::delete` is synchronous — run on a blocking thread.
    let path_str = validated.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || {
        trash::delete(&path_str).map_err(|e| format!("failed to move to trash: {e}"))
    })
    .await
    .map_err(|e| format!("task join error: {e}"))??;

    Ok(())
}

/// Copy a file. Does not copy directories (use for files only).
/// Appends `-copy` if the destination name collides.
#[tauri::command]
pub async fn copy_file(
    src: String,
    dest: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<String, String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let src_validated = validate_in_workspace(&src, &root)?;

    // If dest already exists, append `-copy` before the extension.
    let dest_path = PathBuf::from(&dest);
    let final_dest = if dest_path.exists() {
        let stem = dest_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = dest_path.extension().map(|s| s.to_string_lossy().into_owned());
        let parent = dest_path.parent().unwrap_or_else(|| Path::new(""));
        let new_name = match ext {
            Some(e) => format!("{}-copy.{}", stem, e),
            None => format!("{}-copy", stem),
        };
        parent.join(new_name)
    } else {
        dest_path
    };

    let dest_validated = validate_in_workspace(&final_dest.to_string_lossy(), &root)?;
    fs::copy(&src_validated, &dest_validated).await.map_err(|e| e.to_string())?;
    Ok(dest_validated.to_string_lossy().into_owned())
}

/// Read a file's full contents as UTF-8 text. Used by the editor on tab
/// restore (re-read from disk to avoid stale cached content — see
/// `PLAN.md` §13.8) and by the "Revert File" action (future).
///
/// Returns the file contents as a string. Fails if the path is outside the
/// workspace, the file doesn't exist, or it isn't valid UTF-8.
#[tauri::command]
pub async fn read_file(
    path: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<String, String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let validated = validate_in_workspace(&path, &root)?;
    if !validated.exists() {
        return Err(format!("file does not exist: {}", validated.display()));
    }
    if !validated.is_file() {
        return Err(format!("not a file: {}", validated.display()));
    }
    fs::read_to_string(&validated)
        .await
        .map_err(|e| format!("failed to read file '{}': {e}", validated.display()))
}

/// Write text contents to a file. Used by the editor's Save action (`Ctrl+S`)
/// — see `PLAN.md` §13.2. Overwrites the file if it exists; creates it if it
/// doesn't (though in practice the editor only saves files that were opened
/// from disk, so the file always exists).
///
/// The path must be inside the workspace root. Returns the canonicalized
/// path that was written (so the frontend can update the tab's path if the
/// filesystem canonicalized it differently, e.g. resolved a symlink).
#[tauri::command]
pub async fn write_file(
    path: String,
    contents: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<String, String> {
    let root = state.0.lock().unwrap().clone();
    let root = root.ok_or("workspace not set")?;
    let validated = validate_in_workspace(&path, &root)?;
    // Create parent directories if they don't exist (handles the rare case
    // of saving a file whose parent dir was deleted externally). Most saves
    // hit an existing file, so this is a no-op in the common path.
    if let Some(parent) = validated.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create parent directory: {e}"))?;
    }
    fs::write(&validated, &contents)
        .await
        .map_err(|e| format!("failed to write file '{}': {e}", validated.display()))?;
    Ok(validated.to_string_lossy().into_owned())
}

/// Set the workspace root path. Called by the frontend when a folder is
/// opened. All subsequent filesystem commands validate paths against this root.
#[tauri::command]
pub fn set_workspace_root(
    path: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<(), String> {
    *state.0.lock().unwrap() = Some(PathBuf::from(path));
    Ok(())
}
