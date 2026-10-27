//! The update card window: a small glass window at the bottom right of the work area. It is
//! built at runtime so the pages that need it are the only ones that pay for it. It never takes
//! focus (like the widget), so the app being dictated into keeps the keyboard.

use crate::updater::CardPayload;
use crate::widget_host::set_frame;
use shepit_core::widget::Rect;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "update";

/// 380 logical pixels of card plus 20 on each side for the margin (UpdatePanel.swift:22, 130).
pub const CARD_WIDTH: f64 = 420.0;
pub const MIN_HEIGHT: f64 = 60.0;
pub const MAX_HEIGHT: f64 = 480.0;
/// Height of the window before the page reports the real one.
const FIRST_HEIGHT: f64 = 200.0;

/// Where the card sits: the bottom-right corner of the primary work area, physical pixels.
#[derive(Clone, Copy)]
struct Corner {
    right: i32,
    bottom: i32,
    scale: f64,
}

/// Also serializes `open`: two opens at once would both find no window and the second build would
/// fail on the label.
static CORNER: Mutex<Option<Corner>> = Mutex::new(None);

/// The card's frame in physical pixels, its bottom-right corner on (`right`, `bottom`). `height`
/// is logical and is clamped to `MIN_HEIGHT..=MAX_HEIGHT`; NaN counts as the minimum.
pub fn card_frame(right: i32, bottom: i32, scale: f64, height: f64) -> Rect {
    let height = if height.is_nan() {
        MIN_HEIGHT
    } else {
        height.clamp(MIN_HEIGHT, MAX_HEIGHT)
    };
    let width = (CARD_WIDTH * scale).round() as i32;
    let height = (height * scale).round() as i32;
    Rect {
        x: right - width,
        y: bottom - height,
        width,
        height,
    }
}

/// Opens the card, or does nothing when it is open. Call from a thread that is not the main
/// thread: building a window waits for it. The page shows itself through `resize` once it knows
/// its height.
pub fn open(app: &AppHandle) -> Result<(), String> {
    let mut corner = CORNER.lock().unwrap_or_else(|e| e.into_inner());
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    let monitor = app
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no monitor")?;
    let area = monitor.work_area();
    *corner = Some(Corner {
        right: area.position.x + area.size.width as i32,
        bottom: area.position.y + area.size.height as i32,
        scale: monitor.scale_factor(),
    });
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("update.html".into()))
        .title("Shepit update")
        .inner_size(CARD_WIDTH, FIRST_HEIGHT)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // The card must not take the keyboard from the app being dictated into.
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Sets the card height (logical pixels), keeps the bottom-right corner and shows the window.
pub fn resize(app: &AppHandle, height: f64) -> Result<(), String> {
    let Some(corner) = *CORNER.lock().unwrap_or_else(|e| e.into_inner()) else {
        return Ok(());
    };
    let frame = card_frame(corner.right, corner.bottom, corner.scale, height);
    let handle = app.clone();
    // The widget places its window on the main thread too; SetWindowPos from here could wait on it.
    app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(LABEL) else {
            return;
        };
        if let Err(err) = set_frame(&window, frame) {
            eprintln!("could not place the update card: {err}");
        }
        if !window.is_visible().unwrap_or(true) {
            let _ = window.show();
        }
    })
    .map_err(|e| e.to_string())
}

/// Sends the page its new content.
pub fn refresh(app: &AppHandle, card: &CardPayload) {
    if app.get_webview_window(LABEL).is_some() {
        if let Err(err) = app.emit_to(LABEL, "update-card", card) {
            eprintln!("could not send the update card: {err}");
        }
    }
}

pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(right: i32, bottom: i32, scale: f64, height: f64) -> (i32, i32, i32, i32) {
        let r = card_frame(right, bottom, scale, height);
        (r.x, r.y, r.width, r.height)
    }

    #[test]
    fn the_card_keeps_its_bottom_right_corner_at_100_percent() {
        assert_eq!(frame(1920, 1040, 1.0, 200.0), (1500, 840, 420, 200));
        // A taller card grows upward and the corner stays.
        assert_eq!(frame(1920, 1040, 1.0, 320.0), (1500, 720, 420, 320));
    }

    #[test]
    fn the_card_scales_with_the_display_at_150_percent() {
        assert_eq!(frame(2880, 1560, 1.5, 200.0), (2250, 1260, 630, 300));
    }

    #[test]
    fn sizes_round_to_whole_pixels() {
        // 420 * 1.25 = 525 and 201 * 1.25 = 251.25.
        assert_eq!(frame(2400, 1300, 1.25, 201.0), (1875, 1049, 525, 251));
    }

    #[test]
    fn the_height_is_clamped() {
        assert_eq!(frame(1920, 1040, 1.0, 10.0).3, 60);
        assert_eq!(frame(1920, 1040, 1.0, 5000.0).3, 480);
        assert_eq!(frame(1920, 1040, 1.5, 5000.0).3, 720);
        assert_eq!(frame(1920, 1040, 1.0, -3.0).3, 60);
    }

    #[test]
    fn a_nan_height_counts_as_the_minimum() {
        assert_eq!(frame(1920, 1040, 1.0, f64::NAN), (1500, 980, 420, 60));
    }
}
