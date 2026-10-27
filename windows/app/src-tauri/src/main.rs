#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod controller;
mod hook;
mod insert;
mod recognizer;
mod recorder;
mod tray;
mod widget;

use controller::{log_line, Msg, Runner};
use insert::Request;
use recognizer::ParakeetEngine;
use recorder::CpalRecorder;
use shepit_core::dictation::{Job, ModelStatus, Phase, View};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

/// Looked up later to change the tooltip while the model downloads.
const TRAY_ID: &str = "main";

/// Added to the job id base per controller restart, so a result from the old controller can never
/// match a job of the new one.
const JOB_ID_STRIDE: u64 = 1 << 32;

fn platform_desktop() -> impl insert::Desktop {
    insert::NullDesktop
}

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

            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
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

            // Bounded so a stalled controller cannot grow memory; senders drop on full.
            let (tx, rx) = mpsc::sync_channel::<Msg>(256);
            hook::start(tx.clone(), shepit_core::keys::VK_RCONTROL);
            // One recognizer for the whole run: it outlives controller restarts, so the model is
            // loaded once. It preloads (and downloads) at launch.
            let (job_tx, job_rx) = mpsc::channel::<Job>();
            let wanted = Arc::new(AtomicU64::new(0));
            {
                let (wanted, tx) = (wanted.clone(), tx.clone());
                std::thread::Builder::new()
                    .name("recognizer".into())
                    .spawn(move || recognizer::run(ParakeetEngine::new(), job_rx, wanted, tx))?;
            }
            // One worker for the target context and the paste: both need the same COM and
            // clipboard-owner state, which belongs to a single thread. The desktop is built inside
            // the thread because the owner window and COM apartment live there.
            let (insert_tx, insert_rx) = mpsc::channel::<Request>();
            {
                let tx = tx.clone();
                std::thread::Builder::new()
                    .name("insert".into())
                    .spawn(move || insert::run(platform_desktop(), insert_rx, tx))?;
            }
            let handle = app.handle().clone();
            // The recorder is built inside the thread: a cpal Stream may not be Send.
            std::thread::Builder::new()
                .name("controller".into())
                .spawn(move || run_controller(handle, tx, rx, job_tx, insert_tx, wanted))?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Shepit");
}

/// Where the controller sends work: the recognizer and the insert worker.
struct Workers {
    jobs: Sender<Job>,
    inserts: Sender<Request>,
}

/// Runs the controller loop and restarts it with a fresh controller if it panics, so the hotkey
/// keeps working. The receiver outlives the loop; only the controller state is lost.
fn run_controller(
    app: AppHandle,
    tx: SyncSender<Msg>,
    rx: Receiver<Msg>,
    jobs: Sender<Job>,
    inserts: Sender<Request>,
    wanted: Arc<AtomicU64>,
) {
    let workers = Workers { jobs, inserts };
    let mut job_id_base: u64 = 0;
    // Outlives the loop below: filled from every Model message before the controller sees it.
    let model = Arc::new(Mutex::new(ModelStatus::Loading));
    loop {
        // Ids start at 1; 0 means "no job wanted".
        let first_job_id = job_id_base + 1;
        let result = catch_unwind(AssertUnwindSafe(|| {
            controller_loop(&app, &tx, &rx, &workers, &wanted, first_job_id, &model)
        }));
        if result.is_ok() {
            return;
        }
        // No payload in the log: it could carry key data.
        eprintln!("controller thread panicked, restarting");
        job_id_base += JOB_ID_STRIDE;
        wanted.store(0, Ordering::SeqCst);
        apply_view(&app, &widget::idle_view(), true);
    }
}

/// Owns the dictation controller: feeds it hook and level messages, polls it every 100 ms and
/// pushes each view change to the widget.
fn controller_loop(
    app: &AppHandle,
    tx: &SyncSender<Msg>,
    rx: &Receiver<Msg>,
    workers: &Workers,
    wanted: &AtomicU64,
    first_job_id: u64,
    model: &Mutex<ModelStatus>,
) {
    let epoch = Instant::now();
    let known = *model.lock().unwrap_or_else(|e| e.into_inner());
    let mut runner = Runner::new(CpalRecorder::new(tx.clone()), first_job_id, known);
    let mut tooltip = String::from("Shepit");
    let mut last_view: Option<View> = None;
    let mut listening_since: Option<Instant> = None;
    loop {
        let msg = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => Msg::Tick,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let Msg::Model(status) = &msg {
            *model.lock().unwrap_or_else(|e| e.into_inner()) = *status;
            set_tooltip(app, &mut tooltip, tray::tooltip_for(*status));
        }
        if cfg!(debug_assertions) {
            if let Msg::Context { context, .. } = &msg {
                println!("{}", shepit_core::context::presence_flags_line(context));
            }
        }
        // A tick after every message keeps the failed reset on time while keys keep arriving.
        let now = Instant::now();
        let mut changed = [
            runner.step_at(msg, epoch, now),
            runner.step_at(Msg::Tick, epoch, now),
        ]
        .into_iter()
        .flatten()
        .last();
        // The wanted id goes first: the worker skips a job it has not started once Esc cleared it.
        wanted.store(runner.wanted(), Ordering::SeqCst);
        if let Some(job) = runner.take_job() {
            let id = job.id;
            if workers.jobs.send(job).is_err() {
                let fed = runner.step_at(
                    Msg::Recognized {
                        id,
                        result: Err("Recognizer is not running".into()),
                    },
                    epoch,
                    Instant::now(),
                );
                changed = fed.or(changed);
                wanted.store(runner.wanted(), Ordering::SeqCst);
            }
        }
        if let Some(id) = runner.take_capture() {
            // No worker means no context; recording and insertion carry on without it.
            let _ = workers.inserts.send(Request::Capture { id });
        }
        if let Some(job) = runner.take_insert() {
            if let Some(line) = log_line(&job.text, cfg!(debug_assertions)) {
                println!("{line}");
            }
            let id = job.id;
            if workers.inserts.send(Request::Insert(job)).is_err() {
                let fed = runner.step_at(
                    Msg::Inserted {
                        id,
                        result: Err("Insertion is not running".into()),
                    },
                    epoch,
                    Instant::now(),
                );
                changed = fed.or(changed);
            }
        }
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
        let resize = last_view
            .as_ref()
            .is_none_or(|old| widget::needs_resize(old, &view));
        apply_view(app, &view, resize);
        last_view = Some(view);
    }
}

fn set_tooltip(app: &AppHandle, current: &mut String, text: String) {
    if *current == text {
        return;
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Err(err) = tray.set_tooltip(Some(text.as_str())) {
            eprintln!("could not set the tray tooltip: {err}");
        }
    }
    *current = text;
}

/// Re-seats the widget when `resize` is set, then sends it the state.
fn apply_view(app: &AppHandle, view: &View, resize: bool) {
    if resize {
        if let Some(window) = app.get_webview_window("widget") {
            if let Err(err) = place(&window, view) {
                eprintln!("could not place the widget: {err}");
            }
        }
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
