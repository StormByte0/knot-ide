//! Filesystem watcher — recursively watches the workspace for changes and
//! emits `fs-changed` events to the frontend.
//!
//! Uses `notify-debouncer-mini` to coalesce rapid events (editors that write
//! in chunks, renames that produce create+delete, etc.) into single notifications.

use notify_debouncer_mini::{new_debouncer, DebouncedEvent};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tracing::{info, warn};

/// Holds the debouncer so it isn't dropped (which would stop watching).
/// The type parameter is the underlying watcher kind — `notify::FsEventWatcher`
/// is the default on all desktop platforms, but it's feature-gated, so we
/// use `notify::RecommendedWatcher` instead which is always available.
pub struct WatcherState(pub Mutex<Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>>);

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsChangedEvent {
    /// "create" | "modify" | "remove" | "rename"
    pub kind: String,
    /// Full path of the changed file/dir.
    pub path: String,
}

/// Start watching the workspace root. Replaces any existing watcher.
#[tauri::command]
pub fn watch_workspace(
    root_path: String,
    app: AppHandle,
    state: State<'_, WatcherState>,
) -> Result<(), String> {
    // Stop any existing watcher.
    {
        let mut guard = state.0.lock().unwrap();
        *guard = None;
    }

    let root = PathBuf::from(&root_path);
    if !root.exists() {
        return Err(format!("workspace root does not exist: {}", root.display()));
    }

    // Debounce: coalesce events within 200ms into a single notification.
    // notify-debouncer-mini 0.5 changed the callback signature to
    // `Result<Vec<DebouncedEvent>, notify::Error>` (single error, not Vec).
    let app_handle = app.clone();
    let mut debouncer = new_debouncer(
        Duration::from_millis(200),
        move |result: Result<Vec<DebouncedEvent>, notify::Error>| {
            match result {
                Ok(events) => {
                    for event in events {
                        let path = event.path.to_string_lossy().into_owned();
                        let kind = "modify";
                        info!(kind = kind, path = %path, "fs event");
                        let _ = app_handle.emit(
                            "fs-changed",
                            FsChangedEvent { kind: kind.to_string(), path },
                        );
                    }
                }
                Err(e) => {
                    warn!(error = %e, "watcher error");
                }
            }
        },
    )
    .map_err(|e| format!("failed to create watcher: {e}"))?;

    // Recursively watch the workspace root.
    debouncer
        .watcher()
        .watch(&root, notify::RecursiveMode::Recursive)
        .map_err(|e| format!("failed to start watching: {e}"))?;

    info!(root = %root.display(), "workspace watcher started");

    // Store the debouncer so it stays alive.
    {
        let mut guard = state.0.lock().unwrap();
        *guard = Some(debouncer);
    }

    Ok(())
}

/// Stop watching (called on workspace close or app shutdown).
#[tauri::command]
pub fn stop_watching(state: State<'_, WatcherState>) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    *guard = None;
    info!("workspace watcher stopped");
    Ok(())
}
