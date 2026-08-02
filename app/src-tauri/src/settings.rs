//! Editor-level settings — global per-user settings stored in `<appData>/settings.json`.
//!
//! These are user preferences that apply across all workspaces: font, tab size,
//! word wrap, minimap, theme, Tweego executable path. Loaded/saved via Tauri
//! commands that resolve the app data directory.
//!
//! ## File location
//!
//! - Windows: `C:\Users\<user>\AppData\Roaming\dev.knot.ide\settings.json`
//! - macOS:   `~/Library/Application Support/dev.knot.ide/settings.json`
//! - Linux:   `~/.local/share/dev.knot.ide/settings.json`

use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tokio::fs;

/// Default editor settings JSON (returned when no settings file exists).
const DEFAULT_EDITOR_SETTINGS: &str = r#"{
  "fontFamily": "Consolas, \"Courier New\", monospace",
  "fontSize": 14,
  "tabSize": 2,
  "wordWrap": "on",
  "minimap": true,
  "bracketPairColorization": true,
  "theme": "vs-dark",
  "tweegoPath": null
}"#;

/// Resolve the path to `<appData>/settings.json`.
fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

/// Load editor settings from `<appData>/settings.json`. Returns the raw JSON
/// string (the frontend parses it). If the file doesn't exist, returns defaults.
#[tauri::command]
pub async fn load_editor_settings(app: AppHandle) -> Result<String, String> {
    let path = settings_path(&app)?;
    match fs::read_to_string(&path).await {
        Ok(content) => Ok(content),
        Err(_) => {
            tracing::debug!(
                "no editor settings at {}, returning defaults",
                path.display()
            );
            Ok(DEFAULT_EDITOR_SETTINGS.to_string())
        }
    }
}

/// Save editor settings to `<appData>/settings.json`. Creates the directory
/// if it doesn't exist.
#[tauri::command]
pub async fn save_editor_settings(app: AppHandle, json: String) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(|e| {
            format!("failed to create app data directory: {e}")
        })?;
    }
    fs::write(&path, &json).await.map_err(|e| {
        format!("failed to write editor settings: {e}")
    })?;
    tracing::info!("saved editor settings to {}", path.display());
    Ok(())
}

/// Detect the Tweego executable path by scanning PATH + common install locations.
/// Returns the path string if found, or `null` if not found.
#[tauri::command]
pub async fn detect_tweego() -> Result<Option<String>, String> {
    // 1. Try `which` to find tweego on PATH.
    if let Ok(path) = which::which("tweego") {
        let path_str = path.to_string_lossy().to_string();
        tracing::info!("detected tweego on PATH: {}", path_str);
        return Ok(Some(path_str));
    }
    // 2. Try common Windows install locations.
    #[cfg(target_os = "windows")]
    {
        if let Some(prog_files) = std::env::var_os("ProgramFiles") {
            let candidate = PathBuf::from(prog_files).join("Tweego").join("tweego.exe");
            if candidate.exists() {
                let path_str = candidate.to_string_lossy().to_string();
                tracing::info!("detected tweego at Program Files: {}", path_str);
                return Ok(Some(path_str));
            }
        }
        if let Some(prog_files_x86) = std::env::var_os("ProgramFiles(x86)") {
            let candidate = PathBuf::from(prog_files_x86).join("Tweego").join("tweego.exe");
            if candidate.exists() {
                let path_str = candidate.to_string_lossy().to_string();
                tracing::info!("detected tweego at ProgramFiles(x86): {}", path_str);
                return Ok(Some(path_str));
            }
        }
    }
    // 3. Try common macOS/Linux locations.
    #[cfg(not(target_os = "windows"))]
    {
        for candidate in [
            "/usr/local/bin/tweego",
            "/usr/bin/tweego",
            "/opt/tweego/tweego",
        ] {
            if PathBuf::from(candidate).exists() {
                tracing::info!("detected tweego at: {}", candidate);
                return Ok(Some(candidate.to_string()));
            }
        }
    }
    tracing::info!("tweego not detected on PATH or common locations");
    Ok(None)
}
