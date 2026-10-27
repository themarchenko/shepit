#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod controller;
mod history_store;
mod history_worker;
mod hook;
mod insert;
#[cfg(windows)]
mod insert_win;
mod mic_check;
mod polish;
mod recognizer;
mod recorder;
mod recordings;
mod settings_store;
mod shell;
mod tray;
mod ui;
mod update_card;
mod updater;
mod widget;
mod widget_host;

use controller::{log_line, Msg, Runner};
use insert::Request;
use polish::PolishHandle;
use recognizer::ParakeetEngine;
use recorder::CpalRecorder;
use shepit_core::cleanup::passthrough_clean;
use shepit_core::dictation::{Job, ModelStatus, Phase};
use shepit_core::keys::PickResult;
use shepit_core::settings::Settings;
use shepit_core::update::UpdateState;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use settings_store::SettingsStore;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
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
            widget_pointer,
            commands::get_settings,
            commands::update_settings,
            commands::start_hotkey_pick,
            commands::cancel_hotkey_pick,
            commands::detect_agents,
            commands::check_microphone,
            commands::open_microphone_privacy,
            commands::reset_widget_position,
            commands::widget_demo,
            commands::finish_tour,
            commands::show_tour,
            updater::get_update_status,
            updater::check_updates_now,
            updater::show_update_card,
            updater::update_card_ready,
            updater::update_action,
            updater::update_resize
        ])
        // A second launch must not open a second widget. Windows 11 hides new tray icons, so the
        // second launch is also a way to reach Settings.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            open_settings_later(app);
        }))
        .plugin(updater::plugin())
        .setup(|app| {
            // Bounded so a stalled controller cannot grow memory; senders drop on full.
            let (tx, rx) = mpsc::sync_channel::<Msg>(256);
            let store = SettingsStore::new(app.path().app_config_dir()?);
            let host = WidgetHost::new(app.handle().clone(), tx.clone(), store);
            app.manage(host.clone());

            // The recognizer's latest state: written by the controller thread, read by the commands.
            let model = Arc::new(Mutex::new(ModelStatus::Loading));
            app.manage(model.clone());

            let settings = MenuItemBuilder::with_id("settings", "Settings\u{2026}").build(app)?;
            let show_widget = CheckMenuItemBuilder::with_id("toggle-widget", "Show widget")
                .checked(host.visible())
                .build(app)?;
            let reset_widget =
                MenuItemBuilder::with_id("reset-widget", "Reset widget position").build(app)?;
            let check_updates =
                MenuItemBuilder::with_id("check-updates", tray::update_label(&UpdateState::Idle))
                    .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit Shepit").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&settings)
                .item(&check_updates)
                .separator()
                .item(&show_widget)
                .item(&reset_widget)
                .separator()
                .item(&quit)
                .build()?;

            app.manage(tray::ShowWidgetItem(show_widget.clone()));
            app.manage(tray::UpdateItem(check_updates.clone()));
            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
                .tooltip("Shepit")
                .menu(&menu)
                // Windows convention: a left click opens Settings, a right click shows the menu.
                .show_menu_on_left_click(false)
                .on_menu_event(
                    move |app, event| match tray::action_for(event.id().as_ref()) {
                        tray::MenuAction::Quit => app.exit(0),
                        // On a thread: the handler runs on the main thread, and the host waits on the
                        // main thread for monitor data while it holds its lock.
                        tray::MenuAction::ToggleWidget => {
                            let app = app.clone();
                            std::thread::spawn(move || {
                                let result = settings_store::apply(&app, |current| {
                                    Ok(Settings {
                                        widget_visible: !current.widget_visible,
                                        ..current.clone()
                                    })
                                });
                                if let Err(err) = result {
                                    eprintln!("could not toggle the widget: {err}");
                                }
                            });
                        }
                        tray::MenuAction::ResetWidget => {
                            let host = app.state::<WidgetHost>().inner().clone();
                            std::thread::spawn(move || host.reset_position());
                        }
                        tray::MenuAction::OpenSettings => open_settings_later(app),
                        // Not wired yet: ui::open_history comes with the history work.
                        tray::MenuAction::OpenHistory => {}
                        // The worker takes it; this handler runs on the main thread and must not wait.
                        tray::MenuAction::CheckUpdates => {
                            if let Some(updates) = app.try_state::<updater::Updates>() {
                                updates.check_from_tray();
                            }
                        }
                        tray::MenuAction::None => {}
                    },
                )
                .on_tray_icon_event(|tray, event| {
                    // Up only: a click also reports the press.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        open_settings_later(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Off the main thread for the same reason as the tray handlers.
            {
                let host = host.clone();
                std::thread::spawn(move || host.start());
            }

            hook::start(tx.clone(), host.settings().hotkey);
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
            // Polishing: the dispatcher answers the routes that need no agent, the agent thread owns
            // the worker. Spawned after the insert worker; it warms the agent up at launch.
            let polish = polish::start(tx.clone(), host.settings())?;
            app.manage(polish.clone());
            updater::start(app.handle())?;
            let workers = Workers {
                jobs: job_tx,
                polish,
                inserts: insert_tx,
            };
            let handle = app.handle().clone();
            // The recorder is built inside the thread: a cpal Stream may not be Send.
            std::thread::Builder::new()
                .name("controller".into())
                .spawn(move || run_controller(handle, tx, rx, workers, wanted, model))?;
            ui::show_tour_on_first_run(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Shepit");
}

/// Where the controller sends work: the recognizer, the polish dispatcher and the insert worker.
struct Workers {
    jobs: Sender<Job>,
    polish: PolishHandle,
    inserts: Sender<Request>,
}

/// Runs the controller loop and restarts it with a fresh controller if it panics, so the hotkey
/// keeps working. The receiver outlives the loop; only the controller state is lost.
fn run_controller(
    app: AppHandle,
    tx: SyncSender<Msg>,
    rx: Receiver<Msg>,
    workers: Workers,
    wanted: Arc<AtomicU64>,
    // Outlives the loop below: filled from every Model message before the controller sees it.
    model: Arc<Mutex<ModelStatus>>,
) {
    let mut job_id_base: u64 = 0;
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
        workers.polish.want(0);
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
    // After a restart the hook still holds the key in force, so the new controller starts on it. A
    // pick that was running died with the old controller: the hook stops diverting and Settings
    // is told.
    let mut runner =
        Runner::new(CpalRecorder::new(tx.clone()), first_job_id, known).with_hotkey(hook::hotkey());
    if hook::picking() {
        hook::set_picking(false);
        emit_pick(app, PickResult::Cancelled);
    }
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
            ui::emit_to_windows(app, "model-status", commands::model_payload(*status));
            set_tooltip(app, &mut tooltip, tray::tooltip_for(*status));
        }
        if cfg!(debug_assertions) {
            if let Msg::Context { context, .. } = &msg {
                println!("{}", shepit_core::context::presence_flags_line(context));
            }
        }
        let new_key = match &msg {
            Msg::Hotkey(hotkey) => Some(*hotkey),
            _ => None,
        };
        // A tick after every message keeps the failed reset on time while keys keep arriving.
        let now = Instant::now();
        let mut changed = [
            runner.step_at(msg, epoch, now),
            runner.step_at(Msg::Tick, epoch, now),
        ]
        .into_iter()
        .flatten()
        .last();
        // Only this thread writes the hook state, and the key changes after the runner has dropped
        // the old press.
        if let Some(hotkey) = new_key {
            hook::set_hotkey(hotkey);
        }
        hook::set_picking(runner.picking());
        if let Some(result) = runner.take_pick() {
            finish_pick(app, &mut runner, result, epoch);
        }
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
        // Same order as the recognizer: the id first, so Esc cancels a running agent request and
        // a job that has not started is skipped.
        workers.polish.want(runner.formatting());
        if let Some(job) = runner.take_format() {
            let (id, heard) = (job.id, job.transcript.clone());
            if !workers.polish.format(job) {
                // No dispatcher: insert the transcript as it was heard rather than hang.
                let fed = runner.step_at(
                    Msg::Formatted {
                        id,
                        text: passthrough_clean(&heard),
                    },
                    epoch,
                    Instant::now(),
                );
                changed = fed.or(changed);
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

/// A pick ended. A chosen key is saved and switched here, on this thread: sending `Msg::Hotkey` to
/// our own bounded channel would block this thread forever once the channel is full.
fn finish_pick(
    app: &AppHandle,
    runner: &mut Runner<CpalRecorder>,
    result: PickResult,
    epoch: Instant,
) {
    if let PickResult::Picked(hotkey) = result {
        let saved = settings_store::apply_on_controller(app, |current| {
            Ok(Settings {
                hotkey,
                ..current.clone()
            })
        });
        match saved {
            Ok(_) => {
                runner.step_at(Msg::Hotkey(hotkey), epoch, Instant::now());
                hook::set_hotkey(hotkey);
            }
            Err(err) => eprintln!("could not save the picked key: {err}"),
        }
    }
    emit_pick(app, result);
}

fn emit_pick(app: &AppHandle, result: PickResult) {
    ui::emit_to_windows(app, "hotkey-pick", commands::pick_payload(result));
}

/// Opens Settings from a handler that runs on the main thread: building a window waits for the main
/// thread, so it happens on a thread of its own.
fn open_settings_later(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(err) = ui::open_settings(&app) {
            eprintln!("could not open Settings: {err}");
        }
    });
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
