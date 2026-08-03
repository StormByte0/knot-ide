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

/// Run `<tweegoPath> --version` and return the version string.
///
/// Used to populate the status bar's "Tweego: <version>" item. Combines with
/// `detect_tweego` (which finds the path) — this verifies the binary actually
/// runs + extracts its version.
///
/// Returns `Ok(Some(version))` on success, `Ok(None)` if the path is empty or
/// the binary can't be executed, and `Err` only on unexpected failures (the
/// frontend treats `None` as "not configured" silently — no error toast).
///
/// ## Parsing
///
/// Tweego's `--version` output looks like:
/// ```text
/// Tweego (a Twee compiler) 2.1.1
/// ```
///
/// We return the first whitespace-separated token after the last space, which
/// captures `2.1.1`. If parsing fails (unexpected output format), we return
/// the raw stdout trimmed — better to show something than nothing.
#[tauri::command]
pub async fn detect_tweego_version(tweego_path: String) -> Result<Option<String>, String> {
    if tweego_path.trim().is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(&tweego_path);
    if !path.exists() {
        tracing::info!("tweego path does not exist: {}", tweego_path);
        return Ok(None);
    }
    // Spawn `--version` (not `--help` — `--version` is fast + universally
    // supported). Capture stdout + stderr separately; we only parse stdout.
    let output = tokio::process::Command::new(&path)
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("failed to spawn tweego: {e}"))?;
    if !output.status.success() {
        tracing::info!(
            "tweego --version exited with status {} (path: {})",
            output.status,
            tweego_path
        );
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = parse_tweego_version(&stdout);
    tracing::info!("detected tweego version: {} (from: {:?})", version, stdout.trim());
    Ok(Some(version))
}

/// Parse the version string from `tweego --version` output.
///
/// Extracts the last whitespace-separated token, which is the version number
/// in Tweego's standard output format (`Tweego (a Twee compiler) 2.1.1`).
/// Falls back to the trimmed stdout if parsing fails.
fn parse_tweego_version(stdout: &str) -> String {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    // Take the last whitespace-separated token. `split_whitespace().last()`
    // is equivalent to the (non-existent) `rsplit_whitespace().next()`.
    let last_token = trimmed.split_whitespace().last();
    match last_token {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => trimmed.to_string(),
    }
}
