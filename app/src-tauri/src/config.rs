//! Project-level settings — `.knot/config.json` at the workspace root.
//!
//! Per-workspace configuration: story format, build config, include/exclude
//! patterns, Story Map layout preference. Loaded/saved via Tauri commands
//! that the frontend calls with the workspace root path.
//!
//! ## Migration
//!
//! On first open of a workspace that has `.vscode/knot.json` (the old VS Code
//! extension config) but no `.knot/config.json`, the migration command copies
//! the old file to `.vscode/knot.json.bak`, parses it, and writes a new
//! `.knot/config.json` with the migrated values.

use std::path::{Path, PathBuf};
use tauri::State;
use tokio::fs;

use crate::fs_ops::WorkspaceRoot;
use crate::workspace::validate_workspace_root;

/// Default project settings JSON (returned when no config file exists).
const DEFAULT_PROJECT_CONFIG: &str = r#"{
  "storyFormat": "sugarcube",
  "buildConfig": {
    "outputDir": "build",
    "outputFormat": "html",
    "tweegoFlags": []
  },
  "includePatterns": [],
  "excludePatterns": [],
  "storymapLayout": "manual"
}"#;

/// Path to `.knot/config.json` relative to the workspace root.
fn config_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".knot").join("config.json")
}

/// Path to `.vscode/knot.json` (old VS Code extension config).
fn vscode_config_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".vscode").join("knot.json")
}

/// Load project settings from `.knot/config.json`. Returns the raw JSON string
/// (the frontend parses it). If the file doesn't exist, returns default settings.
#[tauri::command]
pub async fn load_project_settings(
    workspace_root: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<String, String> {
    let root = lookup_workspace_root(&workspace_root, &state)?;
    let path = config_path(&root);
    match fs::read_to_string(&path).await {
        Ok(content) => Ok(content),
        Err(_) => {
            tracing::debug!(
                "no project config at {}, returning defaults",
                path.display()
            );
            Ok(DEFAULT_PROJECT_CONFIG.to_string())
        }
    }
}

/// Save project settings to `.knot/config.json`. Creates the `.knot/` directory
/// if it doesn't exist.
#[tauri::command]
pub async fn save_project_settings(
    workspace_root: String,
    json: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<(), String> {
    let root = lookup_workspace_root(&workspace_root, &state)?;
    let path = config_path(&root);
    // Create .knot/ directory if it doesn't exist.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create .knot directory: {e}"))?;
    }
    fs::write(&path, &json)
        .await
        .map_err(|e| format!("failed to write project config: {e}"))?;
    tracing::info!("saved project config to {}", path.display());
    Ok(())
}

/// Migrate `.vscode/knot.json` → `.knot/config.json` if the old file exists
/// and the new one doesn't. Returns `true` if migration was performed,
/// `false` if no migration was needed.
///
/// Steps:
/// 1. If `.knot/config.json` already exists → return `false` (already migrated).
/// 2. If `.vscode/knot.json` doesn't exist → return `false` (nothing to migrate).
/// 3. Copy `.vscode/knot.json` → `.vscode/knot.json.bak` (backup).
/// 4. Parse the old config, extract known fields, write new `.knot/config.json`.
#[tauri::command]
pub async fn migrate_vscode_config(
    workspace_root: String,
    state: State<'_, WorkspaceRoot>,
) -> Result<bool, String> {
    let root = lookup_workspace_root(&workspace_root, &state)?;
    let new_path = config_path(&root);
    let old_path = vscode_config_path(&root);

    // 1. If new config already exists, no migration needed.
    if new_path.exists() {
        return Ok(false);
    }

    // 2. If old config doesn't exist, nothing to migrate.
    if !old_path.exists() {
        return Ok(false);
    }

    tracing::info!("migrating {} → {}", old_path.display(), new_path.display());

    // 3. Read the old config.
    let old_content = fs::read_to_string(&old_path)
        .await
        .map_err(|e| format!("failed to read .vscode/knot.json: {e}"))?;

    // 4. Write backup (.vscode/knot.json.bak).
    let backup_path = old_path.with_extension("json.bak");
    fs::write(&backup_path, &old_content)
        .await
        .map_err(|e| format!("failed to write backup: {e}"))?;
    tracing::info!("wrote backup to {}", backup_path.display());

    // 5. Parse old config + migrate to new format.
    let migrated = migrate_config_content(&old_content);

    // 6. Write new config.
    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create .knot directory: {e}"))?;
    }
    fs::write(&new_path, &migrated)
        .await
        .map_err(|e| format!("failed to write .knot/config.json: {e}"))?;

    tracing::info!("migration complete — new config at {}", new_path.display());
    Ok(true)
}

/// Parse the old `.vscode/knot.json` content + produce new-format JSON.
/// The old format had fields like `format`, `tweegoPath`, etc. The new format
/// uses `storyFormat`, `buildConfig.tweegoPath`, etc. Unknown fields are dropped.
fn migrate_config_content(old_content: &str) -> String {
    let old: serde_json::Value = match serde_json::from_str(old_content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("failed to parse old config as JSON, using defaults: {e}");
            return DEFAULT_PROJECT_CONFIG.to_string();
        }
    };
    let old_obj = old.as_object();

    // Build the new config object, pulling known fields from the old one.
    let story_format = old_obj
        .and_then(|o| o.get("format"))
        .and_then(|v| v.as_str())
        .unwrap_or("sugarcube");

    let tweego_path = old_obj
        .and_then(|o| o.get("tweegoPath"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut build_config = serde_json::Map::new();
    build_config.insert(
        "outputDir".to_string(),
        serde_json::Value::String("build".to_string()),
    );
    build_config.insert(
        "outputFormat".to_string(),
        serde_json::Value::String("html".to_string()),
    );
    build_config.insert("tweegoFlags".to_string(), serde_json::Value::Array(vec![]));
    if !tweego_path.is_empty() {
        build_config.insert(
            "tweegoPath".to_string(),
            serde_json::Value::String(tweego_path.to_string()),
        );
    }

    let mut new_obj = serde_json::Map::new();
    new_obj.insert(
        "storyFormat".to_string(),
        serde_json::Value::String(story_format.to_string()),
    );
    new_obj.insert(
        "buildConfig".to_string(),
        serde_json::Value::Object(build_config),
    );
    new_obj.insert(
        "includePatterns".to_string(),
        serde_json::Value::Array(vec![]),
    );
    new_obj.insert(
        "excludePatterns".to_string(),
        serde_json::Value::Array(vec![]),
    );
    new_obj.insert(
        "storymapLayout".to_string(),
        serde_json::Value::String("manual".to_string()),
    );

    serde_json::to_string_pretty(&serde_json::Value::Object(new_obj))
        .unwrap_or_else(|_| DEFAULT_PROJECT_CONFIG.to_string())
}

/// Lock the tracked workspace root state + validate the frontend-provided
/// path against it. Thin wrapper around the shared pure helper in
/// `workspace.rs`. Kept here so the three `#[tauri::command]` functions above
/// don't each repeat the lock+validate dance.
fn lookup_workspace_root(
    workspace_root: &str,
    state: &State<'_, WorkspaceRoot>,
) -> Result<PathBuf, String> {
    let tracked = state.0.lock().unwrap().clone().ok_or("workspace not set")?;
    validate_workspace_root(workspace_root, &tracked)
}
