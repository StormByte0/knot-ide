//! Native menu bar — per-platform (macOS screen-top, Windows/Linux in-window).
//!
//! Menu items emit `menu-action` events to the frontend, which listens and
//! dispatches. Only File menu items are wired for the spike; Edit/View/Build/Help
//! are stubs for later phases.

use tauri::{AppHandle, Emitter};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

/// Build and set the application menu. Called once during app setup.
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

    app.set_menu(menu)?;
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
