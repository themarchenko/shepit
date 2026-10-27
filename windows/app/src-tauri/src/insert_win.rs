//! The Win32 side of insertion: clipboard, SendInput, UI Automation and the foreground probe.
//! Everything runs on the single "insert" worker thread, which owns the message-only window that
//! serves as clipboard owner and the COM apartment. Nothing here logs clipboard data, dictated
//! text or context values.

use crate::insert::{Desktop, SavedClipboard};
use shepit_core::context::{
    app_name, build_context, exe_file_name, file_description_query, is_editable, is_frame_host,
    may_read_text, AppInfo, FocusRead, TargetContext,
};
use shepit_core::insert::{
    document_too_long, is_restorable_format, paste_keys, within_capture_deadline, CLIPBOARD_BUSY,
    CLIPBOARD_PRIVACY_MARKERS, DOCUMENT_READ_UNITS,
};
use std::time::{Duration, Instant};
use windows::core::{w, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation6, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextPattern2, IUIAutomationTextRange, IUIAutomationValuePattern,
    TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
    UIA_TextPattern2Id, UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, EnumChildWindows, GetClassNameW,
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, PeekMessageW, TranslateMessage,
    HWND_MESSAGE, MSG, PM_REMOVE, WINDOW_EX_STYLE, WINDOW_STYLE,
};

/// OpenClipboard attempts before giving up, 20 ms apart (another app may hold it briefly).
const OPEN_ATTEMPTS: u32 = 5;
const OPEN_RETRY_MS: u64 = 20;
/// UI Automation gives up on an unresponsive app after this long.
const UIA_TIMEOUT_MS: u32 = 1000;
/// Window titles are cut at this many UTF-16 units.
const TITLE_UNITS: usize = 1024;
/// Characters read back from the caret; more than the 500 graphemes the context keeps.
const CARET_LOOKBACK_CHARS: i32 = 1000;
/// UTF-16 units read from a selection. Reaching this means the selection is too long to keep.
const SELECTION_READ_UNITS: i32 = 20_000;
const IMAGE_PATH_UNITS: usize = 4096;
const CORE_WINDOW_CLASS: &str = "Windows.UI.Core.CoreWindow";

pub struct WinDesktop {
    /// Message-only clipboard owner; invalid when the window could not be created.
    owner: HWND,
    com_ready: bool,
    uia: Option<IUIAutomation6>,
}

impl WinDesktop {
    /// Must run on the worker thread: the window and the COM apartment belong to it.
    pub fn new() -> Self {
        // SAFETY: a null reserved pointer is required; MTA init has no other preconditions. S_FALSE
        // (already initialized) still counts as ready and must be balanced by CoUninitialize.
        let com_ready = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        // SAFETY: the predefined STATIC class needs no registration; HWND_MESSAGE makes a
        // message-only window, which never shows and never receives broadcasts. A NULL owner would
        // make SetClipboardData fail after EmptyClipboard.
        let owner = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("ShepitClipboardOwner"),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .unwrap_or_default();
        Self {
            owner,
            com_ready,
            uia: None,
        }
    }

    fn owner(&self) -> Option<HWND> {
        (!self.owner.is_invalid()).then_some(self.owner)
    }

    /// Delivers pending messages to the owner window (WM_DESTROYCLIPBOARD and friends).
    fn pump(&self) {
        let mut msg = MSG::default();
        // SAFETY: msg is a valid out pointer; a null window filter reads this thread's whole
        // queue, and the message goes straight to its window procedure.
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: msg was just filled by PeekMessageW.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn pump_for(&self, ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        loop {
            self.pump();
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            std::thread::sleep(left.min(Duration::from_millis(20)));
        }
    }

    /// Opens the clipboard with retries. The guard closes it again.
    fn open_clipboard(&self) -> Option<ClipboardGuard> {
        for attempt in 0..OPEN_ATTEMPTS {
            // SAFETY: the owner handle is ours (or None); the call has no other preconditions.
            if unsafe { OpenClipboard(self.owner()) }.is_ok() {
                return Some(ClipboardGuard);
            }
            if attempt + 1 < OPEN_ATTEMPTS {
                self.pump_for(OPEN_RETRY_MS);
            }
        }
        None
    }

    fn uia(&mut self) -> Option<&IUIAutomation6> {
        if self.uia.is_none() && self.com_ready {
            // SAFETY: CUIAutomation8 is the documented class id and the call runs in our MTA.
            let created: windows::core::Result<IUIAutomation6> = unsafe {
                CoCreateInstance(
                    &CUIAutomation8,
                    None::<&windows::core::IUnknown>,
                    CLSCTX_INPROC_SERVER,
                )
            };
            if let Ok(uia) = created {
                // SAFETY: plain setters on an object we own. A hung app then costs 1 s, not more.
                unsafe {
                    let _ = uia.SetConnectionTimeout(UIA_TIMEOUT_MS);
                    let _ = uia.SetTransactionTimeout(UIA_TIMEOUT_MS);
                }
                self.uia = Some(uia);
            }
        }
        self.uia.as_ref()
    }
}

impl Drop for WinDesktop {
    fn drop(&mut self) {
        // The UIA object must go before the apartment does.
        self.uia = None;
        if let Some(owner) = self.owner() {
            // SAFETY: the window was created on this thread, which is the one dropping it.
            let _ = unsafe { DestroyWindow(owner) };
        }
        if self.com_ready {
            // SAFETY: balances the successful CoInitializeEx in new().
            unsafe { CoUninitialize() };
        }
    }
}

/// Closes the clipboard when dropped.
struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: only built after a successful OpenClipboard on this thread.
        let _ = unsafe { CloseClipboard() };
    }
}

/// Copies `bytes` into a movable global block, as the clipboard wants it.
fn global_from_bytes(bytes: &[u8]) -> Option<HGLOBAL> {
    // SAFETY: GMEM_MOVEABLE with a byte count has no other preconditions.
    let block = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }.ok()?;
    // SAFETY: block is a fresh movable allocation of bytes.len() bytes.
    let ptr = unsafe { GlobalLock(block) };
    if ptr.is_null() {
        // SAFETY: block is ours and was never handed to anyone.
        let _ = unsafe { GlobalFree(Some(block)) };
        return None;
    }
    // SAFETY: ptr is a locked block of bytes.len() bytes and cannot overlap a Rust slice.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len()) };
    // SAFETY: block was locked above. GlobalUnlock reports an error when the count reaches zero,
    // which is the normal case, so the result carries no information.
    let _ = unsafe { GlobalUnlock(block) };
    Some(block)
}

/// Gives the clipboard a copy of `bytes` as format `format`. The clipboard must be open.
fn set_format(format: u32, bytes: &[u8]) -> bool {
    let Some(block) = global_from_bytes(bytes) else {
        return false;
    };
    // SAFETY: the clipboard is open (caller contract) and block is a movable global block. On
    // success the system owns the block; on failure it is still ours and is freed below.
    let set = unsafe { SetClipboardData(format, Some(HANDLE(block.0))) };
    if set.is_err() {
        // SAFETY: SetClipboardData failed, so ownership did not transfer.
        let _ = unsafe { GlobalFree(Some(block)) };
        return false;
    }
    true
}

/// Reads the bytes behind one clipboard format. The clipboard must be open.
fn read_format(format: u32) -> Option<Vec<u8>> {
    // SAFETY: the clipboard is open (caller contract). The returned handle stays owned by the
    // clipboard and is only read while it is open.
    let handle = unsafe { GetClipboardData(format) }.ok()?;
    let block = HGLOBAL(handle.0);
    // SAFETY: formats on the skip list (GDI handles) never get here, so the handle is a global
    // memory block.
    let size = unsafe { GlobalSize(block) };
    if size == 0 {
        return None;
    }
    // SAFETY: block is a global memory block owned by the clipboard.
    let ptr = unsafe { GlobalLock(block) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: ptr points at size locked bytes; they are copied before the unlock.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) }.to_vec();
    // SAFETY: block was locked above; the result is not informative (see global_from_bytes).
    let _ = unsafe { GlobalUnlock(block) };
    Some(bytes)
}

fn key_input(vk: u16, scan: u16, key_up: bool, extended: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if key_up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> u32 {
    // SAFETY: inputs is a valid slice of fully initialized INPUT structs and the size argument is
    // the size of one of them, as SendInput requires.
    unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) }
}

impl Desktop for WinDesktop {
    fn save_clipboard(&mut self) -> Result<SavedClipboard, String> {
        let _guard = self.open_clipboard().ok_or(CLIPBOARD_BUSY)?;
        let mut formats = Vec::new();
        let mut format = 0u32;
        loop {
            // SAFETY: the clipboard is open; 0 starts the enumeration, the previous id continues it.
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            if !is_restorable_format(format) {
                continue;
            }
            // A format that cannot be read (delay-rendered and refused) is skipped, not fatal.
            if let Some(bytes) = read_format(format) {
                formats.push((format, bytes));
            }
        }
        Ok(SavedClipboard { formats })
    }

    fn put_text(&mut self, text: &str) -> Result<u32, String> {
        let mut bytes: Vec<u8> = Vec::with_capacity(text.len() * 2 + 2);
        for unit in text.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        {
            let _guard = self.open_clipboard().ok_or(CLIPBOARD_BUSY)?;
            // SAFETY: the clipboard is open and owned by our window.
            unsafe { EmptyClipboard() }.map_err(|_| "Could not clear the clipboard".to_string())?;
            if !set_format(u32::from(CF_UNICODETEXT.0), &bytes) {
                return Err("Could not put the text on the clipboard".into());
            }
            // Best effort: the text is already there, a refused marker only costs privacy.
            for (name, value) in CLIPBOARD_PRIVACY_MARKERS {
                let wide = wide_nul(name);
                // SAFETY: wide is a nul-terminated string that outlives the call.
                let format = unsafe { RegisterClipboardFormatW(PCWSTR::from_raw(wide.as_ptr())) };
                if format != 0 {
                    // Windows only looks for the format; a zero DWORD stands for the empty marker.
                    set_format(format, &value.unwrap_or(0).to_le_bytes());
                }
            }
        }
        // Read after the clipboard closed so the number covers our whole write.
        // SAFETY: no preconditions.
        Ok(unsafe { GetClipboardSequenceNumber() })
    }

    fn send_paste(&mut self) -> Result<(), String> {
        let keys = paste_keys();
        let inputs: Vec<INPUT> = keys
            .iter()
            .map(|k| key_input(k.vk, k.scan, k.key_up, k.extended))
            .collect();
        let sent = send(&inputs);
        if sent as usize == inputs.len() {
            return Ok(());
        }
        if sent > 0 {
            // Part of the chord went in: release both keys so none stays held down.
            send(&[
                key_input(VK_V.0, 0x2F, true, false),
                key_input(VK_CONTROL.0, 0x1D, true, false),
            ]);
        }
        Err("The paste keys were blocked".into())
    }

    fn wait(&mut self, ms: u64) {
        self.pump_for(ms);
    }

    fn sequence_number(&mut self) -> u32 {
        // SAFETY: no preconditions.
        unsafe { GetClipboardSequenceNumber() }
    }

    fn restore(&mut self, saved: SavedClipboard) -> Result<(), String> {
        let _guard = self.open_clipboard().ok_or(CLIPBOARD_BUSY)?;
        // SAFETY: the clipboard is open and owned by our window.
        unsafe { EmptyClipboard() }.map_err(|_| "Could not clear the clipboard".to_string())?;
        for (format, bytes) in &saved.formats {
            // One format failing must not stop the others coming back.
            set_format(*format, bytes);
        }
        Ok(())
    }

    fn integrity_levels(&mut self) -> (Option<u32>, Option<u32>) {
        // SAFETY: the pseudo handle of the current process needs no closing.
        let ours = integrity_of(unsafe { GetCurrentProcess() });
        // SAFETY: no preconditions; returns a null handle when no window has focus.
        let hwnd = unsafe { GetForegroundWindow() };
        let mut pid = 0u32;
        if !hwnd.is_invalid() {
            // SAFETY: pid is a valid out pointer.
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        }
        let target = (pid != 0).then(|| {
            // SAFETY: a plain open with a limited access right; the handle is closed below.
            let process =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
            let level = integrity_of(process);
            // SAFETY: process was opened above and is not used afterwards.
            let _ = unsafe { CloseHandle(process) };
            level
        });
        (ours, target.flatten())
    }

    fn probe_password(&mut self) -> Option<bool> {
        let uia = self.uia()?;
        // SAFETY: UIA calls on an object owned by this thread; no pointer arguments.
        unsafe {
            let element = uia.GetFocusedElement().ok()?;
            Some(element.CurrentIsPassword().ok()?.as_bool())
        }
    }

    fn capture_context(&mut self) -> TargetContext {
        let start = Instant::now();
        let app = foreground_app();
        let focus = self.uia().and_then(|uia| read_focus(uia, start));
        build_context(app, focus)
    }
}

/// True once the capture has used up its overall UIA budget.
fn out_of_time(start: Instant) -> bool {
    !within_capture_deadline(start.elapsed().as_millis() as u64)
}

/// The mandatory integrity level RID of a process, None when its token cannot be read.
fn integrity_of(process: HANDLE) -> Option<u32> {
    let mut token = HANDLE::default();
    // SAFETY: token is a valid out pointer; the handle is closed below.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    let level = integrity_of_token(token);
    // SAFETY: token was opened above and is not used afterwards.
    let _ = unsafe { CloseHandle(token) };
    level
}

fn integrity_of_token(token: HANDLE) -> Option<u32> {
    let mut needed = 0u32;
    // SAFETY: the size query: a null buffer of length 0 makes the call report the size it needs.
    let _ = unsafe { GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut needed) };
    if (needed as usize) < std::mem::size_of::<TOKEN_MANDATORY_LABEL>() {
        return None;
    }
    // u64 units keep the buffer aligned for the label struct.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    // SAFETY: buf holds at least `needed` writable bytes.
    unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
    }
    .ok()?;
    // SAFETY: the call filled buf with a TOKEN_MANDATORY_LABEL whose SID points inside buf. The
    // last sub-authority of an integrity SID is the level.
    unsafe {
        let label = &*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        if count == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(sid, u32::from(count) - 1))
    }
}

/// Reads the focused element. None when UIA cannot say (unreachable or elevated target). Stops
/// asking once `start` is older than the capture deadline and returns what it has.
fn read_focus(uia: &IUIAutomation6, start: Instant) -> Option<FocusRead> {
    // SAFETY: UIA calls on objects owned by this thread; the only out pointer is a local.
    let element = unsafe { uia.GetFocusedElement() }.ok()?;
    // SAFETY: as above.
    let is_password = unsafe { element.CurrentIsPassword() }
        .ok()
        .map(|b| b.as_bool());
    // SAFETY: as above.
    let control_type = unsafe { element.CurrentControlType() }
        .map(|t| t.0)
        .unwrap_or(0);
    let mut read = FocusRead {
        is_password,
        editable: is_editable(control_type, is_password == Some(true)),
        ..FocusRead::default()
    };
    // Text is read only when the field is known not to be a password.
    if !may_read_text(is_password) {
        return Some(read);
    }
    read_text(&element, &mut read, start);
    Some(read)
}

fn read_text(element: &IUIAutomationElement, read: &mut FocusRead, start: Instant) {
    let mut before = None;
    if out_of_time(start) {
        return;
    }
    // SAFETY: UIA calls on objects owned by this thread; the only out pointer is a local.
    let text_pattern = unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok()
    };
    // Same rule as macOS: a document of 50000 or more characters gives no text-before-caret.
    let too_long = text_pattern.as_ref().is_some_and(|p| {
        // SAFETY: as above.
        let doc = unsafe {
            p.DocumentRange()
                .and_then(|range| range.GetText(DOCUMENT_READ_UNITS as i32))
        };
        doc.is_ok_and(|text| document_too_long(&text.to_string()))
    });
    if out_of_time(start) {
        return;
    }
    // SAFETY: UIA calls on objects owned by this thread; the only out pointer is a local.
    let caret_range = if too_long {
        None
    } else {
        unsafe {
            element
                .GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
                .ok()
                .and_then(|p| {
                    let mut active = BOOL(0);
                    p.GetCaretRange(&mut active).ok()
                })
        }
    };
    if let Some(range) = &caret_range {
        before = text_before(range);
    }
    if out_of_time(start) {
        read.value = before;
        return;
    }
    // SAFETY: as above.
    let selection = unsafe {
        text_pattern
            .as_ref()
            .and_then(|p| p.GetSelection().ok())
            .and_then(|array| {
                if array.Length().ok()? > 0 {
                    array.GetElement(0).ok()
                } else {
                    None
                }
            })
    };
    if before.is_none() && !too_long && !out_of_time(start) {
        if let Some(range) = &selection {
            before = text_before(range);
        }
    }
    if out_of_time(start) {
        read.value = before;
        return;
    }
    if let Some(range) = &selection {
        // SAFETY: as above.
        if let Ok(text) = unsafe { range.GetText(SELECTION_READ_UNITS) } {
            // A read that filled the buffer was cut short, so the selection is too long to keep.
            if text.len() < SELECTION_READ_UNITS as usize {
                read.selection = Some(String::from_utf16_lossy(&text));
            }
        }
    }
    if too_long {
        return;
    }
    if before.is_none() {
        if out_of_time(start) {
            return;
        }
        // SAFETY: as above.
        let value = unsafe {
            element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()
                .and_then(|p| p.CurrentValue().ok())
        };
        // The value has no caret position, so the core takes the end of it.
        read.value = value.map(|v| String::from_utf16_lossy(&v));
        return;
    }
    // The tail is already cut at the caret, so the core takes its end.
    read.value = before;
}

/// The text just before the start of `range`, up to CARET_LOOKBACK_CHARS characters.
fn text_before(range: &IUIAutomationTextRange) -> Option<String> {
    // SAFETY: UIA calls on objects owned by this thread. The clone is moved: its end is pulled to
    // its start (collapsed at the caret), then its start back by a bounded number of characters.
    unsafe {
        let tail = range.Clone().ok()?;
        tail.MoveEndpointByRange(
            TextPatternRangeEndpoint_End,
            &tail,
            TextPatternRangeEndpoint_Start,
        )
        .ok()?;
        tail.MoveEndpointByUnit(
            TextPatternRangeEndpoint_Start,
            TextUnit_Character,
            -CARET_LOOKBACK_CHARS,
        )
        .ok()?;
        let text = tail.GetText(-1).ok()?;
        Some(String::from_utf16_lossy(&text))
    }
}

fn wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// App identity of the foreground window: exe file name, display name and window title.
fn foreground_app() -> AppInfo {
    // SAFETY: no preconditions; returns a null handle when no window has focus.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return AppInfo::default();
    }
    let mut title_buf = [0u16; TITLE_UNITS];
    // SAFETY: hwnd is a window handle and title_buf a writable slice of TITLE_UNITS units.
    let title_len = unsafe { GetWindowTextW(hwnd, &mut title_buf) };
    let window_title = (title_len > 0)
        .then(|| String::from_utf16_lossy(&title_buf[..title_len as usize]))
        .filter(|t| !t.is_empty());

    let mut pid = 0u32;
    // SAFETY: pid is a valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let mut path = image_path(pid);
    let mut name = path.as_deref().and_then(exe_file_name);
    if name.as_deref().is_some_and(is_frame_host) {
        // UWP apps run in a separate process behind ApplicationFrameHost; its CoreWindow child
        // belongs to the real one.
        if let Some(real) = core_window_pid(hwnd, pid) {
            if let Some(real_path) = image_path(real) {
                name = exe_file_name(&real_path);
                path = Some(real_path);
            }
        }
    }
    let Some(name) = name else {
        return AppInfo {
            window_title,
            ..AppInfo::default()
        };
    };
    let description = path.as_deref().and_then(file_description);
    AppInfo {
        app_name: Some(app_name(description.as_deref(), &name)),
        app_id: Some(name),
        window_title,
    }
}

fn image_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    // SAFETY: a plain open with a limited access right; the handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = vec![0u16; IMAGE_PATH_UNITS];
    let mut len = buf.len() as u32;
    // SAFETY: buf holds len writable units; len is updated to the number written.
    let queried = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR::from_raw(buf.as_mut_ptr()),
            &mut len,
        )
    };
    // SAFETY: process was opened above and is not used afterwards.
    let _ = unsafe { CloseHandle(process) };
    queried.ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

struct ChildSearch {
    host_pid: u32,
    found: u32,
}

unsafe extern "system" fn find_core_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the ChildSearch passed to EnumChildWindows in core_window_pid, which
    // outlives the enumeration.
    let search = unsafe { &mut *(lparam.0 as *mut ChildSearch) };
    let mut class = [0u16; 64];
    // SAFETY: hwnd comes from the enumeration; class is a writable slice.
    let n = unsafe { GetClassNameW(hwnd, &mut class) };
    if n > 0 && String::from_utf16_lossy(&class[..n as usize]) == CORE_WINDOW_CLASS {
        let mut pid = 0u32;
        // SAFETY: pid is a valid out pointer.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != 0 && pid != search.host_pid {
            search.found = pid;
            return BOOL(0);
        }
    }
    BOOL(1)
}

fn core_window_pid(frame: HWND, host_pid: u32) -> Option<u32> {
    let mut search = ChildSearch { host_pid, found: 0 };
    // SAFETY: the callback only reads the ChildSearch behind lparam, which lives past the call.
    // The return value says whether the enumeration ran to the end, which is not needed.
    let _ = unsafe {
        EnumChildWindows(
            Some(frame),
            Some(find_core_window),
            LPARAM(&mut search as *mut ChildSearch as isize),
        )
    };
    (search.found != 0).then_some(search.found)
}

/// The FileDescription string of the exe, in its first declared language.
fn file_description(path: &str) -> Option<String> {
    let wide = wide_nul(path);
    let file = PCWSTR::from_raw(wide.as_ptr());
    // SAFETY: file points at a nul-terminated string that outlives the call.
    let size = unsafe { GetFileVersionInfoSizeW(file, None) };
    if size == 0 {
        return None;
    }
    let mut block = vec![0u8; size as usize];
    // SAFETY: block is size writable bytes, as the size query asked.
    unsafe { GetFileVersionInfoW(file, None, size, block.as_mut_ptr().cast()) }.ok()?;

    let mut ptr = std::ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: block holds the version resource; ptr and len are valid out pointers. The returned
    // pointer points inside block, which outlives its use below.
    let found = unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            w!("\\VarFileInfo\\Translation"),
            &mut ptr,
            &mut len,
        )
    };
    if !found.as_bool() || len < 4 {
        return None;
    }
    // SAFETY: the Translation value is an array of (language, codepage) u16 pairs and len >= 4.
    // It is not necessarily aligned, hence read_unaligned.
    let (lang, codepage) = unsafe {
        let pair = ptr.cast::<u16>();
        (pair.read_unaligned(), pair.add(1).read_unaligned())
    };
    let query = wide_nul(&file_description_query(lang, codepage));
    let mut text = std::ptr::null_mut();
    let mut text_len = 0u32;
    // SAFETY: as above; query is nul-terminated and outlives the call.
    let found = unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            PCWSTR::from_raw(query.as_ptr()),
            &mut text,
            &mut text_len,
        )
    };
    if !found.as_bool() || text_len == 0 {
        return None;
    }
    // SAFETY: the value is text_len UTF-16 units inside block (the count includes the nul).
    let units = unsafe { std::slice::from_raw_parts(text.cast::<u16>(), text_len as usize) };
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    Some(String::from_utf16_lossy(&units[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

    fn unicode_bytes(s: &str) -> Vec<u8> {
        wide_nul(s).iter().flat_map(|u| u.to_le_bytes()).collect()
    }

    fn find(saved: &SavedClipboard, format: u32) -> Option<&Vec<u8>> {
        saved
            .formats
            .iter()
            .find(|(f, _)| *f == format)
            .map(|(_, b)| b)
    }

    #[test]
    fn the_clipboard_round_trips_text_and_a_registered_format() {
        let mut desktop = WinDesktop::new();
        let text_format = u32::from(CF_UNICODETEXT.0);
        // SAFETY: a nul-terminated literal name.
        let custom = unsafe { RegisterClipboardFormatW(w!("ShepitRoundTripTest")) };
        assert_ne!(custom, 0);
        let original = SavedClipboard {
            formats: vec![
                (text_format, unicode_bytes("before")),
                (custom, vec![1, 2, 3, 4, 5]),
            ],
        };
        // A session without a usable clipboard (headless runner) cannot run this test.
        if desktop.restore(original.clone()).is_err() {
            println!("SKIPPED: OpenClipboard failed");
            return;
        }
        let saved = desktop.save_clipboard().expect("save");
        assert_eq!(find(&saved, text_format), Some(&unicode_bytes("before")));
        assert_eq!(find(&saved, custom), Some(&vec![1, 2, 3, 4, 5]));

        let sequence = desktop.put_text("dictated").expect("put");
        assert_eq!(desktop.sequence_number(), sequence);
        let during = desktop.save_clipboard().expect("save during");
        assert_eq!(find(&during, text_format), Some(&unicode_bytes("dictated")));
        assert_eq!(find(&during, custom), None);

        desktop.restore(saved).expect("restore");
        let after = desktop.save_clipboard().expect("save after");
        assert_eq!(find(&after, text_format), Some(&unicode_bytes("before")));
        assert_eq!(find(&after, custom), Some(&vec![1, 2, 3, 4, 5]));
    }
}
