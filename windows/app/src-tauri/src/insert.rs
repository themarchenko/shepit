//! The insert worker: saves the clipboard, pastes the dictated text, restores the clipboard. The
//! Win32 side sits behind `Desktop` so the order of steps runs in tests on any host.

use shepit_core::context::TargetContext;
use shepit_core::dictation::InsertJob;
use shepit_core::insert::{decide, paste_mode, InsertOutcome, PasteMode, RESTORE_DELAY_MS};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{Receiver, SyncSender};

use crate::controller::Msg;

/// Every clipboard format saved before our write: (format id, bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedClipboard {
    pub formats: Vec<(u32, Vec<u8>)>,
}

/// Overwrites every saved byte. Volatile so the compiler cannot drop the writes as dead stores.
fn wipe(formats: &mut [(u32, Vec<u8>)]) {
    for (_, bytes) in formats {
        for b in bytes.iter_mut() {
            // SAFETY: b is a valid, aligned, exclusive reference to one byte.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

// The snapshot is the user's previous clipboard, possibly a password from a password manager.
impl Drop for SavedClipboard {
    fn drop(&mut self) {
        wipe(&mut self.formats);
    }
}

/// What the worker asks of the desktop. Each method is one step the tests can observe.
pub trait Desktop {
    /// Reads every restorable clipboard format. Err carries the user-facing message.
    fn save_clipboard(&mut self) -> Result<SavedClipboard, String>;
    /// Replaces the clipboard with `text`; returns the sequence number right after the write.
    fn put_text(&mut self, text: &str) -> Result<u32, String>;
    /// Sends Ctrl+V. Err means fewer key events than needed went in.
    fn send_paste(&mut self) -> Result<(), String>;
    /// Waits `ms` milliseconds while pumping the clipboard owner's messages.
    fn wait(&mut self, ms: u64);
    /// The current clipboard sequence number.
    fn sequence_number(&mut self) -> u32;
    /// Puts the saved formats back.
    fn restore(&mut self, saved: SavedClipboard) -> Result<(), String>;
    /// Integrity level RIDs of (this process, the foreground window's process); None when one
    /// cannot be read.
    fn integrity_levels(&mut self) -> (Option<u32>, Option<u32>);
    /// A fresh password check of the focused field; None when unreadable.
    fn probe_password(&mut self) -> Option<bool>;
    /// Reads the foreground app and focused field.
    fn capture_context(&mut self) -> TargetContext;
}

/// Requests for the worker thread.
#[derive(Debug, Clone)]
pub enum Request {
    Capture { id: u64 },
    Insert(InsertJob),
}

/// Pastes `job.text`. Returns Pasted, ClipboardOnly (text left on the clipboard because the keys
/// did not go in) or the message to show.
pub fn insert<D: Desktop>(desktop: &mut D, job: &InsertJob) -> Result<InsertOutcome, String> {
    // Fresh check right before the clipboard is touched: the focus may have moved to a password
    // field since the recording started.
    let now = desktop.probe_password();
    decide(&job.text, job.captured_password, now).map_err(|e| e.to_string())?;

    let saved = desktop.save_clipboard()?;
    let written = match desktop.put_text(&job.text) {
        Ok(sequence) => sequence,
        Err(message) => {
            // The write may have emptied the clipboard before failing; put the old content back.
            let _ = desktop.restore(saved);
            return Err(message);
        }
    };
    let (ours, target) = desktop.integrity_levels();
    if paste_mode(ours, target) == PasteMode::ClipboardOnly {
        // UIPI would drop the keys; the text stays for a manual Ctrl+V and nothing is restored.
        return Ok(InsertOutcome::ClipboardOnly);
    }
    if desktop.send_paste().is_err() {
        // The text stays on the clipboard so the user can press Ctrl+V; restoring would lose it.
        return Ok(InsertOutcome::ClipboardOnly);
    }
    desktop.wait(RESTORE_DELAY_MS);
    // Someone else wrote to the clipboard during the wait: their content wins (PasteInserter.swift:72).
    if desktop.sequence_number() == written && desktop.restore(saved).is_err() {
        // Outcome kind only: the clipboard content is never logged.
        eprintln!("clipboard restore failed");
    }
    Ok(InsertOutcome::Pasted)
}

/// Serves requests until the sender closes. A panic in one request becomes an empty context or
/// an error result and the worker keeps running.
pub fn run<D: Desktop>(mut desktop: D, requests: Receiver<Request>, results: SyncSender<Msg>) {
    while let Ok(request) = requests.recv() {
        let msg = match request {
            Request::Capture { id } => {
                let context = catch_unwind(AssertUnwindSafe(|| desktop.capture_context()))
                    .unwrap_or_default();
                Msg::Context { id, context }
            }
            Request::Insert(job) => {
                let result = catch_unwind(AssertUnwindSafe(|| insert(&mut desktop, &job)))
                    .unwrap_or_else(|_| Err("Insertion failed unexpectedly".to_string()));
                Msg::Inserted { id: job.id, result }
            }
        };
        if results.send(msg).is_err() {
            return;
        }
    }
}

/// Stand-in on hosts without a Windows desktop.
#[cfg_attr(windows, allow(dead_code))]
pub struct NullDesktop;

impl Desktop for NullDesktop {
    fn save_clipboard(&mut self) -> Result<SavedClipboard, String> {
        Err("Insertion needs Windows".into())
    }
    fn put_text(&mut self, _text: &str) -> Result<u32, String> {
        Err("Insertion needs Windows".into())
    }
    fn send_paste(&mut self) -> Result<(), String> {
        Err("Insertion needs Windows".into())
    }
    fn wait(&mut self, _ms: u64) {}
    fn sequence_number(&mut self) -> u32 {
        0
    }
    fn restore(&mut self, _saved: SavedClipboard) -> Result<(), String> {
        Ok(())
    }
    fn integrity_levels(&mut self) -> (Option<u32>, Option<u32>) {
        (None, None)
    }
    fn probe_password(&mut self) -> Option<bool> {
        None
    }
    fn capture_context(&mut self) -> TargetContext {
        TargetContext::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::insert::{
        CLIPBOARD_BUSY, INTEGRITY_HIGH, INTEGRITY_MEDIUM, PASSWORD_REFUSED, RESTORE_DELAY_MS,
    };
    use std::sync::mpsc::{channel, sync_channel};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        Probe,
        Save,
        Put(String),
        Integrity,
        Paste,
        Wait(u64),
        Sequence,
        Restore,
    }

    /// Records every call; each step can be told to fail or answer differently.
    struct FakeDesktop {
        calls: Vec<Call>,
        probe: Option<bool>,
        save_error: Option<String>,
        put_error: Option<String>,
        paste_error: Option<String>,
        integrity: (Option<u32>, Option<u32>),
        put_sequence: u32,
        later_sequence: u32,
        panic_on_capture: bool,
        panic_on_save: bool,
        context: TargetContext,
    }

    impl FakeDesktop {
        fn new() -> Self {
            Self {
                calls: Vec::new(),
                probe: Some(false),
                save_error: None,
                put_error: None,
                paste_error: None,
                integrity: (Some(INTEGRITY_MEDIUM), Some(INTEGRITY_MEDIUM)),
                put_sequence: 7,
                later_sequence: 7,
                panic_on_capture: false,
                panic_on_save: false,
                context: TargetContext::default(),
            }
        }
    }

    impl Desktop for FakeDesktop {
        fn save_clipboard(&mut self) -> Result<SavedClipboard, String> {
            self.calls.push(Call::Save);
            if self.panic_on_save {
                panic!("boom");
            }
            match &self.save_error {
                Some(e) => Err(e.clone()),
                None => Ok(SavedClipboard::default()),
            }
        }
        fn put_text(&mut self, text: &str) -> Result<u32, String> {
            self.calls.push(Call::Put(text.to_string()));
            match &self.put_error {
                Some(e) => Err(e.clone()),
                None => Ok(self.put_sequence),
            }
        }
        fn send_paste(&mut self) -> Result<(), String> {
            self.calls.push(Call::Paste);
            match &self.paste_error {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }
        fn wait(&mut self, ms: u64) {
            self.calls.push(Call::Wait(ms));
        }
        fn sequence_number(&mut self) -> u32 {
            self.calls.push(Call::Sequence);
            self.later_sequence
        }
        fn restore(&mut self, _saved: SavedClipboard) -> Result<(), String> {
            self.calls.push(Call::Restore);
            Ok(())
        }
        fn integrity_levels(&mut self) -> (Option<u32>, Option<u32>) {
            self.calls.push(Call::Integrity);
            self.integrity
        }
        fn probe_password(&mut self) -> Option<bool> {
            self.calls.push(Call::Probe);
            self.probe
        }
        fn capture_context(&mut self) -> TargetContext {
            if self.panic_on_capture {
                panic!("boom");
            }
            self.context.clone()
        }
    }

    fn job(captured_password: Option<bool>) -> InsertJob {
        InsertJob {
            id: 5,
            text: "hello".into(),
            captured_password,
        }
    }

    #[test]
    fn a_paste_saves_puts_pastes_waits_checks_and_restores_in_order() {
        let mut d = FakeDesktop::new();
        let out = insert(&mut d, &job(Some(false)));
        assert_eq!(out, Ok(InsertOutcome::Pasted));
        assert_eq!(
            d.calls,
            vec![
                Call::Probe,
                Call::Save,
                Call::Put("hello".into()),
                Call::Integrity,
                Call::Paste,
                Call::Wait(RESTORE_DELAY_MS),
                Call::Sequence,
                Call::Restore,
            ]
        );
    }

    #[test]
    fn a_higher_integrity_target_keeps_the_text_without_keys_or_restore() {
        let mut d = FakeDesktop::new();
        d.integrity = (Some(INTEGRITY_MEDIUM), Some(INTEGRITY_HIGH));
        assert_eq!(
            insert(&mut d, &job(Some(false))),
            Ok(InsertOutcome::ClipboardOnly)
        );
        assert!(d.calls.contains(&Call::Put("hello".into())));
        assert!(!d.calls.contains(&Call::Paste), "{:?}", d.calls);
        assert!(!d.calls.contains(&Call::Restore), "{:?}", d.calls);
        assert!(!d.calls.iter().any(|c| matches!(c, Call::Wait(_))));
    }

    #[test]
    fn an_unreadable_target_integrity_keeps_the_text_without_keys() {
        let mut d = FakeDesktop::new();
        d.integrity = (Some(INTEGRITY_MEDIUM), None);
        assert_eq!(
            insert(&mut d, &job(Some(false))),
            Ok(InsertOutcome::ClipboardOnly)
        );
        assert!(!d.calls.contains(&Call::Paste), "{:?}", d.calls);
    }

    #[test]
    fn dropping_a_saved_clipboard_wipes_its_bytes() {
        let mut formats = vec![(1, vec![1u8, 2, 3]), (13, vec![9u8; 4])];
        wipe(&mut formats);
        assert!(formats.iter().all(|(_, b)| b.iter().all(|&x| x == 0)));
        assert_eq!(formats[0].1.len(), 3);
    }

    #[test]
    fn a_password_at_start_refuses_without_any_clipboard_call() {
        let mut d = FakeDesktop::new();
        let out = insert(&mut d, &job(Some(true)));
        assert_eq!(out, Err(PASSWORD_REFUSED.to_string()));
        assert!(!d.calls.iter().any(|c| *c != Call::Probe), "{:?}", d.calls);
    }

    #[test]
    fn a_password_found_at_insert_time_refuses_without_any_clipboard_call() {
        let mut d = FakeDesktop::new();
        d.probe = Some(true);
        let out = insert(&mut d, &job(Some(false)));
        assert_eq!(out, Err(PASSWORD_REFUSED.to_string()));
        assert!(!d.calls.iter().any(|c| *c != Call::Probe), "{:?}", d.calls);
    }

    #[test]
    fn an_unreadable_password_flag_still_pastes() {
        let mut d = FakeDesktop::new();
        d.probe = None;
        assert_eq!(insert(&mut d, &job(None)), Ok(InsertOutcome::Pasted));
    }

    #[test]
    fn empty_text_is_refused_without_any_clipboard_call() {
        let mut d = FakeDesktop::new();
        let empty = InsertJob {
            id: 5,
            text: String::new(),
            captured_password: Some(false),
        };
        assert!(insert(&mut d, &empty).is_err());
        assert!(!d.calls.iter().any(|c| *c != Call::Probe), "{:?}", d.calls);
    }

    #[test]
    fn a_clipboard_changed_by_someone_else_is_not_restored() {
        let mut d = FakeDesktop::new();
        d.later_sequence = 8;
        assert_eq!(insert(&mut d, &job(Some(false))), Ok(InsertOutcome::Pasted));
        assert!(d.calls.contains(&Call::Sequence));
        assert!(!d.calls.contains(&Call::Restore), "{:?}", d.calls);
    }

    #[test]
    fn a_failed_paste_leaves_the_text_without_waiting_or_restoring() {
        let mut d = FakeDesktop::new();
        d.paste_error = Some("keys".into());
        assert_eq!(
            insert(&mut d, &job(Some(false))),
            Ok(InsertOutcome::ClipboardOnly)
        );
        assert!(!d.calls.iter().any(|c| matches!(c, Call::Wait(_))));
        assert!(!d.calls.contains(&Call::Restore), "{:?}", d.calls);
    }

    #[test]
    fn a_failed_save_reports_the_busy_message_and_puts_nothing() {
        let mut d = FakeDesktop::new();
        d.save_error = Some(CLIPBOARD_BUSY.to_string());
        assert_eq!(
            insert(&mut d, &job(Some(false))),
            Err(CLIPBOARD_BUSY.to_string())
        );
        assert!(!d.calls.iter().any(|c| matches!(c, Call::Put(_))));
        assert!(!d.calls.contains(&Call::Paste));
    }

    #[test]
    fn a_failed_put_still_tries_to_restore_and_does_not_paste() {
        let mut d = FakeDesktop::new();
        d.put_error = Some("put failed".into());
        assert_eq!(
            insert(&mut d, &job(Some(false))),
            Err("put failed".to_string())
        );
        assert!(d.calls.contains(&Call::Restore), "{:?}", d.calls);
        assert!(!d.calls.contains(&Call::Paste));
    }

    #[test]
    fn the_null_desktop_cannot_save_the_clipboard() {
        assert_eq!(
            NullDesktop.save_clipboard(),
            Err("Insertion needs Windows".to_string())
        );
    }

    #[test]
    fn inserting_through_the_null_desktop_fails_with_its_message() {
        let out = insert(&mut NullDesktop, &job(Some(false)));
        assert_eq!(out, Err("Insertion needs Windows".to_string()));
    }

    #[test]
    fn a_capture_request_answers_with_the_context_for_its_id() {
        let mut d = FakeDesktop::new();
        d.context.app_id = Some("notepad.exe".into());
        let (req_tx, req_rx) = channel();
        let (res_tx, res_rx) = sync_channel(4);
        req_tx.send(Request::Capture { id: 3 }).unwrap();
        drop(req_tx);
        run(d, req_rx, res_tx);
        match res_rx.recv().unwrap() {
            Msg::Context { id, context } => {
                assert_eq!(id, 3);
                assert_eq!(context.app_id.as_deref(), Some("notepad.exe"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn an_insert_request_answers_with_the_outcome_for_its_id() {
        let (req_tx, req_rx) = channel();
        let (res_tx, res_rx) = sync_channel(4);
        req_tx.send(Request::Insert(job(Some(false)))).unwrap();
        drop(req_tx);
        run(FakeDesktop::new(), req_rx, res_tx);
        match res_rx.recv().unwrap() {
            Msg::Inserted { id, result } => {
                assert_eq!(id, 5);
                assert_eq!(result, Ok(InsertOutcome::Pasted));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_panic_while_capturing_gives_an_empty_context_and_the_worker_goes_on() {
        let mut d = FakeDesktop::new();
        d.panic_on_capture = true;
        let (req_tx, req_rx) = channel();
        let (res_tx, res_rx) = sync_channel(4);
        req_tx.send(Request::Capture { id: 3 }).unwrap();
        req_tx.send(Request::Capture { id: 4 }).unwrap();
        drop(req_tx);
        run(d, req_rx, res_tx);
        for want in [3, 4] {
            match res_rx.recv().unwrap() {
                Msg::Context { id, context } => {
                    assert_eq!(id, want);
                    assert_eq!(context, TargetContext::default());
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn a_panic_while_inserting_gives_an_error_result() {
        let mut d = FakeDesktop::new();
        d.panic_on_save = true;
        let (req_tx, req_rx) = channel();
        let (res_tx, res_rx) = sync_channel(4);
        req_tx.send(Request::Insert(job(Some(false)))).unwrap();
        drop(req_tx);
        run(d, req_rx, res_tx);
        match res_rx.recv().unwrap() {
            Msg::Inserted { id, result } => {
                assert_eq!(id, 5);
                assert!(result.is_err());
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
