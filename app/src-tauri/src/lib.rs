//! Knot app — Tauri 2 backend.

mod config;
mod fs_ops;
mod lsp;
mod menu;
mod settings;
mod watcher;

use fs_ops::WorkspaceRoot;
use lsp::{LspSupervisor};
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
                // Extract Arcs from state in a block so the State<'_> borrow
                // is dropped before spawn_server_impl is awaited — keeping the
                // future `Send`. See lsp.rs docs for the rationale.
                let arcs = {
                    let state = app_handle.state::<LspSupervisor>();
                    state.arcs()
                };
                if let Err(e) = lsp::spawn_server_impl(app_handle.clone(), arcs).await {
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
            fs_ops::create_dir_all,
            fs_ops::rename_path,
            fs_ops::delete_path,
            fs_ops::copy_file,
            fs_ops::set_workspace_root,
            watcher::watch_workspace,
            watcher::stop_watching,
            config::load_project_settings,
            config::save_project_settings,
            config::migrate_vscode_config,
            settings::load_editor_settings,
            settings::save_editor_settings,
            settings::detect_tweego,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
