//! The macOS menu bar: Tauri's default, less "Close Window".
//!
//! Tauri gives a macOS app a default menu whose File and Window menus carry
//! "Close Window" on ⌘W. Axiomata is one window, and ⌘W belongs to what is
//! inside it — the file app's tabs, the file panel, the IDE's tabs
//! (`docs/plans/editor.md` D16, W7, W11) — so closing the whole app on that key
//! would lose the owner's place. Everything else is kept as the default has
//! it; the Edit menu in particular, which is what makes ⌘C/⌘V/⌘Z reach text
//! fields in a web view on macOS. ⌘Q still quits.

use tauri::menu::{AboutMetadata, Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Runtime};

/// Builds the app's menu bar (macOS only; other platforms keep no menu bar).
pub fn app_menu<R: Runtime>(handle: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let info = handle.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        ..Default::default()
    };
    let app = Submenu::with_items(
        handle,
        info.name.clone(),
        true,
        &[
            &PredefinedMenuItem::about(handle, None, Some(about))?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::services(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::hide(handle, None)?,
            &PredefinedMenuItem::hide_others(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::quit(handle, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        handle,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(handle, None)?,
            &PredefinedMenuItem::redo(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::cut(handle, None)?,
            &PredefinedMenuItem::copy(handle, None)?,
            &PredefinedMenuItem::paste(handle, None)?,
            &PredefinedMenuItem::select_all(handle, None)?,
        ],
    )?;
    let view = Submenu::with_items(
        handle,
        "View",
        true,
        &[&PredefinedMenuItem::fullscreen(handle, None)?],
    )?;
    let window = Submenu::with_items(
        handle,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(handle, None)?,
            &PredefinedMenuItem::maximize(handle, None)?,
        ],
    )?;
    // Empty, as in Tauri's default: macOS puts its search-the-menus field there.
    let help = Submenu::with_items(handle, "Help", true, &[])?;
    Menu::with_items(handle, &[&app, &edit, &view, &window, &help])
}
