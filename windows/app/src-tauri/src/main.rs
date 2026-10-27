#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod controller;
mod hook;
mod insert;
#[cfg(windows)]
mod insert_win;
mod recognizer;
mod recorder;
mod settings_store;
mod tray;
mod widget;
mod widget_host;

use controller::{log_line, Msg, Runner};
use insert::Request;
use recognizer::ParakeetEngine;
use recorder::CpalRecorder;
use shepit_core::dictation::{Job, ModelStatus, Phase};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use settings_store::SettingsStore;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};
use widget_host::WidgetHost;

/// Looked up later to change the tooltip while the model downloads.
const TRAY_ID: &str = "main";

/// Added to the job id base per controller restart, so a result from the old controller can never
/// match a job of the new one.
const JOB_ID_STRIDE: u64 = 1 << 32;

#[cfg(windows)]
fn platform_desktop() -> impl insert::Desktop {
    insert_win::WinDesktop::new()
}

#[cfg(not(windows))]
fn platform_desktop() -> impl insert::Desktop {
    insert::NullDesktop
}

// `widget_ready` is async so it runs off the main thread: the host asks the main thread for monitor
// data and would wait on itself otherwise.

#[tauri::command]
async fn widget_ready(host: State<'_, WidgetHost>) -> Result<(), String> {
    host.ready();
    Ok(())
}

// Sync on purpose: Tauri runs sync commands on the main thread in the order the IPC messages arrive,
// and these only queue the event, so a click's down can never be handled after its up.
#[tauri::command]
fn widget_hover(host: State<'_, WidgetHost>, hovering: bool) {
    host.push_hover(hovering);
}

#[tauri::command]
fn widget_pointer(host: State<'_, WidgetHost>, kind: String) -> Result<(), String> {
    host.push_pointer(&kind)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            widget_ready,
            widget_hover,
            widget_pointer
        ])
        // A second launch must not open a second widget; it just exits.
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .setup(|app| {
            // Bounded so a stalled controller cannot grow memory; senders drop on full.
            let (tx, rx) = mpsc::sync_channel::<Msg>(256);
            let store = SettingsStore::new(app.path().app_config_dir()?);
            let host = WidgetHost::new(app.handle().clone(), tx.clone(), store);
            app.manage(host.clone());

            let settings = MenuItemBuilder::with_id("settings", "Settings\u{2026}")
                .enabled(false)
                .build(app)?;
            let show_widget = CheckMenuItemBuilder::with_id("toggle-widget", "Show widget")
                .checked(host.visible())
                .build(app)?;
            let reset_widget =
                MenuItemBuilder::with_id("reset-widget", "Reset widget position").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit Shepit").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&settings)
                .separator()
                .item(&show_widget)
                .item(&reset_widget)
                .separator()
                .item(&quit)
                .build()?;

            let checkbox = show_widget.clone();
            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
                .tooltip("Shepit")
                .menu(&menu)
                .on_menu_event(
                    move |app, event| match tray::action_for(event.id().as_ref()) {
                        tray::MenuAction::Quit => app.exit(0),
                        // On a thread: the handler runs on the main thread, and the host waits on the
                        // main thread for monitor data while it holds its lock.
                        tray::MenuAction::ToggleWidget => {
                            let host = app.state::<WidgetHost>().inner().clone();
                            let checkbox = checkbox.clone();
                            std::thread::spawn(move || {
                                let visible = host.toggle_visible();
                                // The click already flipped the check; set it to the real state.
                                let _ = checkbox.set_checked(visible);
                            });
                        }
                        tray::MenuAction::ResetWidget => {
                            let host = app.state::<WidgetHost>().inner().clone();
                            std::thread::spawn(move || host.reset_position());
                        }
                        tray::MenuAction::None => {}
                    },
                );
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Off the main thread for the same reason as the tray handlers.
            {
                let host = host.clone();
                std::thread::spawn(move || host.start());
            }

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
        app.state::<WidgetHost>().view(widget::idle_view());
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
        app.state::<WidgetHost>().view(view);
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
