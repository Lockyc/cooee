//! The menu-bar item: cooee's only always-present surface.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Runtime};

pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, "quit", "Quit cooee", true, Some("Cmd+Q"))?;
    let menu = Menu::with_items(app, &[&quit])?;
    TrayIconBuilder::with_id("cooee")
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .icon_as_template(false)
        .tooltip("cooee")
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                app.exit(0);
            }
        })
        .build(app)?;
    Ok(())
}
