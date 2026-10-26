#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;
mod widget;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, PhysicalPosition};

fn main() {
    tauri::Builder::default()
        // A second launch must not open a second widget; it just exits.
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .setup(|app| {
            let settings = MenuItemBuilder::with_id("settings", "Settings\u{2026}")
                .enabled(false)
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit Shepit").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&settings)
                .separator()
                .item(&quit)
                .build()?;

            let mut tray = TrayIconBuilder::new()
                .tooltip("Shepit")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if tray::action_for(event.id().as_ref()) == tray::MenuAction::Quit {
                        app.exit(0);
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if let Some(window) = app.get_webview_window("widget") {
                if let Some(monitor) = app.primary_monitor()? {
                    let area = monitor.work_area();
                    let gap = (4.0 * monitor.scale_factor()).round() as i32;
                    let size = window.outer_size()?;
                    let (x, y) = widget::default_position(
                        (
                            area.position.x,
                            area.position.y,
                            area.size.width as i32,
                            area.size.height as i32,
                        ),
                        (size.width as i32, size.height as i32),
                        gap,
                    );
                    window.set_position(PhysicalPosition::new(x, y))?;
                }
                window.show()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Shepit");
}
