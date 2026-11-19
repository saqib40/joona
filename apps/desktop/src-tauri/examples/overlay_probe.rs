//! Reports where the Joona Bar is and which app has the keyboard, to check
//! that starting Joona and showing the bar never take focus from the user.
//!
//! Start Joona, then: cargo run --example overlay_probe

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    probe::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("overlay_probe only runs on Windows");
}

#[cfg(windows)]
mod probe {
    use anyhow::bail;
    use joona_lib::insert::foreground;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, IsWindowVisible,
        WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT,
    };
    use windows::core::HSTRING;

    pub fn run() -> anyhow::Result<()> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let app = foreground();
        println!("foreground: {app:?}");

        for title in ["Joona Bar", "Joona"] {
            let Ok(hwnd) = (unsafe { FindWindowW(None, &HSTRING::from(title)) }) else {
                println!("{title}: no window");
                continue;
            };
            let mut r = RECT::default();
            let (visible, style) = unsafe {
                let _ = GetWindowRect(hwnd, &mut r);
                (
                    IsWindowVisible(hwnd).as_bool(),
                    GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32,
                )
            };
            let flags: Vec<&str> = [
                (WS_EX_NOACTIVATE.0, "no-activate"),
                (WS_EX_TOOLWINDOW.0, "tool-window"),
                (WS_EX_APPWINDOW.0, "app-window"),
                (WS_EX_TOPMOST.0, "topmost"),
                (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0, "click-through"),
            ]
            .into_iter()
            .filter(|(flag, _)| style & flag == *flag)
            .map(|(_, name)| name)
            .collect();
            println!(
                "{title}: visible={visible} at ({}, {}) size {}x{} [{}]",
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                flags.join(", ")
            );
        }

        if app.is_some_and(|a| a.exe == "joona.exe") {
            bail!("Joona has the foreground, so keystrokes meant for another app go to Joona");
        }
        Ok(())
    }
}
