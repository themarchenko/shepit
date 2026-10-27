//! The Settings and tour windows. Call these from a thread that is not the main thread (a tray
//! handler hands over to a thread first): building a window waits for the main thread, which would
//! wait on itself. Nothing here is testable without a window system; Windows CI compiles it.

use crate::controller::{Msg, PickMsg};
use crate::settings_store;
use crate::widget_host::WidgetHost;
use serde::Serialize;
use shepit_core::settings::Settings;
use std::sync::Mutex;
use std::time::Duration;
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

pub const SETTINGS: &str = "settings";
pub const TOUR: &str = "tour";
/// The History window (HistoryView.swift:42,65-68).
pub const HISTORY: &str = "history";
pub const HISTORY_TITLE: &str = "Shepit History";
pub const HISTORY_SIZE: (f64, f64) = (760.0, 520.0);
pub const HISTORY_MIN_SIZE: (f64, f64) = (620.0, 420.0);

/// The tour opens this long after setup (AppDelegate.swift:59-65), once the tray and the widget
/// are up.
const FIRST_RUN_DELAY: Duration = Duration::from_millis(600);

/// Two opens at once would both find no window and the second build would fail on the label.
static OPENING: Mutex<()> = Mutex::new(());

/// Sends `event` to the Settings and tour windows that are open. The widget never gets it.
pub fn emit_to_windows<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    for label in [SETTINGS, TOUR] {
        if app.get_webview_window(label).is_some() {
            if let Err(err) = app.emit_to(label, event, payload.clone()) {
                eprintln!("could not send {event} to {label}: {err}");
            }
        }
    }
}

fn focus(window: &WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Opens Settings, or focuses it when it is already open.
pub fn open_settings(app: &AppHandle) -> Result<(), String> {
    let _opening = OPENING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(window) = app.get_webview_window(SETTINGS) {
        focus(&window);
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title("Shepit Settings")
        .inner_size(620.0, 600.0)
        .resizable(false)
        .maximizable(false)
        // The dark title bar and the window colour keep it from flashing white before the page paints.
        .theme(Some(Theme::Dark))
        .background_color(Color(0x1f, 0x20, 0x23, 0xff))
        .skip_taskbar(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
        ) {
            cancel_pick(&handle);
        }
    });
    Ok(())
}

/// Opens the History window, or focuses it when it is already open.
pub fn open_history(app: &AppHandle) -> Result<(), String> {
    let _opening = OPENING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(window) = app.get_webview_window(HISTORY) {
        focus(&window);
        return Ok(());
    }
    WebviewWindowBuilder::new(app, HISTORY, WebviewUrl::App("history.html".into()))
        .title(HISTORY_TITLE)
        .inner_size(HISTORY_SIZE.0, HISTORY_SIZE.1)
        .min_inner_size(HISTORY_MIN_SIZE.0, HISTORY_MIN_SIZE.1)
        .resizable(true)
        .theme(Some(Theme::Dark))
        .background_color(Color(0x1f, 0x20, 0x23, 0xff))
        .skip_taskbar(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Tells an open History window that a row was added or the history was cleared.
pub fn emit_history_changed(app: &AppHandle) {
    if app.get_webview_window(HISTORY).is_some() {
        if let Err(err) = app.emit_to(HISTORY, "history-changed", ()) {
            eprintln!("could not send history-changed: {err}");
        }
    }
}

/// Opens the welcome tour, or focuses it when it is already open.
pub fn open_tour(app: &AppHandle) -> Result<(), String> {
    let _opening = OPENING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(window) = app.get_webview_window(TOUR) {
        focus(&window);
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, TOUR, WebviewUrl::App("tour.html".into()))
        .title("Welcome to Shepit")
        .inner_size(620.0, 540.0)
        // No title bar, but Windows' own shadow, a normal window level and a taskbar entry
        // (DECISIONS.md:314-319). The page marks its background as the drag region.
        .decorations(false)
        .shadow(true)
        .resizable(false)
        .maximizable(false)
        .background_color(Color(0x0e, 0x0e, 0x10, 0xff))
        .skip_taskbar(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    let handle = app.clone();
    // Finish, Skip, Esc and Alt+F4 all end here, so every way out marks the tour as done.
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
        ) {
            cancel_pick(&handle);
            let handle = handle.clone();
            // The handler runs on the main thread, and saving may wait for it.
            std::thread::spawn(move || mark_tour_done(&handle));
        }
    });
    Ok(())
}

/// Marks the tour done and closes it.
pub fn finish_tour(app: &AppHandle) {
    mark_tour_done(app);
    if let Some(window) = app.get_webview_window(TOUR) {
        let _ = window.destroy();
    }
}

fn mark_tour_done(app: &AppHandle) {
    let result = settings_store::apply(app, |current| {
        Ok(Settings {
            onboarding_completed: true,
            ..current.clone()
        })
    });
    if let Err(err) = result {
        eprintln!("could not save that the tour is done: {err}");
    }
}

/// A window that held the key picker went away: the hook must stop diverting keys.
fn cancel_pick(app: &AppHandle) {
    let host = app.state::<WidgetHost>();
    // A full channel drops this; the picker still ends by itself after 15 s.
    let _ = host.send(Msg::Pick(PickMsg::Cancel));
}

/// Opens the tour shortly after launch when it was never finished.
pub fn show_tour_on_first_run(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_RUN_DELAY);
        if app.state::<WidgetHost>().settings().onboarding_completed {
            return;
        }
        if let Err(err) = open_tour(&app) {
            eprintln!("could not open the tour: {err}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_history_window_is_labelled_titled_and_sized_like_macos() {
        assert_eq!(HISTORY, "history");
        assert_eq!(HISTORY_TITLE, "Shepit History");
        assert_eq!(HISTORY_SIZE, (760.0, 520.0));
        assert_eq!(HISTORY_MIN_SIZE, (620.0, 420.0));
    }

    #[test]
    fn the_windows_have_distinct_labels() {
        let labels = [SETTINGS, TOUR, HISTORY];
        for (i, a) in labels.iter().enumerate() {
            for b in &labels[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
