//! The Hub window's look (Mica on Windows 11, the theme) and the facts its
//! greeting and About page show.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::WebviewWindow;
use tauri::window::{Effect, EffectState, EffectsBuilder};

use crate::settings::Theme;

static MICA: AtomicBool = AtomicBool::new(false);

/// The Windows build number, for example 26100.
pub fn windows_build() -> Option<u32> {
    if !cfg!(windows) {
        return None;
    }
    sysinfo::System::kernel_version().ok()?.trim().parse().ok()
}

/// For example `Windows 11 (build 26100)`.
pub fn os_description() -> String {
    match windows_build() {
        Some(build) if build >= 22_000 => format!("Windows 11 (build {build})"),
        Some(build) => format!("Windows 10 (build {build})"),
        None => sysinfo::System::long_os_version().unwrap_or_else(|_| std::env::consts::OS.into()),
    }
}

/// Whether the Hub currently has the Mica backdrop.
pub fn mica() -> bool {
    MICA.load(Ordering::Relaxed)
}

/// Applies the theme to the title bar and, on Windows 11, the Mica backdrop
/// in the same theme.
pub fn style(window: &WebviewWindow, theme: Theme) {
    let _ = window.set_theme(match theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    });
    if !windows_build().is_some_and(|b| b >= 22_000) {
        return;
    }
    let effect = match theme {
        Theme::System => Effect::Mica,
        Theme::Light => Effect::MicaLight,
        Theme::Dark => Effect::MicaDark,
    };
    let applied = window
        .set_effects(
            EffectsBuilder::new()
                .effect(effect)
                .state(EffectState::FollowsWindowActiveState)
                .build(),
        )
        .inspect_err(|e| tracing::warn!("couldn't apply Mica: {e}"))
        .is_ok();
    MICA.store(applied, Ordering::Relaxed);
}

/// The user's first name from their Windows account, for the greeting.
pub fn first_name() -> Option<String> {
    display_name().as_deref().and_then(first_name_of)
}

#[cfg(windows)]
fn display_name() -> Option<String> {
    use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
    use windows::core::PWSTR;

    let mut len = 256u32;
    let mut buf = vec![0u16; len as usize];
    let ok = unsafe { GetUserNameExW(NameDisplay, Some(PWSTR(buf.as_mut_ptr())), &mut len) };
    if !ok {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

#[cfg(target_os = "macos")]
fn display_name() -> Option<String> {
    let name = objc2_foundation::NSFullUserName().to_string();
    if name.trim().is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn display_name() -> Option<String> {
    None
}

/// `Doe, Jane` and `Jane Doe` both give `Jane`. Account ids like `123456` give nothing.
fn first_name_of(display: &str) -> Option<String> {
    let display = display.trim();
    let first = match display.split_once(',') {
        Some((_, rest)) => rest.split_whitespace().next(),
        None => display.split_whitespace().next(),
    }?;
    let letters = first.chars().filter(|c| c.is_alphabetic()).count();
    (letters * 2 >= first.chars().count() && letters > 0).then(|| first.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_names() {
        assert_eq!(first_name_of("Jane Doe").as_deref(), Some("Jane"));
        assert_eq!(first_name_of("Doe, Jane Q").as_deref(), Some("Jane"));
        assert_eq!(first_name_of("प्रिया शर्मा").as_deref(), Some("प्रिया"));
        assert_eq!(first_name_of("123456"), None);
        assert_eq!(first_name_of("  "), None);
    }
}
