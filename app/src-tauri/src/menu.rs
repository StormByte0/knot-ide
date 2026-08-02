//! Native menu bar — per-platform (macOS screen-top, Windows/Linux in-window).
//!
//! Menu items emit `menu-action` events to the frontend, which listens and
//! dispatches. Only File menu items are wired for the spike; Edit/View/Build/Help
//! are stubs for later phases.
//!
//! ## Per-window menu (Task 5)
//!
//! The menu is set ONLY on the main window, not globally. Child windows
//! (detached tabs created via `WebviewWindow` from the frontend) have no
//! menu bar — they're thin view windows, not full app shells. This avoids
//! `app.set_menu()` which sets a global default inherited by ALL windows on
//! Windows/Linux.

use tauri::{AppHandle, Emitter, Manager};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

/// Build and set the application menu on the main window only. Called once
/// during app setup.
pub fn setup_menu(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let menu = Menu::with_items(app, &[
        // ── File ──
        &Submenu::with_items(app, "File", true, &[
            &MenuItem::with_id(app, "new-file", "New File…", true, Some("Ctrl+N"))?,
            &MenuItem::with_id(app, "new-folder", "New Folder…", true, Some("Ctrl+Shift+N"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "open-folder", "Open Folder…", true, Some("Ctrl+O"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "save", "Save", true, Some("Ctrl+S"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "settings", "Settings…", true, Some("Ctrl+,"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit", true, Some("Ctrl+Q"))?,
        ])?,
        // ── Edit ──
        &Submenu::with_items(app, "Edit", true, &[
            &PredefinedMenuItem::undo(app, Some("Undo"))?,
            &PredefinedMenuItem::redo(app, Some("Redo"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some("Cut"))?,
            &PredefinedMenuItem::copy(app, Some("Copy"))?,
            &PredefinedMenuItem::paste(app, Some("Paste"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "find", "Find…", true, Some("Ctrl+F"))?,
            &MenuItem::with_id(app, "rename", "Rename…", true, Some("F2"))?,
        ])?,
        // ── View ──
        &Submenu::with_items(app, "View", true, &[
            &MenuItem::with_id(app, "toggle-file-browser", "Toggle File Browser", true, Some("Ctrl+Shift+E"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "zoom-in", "Zoom In", true, Some("Ctrl+="))?,
            &MenuItem::with_id(app, "zoom-out", "Zoom Out", true, Some("Ctrl+-"))?,
            &MenuItem::with_id(app, "reset-zoom", "Reset Zoom", true, Some("Ctrl+0"))?,
        ])?,
        // ── Build ──
        &Submenu::with_items(app, "Build", true, &[
            &MenuItem::with_id(app, "build", "Build Story", true, Some("Ctrl+Shift+B"))?,
            &MenuItem::with_id(app, "play", "Play Story", true, Some("F5"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "watch-toggle", "Toggle Watch", true, Some("Ctrl+Shift+W"))?,
        ])?,
        // ── Help ──
        &Submenu::with_items(app, "Help", true, &[
            &MenuItem::with_id(app, "documentation", "Documentation", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "check-updates", "Check for Updates…", true, None::<&str>)?,
            &MenuItem::with_id(app, "about", "About Knot", true, None::<&str>)?,
        ])?,
    ])?;

    // Set the menu ONLY on the main window, not globally. `app.set_menu()`
    // would set a global default inherited by all child windows on
    // Windows/Linux — child windows (detached tabs) should have no menu bar.
    match app.get_webview_window("main") {
        Some(main_window) => {
            main_window.set_menu(menu)?;
        }
        None => {
            tracing::warn!("main window not found during menu setup — menu not set");
        }
    }
    Ok(())
}

/// Handle menu item clicks. Emits a `menu-action` event with the item id
/// to the frontend, which dispatches.
pub fn handle_menu_event(app: &AppHandle, id: &str) {
    // Log the action for debugging.
    tracing::debug!(menu_action = id, "menu item clicked");

    // Forward to frontend.
    let _ = app.emit("menu-action", id);

    // Some actions are handled backend-side.
    match id {
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}
