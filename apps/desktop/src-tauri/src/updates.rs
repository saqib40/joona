//! New versions from GitHub Releases, verified with the public key in
//! `tauri.conf.json`. Joona looks now and then, and downloads and installs
//! an update only when the user asks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_updater::{Error, Update, UpdaterExt};
use tauri_specta::Event;

use crate::state::AppState;

/// The first check waits for start-up to be over.
const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_secs(12 * 60 * 60);

/// Where updating is at, for the About page and the tray.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateStatus {
    /// Development builds don't update themselves.
    Unsupported,
    /// Nothing has been checked yet.
    Unknown,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
    },
    /// `percent` is missing when the server doesn't say how big the download is.
    Downloading {
        version: String,
        percent: Option<u8>,
    },
    /// The installer took over, and Joona is about to close.
    Installing {
        version: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct UpdateChanged(pub UpdateStatus);

pub struct Updates {
    status: Mutex<UpdateStatus>,
    /// The release the last check found.
    found: Mutex<Option<Update>>,
    checking: AtomicBool,
    installing: AtomicBool,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            status: Mutex::new(match cfg!(debug_assertions) {
                true => UpdateStatus::Unsupported,
                false => UpdateStatus::Unknown,
            }),
            found: Mutex::new(None),
            checking: AtomicBool::new(false),
            installing: AtomicBool::new(false),
        }
    }
}

struct ResetFlag<'a>(&'a AtomicBool);

impl Drop for ResetFlag<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Updates {
    pub fn status(&self) -> UpdateStatus {
        self.lock_status().clone()
    }

    /// The version waiting to be installed.
    pub fn available(&self) -> Option<String> {
        match &*self.lock_status() {
            UpdateStatus::Available { version, .. } => Some(version.clone()),
            _ => None,
        }
    }

    fn lock_status(&self) -> MutexGuard<'_, UpdateStatus> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_found(&self) -> MutexGuard<'_, Option<Update>> {
        self.found.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Checks now and then, while the setting allows it.
pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    if state.updates.status() == UpdateStatus::Unsupported {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if state.settings.get().general.check_updates {
                check(&app, &state, true).await;
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

/// Asks GitHub for a newer release. Quiet checks keep failures to the log,
/// since being offline now and then is normal.
pub async fn check(app: &AppHandle, state: &AppState, quiet: bool) -> UpdateStatus {
    let updates = &state.updates;
    let before = updates.status();
    if matches!(
        before,
        UpdateStatus::Unsupported
            | UpdateStatus::Downloading { .. }
            | UpdateStatus::Installing { .. }
    ) || updates.installing.load(Ordering::SeqCst)
        || updates.checking.swap(true, Ordering::SeqCst)
    {
        return before;
    }
    let _checking_guard = ResetFlag(&updates.checking);
    if !quiet {
        set(app, updates, UpdateStatus::Checking);
    }
    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    if updates.installing.load(Ordering::SeqCst)
        || matches!(
            updates.status(),
            UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. }
        )
    {
        return updates.status();
    }
    let status = match found {
        Ok(Some(update)) => {
            tracing::info!(version = %update.version, "an update is available");
            let status = UpdateStatus::Available {
                version: update.version.clone(),
                notes: update.body.clone().filter(|notes| !notes.trim().is_empty()),
            };
            *updates.lock_found() = Some(update);
            status
        }
        Ok(None) => {
            *updates.lock_found() = None;
            UpdateStatus::UpToDate
        }
        Err(e) => {
            tracing::warn!("couldn't check for updates: {e}");
            match quiet {
                true => before,
                false => UpdateStatus::Failed {
                    message: failure(&e),
                },
            }
        }
    };
    set(app, updates, status.clone());
    status
}

/// Downloads the update the last check found, then hands over to its
/// installer, which closes Joona and opens it again when it's done.
pub async fn install(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let updates = &state.updates;
    if updates.installing.swap(true, Ordering::SeqCst) {
        return Err("An update is already being installed.".into());
    }
    let _installing_guard = ResetFlag(&updates.installing);
    let Some(update) = updates.lock_found().clone() else {
        return Err("There's no update to install. Check for updates again.".into());
    };
    let version = update.version.clone();
    let downloading = |percent| UpdateStatus::Downloading {
        version: version.clone(),
        percent,
    };
    set(app, updates, downloading(Some(0)));
    let mut received = 0;
    let mut shown = Some(0);
    let downloaded = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                let now = percent(received, total);
                if now != shown {
                    shown = now;
                    set(app, updates, downloading(now));
                }
            },
            || {},
        )
        .await;
    let fail = |e: Error, what: &str| {
        tracing::warn!("couldn't {what} the update: {e}");
        let message = failure(&e);
        set(
            app,
            updates,
            UpdateStatus::Failed {
                message: message.clone(),
            },
        );
        message
    };
    let bytes = downloaded.map_err(|e| fail(e, "download"))?;

    tracing::info!(%version, "installing the update");
    set(app, updates, UpdateStatus::Installing { version });
    // The installer replaces the engines, so they can't be running.
    state.engine.stop().await;
    state.local_ai.stop().await;
    // Starts the installer and closes Joona.
    update.install(bytes).map_err(|e| fail(e, "install"))
}

fn set(app: &AppHandle, updates: &Updates, status: UpdateStatus) {
    let was_available = {
        let mut current = updates.lock_status();
        let was = matches!(*current, UpdateStatus::Available { .. });
        *current = status.clone();
        was
    };
    let available = matches!(status, UpdateStatus::Available { .. });
    let _ = UpdateChanged(status).emit_to(app, "hub");
    if was_available != available {
        crate::tray::refresh_menu(app);
    }
}

fn percent(received: u64, total: Option<u64>) -> Option<u8> {
    let total = total.filter(|&total| total > 0)?;
    Some((received.saturating_mul(100) / total).min(100) as u8)
}

fn failure(error: &Error) -> String {
    match error {
        Error::Reqwest(_) | Error::Network(_) => {
            "Can't reach GitHub. Check your internet connection.".into()
        }
        Error::ReleaseNotFound | Error::TargetNotFound(_) | Error::TargetsNotFound(_) => {
            "GitHub doesn't have an update for this PC yet.".into()
        }
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => {
            "The update isn't signed by Joona's developers, so it wasn't installed.".into()
        }
        other => format!("Something went wrong: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_needs_a_size() {
        assert_eq!(percent(50, Some(200)), Some(25));
        assert_eq!(percent(300, Some(200)), Some(100));
        assert_eq!(percent(50, None), None);
        assert_eq!(percent(50, Some(0)), None);
    }

    #[test]
    fn failures_say_what_to_do() {
        assert!(failure(&Error::Network("reset".into())).contains("internet connection"));
        assert!(failure(&Error::MissingSignedVersion).contains("isn't signed"));
        assert!(failure(&Error::EmptyEndpoints).starts_with("Something went wrong"));
    }

    #[test]
    fn development_builds_dont_update() {
        let updates = Updates::default();
        assert_eq!(
            updates.status() == UpdateStatus::Unsupported,
            cfg!(debug_assertions)
        );
        assert_eq!(updates.available(), None);
    }
}
