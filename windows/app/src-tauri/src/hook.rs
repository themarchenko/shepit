//! Global low-level keyboard hook. The callback only copies the event and hands it to the
//! controller thread; filtering lives in shepit_core::keys. It never swallows a key, and only
//! the configured hotkey and Esc are ever copied out of the proc.

use crate::controller::Msg;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::SyncSender;

/// Index into `Hotkey::ALL`.
static HOTKEY: AtomicU8 = AtomicU8::new(0);
static PICKING: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
mod imp {
    use super::*;
    use shepit_core::keys::{is_accepted, RawKey};
    use std::sync::OnceLock;
    use std::time::Instant;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    };

    static SENDER: OnceLock<SyncSender<Msg>> = OnceLock::new();

    /// The hook callback runs on the hook thread inside the system's input path, so it does no
    /// work beyond a copy and a channel send.
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            if let Some(tx) = SENDER.get() {
                // SAFETY: for WH_KEYBOARD_LL with code >= 0, lparam points to a valid
                // KBDLLHOOKSTRUCT that lives for the duration of this call.
                let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
                // Any other key stays inside the proc: nothing about it is stored or sent.
                if is_accepted(info.vkCode, info.flags.0, super::hotkey(), super::picking()) {
                    let raw = RawKey {
                        message: wparam.0 as u32,
                        vk: info.vkCode,
                        flags: info.flags.0,
                    };
                    // The proc must never block: a full or closed channel drops the event.
                    let _ = tx.try_send(Msg::KeyAt(raw, Instant::now()));
                }
            }
        }
        // SAFETY: plain forward of the arguments we were given; a null hook handle is allowed
        // because the system finds the next hook itself.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    pub fn start(tx: SyncSender<Msg>) {
        if SENDER.set(tx).is_err() {
            eprintln!("keyboard hook already started");
            return;
        }
        let spawned = std::thread::Builder::new()
            .name("keyboard-hook".into())
            .spawn(|| {
                // The live preview keeps the model busy while a key is held. The hook sits in
                // every keystroke's path and stamps the release, so it must not queue behind
                // that load. A failure only costs the margin: carry on.
                // SAFETY: GetCurrentThread returns a pseudo handle for this thread, valid
                // without closing; SetThreadPriority only reads its arguments.
                if let Err(err) =
                    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) }
                {
                    eprintln!("could not raise the keyboard hook priority: {err}");
                }
                // SAFETY: GetModuleHandleW with a null name returns the handle of the current
                // executable and has no other preconditions.
                let module = unsafe { GetModuleHandleW(PCWSTR::null()) };
                let module = module.ok().map(|m| HINSTANCE(m.0));
                // SAFETY: hook_proc has the HOOKPROC signature and stays valid for the process
                // lifetime. Thread id 0 makes the hook global, as WH_KEYBOARD_LL requires.
                let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module, 0) };
                if let Err(err) = hook {
                    eprintln!("could not install the keyboard hook: {err}");
                    return;
                }
                // The hook is called from this thread's message loop, so it must keep pumping.
                let mut msg = MSG::default();
                // SAFETY: msg is a valid out pointer; a null window filter reads this thread's
                // whole queue. GetMessageW returns 0 on WM_QUIT and -1 on error.
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {}
            });
        if let Err(err) = spawned {
            eprintln!("could not start the keyboard hook thread: {err}");
        }
    }
}

/// Starts the hook on `hotkey` (from the settings).
pub fn start(tx: SyncSender<Msg>, hotkey: shepit_core::keys::Hotkey) {
    set_hotkey(hotkey);
    #[cfg(windows)]
    imp::start(tx);
    // The hook exists only on Windows; elsewhere the app runs without a hotkey.
    #[cfg(not(windows))]
    let _ = tx;
}

/// The hotkey the hook proc filters on. Only the controller thread writes it; the proc reads it.
/// Lives outside `imp` so every host compiles and tests it.
pub fn hotkey() -> shepit_core::keys::Hotkey {
    shepit_core::keys::Hotkey::ALL
        .get(HOTKEY.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or_default()
}

pub fn set_hotkey(hotkey: shepit_core::keys::Hotkey) {
    let index = shepit_core::keys::Hotkey::ALL
        .iter()
        .position(|h| *h == hotkey)
        .unwrap_or(0);
    HOTKEY.store(index as u8, Ordering::Relaxed);
}

/// While true the proc also lets every candidate key out, for the picker.
pub fn picking() -> bool {
    PICKING.load(Ordering::Relaxed)
}

pub fn set_picking(picking: bool) {
    PICKING.store(picking, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::keys::Hotkey;

    // One test: the statics are process-wide, so parallel tests would race.
    #[test]
    fn the_hook_state_round_trips_every_hotkey_and_the_picking_flag() {
        assert_eq!(
            hotkey(),
            Hotkey::RightCtrl,
            "the default before anything is set"
        );
        assert!(!picking());
        for h in Hotkey::ALL {
            set_hotkey(h);
            assert_eq!(hotkey(), h);
        }
        set_picking(true);
        assert!(picking());
        set_picking(false);
        assert!(!picking());
        set_hotkey(Hotkey::RightCtrl);
    }
}
