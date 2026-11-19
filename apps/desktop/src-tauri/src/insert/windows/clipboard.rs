//! The Windows clipboard, driven from a hidden message-only window on its own
//! thread. Owning the clipboard from a window that is always pumping messages
//! lets Joona answer delayed-rendering requests (`WM_RENDERFORMAT`) while the
//! inserting thread waits to learn whether the app read the text.

use std::ffi::c_void;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ::windows::Win32::Foundation::{
    GlobalFree, HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use ::windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    MSG, PM_NOREMOVE, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_DESTROY, WM_DESTROYCLIPBOARD, WM_RENDERALLFORMATS, WM_RENDERFORMAT,
    WNDCLASSW,
};
use ::windows::core::{PCWSTR, w};
use anyhow::{Context, anyhow, bail};

pub const CF_UNICODETEXT: u32 = 13;
const WM_RUN_JOBS: u32 = WM_APP + 1;
const WM_CLOSE_OWNER: u32 = WM_APP + 2;
const OPEN_TIMEOUT: Duration = Duration::from_millis(500);
const JOB_TIMEOUT: Duration = Duration::from_secs(5);
/// Copying every format of a large spreadsheet selection can make the app
/// that owns it render dozens of formats. Beyond these limits the snapshot
/// keeps what it has.
const SNAPSHOT_TIME: Duration = Duration::from_millis(750);
const SNAPSHOT_BYTES: usize = 128 << 20;

/// The clipboard contents saved before a paste.
#[derive(Debug, Default)]
pub struct Snapshot {
    formats: Vec<(u32, Vec<u8>)>,
}

/// Formats whose data is plain memory and can be copied byte for byte. The
/// rest hold GDI objects or private handles, which Windows also synthesises
/// from the plain formats where it matters (bitmaps from DIBs).
pub(super) fn copyable(format: u32) -> bool {
    const HANDLES: [u32; 8] = [
        2,    // CF_BITMAP
        3,    // CF_METAFILEPICT
        9,    // CF_PALETTE
        14,   // CF_ENHMETAFILE
        0x80, // CF_OWNERDISPLAY
        0x82, // CF_DSPBITMAP
        0x83, // CF_DSPMETAFILEPICT
        0x8E, // CF_DSPENHMETAFILE
    ];
    // CF_PRIVATEFIRST..CF_GDIOBJLAST
    !HANDLES.contains(&format) && !(0x0200..=0x03FF).contains(&format)
}

/// UTF-16 with Windows line endings and a terminating NUL.
pub(super) fn clipboard_text(text: &str) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len() + 1);
    let mut previous = '\0';
    for ch in text.chars() {
        if ch == '\n' && previous != '\r' {
            out.push('\r' as u16);
        }
        let mut units = [0u16; 2];
        out.extend_from_slice(ch.encode_utf16(&mut units));
        previous = ch;
    }
    out.push(0);
    out
}

/// Text offered with delayed rendering, shared with the window procedure.
struct Offer {
    text: Option<Vec<u16>>,
    rendered_at: Option<Instant>,
}

static OFFER: Mutex<Offer> = Mutex::new(Offer {
    text: None,
    rendered_at: None,
});
static RENDERED: Condvar = Condvar::new();

fn offer_state() -> MutexGuard<'static, Offer> {
    OFFER.lock().unwrap_or_else(|e| e.into_inner())
}

type Job = Box<dyn FnOnce(HWND) + Send>;

pub struct Clipboard {
    hwnd: usize,
    jobs: Sender<Job>,
    thread: Option<JoinHandle<()>>,
}

impl Clipboard {
    pub fn start() -> anyhow::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (jobs, queue) = mpsc::channel::<Job>();
        let thread = std::thread::Builder::new()
            .name("joona-clipboard".into())
            .spawn(move || owner_thread(ready_tx, queue))?;
        let hwnd = ready_rx.recv().context("the clipboard thread exited")??;
        Ok(Self {
            hwnd,
            jobs,
            thread: Some(thread),
        })
    }

    fn window(&self) -> HWND {
        HWND(self.hwnd as *mut c_void)
    }

    /// Runs `job` on the clipboard thread and waits for its result.
    fn run<R: Send + 'static>(
        &self,
        job: impl FnOnce(HWND) -> R + Send + 'static,
    ) -> anyhow::Result<R> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.jobs
            .send(Box::new(move |hwnd| {
                let _ = tx.send(job(hwnd));
            }))
            .map_err(|_| anyhow!("the clipboard thread stopped"))?;
        // SAFETY: posting to our own window.
        unsafe { PostMessageW(Some(self.window()), WM_RUN_JOBS, WPARAM(0), LPARAM(0)) }?;
        rx.recv_timeout(JOB_TIMEOUT)
            .context("the clipboard didn't respond")
    }

    pub fn snapshot(&self) -> anyhow::Result<Snapshot> {
        self.run(snapshot)?
    }

    /// Puts `text` on the clipboard with delayed rendering, so Joona learns
    /// when an app reads it. It is kept out of clipboard history.
    pub fn offer(&self, text: &str) -> anyhow::Result<()> {
        let units = clipboard_text(text);
        self.run(move |hwnd| offer(hwnd, units))?
    }

    /// When an app first asked for the offered text, waiting up to `timeout`.
    pub fn wait_rendered(&self, timeout: Duration) -> Option<Instant> {
        let state = offer_state();
        let (state, _) = RENDERED
            .wait_timeout_while(state, timeout, |s| {
                s.rendered_at.is_none() && s.text.is_some()
            })
            .unwrap_or_else(|e| e.into_inner());
        state.rendered_at
    }

    /// Makes the offered text real, so it can be pasted after Joona exits.
    pub fn render_now(&self) -> anyhow::Result<()> {
        self.run(|hwnd| {
            let _open = Open::new(hwnd)?;
            // SAFETY: the clipboard is open.
            if unsafe { GetClipboardOwner() }.ok() == Some(hwnd) {
                render();
            }
            Ok(())
        })?
    }

    /// Puts `snapshot` back. Unless `force` is set, it does nothing (and
    /// returns false) when another app has replaced the clipboard meanwhile.
    pub fn restore(&self, snapshot: Snapshot, force: bool) -> anyhow::Result<bool> {
        self.run(move |hwnd| restore(hwnd, snapshot, force))?
    }

    /// Puts `text` on the clipboard. `private` keeps it out of clipboard history.
    pub fn set_text(&self, text: &str, private: bool) -> anyhow::Result<()> {
        let units = clipboard_text(text);
        self.run(move |hwnd| {
            let _open = Open::new(hwnd)?;
            // SAFETY: the clipboard is open.
            unsafe { EmptyClipboard() }?;
            set_global(CF_UNICODETEXT, &units_to_bytes(&units))?;
            if private {
                mark_private();
            }
            Ok(())
        })?
    }

    pub fn text(&self) -> anyhow::Result<Option<String>> {
        self.run(|hwnd| {
            let _open = Open::new(hwnd)?;
            // SAFETY: the clipboard is open.
            Ok(unsafe { read_text() })
        })?
    }

    /// The text an app just copied, unless it copied the whole line because
    /// nothing was selected, as code editors do.
    pub fn copied_selection(&self) -> anyhow::Result<Option<String>> {
        self.run(|hwnd| {
            let _open = Open::new(hwnd)?;
            // SAFETY: the clipboard is open.
            unsafe {
                if whole_line_copied() {
                    return Ok(None);
                }
                Ok(read_text())
            }
        })?
    }

    /// Changes every time anything is put on the clipboard.
    pub fn sequence() -> u32 {
        // SAFETY: no preconditions.
        unsafe { GetClipboardSequenceNumber() }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: posting to our own window. Destroying it renders any text
        // still on offer, so it outlives the app.
        unsafe {
            let _ = PostMessageW(Some(self.window()), WM_CLOSE_OWNER, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn owner_thread(ready: SyncSender<anyhow::Result<usize>>, queue: Receiver<Job>) {
    let hwnd = match create_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(hwnd.0 as usize));
    let mut msg = MSG::default();
    loop {
        // SAFETY: a standard message loop on the thread that owns the window.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
        match msg.message {
            WM_RUN_JOBS => {
                while let Ok(job) = queue.try_recv() {
                    job(hwnd);
                }
            }
            WM_CLOSE_OWNER => unsafe {
                let _ = DestroyWindow(hwnd);
            },
            _ => unsafe {
                DispatchMessageW(&msg);
            },
        }
    }
}

fn create_window() -> anyhow::Result<HWND> {
    let class = w!("JoonaClipboardOwner");
    // SAFETY: registering a class and creating a message-only window with it.
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        };
        // Fails harmlessly if the class already exists.
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Joona clipboard"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )?;
        Ok(hwnd)
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_RENDERFORMAT => {
            if wparam.0 as u32 == CF_UNICODETEXT {
                render();
            }
            LRESULT(0)
        }
        WM_RENDERALLFORMATS => {
            // SAFETY: the system asks the owner to open the clipboard itself here.
            unsafe {
                if OpenClipboard(Some(hwnd)).is_ok() {
                    if GetClipboardOwner().ok() == Some(hwnd) {
                        render();
                    }
                    let _ = CloseClipboard();
                }
            }
            LRESULT(0)
        }
        WM_DESTROYCLIPBOARD => {
            let mut state = offer_state();
            state.text = None;
            RENDERED.notify_all();
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Supplies the offered text. Only called while the clipboard is open, by us
/// or by the app that asked for it.
fn render() {
    let mut state = offer_state();
    let Some(text) = state.text.as_ref() else {
        return;
    };
    if let Err(e) = set_global(CF_UNICODETEXT, &units_to_bytes(text)) {
        tracing::warn!("couldn't render the pasted text: {e}");
    }
    if state.rendered_at.is_none() {
        state.rendered_at = Some(Instant::now());
    }
    RENDERED.notify_all();
}

/// Keeps the clipboard open for as long as it lives.
struct Open;

impl Open {
    fn new(hwnd: HWND) -> anyhow::Result<Self> {
        let deadline = Instant::now() + OPEN_TIMEOUT;
        loop {
            // SAFETY: the window belongs to this thread.
            let error = match unsafe { OpenClipboard(Some(hwnd)) } {
                Ok(()) => return Ok(Open),
                Err(e) => e,
            };
            if Instant::now() >= deadline {
                bail!("couldn't open the clipboard: {}", error.message());
            }
            // The app holding the clipboard may be waiting for us to render
            // something; peeking answers its request.
            let mut msg = MSG::default();
            // SAFETY: peeking without removing only dispatches sent messages.
            unsafe {
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: we opened it.
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn snapshot(hwnd: HWND) -> anyhow::Result<Snapshot> {
    let _open = Open::new(hwnd)?;
    let started = Instant::now();
    let mut formats = Vec::new();
    let mut total = 0;
    let mut format = 0;
    loop {
        // SAFETY: the clipboard is open.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if !copyable(format) {
            continue;
        }
        // SAFETY: the clipboard is open.
        if let Some(bytes) = unsafe { read_global(format) } {
            total += bytes.len();
            formats.push((format, bytes));
        }
        if total > SNAPSHOT_BYTES || started.elapsed() > SNAPSHOT_TIME {
            tracing::debug!(
                formats = formats.len(),
                total,
                "clipboard snapshot cut short"
            );
            break;
        }
    }
    Ok(Snapshot { formats })
}

fn offer(hwnd: HWND, units: Vec<u16>) -> anyhow::Result<()> {
    let _open = Open::new(hwnd)?;
    // SAFETY: the clipboard is open. Emptying it sends WM_DESTROYCLIPBOARD
    // to the previous owner, which may be us, so the new offer is recorded
    // after that.
    unsafe { EmptyClipboard() }?;
    {
        let mut state = offer_state();
        state.text = Some(units);
        state.rendered_at = None;
    }
    // SAFETY: a null handle requests delayed rendering. Windows reports that
    // as a null result, so the result isn't checked.
    unsafe {
        let _ = SetClipboardData(CF_UNICODETEXT, None);
    }
    mark_private();
    Ok(())
}

fn restore(hwnd: HWND, snapshot: Snapshot, force: bool) -> anyhow::Result<bool> {
    let _open = Open::new(hwnd)?;
    // SAFETY: the clipboard is open.
    if !force && unsafe { GetClipboardOwner() }.ok() != Some(hwnd) {
        return Ok(false);
    }
    // SAFETY: the clipboard is open.
    unsafe { EmptyClipboard() }?;
    for (format, bytes) in &snapshot.formats {
        if let Err(e) = set_global(*format, bytes) {
            tracing::debug!(format, "couldn't restore a clipboard format: {e}");
        }
    }
    // The restored contents are already in clipboard history once.
    mark_private();
    Ok(true)
}

/// Asks clipboard history, cloud clipboard and clipboard managers to skip
/// the current contents. Must be called with the clipboard open.
fn mark_private() {
    let markers: [PCWSTR; 3] = [
        w!("ExcludeClipboardContentFromMonitorProcessing"),
        w!("CanIncludeInClipboardHistory"),
        w!("CanUploadToCloudClipboard"),
    ];
    for name in markers {
        // SAFETY: registering a named format has no preconditions.
        let format = unsafe { RegisterClipboardFormatW(name) };
        if format != 0 {
            let _ = set_global(format, &0u32.to_le_bytes());
        }
    }
}

fn units_to_bytes(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|u| u.to_le_bytes()).collect()
}

/// The clipboard's text. The clipboard must be open.
unsafe fn read_text() -> Option<String> {
    // SAFETY: the caller opened the clipboard.
    let bytes = unsafe { read_global(CF_UNICODETEXT) }?;
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| u16::from_le_bytes(b))
        .take_while(|&u| u != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// VS Code and its forks, and Visual Studio, copy the line the cursor is on
/// when nothing is selected, and say so in a format of their own. The
/// clipboard must be open.
unsafe fn whole_line_copied() -> bool {
    // SAFETY: registering a named format has no preconditions, and the
    // caller opened the clipboard.
    unsafe {
        let vscode = RegisterClipboardFormatW(w!("vscode-editor-data"));
        if vscode != 0
            && let Some(data) = read_global(vscode)
            && empty_selection(&data)
        {
            return true;
        }
        [
            w!("VisualStudioEditorOperationsLineCutCopyClipboardTag"),
            w!("MSDEVLineSelect"),
        ]
        .into_iter()
        .map(|name| RegisterClipboardFormatW(name))
        .any(|format| format != 0 && GetClipboardData(format).is_ok())
    }
}

/// VS Code's clipboard data, as in `{"version":1,"isFromEmptySelection":true}`.
fn empty_selection(data: &[u8]) -> bool {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    serde_json::from_slice::<serde_json::Value>(&data[..end])
        .is_ok_and(|json| json["isFromEmptySelection"] == true)
}

/// Copies the data of a memory-based format. The clipboard must be open.
unsafe fn read_global(format: u32) -> Option<Vec<u8>> {
    // SAFETY: the caller opened the clipboard; the handle stays valid until
    // it is closed, and the lock bounds the copy.
    unsafe {
        let handle = GetClipboardData(format).ok()?;
        let mem = HGLOBAL(handle.0);
        let size = GlobalSize(mem);
        if size == 0 {
            return None;
        }
        let ptr = GlobalLock(mem);
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr as *const u8, size).to_vec();
        let _ = GlobalUnlock(mem);
        Some(bytes)
    }
}

/// Puts a copy of `bytes` on the clipboard. The clipboard must be open.
fn set_global(format: u32, bytes: &[u8]) -> anyhow::Result<()> {
    // SAFETY: the memory is sized for the copy, and ownership passes to the
    // system only when SetClipboardData succeeds.
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
        let ptr = GlobalLock(mem);
        if ptr.is_null() {
            let _ = GlobalFree(Some(mem));
            bail!("couldn't lock clipboard memory");
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        let _ = GlobalUnlock(mem);
        if let Err(e) = SetClipboardData(format, Some(HANDLE(mem.0))) {
            let _ = GlobalFree(Some(mem));
            return Err(e.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_memory_formats_are_copied() {
        for format in [1, 7, 8, 13, 15, 16, 17, 0xC000, 0xC123] {
            assert!(copyable(format), "format {format:#x}");
        }
        for format in [2, 3, 9, 14, 0x80, 0x82, 0x200, 0x2FF, 0x300, 0x3FF] {
            assert!(!copyable(format), "format {format:#x}");
        }
    }

    #[test]
    fn a_line_copied_without_a_selection_is_spotted() {
        assert!(empty_selection(
            br#"{"version":1,"isFromEmptySelection":true,"multicursorText":null,"mode":"rust"}"#
        ));
        assert!(empty_selection(b"{\"isFromEmptySelection\":true}\0\0"));
        assert!(!empty_selection(
            br#"{"version":1,"isFromEmptySelection":false,"mode":"rust"}"#
        ));
        assert!(!empty_selection(b"not json"));
    }

    #[test]
    fn clipboard_text_uses_windows_line_endings() {
        let units = clipboard_text("a\nb\r\nc");
        assert_eq!(String::from_utf16(&units).unwrap(), "a\r\nb\r\nc\0");
        assert_eq!(clipboard_text(""), vec![0]);
    }
}
