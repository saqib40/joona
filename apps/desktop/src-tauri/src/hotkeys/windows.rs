//! Low-level keyboard (and optional mouse) hook on a dedicated thread.
//!
//! Windows calls a low-level hook on the thread that installed it, through that
//! thread's message loop, and silently removes hooks that respond slowly. So the
//! callback only runs the pure [`GestureMachine`] under a short lock and queues
//! the resulting actions. Everything that touches other windows, like injecting
//! the Start-menu mask key, happens later in the message loop.

use std::sync::Mutex;
use std::sync::mpsc as std_mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::UnboundedSender;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP,
    WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN, WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::gesture::{Action, GestureConfig, GestureMachine, KeyEvent, Modifier, vk};
use super::record::{Recorded, Recorder};

/// Marks input that Joona itself injects, so the hook can ignore it.
pub const JOONA_INPUT_MARKER: usize = 0x4A4F_4F4E; // "JOON"

const WM_JOONA_MASK: u32 = WM_APP + 1;
const WM_JOONA_REFRESH: u32 = WM_APP + 2;
const TICK_MS: u32 = 25;
/// Windows can drop a hook without telling us, so it is re-installed regularly while idle.
const REINSTALL_EVERY: Duration = Duration::from_secs(300);
const LLMHF_INJECTED: u32 = 0x0000_0001;
const XBUTTON1: u32 = 0x0001;
/// Windows updates the async key state only after the hook returns, so a single
/// tick can see a key that is really down as up. A key counts as stuck only
/// after this many ticks in a row report it released.
const STUCK_TICKS: u8 = 4;

struct Shared {
    machine: GestureMachine,
    tx: UnboundedSender<Action>,
    epoch: Instant,
    thread_id: u32,
    suspects: Vec<(u16, u8)>,
    /// Set while Settings records a new shortcut.
    recorder: Option<(Recorder, UnboundedSender<Recorded>)>,
}

impl Shared {
    fn wants_mouse_hook(&self) -> bool {
        self.recorder.is_some() || wants_mouse_hook(self.machine.config())
    }
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

fn with_shared<R>(f: impl FnOnce(&mut Shared) -> R) -> Option<R> {
    let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_mut().map(f)
}

pub struct HookThread {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl HookThread {
    pub fn start(cfg: GestureConfig, tx: UnboundedSender<Action>) -> anyhow::Result<Self> {
        let (ready_tx, ready_rx) = std_mpsc::channel::<anyhow::Result<u32>>();
        let join = std::thread::Builder::new()
            .name("joona-hotkeys".into())
            .spawn(move || run_hook_thread(cfg, tx, ready_tx))?;
        let thread_id = ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow::anyhow!("hotkey thread did not start"))??;
        Ok(Self {
            thread_id,
            join: Some(join),
        })
    }

    pub fn update_config(&self, cfg: GestureConfig) {
        with_shared(|s| s.machine.set_config(cfg));
        self.post(WM_JOONA_REFRESH);
    }

    pub fn set_paused(&self, paused: bool) {
        with_shared(|s| s.machine.set_paused(paused));
    }

    pub fn session_ended(&self) {
        with_shared(|s| s.machine.session_ended());
    }

    pub fn begin_hands_free(&self) {
        with_shared(|s| s.machine.begin_hands_free());
    }

    pub fn record(&self, tx: UnboundedSender<Recorded>) {
        with_shared(|s| s.recorder = Some((Recorder::default(), tx)));
        self.post(WM_JOONA_REFRESH);
    }

    pub fn stop_recording(&self) {
        with_shared(|s| s.recorder = None);
        self.post(WM_JOONA_REFRESH);
    }

    fn post(&self, msg: u32) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, msg, WPARAM(0), LPARAM(0));
        }
    }
}

impl Drop for HookThread {
    fn drop(&mut self) {
        self.post(WM_QUIT);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        *SHARED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn wants_mouse_hook(cfg: &GestureConfig) -> bool {
    matches!(cfg.trigger.key, Some(vk::XBUTTON1 | vk::XBUTTON2))
}

fn run_hook_thread(
    cfg: GestureConfig,
    tx: UnboundedSender<Action>,
    ready: std_mpsc::Sender<anyhow::Result<u32>>,
) {
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut msg = MSG::default();
    // Creates this thread's message queue so PostThreadMessageW works right away.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, WM_APP, WM_APP, PM_NOREMOVE);
    }

    let mouse = wants_mouse_hook(&cfg);
    *SHARED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Shared {
        machine: GestureMachine::new(cfg),
        tx,
        epoch: Instant::now(),
        thread_id,
        suspects: Vec::new(),
        recorder: None,
    });

    let mut hooks = match Hooks::install(mouse) {
        Ok(h) => h,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(thread_id));

    unsafe {
        SetTimer(None, 0, TICK_MS, None);
    }
    let mut installed_at = Instant::now();

    loop {
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        match msg.message {
            WM_TIMER => {
                tick();
                let idle = with_shared(|s| !s.machine.is_active()).unwrap_or(true);
                if idle && installed_at.elapsed() >= REINSTALL_EVERY {
                    let mouse = with_shared(|s| s.wants_mouse_hook()).unwrap_or(false);
                    if let Ok(fresh) = Hooks::install(mouse) {
                        hooks = fresh;
                        installed_at = Instant::now();
                    }
                }
            }
            WM_JOONA_MASK => inject_mask(),
            WM_JOONA_REFRESH => {
                let mouse = with_shared(|s| s.wants_mouse_hook()).unwrap_or(false);
                if mouse != hooks.mouse.is_some()
                    && let Ok(fresh) = Hooks::install(mouse)
                {
                    hooks = fresh;
                    installed_at = Instant::now();
                }
            }
            _ => unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            },
        }
    }
    drop(hooks);
}

struct Hooks {
    keyboard: HHOOK,
    mouse: Option<HHOOK>,
}

impl Hooks {
    fn install(mouse: bool) -> anyhow::Result<Self> {
        let module: HINSTANCE = unsafe { GetModuleHandleW(None)?.into() };
        let keyboard =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module), 0)? };
        let mouse = if mouse {
            Some(unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module), 0)? })
        } else {
            None
        };
        Ok(Self { keyboard, mouse })
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.keyboard);
            if let Some(m) = self.mouse {
                let _ = UnhookWindowsHookEx(m);
            }
        }
    }
}

fn feed(vk_code: u16, down: bool) -> bool {
    with_shared(|s| {
        if let Some((recorder, tx)) = &mut s.recorder {
            if let Some(event) = recorder.on_key(vk_code, down) {
                let finished = !matches!(event, Recorded::Preview(_));
                let _ = tx.send(event);
                if finished {
                    s.recorder = None;
                    unsafe {
                        let _ =
                            PostThreadMessageW(s.thread_id, WM_JOONA_REFRESH, WPARAM(0), LPARAM(0));
                    }
                }
            }
            return true;
        }
        s.suspects.retain(|(k, _)| *k != vk_code);
        let time_ms = s.epoch.elapsed().as_millis() as u64;
        let out = s.machine.on_key(KeyEvent {
            vk: vk_code,
            down,
            time_ms,
        });
        for action in out.actions {
            if action == Action::InjectMask {
                unsafe {
                    let _ = PostThreadMessageW(s.thread_id, WM_JOONA_MASK, WPARAM(0), LPARAM(0));
                }
            } else {
                let _ = s.tx.send(action);
            }
        }
        out.swallow
    })
    .unwrap_or(false)
}

fn tick() {
    with_shared(|s| {
        let now = s.epoch.elapsed().as_millis() as u64;
        // A release can be missed, for example when UAC or the lock screen takes
        // over the desktop. Modifiers are never swallowed, so their real state is
        // reliable here.
        let mut stuck = Vec::new();
        let held: Vec<u16> = s.machine.held_keys().to_vec();
        s.suspects.retain(|(k, _)| held.contains(k));
        for k in held.into_iter().filter(|k| Modifier::of(*k).is_some()) {
            if is_physically_down(k) {
                s.suspects.retain(|(x, _)| *x != k);
                continue;
            }
            match s.suspects.iter_mut().find(|(x, _)| *x == k) {
                Some((_, n)) => {
                    *n += 1;
                    if *n >= STUCK_TICKS {
                        stuck.push(k);
                    }
                }
                None => s.suspects.push((k, 1)),
            }
        }
        s.suspects.retain(|(k, _)| !stuck.contains(k));
        let mut actions = Vec::new();
        for k in stuck {
            tracing::debug!(vk = k, "released a key whose key-up never arrived");
            let out = s.machine.on_key(KeyEvent {
                vk: k,
                down: false,
                time_ms: now,
            });
            actions.extend(out.actions);
        }
        actions.extend(s.machine.on_tick(now));
        for action in actions {
            if action != Action::InjectMask {
                let _ = s.tx.send(action);
            }
        }
    });
}

fn is_physically_down(vk_code: u16) -> bool {
    unsafe { (GetAsyncKeyState(vk_code as i32) as u16 & 0x8000) != 0 }
}

/// True once Ctrl, Win, Alt and Shift are all released, so a simulated Ctrl+V
/// isn't combined with keys the user is still holding.
pub fn modifiers_released() -> bool {
    [
        vk::LCONTROL,
        vk::RCONTROL,
        vk::LWIN,
        vk::RWIN,
        vk::LMENU,
        vk::RMENU,
        vk::LSHIFT,
        vk::RSHIFT,
    ]
    .iter()
    .all(|k| !is_physically_down(*k))
}

fn inject_mask() {
    let input = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk::MASK),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: JOONA_INPUT_MARKER,
            },
        },
    };
    let inputs = [input(KEYBD_EVENT_FLAGS(0)), input(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let ours = kb.dwExtraInfo == JOONA_INPUT_MARKER && (kb.flags.0 & LLKHF_INJECTED.0) != 0;
        // AltGr sends a fake left Ctrl with this scan code; it isn't a real Ctrl press.
        let altgr_ctrl = kb.vkCode == vk::LCONTROL as u32 && kb.scanCode == 0x21D;
        if !ours && !altgr_ctrl {
            let msg = wparam.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let swallow = feed(kb.vkCode as u16, down);
            tracing::trace!(
                vk = kb.vkCode,
                scan = kb.scanCode,
                flags = kb.flags.0,
                down,
                swallow,
                "key"
            );
            if swallow {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let msg = wparam.0 as u32;
        if msg == WM_XBUTTONDOWN || msg == WM_XBUTTONUP {
            let ms = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            if ms.flags & LLMHF_INJECTED == 0 {
                let button = ms.mouseData >> 16;
                let vk_code = if button == XBUTTON1 {
                    vk::XBUTTON1
                } else {
                    vk::XBUTTON2
                };
                if feed(vk_code, msg == WM_XBUTTONDOWN) {
                    return LRESULT(1);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
