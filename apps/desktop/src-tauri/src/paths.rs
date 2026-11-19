//! Where Joona keeps its files. Settings roam with the Windows profile;
//! history, models, runtimes and logs stay on this PC and can be large.
//!
//! `JOONA_HOME` puts everything under one folder, for tests and portable use.

use std::path::PathBuf;

fn home_override() -> Option<PathBuf> {
    std::env::var_os("JOONA_HOME").map(PathBuf::from)
}

#[cfg(windows)]
fn roaming_base() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".").join("AppData").join("Roaming"))
}

#[cfg(windows)]
fn local_base() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".").join("AppData").join("Local"))
}

#[cfg(not(windows))]
fn roaming_base() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(windows))]
fn local_base() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `%APPDATA%\Joona`: `settings.json`.
pub fn config_dir() -> PathBuf {
    home_override().unwrap_or_else(|| roaming_base().join("Joona"))
}

/// `%LOCALAPPDATA%\Joona`.
pub fn local_dir() -> PathBuf {
    home_override().unwrap_or_else(|| local_base().join("Joona"))
}

pub fn models_dir() -> PathBuf {
    local_dir().join("models")
}

/// Optional engine builds downloaded after install, such as the GPU engine.
pub fn runtimes_dir() -> PathBuf {
    local_dir().join("runtimes")
}

/// Joona's own AI model for Smart polish, and the server that runs it.
pub fn local_ai_dir() -> PathBuf {
    local_dir().join("local-ai")
}

pub fn logs_dir() -> PathBuf {
    local_dir().join("logs")
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

/// Dictation history. SQLite's journal files don't belong in a roaming profile.
pub fn database_file() -> PathBuf {
    local_dir().join("joona.db")
}

/// Recordings kept with history entries, when the user turned that on.
pub fn recordings_dir() -> PathBuf {
    local_dir().join("recordings")
}
