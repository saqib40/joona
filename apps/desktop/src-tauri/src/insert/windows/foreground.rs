//! Which app is in front, and whether Joona is allowed to type into it.

use std::ffi::c_void;
use std::path::Path;

use ::windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use ::windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use ::windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
    InternalGetWindowText, IsHungAppWindow,
};
use ::windows::core::{BOOL, PWSTR};

use crate::insert::ForegroundApp;

/// The integrity level of an elevated ("Run as administrator") process.
const HIGH_INTEGRITY: u32 = 0x3000;

pub fn foreground() -> Option<ForegroundApp> {
    // SAFETY: no preconditions.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = window_pid(hwnd);
    let mut path = process_path(pid);
    // Store apps draw inside ApplicationFrameHost; the app itself owns a child window.
    if exe_name(path.as_deref()) == "applicationframehost.exe"
        && let Some(child) = uwp_child_pid(hwnd, pid)
    {
        pid = child;
        path = process_path(child);
    }
    // SAFETY: no preconditions.
    let own = pid == unsafe { GetCurrentProcessId() };
    Some(ForegroundApp {
        exe: exe_name(path.as_deref()),
        title: window_text(hwnd),
        pid,
        elevated: !own && is_elevated_above_us(pid),
        own,
        class: class_name(hwnd),
        // SAFETY: a read-only query on a window handle.
        hung: unsafe { IsHungAppWindow(hwnd) }.as_bool(),
    })
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: `pid` outlives the call.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Reads the title without sending the window a message, so a hung app
/// can't block the call.
fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: the buffer length is passed with the slice.
    let len = unsafe { InternalGetWindowText(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: the buffer length is passed with the slice.
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn exe_name(path: Option<&str>) -> String {
    path.and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

struct Process(HANDLE);

impl Process {
    fn open(pid: u32) -> Option<Self> {
        // SAFETY: limited query access is enough for the calls below.
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .ok()
            .map(Process)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn process_path(pid: u32) -> Option<String> {
    let process = Process::open(pid)?;
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `len` holds the buffer size and receives the written length.
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// The mandatory integrity level of a process token, such as 0x2000 for a
/// normal app and 0x3000 for an elevated one.
fn integrity_level(process: HANDLE) -> Option<u32> {
    // SAFETY: the token handle is closed below; the buffer is u64-aligned for
    // the pointer inside TOKEN_MANDATORY_LABEL and sized by the first call.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut len);
        let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
        let got = GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr() as *mut c_void),
            len,
            &mut len,
        );
        let _ = CloseHandle(token);
        got.ok()?;
        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        if count == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(sid, u32::from(count) - 1))
    }
}

/// Windows blocks simulated input into apps with a higher integrity level
/// than the sender (User Interface Privilege Isolation).
fn is_elevated_above_us(pid: u32) -> bool {
    // SAFETY: the pseudo handle for this process needs no closing.
    let ours = integrity_level(unsafe { GetCurrentProcess() }).unwrap_or(0x2000);
    match Process::open(pid).and_then(|p| integrity_level(p.0)) {
        Some(theirs) => theirs > ours,
        // Not even allowed to look: assume it is elevated unless we are.
        None => ours < HIGH_INTEGRITY,
    }
}

fn uwp_child_pid(frame: HWND, frame_pid: u32) -> Option<u32> {
    struct Search {
        frame_pid: u32,
        found: Option<u32>,
    }
    unsafe extern "system" fn visit(child: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam points at the Search below for the duration of the enumeration.
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        let pid = window_pid(child);
        if pid != search.frame_pid {
            search.found = Some(pid);
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut search = Search {
        frame_pid,
        found: None,
    };
    // SAFETY: `search` outlives the synchronous enumeration.
    unsafe {
        let _ = EnumChildWindows(
            Some(frame),
            Some(visit),
            LPARAM(&mut search as *mut Search as isize),
        );
    }
    search.found
}
