#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod controller;
mod hook;
mod recorder;
mod tray;
mod widget;

use controller::{Msg, Runner};
use recorder::CpalRecorder;
use shepit_core::dictation::{Phase, View};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

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
                    let size = window.outer_size()?;
                    let (x, y) = widget::default_position(
                        (
                            area.position.x,
                            area.position.y,
                            area.size.width as i32,
                            area.size.height as i32,
                        ),
                        (size.width as i32, size.height as i32),
                        monitor.scale_factor(),
                    );
                    window.set_position(PhysicalPosition::new(x, y))?;
                }
                window.show()?;
            }

            let (tx, rx) = mpsc::channel::<Msg>();
            hook::start(tx.clone());
            let handle = app.handle().clone();
            // The recorder is built inside the thread: a cpal Stream may not be Send.
            std::thread::Builder::new()
                .name("controller".into())
                .spawn(move || run_controller(handle, tx, rx))?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Shepit");
}

/// Owns the dictation controller: feeds it hook and level messages, polls it every 100 ms and
/// pushes each view change to the widget.
fn run_controller(app: AppHandle, tx: mpsc::Sender<Msg>, rx: Receiver<Msg>) {
    let epoch = Instant::now();
    let mut runner = Runner::new(CpalRecorder::new(tx));
    let mut listening_since: Option<Instant> = None;
    loop {
        let msg = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => Msg::Tick,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // A tick after every message keeps the failed reset on time while keys keep arriving.
        let now = Instant::now();
        let changed = [
            runner.step_at(msg, epoch, now),
            runner.step_at(Msg::Tick, epoch, now),
        ]
        .into_iter()
        .flatten()
        .last();
        let Some(view) = changed else { continue };
        match (view.phase, listening_since) {
            (Phase::Listening, None) => listening_since = Some(now),
            (Phase::Listening, Some(_)) => {}
            (_, Some(start)) => {
                println!("capture stopped, {:.1} s", start.elapsed().as_secs_f64());
                listening_since = None;
            }
            (_, None) => {}
        }
        apply_view(&app, &view);
    }
}

/// Resizes and re-seats the widget for the view, then sends it the state.
fn apply_view(app: &AppHandle, view: &View) {
    let Some(window) = app.get_webview_window("widget") else {
        return;
    };
    if let Err(err) = place(&window, view) {
        eprintln!("could not place the widget: {err}");
    }
    let state = widget::WidgetState::from_view(view);
    if let Err(err) = app.emit_to("widget", "widget-state", state) {
        eprintln!("could not send the widget state: {err}");
    }
}

fn place(window: &WebviewWindow, view: &View) -> tauri::Result<()> {
    let (w, h) = widget::window_size(view);
    window.set_size(LogicalSize::new(w, h))?;
    if let Some(monitor) = window.primary_monitor()? {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let (x, y) = widget::default_position(
            (
                area.position.x,
                area.position.y,
                area.size.width as i32,
                area.size.height as i32,
            ),
            ((w * scale).round() as i32, (h * scale).round() as i32),
            scale,
        );
        window.set_position(PhysicalPosition::new(x, y))?;
    }
    Ok(())
}
