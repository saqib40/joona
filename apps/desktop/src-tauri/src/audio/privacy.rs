//! Whether Windows lets desktop apps use the microphone. When it doesn't, the
//! microphone still opens but delivers only silence, so Joona has to ask.

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MicAccess {
    Allowed,
    /// "Microphone access" is off for everyone on this PC. Only an
    /// administrator can turn it on.
    BlockedForDevice,
    /// "Microphone access" is off for this Windows account.
    BlockedForUser,
    /// "Let desktop apps access your microphone" is off.
    BlockedForDesktopApps,
}

impl MicAccess {
    pub fn blocked(self) -> bool {
        self != Self::Allowed
    }
}

/// The page of Windows Settings with the switches above.
pub const PRIVACY_SETTINGS_URL: &str = "ms-settings:privacy-microphone";

/// Each switch is stored as `Allow` or `Deny`. A missing value is the
/// Windows default, which allows.
fn decide(device: Option<&str>, user: Option<&str>, desktop_apps: Option<&str>) -> MicAccess {
    let denied = |value: Option<&str>| value.is_some_and(|v| v.trim().eq_ignore_ascii_case("deny"));
    if denied(device) {
        MicAccess::BlockedForDevice
    } else if denied(user) {
        MicAccess::BlockedForUser
    } else if denied(desktop_apps) {
        MicAccess::BlockedForDesktopApps
    } else {
        MicAccess::Allowed
    }
}

#[cfg(windows)]
pub fn mic_access() -> MicAccess {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use windows::core::w;

    let consent = w!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone"
    );
    let desktop_apps = w!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone\\NonPackaged"
    );
    decide(
        read(HKEY_LOCAL_MACHINE, consent).as_deref(),
        read(HKEY_CURRENT_USER, consent).as_deref(),
        read(HKEY_CURRENT_USER, desktop_apps).as_deref(),
    )
}

#[cfg(not(windows))]
pub fn mic_access() -> MicAccess {
    MicAccess::Allowed
}

/// The `Value` string under `key`, if there is one.
#[cfg(windows)]
fn read(
    root: windows::Win32::System::Registry::HKEY,
    key: windows::core::PCWSTR,
) -> Option<String> {
    use windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::w;

    let mut buf = [0u16; 32];
    let mut len = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `len` holds the buffer's size in bytes, and both outlive the call.
    unsafe {
        RegGetValueW(
            root,
            key,
            w!("Value"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    }
    .ok()
    .ok()?;
    let chars = &buf[..(len as usize / 2).min(buf.len())];
    let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    Some(String::from_utf16_lossy(&chars[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_switch_that_is_off_wins() {
        assert_eq!(decide(None, None, None), MicAccess::Allowed);
        assert_eq!(
            decide(Some("Allow"), Some("Allow"), Some("Allow")),
            MicAccess::Allowed
        );
        assert_eq!(
            decide(Some("Deny"), Some("Deny"), None),
            MicAccess::BlockedForDevice
        );
        assert_eq!(
            decide(Some("Allow"), Some("Deny"), Some("Deny")),
            MicAccess::BlockedForUser
        );
        assert_eq!(
            decide(None, Some("Allow"), Some("deny")),
            MicAccess::BlockedForDesktopApps
        );
    }

    #[test]
    #[cfg(windows)]
    fn reads_the_registry() {
        // Whatever this PC's switches are, reading them must not fail or panic.
        let _ = mic_access();
    }
}
