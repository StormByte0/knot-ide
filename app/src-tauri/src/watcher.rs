//! Filesystem watcher — recursively watches the workspace for changes and
//! emits `fs-changed` events to the frontend.
//!
//! Uses `notify-debouncer-full` (not `mini`) because it preserves
//! `EventKind` (create / remove / rename / modify) and pairs rename from/to
//! events into a single event with both paths. The file browser uses the kind
//! to skip unnecessary refreshes on file-content-only `modify` events.
//!
//! ## Event kinds emitted
//!
//! | `notify::EventKind`                         | Emitted `kind`  | `old_path` |
//! |---------------------------------------------|-----------------|------------|
//! | `Create(_)`                                 | `"create"`      | `None`     |
//! | `Remove(_)`                                 | `"remove"`      | `None`     |
//! | `Modify(Name(RenameMode::Both))` + 2 paths  | `"rename"`      | `Some(from)` |
//! | `Modify(Name(RenameMode::From))`            | `"remove"`      | `None`     |
//! | `Modify(Name(RenameMode::To))`              | `"create"`      | `None`     |
//! | `Modify(Name(_))` (unpaired, unknown)       | `"modify"`      | `None`     |
//! | `Modify(_)` (data / metadata)               | `"modify"`      | `None`     |
//! | `Access(_)`                                 | — (skipped)     | —          |
//! | `Any` / `Other`                             | — (skipped)     | —          |

use notify_debouncer_full::{
    new_debouncer, notify::event::{ModifyKind, RenameMode}, notify::{EventKind, RecursiveMode},
    Debouncer, RecommendedCache,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tracing::{info, warn};

/// Re-exported watcher type so the rest of the crate doesn't need to know
/// about the debouncer's generic parameters.
pub type WorkspaceWatcher = Debouncer<notify_debouncer_full::notify::RecommendedWatcher, RecommendedCache>;

/// Holds the debouncer so it isn't dropped (which would stop watching).
pub struct WatcherState(pub Mutex<Option<WorkspaceWatcher>>);

/// Payload emitted to the frontend via the `fs-changed` event.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsChangedEvent {
    /// `"create"` | `"remove"` | `"rename"` | `"modify"`
    pub kind: String,
    /// Full path of the changed file/dir. For rename, this is the **new** path.
    pub path: String,
    /// For `"rename"` only: the previous path that was renamed away.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
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
    let app_handle = app.clone();
    let mut debouncer = new_debouncer(
        Duration::from_millis(200),
        None,
        move |result: notify_debouncer_full::DebounceEventResult| {
            match result {
                Ok(events) => {
                    for event in events {
                        emit_fs_events(&app_handle, &event);
                    }
                }
                Err(errors) => {
                    for err in errors {
                        warn!(error = %err, "watcher error");
                    }
                }
            }
        },
    )
    .map_err(|e| format!("failed to create watcher: {e}"))?;

    // Recursively watch the workspace root.
    debouncer
        .watch(&root, RecursiveMode::Recursive)
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

/// Map a single `notify::Event` to zero or more `fs-changed` emissions.
///
/// Most events have exactly one path and produce one emission. A paired
/// rename (`RenameMode::Both` with 2 paths) produces one `"rename"` emission
/// with `old_path` set. `Access` / `Any` / `Other` events are skipped
/// entirely (non-mutating or meta).
fn emit_fs_events(app: &AppHandle, event: &notify_debouncer_full::notify::Event) {
    if event.paths.is_empty() {
        return;
    }

    match event.kind {
        // Paired rename: paths[0] = from, paths[1] = to.
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            let from = event.paths[0].to_string_lossy().into_owned();
            let to = event.paths[1].to_string_lossy().into_owned();
            info!(kind = "rename", from = %from, to = %to, "fs event");
            let _ = app.emit(
                "fs-changed",
                FsChangedEvent { kind: "rename".to_string(), path: to, old_path: Some(from) },
            );
        }

        // Unpaired rename-from: file was renamed away, we don't know where to.
        // Treat as a remove so the frontend refreshes the parent (node disappears).
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            emit_one(app, "remove", &event.paths[0], None);
        }

        // Unpaired rename-to: file appeared via rename, we don't know where from.
        // Treat as a create so the frontend refreshes the parent (node appears).
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            emit_one(app, "create", &event.paths[0], None);
        }

        EventKind::Create(_) => {
            emit_one(app, "create", &event.paths[0], None);
        }

        EventKind::Remove(_) => {
            emit_one(app, "remove", &event.paths[0], None);
        }

        EventKind::Modify(_) => {
            emit_one(app, "modify", &event.paths[0], None);
        }

        // Non-mutating access events and meta-events: skip.
        EventKind::Access(_) | EventKind::Any | EventKind::Other => {}
    }
}

/// Emit a single `fs-changed` event for `path`.
fn emit_one(app: &AppHandle, kind: &str, path: &std::path::Path, old_path: Option<String>) {
    let path_str = path.to_string_lossy().into_owned();
    info!(kind = kind, path = %path_str, "fs event");
    let _ = app.emit(
        "fs-changed",
        FsChangedEvent {
            kind: kind.to_string(),
            path: path_str,
            old_path,
        },
    );
}
