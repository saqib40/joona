//! The Joona Bar's window: where it sits, when it shows, and which parts of
//! it take clicks.
//!
//! The bar is one transparent, always-on-top window that never takes focus.
//! It is click-through except over the regions the UI reports (the pill,
//! toasts, an open menu). A poller follows the cursor to switch
//! click-through on and off, moves the bar to the cursor's monitor while
//! idle, and hides it over full-screen apps.
//!
//! The window is shown once, without activation, when it is created, and is
//! then driven with Win32 calls. Tauri's show, hide and click-through calls
//! rewrite the window's styles, and showing through them activates the
//! window, which would steal focus from the app being dictated into.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::settings::BarVisibility;

/// Logical size of the bar window, as in `tauri.conf.json`.
pub const BAR_WIDTH: f64 = 460.0;
pub const BAR_HEIGHT: f64 = 280.0;
const POLL: Duration = Duration::from_millis(33);
/// Placement and visibility are checked every this many polls.
const SLOW_EVERY: u32 = 8;

/// A part of the bar that takes the mouse, in CSS pixels from the window's
/// top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Region {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

pub trait OverlayEvents: Send + Sync + 'static {
    /// The cursor entered or left the interactive parts of the bar.
    fn hover(&self, inside: bool);
    /// Whether the bar should draw its resting pill.
    fn resting(&self, visible: bool);
}

pub struct Overlay {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

struct Shared {
    hwnd: isize,
    regions: Mutex<Vec<Region>>,
    dictating: AtomicBool,
    /// Place and raise the bar on the next poll instead of waiting.
    kick: AtomicBool,
    visibility: Mutex<BarVisibility>,
    snoozed_until: Mutex<Option<Instant>>,
    resting: AtomicBool,
    quit: AtomicBool,
    events: Box<dyn OverlayEvents>,
}

impl Overlay {
    /// Takes over the bar window. `hwnd` is its native handle.
    pub fn start(
        hwnd: isize,
        visibility: BarVisibility,
        events: impl OverlayEvents,
    ) -> Arc<Overlay> {
        let shared = Arc::new(Shared {
            hwnd,
            regions: Mutex::new(Vec::new()),
            dictating: AtomicBool::new(false),
            kick: AtomicBool::new(true),
            visibility: Mutex::new(visibility),
            snoozed_until: Mutex::new(None),
            resting: AtomicBool::new(visibility == BarVisibility::Always),
            quit: AtomicBool::new(false),
            events: Box::new(events),
        });
        let poller = shared.clone();
        let thread = std::thread::Builder::new()
            .name("joona-overlay".into())
            .spawn(move || run(poller))
            .map_err(|e| tracing::error!("couldn't start the bar's poller: {e}"))
            .ok();
        Arc::new(Overlay {
            shared,
            thread: Mutex::new(thread),
        })
    }

    pub fn set_regions(&self, regions: Vec<Region>) {
        *lock(&self.shared.regions) = regions;
    }

    pub fn set_dictating(&self, dictating: bool) {
        let was = self.shared.dictating.swap(dictating, Ordering::Relaxed);
        if dictating && !was {
            self.shared.kick.store(true, Ordering::Relaxed);
        }
    }

    pub fn set_visibility(&self, visibility: BarVisibility) {
        *lock(&self.shared.visibility) = visibility;
        self.shared.kick.store(true, Ordering::Relaxed);
    }

    /// "Hide for 1 hour": the bar stays hidden at rest, but still shows while dictating.
    pub fn snooze(&self, duration: Duration) {
        *lock(&self.shared.snoozed_until) = Some(Instant::now() + duration);
        self.shared.kick.store(true, Ordering::Relaxed);
    }

    pub fn unsnooze(&self) {
        *lock(&self.shared.snoozed_until) = None;
        self.shared.kick.store(true, Ordering::Relaxed);
    }

    pub fn snoozed(&self) -> bool {
        lock(&self.shared.snoozed_until).is_some_and(|until| Instant::now() < until)
    }

    /// Whether the resting pill is showing right now.
    pub fn resting_visible(&self) -> bool {
        self.shared.resting.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        self.stop();
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn resting_wanted(&self, fullscreen: bool) -> bool {
        let snoozed = {
            let mut until = lock(&self.snoozed_until);
            if until.is_some_and(|t| Instant::now() >= t) {
                *until = None;
            }
            until.is_some()
        };
        *lock(&self.visibility) == BarVisibility::Always && !snoozed && !fullscreen
    }

    fn report_resting(&self, visible: bool) {
        if self.resting.swap(visible, Ordering::Relaxed) != visible {
            self.events.resting(visible);
        }
    }
}

#[cfg(windows)]
fn run(shared: Arc<Shared>) {
    let hwnd = shared.hwnd;
    let mut hovering = false;
    let mut placed: Option<win::Placement> = None;
    let mut shown = true;
    let mut tick: u32 = 0;
    win::set_click_through(hwnd, true);
    shared
        .events
        .resting(shared.resting.load(Ordering::Relaxed));

    while !shared.quit.load(Ordering::Relaxed) {
        let dictating = shared.dictating.load(Ordering::Relaxed);
        let kick = shared.kick.swap(false, Ordering::Relaxed);
        let cursor = win::cursor();
        let regions = lock(&shared.regions).clone();

        if kick || tick.is_multiple_of(SLOW_EVERY) {
            // Follow the cursor's monitor, but never move the bar mid-dictation.
            if let Some(target) = cursor.and_then(win::placement_at)
                && Some(target) != placed
                && (!dictating || kick)
            {
                win::place(hwnd, &target);
                placed = Some(target);
            }
            let fullscreen = placed.is_some_and(|p| win::fullscreen_app_on(p.monitor, hwnd));
            shared.report_resting(shared.resting_wanted(fullscreen));
            // Over a full-screen app the window is hidden outright, unless
            // there is something to see, so games keep exclusive output.
            let visible = dictating || !fullscreen || !regions.is_empty();
            if visible != shown {
                win::show(hwnd, visible);
                shown = visible;
            }
            if shown && (kick || tick.is_multiple_of(SLOW_EVERY * 4)) {
                win::raise(hwnd);
            }
        }

        let inside = shown && cursor.is_some_and(|c| win::hit(hwnd, c, &regions));
        if inside != hovering {
            hovering = inside;
            win::set_click_through(hwnd, !inside);
            shared.events.hover(inside);
        } else if tick.is_multiple_of(SLOW_EVERY) {
            // Undo any rewrite of the window's styles by the window library.
            win::set_click_through(hwnd, !hovering);
        }

        tick = tick.wrapping_add(1);
        std::thread::sleep(POLL);
    }
}

#[cfg(target_os = "macos")]
fn run(shared: Arc<Shared>) {
    let window_handle = shared.hwnd as usize;
    if window_handle == 0 {
        return;
    }

    mac::setup_window(window_handle);
    mac::place_at_bottom_center(window_handle);

    let mut hovering = false;
    let mut shown = true;
    let mut tick: u32 = 0;

    shared
        .events
        .resting(shared.resting.load(Ordering::Relaxed));

    while !shared.quit.load(Ordering::Relaxed) {
        let dictating = shared.dictating.load(Ordering::Relaxed);
        let kick = shared.kick.swap(false, Ordering::Relaxed);
        let cursor = mac::cursor();
        let regions = lock(&shared.regions).clone();

        if kick || tick.is_multiple_of(SLOW_EVERY) {
            mac::place_at_bottom_center(window_handle);
            shared.report_resting(shared.resting_wanted(false));
            let visible = dictating || !regions.is_empty() || shared.resting.load(Ordering::Relaxed);
            if visible != shown {
                mac::show(window_handle, visible);
                shown = visible;
            }
            if shown && (kick || tick.is_multiple_of(SLOW_EVERY * 4)) {
                mac::raise(window_handle);
            }
        }

        let inside = shown && cursor.is_some_and(|c| mac::hit(c, &regions));
        if inside != hovering {
            hovering = inside;
            mac::set_click_through(window_handle, !inside);
            shared.events.hover(inside);
        }

        tick = tick.wrapping_add(1);
        std::thread::sleep(POLL);
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn run(shared: Arc<Shared>) {
    shared
        .events
        .resting(shared.resting.load(Ordering::Relaxed));
    while !shared.quit.load(Ordering::Relaxed) {
        shared.report_resting(shared.resting_wanted(false));
        std::thread::sleep(POLL * SLOW_EVERY);
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::Mutex;

    use objc2::{class, msg_send};
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    use super::{BAR_HEIGHT, BAR_WIDTH, Region, lock};

    #[link(name = "System", kind = "dylib")]
    unsafe extern "C" {
        static _dispatch_main_q: [u8; 0];
        fn dispatch_async_f(
            queue: *const std::ffi::c_void,
            context: *mut std::ffi::c_void,
            work: unsafe extern "C" fn(*mut std::ffi::c_void),
        );
    }

    #[derive(Clone, Copy, PartialEq)]
    struct PlacedFrame {
        x: f64,
        y: f64,
    }

    static PLACED_FRAME: Mutex<Option<PlacedFrame>> = Mutex::new(None);

    fn run_on_main<F: FnOnce() + Send + 'static>(f: F) {
        let boxed: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(f));
        let raw = Box::into_raw(boxed) as *mut std::ffi::c_void;

        unsafe extern "C" fn trampoline(context: *mut std::ffi::c_void) {
            let boxed = unsafe { Box::from_raw(context as *mut Box<dyn FnOnce() + Send>) };
            boxed();
        }

        unsafe {
            let queue = std::ptr::addr_of!(_dispatch_main_q) as *const std::ffi::c_void;
            dispatch_async_f(queue, raw, trampoline);
        }
    }

    pub fn setup_window(window_ptr: usize) {
        run_on_main(move || unsafe {
            let window = window_ptr as *mut AnyObject;
            if window.is_null() {
                return;
            }
            // kCGOverlayWindowLevelKey = 102 keeps the bar above full-screen apps and floating windows.
            let _: () = msg_send![window, setLevel: 102i64];
            // NSWindowCollectionBehaviorCanJoinAllSpaces (1 << 0) | IgnoresCycle (1 << 6) | FullScreenAuxiliary (1 << 8)
            let behavior: usize = (1 << 0) | (1 << 6) | (1 << 8);
            let _: () = msg_send![window, setCollectionBehavior: behavior];
            let _: () = msg_send![window, setHidesOnDeactivate: false];
            let _: () = msg_send![window, setCanHide: false];
            let _: () = msg_send![window, setAcceptsMouseMovedEvents: true];
            let _: () = msg_send![window, setIgnoresMouseEvents: true];
        });
    }

    pub fn set_click_through(window_ptr: usize, click_through: bool) {
        run_on_main(move || unsafe {
            let window = window_ptr as *mut AnyObject;
            if !window.is_null() {
                let _: () = msg_send![window, setIgnoresMouseEvents: click_through];
            }
        });
    }

    pub fn show(window_ptr: usize, visible: bool) {
        run_on_main(move || unsafe {
            let window = window_ptr as *mut AnyObject;
            if window.is_null() {
                return;
            }
            if visible {
                let _: () = msg_send![window, setLevel: 102i64];
                let _: () = msg_send![window, orderFrontRegardless];
            } else {
                let _: () = msg_send![window, orderOut: std::ptr::null::<AnyObject>()];
            }
        });
    }

    pub fn raise(window_ptr: usize) {
        run_on_main(move || unsafe {
            let window = window_ptr as *mut AnyObject;
            if !window.is_null() {
                let _: () = msg_send![window, setLevel: 102i64];
                let _: () = msg_send![window, orderFrontRegardless];
            }
        });
    }

    pub fn place_at_bottom_center(window_ptr: usize) {
        run_on_main(move || unsafe {
            let window = window_ptr as *mut AnyObject;
            if window.is_null() {
                return;
            }
            let mouse = objc2_app_kit::NSEvent::mouseLocation();
            let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
            let mut screen: *mut AnyObject = std::ptr::null_mut();
            if !screens.is_null() {
                let count: usize = msg_send![screens, count];
                for i in 0..count {
                    let s: *mut AnyObject = msg_send![screens, objectAtIndex: i];
                    if !s.is_null() {
                        let f: NSRect = msg_send![s, frame];
                        if mouse.x >= f.origin.x
                            && mouse.x < f.origin.x + f.size.width
                            && mouse.y >= f.origin.y
                            && mouse.y < f.origin.y + f.size.height
                        {
                            screen = s;
                            break;
                        }
                    }
                }
            }
            if screen.is_null() {
                screen = msg_send![window, screen];
            }
            if screen.is_null() {
                screen = msg_send![class!(NSScreen), mainScreen];
            }
            if screen.is_null() {
                return;
            }
            let visible_frame: NSRect = msg_send![screen, visibleFrame];
            let x = visible_frame.origin.x + (visible_frame.size.width - BAR_WIDTH) / 2.0;
            let y = visible_frame.origin.y + 16.0;
            let current = *lock(&PLACED_FRAME);
            if current != Some(PlacedFrame { x, y }) {
                *lock(&PLACED_FRAME) = Some(PlacedFrame { x, y });

                let new_frame = NSRect::new(
                    NSPoint::new(x, y),
                    NSSize::new(BAR_WIDTH, BAR_HEIGHT),
                );
                let _: () = msg_send![window, setFrame: new_frame, display: true];
                let _: () = msg_send![window, setLevel: 102i64];
                let _: () = msg_send![window, orderFrontRegardless];
            }
        });
    }

    pub fn cursor() -> Option<(f64, f64)> {
        let pt = objc2_app_kit::NSEvent::mouseLocation();
        Some((pt.x, pt.y))
    }

    pub fn hit((cursor_x, cursor_y): (f64, f64), regions: &[Region]) -> bool {
        if regions.is_empty() {
            return false;
        }
        let frame = lock(&PLACED_FRAME).clone();
        let Some(PlacedFrame { x, y }) = frame else {
            return false;
        };
        let rel_x = cursor_x - x;
        let rel_y = y + BAR_HEIGHT - cursor_y;
        regions.iter().any(|r| r.contains(rel_x, rel_y))
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MONITORINFO,
        MonitorFromPoint, MonitorFromWindow,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::Shell::{
        ABE_BOTTOM, ABM_GETSTATE, ABM_GETTASKBARPOS, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetCursorPos, GetForegroundWindow,
        GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HWND_TOPMOST, IsZoomed,
        SW_HIDE, SW_SHOWNOACTIVATE, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_CAPTION,
        WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    };

    use super::{BAR_HEIGHT, BAR_WIDTH, Region};

    /// Where the bar goes on one monitor, in physical pixels.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Placement {
        pub monitor: isize,
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
    }

    fn hwnd(h: isize) -> HWND {
        HWND(h as *mut c_void)
    }

    pub fn cursor() -> Option<(i32, i32)> {
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p) }.ok().map(|_| (p.x, p.y))
    }

    fn monitor_info(monitor: HMONITOR) -> Option<MONITORINFO> {
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe { GetMonitorInfoW(monitor, &mut info) }
            .as_bool()
            .then_some(info)
    }

    /// Bottom centre of the monitor's work area, above the taskbar.
    pub fn placement_at((x, y): (i32, i32)) -> Option<Placement> {
        let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
        let info = monitor_info(monitor)?;
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
        let scale = f64::from(dpi_x.max(96)) / 96.0;
        let width = (BAR_WIDTH * scale).round() as i32;
        let height = (BAR_HEIGHT * scale).round() as i32;
        let work = info.rcWork;
        let gap = hidden_taskbar_gap(&info, scale);
        Some(Placement {
            monitor: monitor.0 as isize,
            x: work.left + (work.right - work.left - width) / 2,
            y: work.bottom - height - gap,
            width,
            height,
        })
    }

    /// An auto-hiding taskbar slides over the bottom of the screen, so the
    /// bar keeps clear of where it will appear.
    fn hidden_taskbar_gap(info: &MONITORINFO, scale: f64) -> i32 {
        let mut data = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            ..Default::default()
        };
        let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) } as u32;
        if state & ABS_AUTOHIDE == 0 || info.rcWork.bottom != info.rcMonitor.bottom {
            return 0;
        }
        let mut pos = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            ..Default::default()
        };
        let found = unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut pos) } != 0;
        if found && pos.uEdge != ABE_BOTTOM {
            return 0;
        }
        // This reports the main taskbar; other monitors have their own, scaled to their DPI.
        let m = info.rcMonitor;
        let here = found
            && pos.rc.left < m.right
            && pos.rc.right > m.left
            && pos.rc.top < m.bottom
            && pos.rc.bottom > m.top;
        let height = if here { pos.rc.bottom - pos.rc.top } else { 0 };
        height.max((48.0 * scale).round() as i32)
    }

    pub fn place(h: isize, p: &Placement) {
        // Moving onto a monitor with another scale makes Windows resize the
        // window once more, so check and correct.
        for _ in 0..2 {
            unsafe {
                let _ = SetWindowPos(
                    hwnd(h),
                    Some(HWND_TOPMOST),
                    p.x,
                    p.y,
                    p.width,
                    p.height,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
            let mut r = RECT::default();
            if unsafe { GetWindowRect(hwnd(h), &mut r) }.is_err()
                || (r.left, r.top, r.right - r.left, r.bottom - r.top)
                    == (p.x, p.y, p.width, p.height)
            {
                break;
            }
        }
    }

    pub fn raise(h: isize) {
        unsafe {
            let _ = SetWindowPos(
                hwnd(h),
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
            );
        }
    }

    pub fn show(h: isize, visible: bool) {
        unsafe {
            let _ = ShowWindow(hwnd(h), if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }

    /// Click-through or not, and always a tool window that never activates.
    /// Without `WS_EX_APPWINDOW`, showing the window again adds no taskbar button.
    pub fn set_click_through(h: isize, on: bool) {
        let click_through = (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0) as isize;
        let always = (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize;
        unsafe {
            let style = GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE);
            let base = (style | always) & !(WS_EX_APPWINDOW.0 as isize);
            let wanted = if on {
                base | click_through
            } else {
                base & !click_through
            };
            if wanted != style {
                SetWindowLongPtrW(hwnd(h), GWL_EXSTYLE, wanted);
            }
        }
    }

    pub fn hit(h: isize, (cx, cy): (i32, i32), regions: &[Region]) -> bool {
        if regions.is_empty() {
            return false;
        }
        let mut r = RECT::default();
        if unsafe { GetWindowRect(hwnd(h), &mut r) }.is_err() {
            return false;
        }
        let dpi = unsafe { GetDpiForWindow(hwnd(h)) };
        let scale = if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 };
        let x = f64::from(cx - r.left) / scale;
        let y = f64::from(cy - r.top) / scale;
        regions.iter().any(|region| region.contains(x, y))
    }

    /// Whether the app in front covers the whole of `monitor`, like a game,
    /// a video or a presentation.
    pub fn fullscreen_app_on(monitor: isize, ours: isize) -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.0.is_null() || fg.0 as isize == ours {
                return false;
            }
            let mut class = [0u16; 64];
            let len = GetClassNameW(fg, &mut class) as usize;
            let class = String::from_utf16_lossy(&class[..len.min(class.len())]);
            if matches!(
                class.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ) {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(fg, Some(&mut pid));
            if pid == std::process::id() {
                return false;
            }
            // A maximised window also covers the monitor when the taskbar auto-hides.
            let caption = WS_CAPTION.0 as isize;
            if IsZoomed(fg).as_bool() || GetWindowLongPtrW(fg, GWL_STYLE) & caption == caption {
                return false;
            }
            let on = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
            if on.0 as isize != monitor {
                return false;
            }
            let (Some(info), mut r) = (monitor_info(on), RECT::default()) else {
                return false;
            };
            if GetWindowRect(fg, &mut r).is_err() {
                return false;
            }
            let m = info.rcMonitor;
            r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ns_screen_frame() {
        #[cfg(target_os = "macos")]
        unsafe {
            use objc2::runtime::AnyObject;
            use objc2::{class, msg_send};
            use objc2_foundation::NSRect;
            let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
            assert!(!screens.is_null());
            let count: usize = msg_send![screens, count];
            assert!(count >= 1);
            let s: *mut AnyObject = msg_send![screens, objectAtIndex: 0usize];
            let f: NSRect = msg_send![s, frame];
            assert!(f.size.width > 0.0 && f.size.height > 0.0);
        }
    }

    #[test]
    fn regions_contain_their_inside_only() {
        let r = Region {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 30.0,
        };
        assert!(r.contains(10.0, 20.0));
        assert!(r.contains(109.0, 49.0));
        assert!(!r.contains(110.0, 30.0));
        assert!(!r.contains(50.0, 19.9));
    }
}
