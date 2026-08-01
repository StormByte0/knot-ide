//! Knot app — Tauri 2 backend.

mod fs_ops;
mod lsp;
mod menu;
mod watcher;

use fs_ops::WorkspaceRoot;
use lsp::LspSupervisor;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tracing_subscriber::EnvFilter;
use watcher::WatcherState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,knot_app_lib=debug")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(LspSupervisor::new())
        .manage(WorkspaceRoot(Mutex::new(None)))
        .manage(WatcherState(Mutex::new(None)))
        .setup(|app| {
            let app_handle = app.handle().clone();
            if let Err(e) = menu::setup_menu(&app_handle) {
                tracing::warn!(error = %e, "failed to set up menu");
            }
            tauri::async_runtime::spawn(async move {
                let state = app_handle.state::<LspSupervisor>();
                if let Err(e) = lsp::spawn_server(app_handle.clone(), state).await {
                    tracing::error!(error = %e, "failed to start knot-server on launch");
                    let _ = app_handle.emit("lsp-start-failed", e);
                }
            });
            Ok(())
        })
        .on_menu_event(|app, event| {
            menu::handle_menu_event(app, event.id().as_ref());
        })
        .invoke_handler(tauri::generate_handler![
            lsp::lsp_send,
            lsp::lsp_start,
            fs_ops::list_dir,
            fs_ops::create_file,
            fs_ops::create_dir,
            fs_ops::rename_path,
            fs_ops::delete_path,
            fs_ops::copy_file,
            fs_ops::set_workspace_root,
            watcher::watch_workspace,
            watcher::stop_watching,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
