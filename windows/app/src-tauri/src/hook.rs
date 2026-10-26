//! Global low-level keyboard hook. The callback only copies the event and hands it to the
//! controller thread; filtering lives in shepit_core::keys. It never swallows a key, and only
//! the configured hotkey and Esc are ever copied out of the proc.

use crate::controller::Msg;
use std::sync::mpsc::SyncSender;

#[cfg(windows)]
mod imp {
    use super::*;
    use shepit_core::keys::{is_accepted, RawKey};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::OnceLock;
    use std::time::Instant;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    };

    static SENDER: OnceLock<SyncSender<Msg>> = OnceLock::new();
    /// The configured hotkey's virtual-key code, set once by the app before the hook installs.
    static HOTKEY_VK: AtomicU32 = AtomicU32::new(0);

    /// The hook callback runs on the hook thread inside the system's input path, so it does no
    /// work beyond a copy and a channel send.
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            if let Some(tx) = SENDER.get() {
                // SAFETY: for WH_KEYBOARD_LL with code >= 0, lparam points to a valid
                // KBDLLHOOKSTRUCT that lives for the duration of this call.
                let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
                // Any other key stays inside the proc: nothing about it is stored or sent.
                if is_accepted(info.vkCode, info.flags.0, HOTKEY_VK.load(Ordering::Relaxed)) {
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

    pub fn start(tx: SyncSender<Msg>, hotkey_vk: u32) {
        HOTKEY_VK.store(hotkey_vk, Ordering::Relaxed);
        if SENDER.set(tx).is_err() {
            eprintln!("keyboard hook already started");
            return;
        }
        let spawned = std::thread::Builder::new()
            .name("keyboard-hook".into())
            .spawn(|| {
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

#[cfg(windows)]
pub use imp::start;

/// The hook exists only on Windows; elsewhere the app runs without a hotkey.
#[cfg(not(windows))]
pub fn start(_tx: SyncSender<Msg>, _hotkey_vk: u32) {}
