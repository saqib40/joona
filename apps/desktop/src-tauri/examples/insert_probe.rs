//! Opens a window with a text box, inserts text into it by pasting and by
//! typing, and checks the text arrived and the clipboard was restored.
//!
//! cargo run --example insert_probe

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    probe::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("insert_probe only runs on Windows");
}

#[cfg(windows)]
mod probe {
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::{Context, bail};
    use joona_lib::insert::{InsertSettings, Inserter, Method, foreground};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        SetFocus, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, ES_AUTOVSCROLL,
        ES_MULTILINE, GetMessageW, GetQueueStatus, GetWindowTextLengthW, GetWindowTextW, HMENU,
        MSG, PostMessageW, PostQuitMessage, QS_INPUT, RegisterClassW, SW_SHOW, SetForegroundWindow,
        SetWindowTextW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
        WM_DESTROY, WNDCLASSW, WS_BORDER, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };
    use windows::core::{HSTRING, w};

    const WM_CHECK: u32 = WM_APP + 10;
    const TEXT: &str = "Hello from Joona, नमस्ते 👍\nsecond line";
    const ORIGINAL: &str = "joona-probe original clipboard";

    static mut EDIT: isize = 0;

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn edit() -> HWND {
        HWND(unsafe { EDIT } as *mut _)
    }

    fn edit_text() -> String {
        unsafe {
            let len = GetWindowTextLengthW(edit());
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(edit(), &mut buf);
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }

    /// Simulated input lets this process take the foreground.
    fn bring_to_front(hwnd: HWND) {
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0xE8),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(
                &[key(KEYBD_EVENT_FLAGS(0))],
                std::mem::size_of::<INPUT>() as i32,
            );
            let _ = SetForegroundWindow(hwnd);
            SendInput(&[key(KEYEVENTF_KEYUP)], std::mem::size_of::<INPUT>() as i32);
            let _ = SetFocus(Some(edit()));
        }
    }

    pub fn run() -> anyhow::Result<()> {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "joona_lib=debug".into()),
            )
            .init();

        if foreground().is_some_and(|app| app.exe == "lockapp.exe") {
            bail!(
                "Windows is locked. Unlock it and run the probe again: a locked session blocks the clipboard and simulated input"
            );
        }
        let inserter = Arc::new(Inserter::new()?);
        inserter.copy(ORIGINAL)?;

        let hwnd = unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("JoonaInsertProbe");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance,
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Joona insert probe"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                640,
                360,
                None,
                None,
                Some(instance),
                None,
            )?;
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                None,
                WS_CHILD
                    | WS_VISIBLE
                    | WS_BORDER
                    | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL) as u32),
                10,
                10,
                600,
                280,
                Some(hwnd),
                Some(HMENU(1 as _)),
                Some(instance),
                None,
            )?;
            EDIT = edit.0 as isize;
            let _ = ShowWindow(hwnd, SW_SHOW);
            hwnd
        };
        bring_to_front(hwnd);

        let window = hwnd.0 as isize;
        let worker = {
            let inserter = inserter.clone();
            std::thread::spawn(move || -> anyhow::Result<()> {
                let hwnd = HWND(window as *mut _);
                std::thread::sleep(Duration::from_millis(400));
                let app = foreground();
                println!("foreground: {app:?}");
                let settings = InsertSettings::default();
                for method in [Method::Paste, Method::Type] {
                    let mut options = settings.options_for(app.as_ref().map(|a| a.exe.as_str()));
                    options.method = method;
                    let started = std::time::Instant::now();
                    let outcome = inserter.insert(TEXT, &options)?;
                    println!(
                        "{method:?}: {outcome:?} in {} ms",
                        started.elapsed().as_millis()
                    );
                    // Simulated keys reach the window's queue a moment after SendInput returns.
                    std::thread::sleep(Duration::from_millis(150));
                    unsafe {
                        PostMessageW(Some(hwnd), WM_CHECK, WPARAM(method as usize), LPARAM(0))?
                    };
                    std::thread::sleep(Duration::from_millis(300));
                    println!(
                        "clipboard after {method:?}: {:?}",
                        inserter.clipboard_text()?
                    );
                }
                unsafe { PostMessageW(Some(hwnd), WM_CHECK, WPARAM(99), LPARAM(0))? };
                Ok(())
            })
        };

        let mut results = Vec::new();
        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_CHECK {
                    // Posted messages jump ahead of keyboard input, so wait for the typing to drain.
                    if (GetQueueStatus(QS_INPUT) >> 16) & QS_INPUT.0 != 0 {
                        PostMessageW(Some(hwnd), WM_CHECK, msg.wParam, msg.lParam)?;
                        continue;
                    }
                    if msg.wParam.0 == 99 {
                        windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd)?;
                        continue;
                    }
                    results.push(edit_text());
                    SetWindowTextW(edit(), &HSTRING::new())?;
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        worker.join().ok().context("the insert thread panicked")??;

        let expected = TEXT.replace('\n', "\r\n");
        let mut failed = false;
        for (method, got) in ["paste", "type"].iter().zip(&results) {
            let ok = *got == expected;
            failed |= !ok;
            println!("{method}: {} {got:?}", if ok { "OK" } else { "MISMATCH" });
        }
        if results.len() != 2 || failed {
            bail!("insertion didn't produce the expected text");
        }
        Ok(())
    }
}
