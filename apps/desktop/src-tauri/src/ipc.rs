//! Commands and events between Rust and the two webviews. `tauri-specta`
//! turns these into `src/lib/bindings.ts`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::ai::{self, LocalAi};
use crate::audio::{self, InputDevice, MicAccess, list_input_devices};
use crate::history::{self, Entry, ExportFormat, History, HomeStats, Insights, RecentApp};
use crate::hotkeys::{Mode, Recorded, Shortcut};
use crate::languages::{self, Language};
use crate::models::catalog::catalog;
use crate::models::download::{Listener, Progress};
use crate::models::overview::{self, EngineState, ModelsOverview};
use crate::overlay::{Overlay, OverlayEvents, Region};
use crate::pipeline::style::{AppRule, Category, CategoryStyle, Tone};
use crate::providers::llm::{self, KeyNeed, Llm};
use crate::providers::{self, ProviderError, SARVAM, stt};
use crate::router::{RoutePreview, Translators};
use crate::session::{BarState, Command, Toast, ToastAction, Ui};
use crate::settings::{PolishLevel, Settings, Theme};
use crate::sounds::{self, Cue};
use crate::state::AppState;
use crate::store::{Db, DictionaryEntry, Imported, Snippet};
use crate::supervisor::Variant;
use crate::updates::{self, UpdateChanged, UpdateStatus};
use crate::{hub, paths, router};

const SNOOZE: Duration = Duration::from_secs(60 * 60);
/// Installed next to Joona by release builds.
const LICENSES_FILE: &str = "THIRD_PARTY_LICENSES.html";
const NOTICES_URL: &str = "https://github.com/saqib40/joona/blob/main/THIRD_PARTY_NOTICES.md";
/// Recording a shortcut gives up after this long without one.
const RECORD_TIMEOUT: Duration = Duration::from_secs(15);
/// The microphone test stops on its own after this long.
const MIC_TEST_LIMIT: Duration = Duration::from_secs(60);
/// Time for the app the user was in to come back to the front after the Hub
/// is minimized, before pasting into it.
const REFOCUS_DELAY: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct BarStateChanged(pub BarState);

/// Microphone level from 0 to 1, about 30 times a second while listening.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct MicLevel(pub f32);

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ToastShown(pub Toast);

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ToastDismissed(pub u32);

/// The cursor entered or left the interactive parts of the bar.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct BarHover(pub bool);

/// Whether the resting pill shows (it hides over full-screen apps, when
/// snoozed, or when the user turned it off).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct BarResting(pub bool);

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct BarConfigChanged(pub BarConfig);

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct SettingsChanged(pub Settings);

/// Asks the Hub to show a page, such as `models`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct HubNavigate(pub String);

/// A model download moved on, several times a second.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ModelDownload(pub Progress);

/// A model was installed or removed, or the engine started or stopped.
/// The Models page fetches `modelsOverview` again.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ModelsChanged;

/// A dictation was saved to the history.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct HistoryAdded(pub Entry);

/// Entries were deleted, for example by the retention setting.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct HistoryChanged;

/// Microphone level from the test in Settings, from 0 to 1.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct MicTestLevel(pub f32);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Recording {
    /// The keys held so far.
    Preview {
        shortcut: Shortcut,
        label: String,
    },
    /// Everything was released. `trigger` says whether it can start dictation.
    Done {
        shortcut: Shortcut,
        label: String,
        trigger: bool,
    },
    Cancelled,
}

/// Progress of recording a shortcut in Settings.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ShortcutRecording(pub Recording);

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// For example `Windows 11 (build 26100)`.
    pub os: String,
    /// The Hub has the Mica backdrop.
    pub mica: bool,
    /// From the Windows account, for the greeting.
    pub first_name: Option<String>,
    pub settings_file: String,
    pub data_dir: String,
    pub models_dir: String,
    pub logs_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LanguageOption {
    pub code: String,
    pub name: String,
    pub native: String,
}

/// What the bar needs from the settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BarConfig {
    /// For example `Ctrl+Win`.
    pub shortcut: String,
    pub languages: Vec<LanguageOption>,
    /// The language picked on the bar. `None` detects among `languages`.
    pub current: Option<String>,
    pub output: Option<String>,
    /// For example `English`.
    pub output_name: Option<String>,
    /// For example `HI → EN`, shown next to the language picker.
    pub route: String,
    pub paused: bool,
    pub theme: Theme,
}

impl BarConfig {
    pub fn from_settings(s: &Settings) -> Self {
        let languages: Vec<LanguageOption> = s
            .languages
            .speak
            .iter()
            .map(|l| {
                let known = languages::find(&l.code);
                LanguageOption {
                    code: l.code.clone(),
                    name: known.map_or_else(|| l.code.to_uppercase(), |k| k.name.to_owned()),
                    native: known.map_or_else(|| l.code.to_uppercase(), |k| k.native.to_owned()),
                }
            })
            .collect();
        Self {
            shortcut: s.shortcuts.trigger.label(),
            languages,
            current: s.languages.current.clone(),
            output: s.languages.output.clone(),
            output_name: s.languages.output.as_deref().map(languages::name),
            route: router::label(s, Mode::Dictate),
            paused: s.general.paused,
            theme: s.general.theme,
        }
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BarSnapshot {
    pub state: BarState,
    pub resting: bool,
    pub snoozed: bool,
    pub config: BarConfig,
}

pub fn specta() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            bar_snapshot,
            bar_set_regions,
            bar_start,
            bar_stop,
            bar_cancel,
            bar_snooze,
            toast_action,
            open_hub,
            set_language,
            set_paused,
            get_settings,
            set_settings,
            list_microphones,
            models_overview,
            model_download,
            model_cancel,
            model_delete,
            model_use,
            open_models_folder,
            app_info,
            open_folder,
            open_licenses,
            update_status,
            update_check,
            update_install,
            history_list,
            history_delete,
            history_clear,
            history_paste,
            history_export,
            home_stats,
            insights,
            dictionary_list,
            dictionary_add,
            dictionary_update,
            dictionary_star,
            dictionary_delete,
            dictionary_import,
            dictionary_export,
            snippets_list,
            snippet_add,
            snippet_update,
            snippet_delete,
            style_overview,
            style_set_tone,
            style_set_app,
            shortcut_record,
            shortcut_record_cancel,
            mic_test_start,
            mic_test_stop,
            mic_access,
            open_mic_privacy,
            languages,
            language_routes,
            provider_keys,
            provider_key_set,
            provider_key_delete,
            provider_test,
            stt_overview,
            ai_overview,
            ai_models,
            ai_test,
            local_ai_download,
            local_ai_cancel,
            local_ai_delete,
        ])
        .events(tauri_specta::collect_events![
            BarStateChanged,
            MicLevel,
            ToastShown,
            ToastDismissed,
            BarHover,
            BarResting,
            BarConfigChanged,
            SettingsChanged,
            HubNavigate,
            ModelDownload,
            ModelsChanged,
            HistoryAdded,
            HistoryChanged,
            MicTestLevel,
            ShortcutRecording,
            AiChanged,
            UpdateChanged,
        ])
        .dangerously_cast_bigints_to_number()
}

#[tauri::command]
#[specta::specta]
fn bar_snapshot(state: State<'_, Arc<AppState>>, overlay: State<'_, Arc<Overlay>>) -> BarSnapshot {
    BarSnapshot {
        state: state.bar.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        resting: overlay.resting_visible(),
        snoozed: overlay.snoozed(),
        config: BarConfig::from_settings(&state.settings.get()),
    }
}

/// The parts of the bar that take the mouse, in CSS pixels.
#[tauri::command]
#[specta::specta]
fn bar_set_regions(regions: Vec<Region>, overlay: State<'_, Arc<Overlay>>) {
    overlay.set_regions(regions);
}

/// A click on the resting bar starts hands-free dictation.
#[tauri::command]
#[specta::specta]
fn bar_start(state: State<'_, Arc<AppState>>) {
    state.session.send(Command::StartHandsFree);
}

#[tauri::command]
#[specta::specta]
fn bar_stop(state: State<'_, Arc<AppState>>) {
    state.session.send(Command::Stop);
}

#[tauri::command]
#[specta::specta]
fn bar_cancel(state: State<'_, Arc<AppState>>) {
    state.session.send(Command::Cancel);
}

/// "Hide for 1 hour" from the bar's menu. `false` shows it again.
#[tauri::command]
#[specta::specta]
fn bar_snooze(snooze: bool, overlay: State<'_, Arc<Overlay>>) {
    match snooze {
        true => overlay.snooze(SNOOZE),
        false => overlay.unsnooze(),
    }
}

#[tauri::command]
#[specta::specta]
fn toast_action(toast: u32, action: ToastAction, state: State<'_, Arc<AppState>>) {
    state.session.send(Command::ToastAction { toast, action });
}

#[tauri::command]
#[specta::specta]
fn open_hub(page: Option<String>, app: AppHandle) {
    crate::show_hub(&app, page.as_deref());
}

/// Picks the spoken language from the bar or tray. `None` detects it.
#[tauri::command]
#[specta::specta]
fn set_language(code: Option<String>, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .settings
        .update(|s| s.languages.current = code)
        .map(|_| ())
        .map_err(|e| format!("{e:#}"))
}

/// Pauses or resumes the shortcuts, as in the tray menu.
#[tauri::command]
#[specta::specta]
fn set_paused(paused: bool, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .settings
        .update(|s| s.general.paused = paused)
        .map(|_| ())
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
fn get_settings(state: State<'_, Arc<AppState>>) -> Settings {
    state.settings.get()
}

#[tauri::command]
#[specta::specta]
fn set_settings(settings: Settings, state: State<'_, Arc<AppState>>) -> Result<Settings, String> {
    state.settings.set(settings).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
async fn list_microphones() -> Vec<InputDevice> {
    tauri::async_runtime::spawn_blocking(list_input_devices)
        .await
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
async fn models_overview(state: State<'_, Arc<AppState>>) -> Result<ModelsOverview, String> {
    let settings = state.settings.get();
    let installed = state.library.installed();
    let hardware = state.hardware().clone();
    let variant = state.engine.variant();
    let gpu = variant == Variant::Vulkan;
    let (models, recommended) = overview::cards(catalog(), &installed, &hardware, &settings, gpu);
    let info = state.engine.info().await;
    let status = state.engine.status_if_running().await;
    Ok(ModelsOverview {
        models,
        recommended,
        current: router::default_model(&settings, &state.library).map(|m| m.id),
        downloads: state.downloads.progress(),
        hardware,
        engine: EngineState {
            running: info.is_some(),
            variant,
            gpu_build: Variant::Vulkan.is_installed(),
            gpu_name: info
                .filter(|_| variant == Variant::Vulkan)
                .and_then(|i| i.gpus.into_iter().next())
                .map(|g| g.name),
            loaded: status
                .map(|s| s.loaded.into_iter().map(|m| m.id).collect())
                .unwrap_or_default(),
            vad: state.library.vad().is_some(),
        },
    })
}

/// Installs a model from the catalog. Progress arrives as `ModelDownload` events.
#[tauri::command]
#[specta::specta]
fn model_download(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.downloads.start(&id).map_err(|e| format!("{e:#}"))
}

/// Stops a download. Downloading again picks up where it stopped.
#[tauri::command]
#[specta::specta]
fn model_cancel(id: String, state: State<'_, Arc<AppState>>) {
    state.downloads.cancel(&id);
}

/// Frees a model from memory and deletes its files.
#[tauri::command]
#[specta::specta]
async fn model_delete(
    id: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Windows can't delete files the engine still has open.
    if let Err(e) = state.engine.unload(Some(id.clone())).await {
        tracing::debug!(model = %id, "couldn't unload before deleting: {e}");
    }
    state.library.delete(&id).map_err(|e| format!("{e:#}"))?;
    let models = state.settings.get().models;
    if models.active.as_deref() == Some(id.as_str())
        || models.english.as_deref() == Some(id.as_str())
    {
        state
            .settings
            .update(|s| {
                s.models.active.take_if(|m| *m == id);
                s.models.english.take_if(|m| *m == id);
            })
            .map_err(|e| format!("{e:#}"))?;
    }
    tracing::info!(model = %id, "deleted a model");
    let _ = ModelsChanged.emit_to(&app, "hub");
    crate::preload_default(state.inner().clone());
    Ok(())
}

/// Picks the model for dictation, or with `english`, the one for English-only
/// dictation. `None` lets Joona choose among the installed models.
#[tauri::command]
#[specta::specta]
fn model_use(
    id: Option<String>,
    english: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if let Some(id) = &id
        && state.library.get(id).is_none()
    {
        return Err(format!("{id} isn't installed"));
    }
    state
        .settings
        .update(|s| match english {
            true => s.models.english = id,
            false => s.models.active = id,
        })
        .map(|_| ())
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
fn open_models_folder(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let root = state.library.root();
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
async fn app_info() -> AppInfo {
    // On a domain PC, the display name can take a round trip to the domain controller.
    let first_name = tauri::async_runtime::spawn_blocking(hub::first_name)
        .await
        .ok()
        .flatten();
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        os: hub::os_description(),
        mica: hub::mica(),
        first_name,
        settings_file: paths::settings_file().to_string_lossy().into_owned(),
        data_dir: paths::local_dir().to_string_lossy().into_owned(),
        models_dir: paths::models_dir().to_string_lossy().into_owned(),
        logs_dir: paths::logs_dir().to_string_lossy().into_owned(),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Folder {
    Data,
    Models,
    Logs,
}

#[tauri::command]
#[specta::specta]
fn open_folder(folder: Folder, app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let path = match folder {
        Folder::Data => paths::local_dir(),
        Folder::Models => paths::models_dir(),
        Folder::Logs => paths::logs_dir(),
    };
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Opens the licences of everything Joona is built with: the copy installed
/// with it, or the notices on GitHub when there isn't one.
#[tauri::command]
#[specta::specta]
fn open_licenses(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let installed = app
        .path()
        .resource_dir()
        .map(|dir| dir.join(LICENSES_FILE))
        .ok()
        .filter(|path| path.is_file());
    match installed {
        Some(path) => app.opener().open_path(path.to_string_lossy(), None::<&str>),
        None => app.opener().open_url(NOTICES_URL, None::<&str>),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
fn update_status(state: State<'_, Arc<AppState>>) -> UpdateStatus {
    state.updates.status()
}

#[tauri::command]
#[specta::specta]
async fn update_check(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<UpdateStatus, String> {
    Ok(updates::check(&app, &state, false).await)
}

/// Downloads and installs the update found by the last check. Joona closes
/// and opens again once it's installed.
#[tauri::command]
#[specta::specta]
async fn update_install(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    updates::install(&app, &state).await
}

fn history_of(state: &AppState) -> Result<Arc<History>, String> {
    state
        .history
        .clone()
        .ok_or_else(|| "History isn't available: Joona couldn't open its database.".into())
}

/// Runs database work off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

/// Newest first. `before` is the `createdAt` of the last entry already shown.
#[tauri::command]
#[specta::specta]
async fn history_list(
    search: String,
    before: Option<i64>,
    limit: u32,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<Entry>, String> {
    let history = history_of(&state)?;
    blocking(move || history.list(&search, before, limit.min(500))).await
}

#[tauri::command]
#[specta::specta]
async fn history_delete(
    id: i64,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let history = history_of(&state)?;
    blocking(move || history.delete(id)).await?;
    let _ = HistoryChanged.emit_to(&app, "hub");
    Ok(())
}

#[tauri::command]
#[specta::specta]
async fn history_clear(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let history = history_of(&state)?;
    blocking(move || history.clear()).await?;
    tracing::info!("cleared the history");
    let _ = HistoryChanged.emit_to(&app, "hub");
    Ok(())
}

/// Pastes an earlier dictation into the app the user was in before the Hub.
#[tauri::command]
#[specta::specta]
async fn history_paste(
    id: i64,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let history = history_of(&state)?;
    let entry = blocking(move || history.get(id))
        .await?
        .ok_or("That dictation was deleted.")?;
    if let Some(hub) = app.get_webview_window("hub") {
        let _ = hub.minimize();
    }
    tokio::time::sleep(REFOCUS_DELAY).await;
    state.session.send(Command::Paste(entry.text));
    Ok(())
}

/// Writes the whole history to `path`. Returns how many entries it wrote.
#[tauri::command]
#[specta::specta]
async fn history_export(
    path: String,
    format: ExportFormat,
    state: State<'_, Arc<AppState>>,
) -> Result<u32, String> {
    let history = history_of(&state)?;
    let count = blocking(move || history.export(std::path::Path::new(&path), format)).await?;
    Ok(count as u32)
}

#[tauri::command]
#[specta::specta]
async fn home_stats(state: State<'_, Arc<AppState>>) -> Result<HomeStats, String> {
    let history = history_of(&state)?;
    blocking(move || history.home_stats(history::now_ms())).await
}

#[tauri::command]
#[specta::specta]
async fn insights(state: State<'_, Arc<AppState>>) -> Result<Insights, String> {
    let history = history_of(&state)?;
    blocking(move || history.insights(history::now_ms())).await
}

fn db_of(state: &AppState) -> Result<Arc<Db>, String> {
    state
        .db
        .clone()
        .ok_or_else(|| "Joona couldn't open its database.".into())
}

/// Changes the dictionary, snippets or styles, then reads them again so the
/// next dictation uses the change.
async fn edit<T: Send + 'static>(
    state: &Arc<AppState>,
    work: impl FnOnce(&Db) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, String> {
    let db = db_of(state)?;
    let state = state.clone();
    blocking(move || {
        let out = work(&db)?;
        state.reload_rules()?;
        Ok(out)
    })
    .await
}

/// Starred first, then the most used.
#[tauri::command]
#[specta::specta]
async fn dictionary_list(state: State<'_, Arc<AppState>>) -> Result<Vec<DictionaryEntry>, String> {
    let db = db_of(&state)?;
    blocking(move || db.dictionary()).await
}

#[tauri::command]
#[specta::specta]
async fn dictionary_add(
    word: String,
    heard: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<DictionaryEntry, String> {
    edit(&state, move |db| {
        db.dictionary_add(&word, heard.as_deref(), history::now_ms())
    })
    .await
}

#[tauri::command]
#[specta::specta]
async fn dictionary_update(
    id: i64,
    word: String,
    heard: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<DictionaryEntry, String> {
    edit(&state, move |db| {
        db.dictionary_update(id, &word, heard.as_deref())
    })
    .await
}

#[tauri::command]
#[specta::specta]
async fn dictionary_star(
    id: i64,
    starred: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    edit(&state, move |db| db.dictionary_star(id, starred)).await
}

#[tauri::command]
#[specta::specta]
async fn dictionary_delete(id: i64, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    edit(&state, move |db| db.dictionary_delete(id)).await
}

/// Adds the words in a CSV file: a word per row, optionally followed by
/// what the speech model writes instead and whether it's starred.
#[tauri::command]
#[specta::specta]
async fn dictionary_import(
    path: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Imported, String> {
    edit(&state, move |db| {
        db.dictionary_import(std::path::Path::new(&path), history::now_ms())
    })
    .await
}

/// Returns how many entries it wrote.
#[tauri::command]
#[specta::specta]
async fn dictionary_export(path: String, state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    let db = db_of(&state)?;
    let count = blocking(move || db.dictionary_export(std::path::Path::new(&path))).await?;
    Ok(count as u32)
}

#[tauri::command]
#[specta::specta]
async fn snippets_list(state: State<'_, Arc<AppState>>) -> Result<Vec<Snippet>, String> {
    let db = db_of(&state)?;
    blocking(move || db.snippets()).await
}

#[tauri::command]
#[specta::specta]
async fn snippet_add(
    phrase: String,
    text: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Snippet, String> {
    edit(&state, move |db| {
        db.snippet_add(&phrase, &text, history::now_ms())
    })
    .await
}

#[tauri::command]
#[specta::specta]
async fn snippet_update(
    id: i64,
    phrase: String,
    text: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Snippet, String> {
    edit(&state, move |db| db.snippet_update(id, &phrase, &text)).await
}

#[tauri::command]
#[specta::specta]
async fn snippet_delete(id: i64, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    edit(&state, move |db| db.snippet_delete(id)).await
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleOverview {
    pub categories: Vec<CategoryStyle>,
    pub apps: Vec<AppRule>,
    /// Apps the user dictated into that aren't in `apps`, to add.
    pub recent: Vec<RecentApp>,
}

/// How many recent apps the Style page offers to add.
const RECENT_APPS: usize = 12;

fn style_overview_of(state: &AppState) -> anyhow::Result<StyleOverview> {
    let rules = state.rules();
    let recent = match &state.history {
        Some(history) => history.recent_apps(50)?,
        None => Vec::new(),
    };
    Ok(StyleOverview {
        categories: rules.styles.overview(),
        apps: rules.styles.rules().to_vec(),
        recent: recent
            .into_iter()
            .filter(|r| !rules.styles.is_listed(&r.app) && r.app != "joona.exe")
            .take(RECENT_APPS)
            .collect(),
    })
}

#[tauri::command]
#[specta::specta]
async fn style_overview(state: State<'_, Arc<AppState>>) -> Result<StyleOverview, String> {
    let state = state.inner().clone();
    blocking(move || style_overview_of(&state)).await
}

#[tauri::command]
#[specta::specta]
async fn style_set_tone(
    category: Category,
    tone: Tone,
    state: State<'_, Arc<AppState>>,
) -> Result<StyleOverview, String> {
    edit(&state, move |db| db.set_tone(category, tone)).await?;
    let state = state.inner().clone();
    blocking(move || style_overview_of(&state)).await
}

/// Puts an app or website in a category, or back to Joona's choice with
/// `None`. `app` is a program such as `slack.exe`, or a website's name.
#[tauri::command]
#[specta::specta]
async fn style_set_app(
    app: String,
    category: Option<Category>,
    state: State<'_, Arc<AppState>>,
) -> Result<StyleOverview, String> {
    edit(&state, move |db| db.set_app_category(&app, category)).await?;
    let state = state.inner().clone();
    blocking(move || style_overview_of(&state)).await
}

/// Starts recording a shortcut. Progress arrives as `ShortcutRecording` events.
#[tauri::command]
#[specta::specta]
fn shortcut_record(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    {
        let hotkeys = state.hotkeys.lock().unwrap_or_else(|e| e.into_inner());
        let hotkeys = hotkeys
            .as_ref()
            .ok_or("Joona's keyboard shortcut isn't running, so it can't record one.")?;
        hotkeys.record(tx);
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let timeout = tokio::time::sleep(RECORD_TIMEOUT);
        tokio::pin!(timeout);
        loop {
            let recording = tokio::select! {
                event = rx.recv() => match event {
                    Some(Recorded::Preview(shortcut)) => Recording::Preview {
                        label: shortcut.label(),
                        shortcut,
                    },
                    Some(Recorded::Done(shortcut)) => Recording::Done {
                        label: shortcut.label(),
                        trigger: shortcut.is_valid_trigger(),
                        shortcut,
                    },
                    Some(Recorded::Cancelled) => Recording::Cancelled,
                    // Replaced by a newer recording, or cancelled from the Hub.
                    None => break,
                },
                () = &mut timeout => {
                    if let Some(hotkeys) = state.hotkeys.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                        hotkeys.stop_recording();
                    }
                    Recording::Cancelled
                }
            };
            let finished = !matches!(recording, Recording::Preview { .. });
            let _ = ShortcutRecording(recording).emit_to(&app, "hub");
            if finished {
                break;
            }
        }
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn shortcut_record_cancel(state: State<'_, Arc<AppState>>) {
    if let Some(hotkeys) = state
        .hotkeys
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        hotkeys.stop_recording();
    }
}

/// Opens the microphone for the level test in Settings and returns its
/// name. Levels arrive as `MicTestLevel` events until `micTestStop`.
#[tauri::command]
#[specta::specta]
async fn mic_test_start(
    device: Option<String>,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let state = state.inner().clone();
    let (token, name) = blocking({
        let state = state.clone();
        move || {
            state.mic_test.start(device.as_deref(), move |level| {
                let _ = MicTestLevel(level).emit_to(&app, "hub");
            })
        }
    })
    .await?;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(MIC_TEST_LIMIT).await;
        tauri::async_runtime::spawn_blocking(move || state.mic_test.stop(Some(token)));
    });
    Ok(name)
}

#[tauri::command]
#[specta::specta]
async fn mic_test_stop(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let state = state.inner().clone();
    blocking(move || {
        state.mic_test.stop(None);
        Ok(())
    })
    .await
}

/// Whether Windows lets desktop apps such as Joona use the microphone.
#[tauri::command]
#[specta::specta]
fn mic_access() -> MicAccess {
    audio::mic_access()
}

/// Opens the microphone privacy switches in Windows Settings.
#[tauri::command]
#[specta::specta]
fn open_mic_privacy(app: AppHandle) -> Result<(), String> {
    open_privacy_settings(&app)
}

fn open_privacy_settings(app: &AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(audio::PRIVACY_SETTINGS_URL, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Every language Joona knows, Indian languages first.
#[tauri::command]
#[specta::specta]
fn languages() -> Vec<Language> {
    languages::LANGUAGES.to_vec()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LanguageRoutes {
    pub dictate: RoutePreview,
    /// For the alternate shortcut, when it has a language.
    pub alternate: Option<RoutePreview>,
}

/// How dictation would work with these settings. The Hub passes its own copy
/// so the answer matches a change that is still being saved.
#[tauri::command]
#[specta::specta]
fn language_routes(settings: Settings, state: State<'_, Arc<AppState>>) -> LanguageRoutes {
    let translators = Translators {
        sarvam_key: state.keys.has(SARVAM),
        ai: ai::chosen(&settings, &state.keys, &state.local_ai)
            .ok()
            .filter(|_| settings.polish.translate)
            .map(|chosen| chosen.spec.name.to_owned()),
        ai_first: settings.polish.level == PolishLevel::Smart,
    };
    LanguageRoutes {
        dictate: router::preview(&settings, Mode::Dictate, &state.library, &translators),
        alternate: settings
            .languages
            .alternate
            .is_some()
            .then(|| router::preview(&settings, Mode::Alternate, &state.library, &translators)),
    }
}

/// Why a provider didn't accept a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderFailure {
    pub message: String,
    /// The key itself is missing or wrong, so there's no point saving it.
    pub key_problem: bool,
}

impl From<ProviderError> for ProviderFailure {
    fn from(error: ProviderError) -> Self {
        Self {
            key_problem: matches!(error, ProviderError::NoKey(_) | ProviderError::BadKey(_)),
            message: error.to_string(),
        }
    }
}

impl ProviderFailure {
    fn other(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            key_problem: false,
        }
    }
}

fn known_provider(provider: &str) -> Result<(), String> {
    match providers::KNOWN.contains(&provider) {
        true => Ok(()),
        false => Err(format!("Joona doesn't know a provider called {provider}.")),
    }
}

/// The providers with a saved API key. The keys themselves never leave Rust.
#[tauri::command]
#[specta::specta]
async fn provider_keys(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let keys = state.keys.clone();
    blocking(move || {
        Ok(providers::KNOWN
            .iter()
            .filter(|p| keys.has(p))
            .map(|p| (*p).to_owned())
            .collect())
    })
    .await
}

/// Saves an API key in Windows Credential Manager.
#[tauri::command]
#[specta::specta]
async fn provider_key_set(
    provider: String,
    key: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    known_provider(&provider)?;
    let key = key.trim().to_owned();
    if key.is_empty() {
        return Err("Paste a key first.".into());
    }
    let keys = state.keys.clone();
    blocking(move || keys.set(&provider, &key)).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
async fn provider_key_delete(
    provider: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    known_provider(&provider)?;
    let keys = state.keys.clone();
    blocking(move || keys.delete(&provider)).await
}

/// Checks a key with the provider: `key`, or the saved one when that's
/// `None`. `endpoint` is the address the user typed for a server they run.
#[tauri::command]
#[specta::specta]
async fn provider_test(
    provider: String,
    key: Option<String>,
    endpoint: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<(), ProviderFailure> {
    known_provider(&provider).map_err(ProviderFailure::other)?;
    let endpoint = match (llm::spec(&provider), stt::spec(&provider)) {
        (Some(spec), _) if spec.endpoint => Some(base_for(spec, endpoint)?),
        (None, Some(spec)) if spec.endpoint => Some(address(endpoint, spec.base, false)?),
        _ => None,
    };
    let key = match (typed(key), providers::key_need(&provider)) {
        (Some(key), _) => Some(key),
        (None, KeyNeed::Never) => None,
        (None, KeyNeed::Optional) => providers::key(&state.keys, &provider).await.ok(),
        (None, KeyNeed::Required) => Some(providers::key(&state.keys, &provider).await?),
    };
    providers::check(&provider, key, endpoint).await?;
    Ok(())
}

fn typed(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// The API address for `spec`: the one the user typed, or its usual one.
fn base_for(spec: &llm::Spec, endpoint: Option<String>) -> Result<String, ProviderFailure> {
    match spec.endpoint {
        true => address(endpoint, spec.base, spec.local),
        false => Ok(spec.base.to_owned()),
    }
}

/// The address the user typed, or `usual`. Joona's own server, which is
/// `local`, picks its address when it starts.
fn address(endpoint: Option<String>, usual: &str, local: bool) -> Result<String, ProviderFailure> {
    match typed(endpoint) {
        Some(typed) => llm::endpoint(&typed).map_err(ProviderFailure::other),
        None if usual.is_empty() && !local => {
            Err(ProviderFailure::other("Type your server's address first."))
        }
        None => Ok(usual.to_owned()),
    }
}

/// Smart polish's setup changed: the local model was installed or deleted.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct AiChanged;

/// A provider of AI models, for the Providers page.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub key: KeyNeed,
    /// It runs on this PC unless the user points it elsewhere.
    pub local: bool,
    /// The user can change its address.
    pub endpoint: bool,
    /// Its usual address, when it has one.
    pub default_endpoint: Option<String>,
    /// Models that work well for dictation, the default first.
    pub models: Vec<String>,
    pub keys_url: Option<String>,
    pub privacy_url: Option<String>,
    /// Something to know before sending it text.
    pub note: Option<String>,
}

impl AiProvider {
    fn new(spec: &llm::Spec) -> Self {
        Self {
            id: spec.id.to_owned(),
            name: spec.name.to_owned(),
            summary: spec.summary.to_owned(),
            key: spec.key,
            local: spec.local,
            endpoint: spec.endpoint,
            default_endpoint: (!spec.base.is_empty()).then(|| spec.base.to_owned()),
            models: spec.models.iter().map(|m| (*m).to_owned()).collect(),
            keys_url: spec.keys_url.map(str::to_owned),
            privacy_url: spec.privacy_url.map(str::to_owned),
            note: spec.note.map(str::to_owned),
        }
    }
}

/// The AI model Smart polish uses.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiChoice {
    pub provider: String,
    pub name: String,
    pub model: String,
    /// Text leaves this PC.
    pub cloud: bool,
}

/// Joona's own AI model.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalAiInfo {
    pub name: String,
    /// Bytes to download.
    pub size: u64,
    pub ram_mb: u32,
    pub license: String,
    pub credit: String,
    pub installed: bool,
    pub running: bool,
    /// A download that's running or just ended.
    pub download: Option<Progress>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiOverview {
    /// In the order the Providers page lists them.
    pub providers: Vec<AiProvider>,
    /// The model Smart polish uses, when it's ready.
    pub ready: Option<AiChoice>,
    /// Why the provider the user picked isn't ready.
    pub problem: Option<String>,
    /// `None` when this build has no local model to offer.
    pub local: Option<LocalAiInfo>,
}

/// A speech-to-text service the user can bring a key for.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SttProvider {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub key: KeyNeed,
    /// The user types the address of a server they run.
    pub endpoint: bool,
    /// Good models for dictation, the default first.
    pub models: Vec<String>,
    /// The models among them that can turn speech straight into English.
    pub translates: Vec<String>,
    /// It gets the words in the dictionary along with each recording.
    pub terms: bool,
    pub keys_url: Option<String>,
    pub privacy_url: Option<String>,
    pub note: Option<String>,
}

impl SttProvider {
    fn new(spec: &stt::Spec) -> Self {
        Self {
            id: spec.id.to_owned(),
            name: spec.name.to_owned(),
            summary: spec.summary.to_owned(),
            key: spec.key,
            endpoint: spec.endpoint,
            models: spec.models.iter().map(|m| (*m).to_owned()).collect(),
            translates: spec
                .models
                .iter()
                .filter(|m| spec.translates(m))
                .map(|m| (*m).to_owned())
                .collect(),
            terms: spec.takes_terms(),
            keys_url: spec.keys_url.map(str::to_owned),
            privacy_url: spec.privacy_url.map(str::to_owned),
            note: spec.note.map(str::to_owned),
        }
    }
}

/// The service that hears dictations in place of the model on this PC.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SttChoice {
    pub provider: String,
    pub name: String,
    pub model: String,
    /// Recordings leave this PC.
    pub cloud: bool,
    /// It turns speech in other languages straight into English.
    pub translates: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SttOverview {
    pub providers: Vec<SttProvider>,
    /// The service in use, when it's ready.
    pub ready: Option<SttChoice>,
    /// Why the service the user picked isn't ready.
    pub problem: Option<String>,
}

/// The speech-to-text services, and the one in use.
#[tauri::command]
#[specta::specta]
async fn stt_overview(state: State<'_, Arc<AppState>>) -> Result<SttOverview, String> {
    let state = state.inner().clone();
    // Whether keys are saved comes from Credential Manager.
    blocking(move || {
        let settings = state.settings.get();
        let (ready, problem) = match stt::chosen(&settings) {
            None => (None, None),
            Some(Err(why)) => (None, Some(why.to_string())),
            Some(Ok(chosen))
                if chosen.spec.key == KeyNeed::Required && !state.keys.has(chosen.spec.id) =>
            {
                (
                    None,
                    Some(format!("Your {} API key is missing.", chosen.spec.name)),
                )
            }
            Some(Ok(chosen)) => (
                Some(SttChoice {
                    provider: chosen.spec.id.to_owned(),
                    name: chosen.spec.name.to_owned(),
                    translates: chosen.spec.translates(&chosen.model),
                    model: chosen.model,
                    cloud: chosen.cloud,
                }),
                None,
            ),
        };
        Ok(SttOverview {
            providers: stt::SPECS.iter().map(SttProvider::new).collect(),
            ready,
            problem,
        })
    })
    .await
}

/// What Smart polish made of a sample dictation.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSample {
    pub before: String,
    pub after: String,
    pub ms: u32,
}

/// The AI providers, the one in use, and Joona's own model.
#[tauri::command]
#[specta::specta]
async fn ai_overview(state: State<'_, Arc<AppState>>) -> Result<AiOverview, String> {
    let state = state.inner().clone();
    // Whether keys are saved comes from Credential Manager.
    blocking(move || {
        let settings = state.settings.get();
        let (ready, problem) = match ai::chosen(&settings, &state.keys, &state.local_ai) {
            Ok(chosen) => (
                Some(AiChoice {
                    provider: chosen.spec.id.to_owned(),
                    name: chosen.spec.name.to_owned(),
                    model: chosen.model,
                    cloud: chosen.cloud,
                }),
                None,
            ),
            Err(ai::NotReady::NoProvider) => (None, None),
            Err(why) => (None, Some(why.to_string())),
        };
        let download = state
            .downloads
            .progress()
            .into_iter()
            .find(|p| p.id == ai::local::DOWNLOAD_ID);
        let local = LocalAi::files().map(|files| LocalAiInfo {
            name: files.name.clone(),
            size: files.download_size(),
            ram_mb: files.ram_mb,
            license: files.license.clone(),
            credit: files.credit.clone(),
            installed: state.local_ai.installed(),
            running: state.local_ai.running(),
            download,
        });
        Ok(AiOverview {
            providers: llm::SPECS.iter().map(AiProvider::new).collect(),
            ready,
            problem,
            local,
        })
    })
    .await
}

/// The provider's chat models. `key` and `endpoint` are what the user typed;
/// without them, the saved key and the usual address are used.
#[tauri::command]
#[specta::specta]
async fn ai_models(
    provider: String,
    key: Option<String>,
    endpoint: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<String>, ProviderFailure> {
    let spec = llm::spec(&provider).ok_or_else(|| {
        ProviderFailure::other(format!("Joona doesn't know a provider called {provider}."))
    })?;
    let base = base_for(spec, endpoint)?;
    let key = match (typed(key), spec.key) {
        (Some(key), _) => Some(key),
        (None, KeyNeed::Never) => None,
        (None, KeyNeed::Optional) => providers::key(&state.keys, spec.id).await.ok(),
        (None, KeyNeed::Required) => Some(providers::key(&state.keys, spec.id).await?),
    };
    Ok(Llm::new(spec, base, key, "").models().await?)
}

/// Runs a sample dictation through Smart polish with the model in use.
#[tauri::command]
#[specta::specta]
async fn ai_test(state: State<'_, Arc<AppState>>) -> Result<AiSample, ProviderFailure> {
    let state = state.inner().clone();
    let chosen = {
        let state = state.clone();
        tauri::async_runtime::spawn_blocking(move || {
            ai::chosen(&state.settings.get(), &state.keys, &state.local_ai)
        })
        .await
        .map_err(|e| ProviderFailure::other(e.to_string()))?
        .map_err(|why| ProviderFailure::other(why.to_string()))?
    };
    let request = ai::Request::sample();
    let started = Instant::now();
    let after = ai::run(&chosen, &state.keys, &state.local_ai, &request).await?;
    Ok(AiSample {
        before: request.text,
        after,
        ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
    })
}

/// Downloads Joona's own AI model and the server that runs it.
#[tauri::command]
#[specta::specta]
async fn local_ai_download(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let files = state.local_ai.downloads();
    if files.is_empty() {
        return Err("This version of Joona has no local AI model.".into());
    }
    // Its files can't be replaced while it runs.
    state.local_ai.stop().await;
    let dir = state.local_ai.dir().to_owned();
    state
        .downloads
        .start_files(ai::local::DOWNLOAD_ID, files, move || {
            ai::local::unpack_server(&dir)
        })
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
fn local_ai_cancel(state: State<'_, Arc<AppState>>) {
    state.downloads.cancel(ai::local::DOWNLOAD_ID);
}

/// Deletes Joona's own AI model. Smart polish stops using it.
#[tauri::command]
#[specta::specta]
async fn local_ai_delete(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .local_ai
        .delete()
        .await
        .map_err(|e| format!("{e:#}"))?;
    if state.settings.get().polish.provider.as_deref() == Some(llm::LOCAL) {
        state
            .settings
            .update(|s| {
                s.polish.provider = None;
                s.polish.model = None;
            })
            .map_err(|e| format!("{e:#}"))?;
    }
    let _ = AiChanged.emit_to(&app, "hub");
    Ok(())
}

/// Sends download progress to the Hub.
pub struct DownloadEvents(pub AppHandle);

impl Listener for DownloadEvents {
    fn progress(&self, progress: &Progress) {
        let _ = ModelDownload(progress.clone()).emit_to(&self.0, "hub");
    }

    fn installed(&self, id: &str) {
        let _ = ModelsChanged.emit_to(&self.0, "hub");
        crate::model_installed(&self.0, id);
    }
}

/// The session's view of the app: the bar window, the tray and sounds.
pub struct TauriUi {
    pub app: AppHandle,
    pub overlay: Arc<Overlay>,
}

impl Ui for TauriUi {
    fn bar(&self, state: &BarState) {
        self.overlay
            .set_dictating(!matches!(state, BarState::Resting));
        let listening = matches!(state, BarState::Listening(_));
        let paused = self
            .app
            .try_state::<Arc<AppState>>()
            .is_some_and(|s| s.settings.get().general.paused);
        crate::tray::set_listening(&self.app, listening, paused);
        let _ = BarStateChanged(state.clone()).emit(&self.app);
    }

    fn level(&self, level: f32) {
        let _ = MicLevel(level).emit_to(&self.app, "bar");
    }

    fn toast(&self, toast: &Toast) {
        let _ = ToastShown(toast.clone()).emit(&self.app);
    }

    fn dismiss_toast(&self, id: u32) {
        let _ = ToastDismissed(id).emit(&self.app);
    }

    fn sound(&self, cue: Cue) {
        sounds::play(cue);
    }

    fn open_hub(&self, page: Option<&str>) {
        crate::show_hub(&self.app, page);
    }

    fn open_mic_privacy(&self) {
        if let Err(e) = open_privacy_settings(&self.app) {
            tracing::warn!("couldn't open the microphone settings: {e}");
        }
    }

    fn history(&self, entry: &Entry) {
        let _ = HistoryAdded(entry.clone()).emit_to(&self.app, "hub");
    }
}

pub struct OverlayBridge(pub AppHandle);

impl OverlayEvents for OverlayBridge {
    fn hover(&self, inside: bool) {
        let _ = BarHover(inside).emit_to(&self.0, "bar");
    }

    fn resting(&self, visible: bool) {
        let _ = BarResting(visible).emit_to(&self.0, "bar");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{LanguageChoice, LanguageEngine};

    #[test]
    fn bar_config_describes_the_route() {
        let mut s = Settings::default();
        assert_eq!(BarConfig::from_settings(&s).route, "EN");
        s.languages.speak = ["hi", "en"]
            .iter()
            .map(|c| LanguageChoice {
                code: (*c).into(),
                engine: LanguageEngine::Local,
            })
            .collect();
        s.languages.output = Some("en".into());
        let config = BarConfig::from_settings(&s);
        assert_eq!(config.route, "Auto → EN");
        assert_eq!(config.languages[0].native, "हिन्दी");
        s.languages.current = Some("en".into());
        assert_eq!(BarConfig::from_settings(&s).route, "EN");
    }

    #[test]
    fn bindings_export() {
        let dir = std::env::temp_dir().join(format!("joona-bindings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bindings.ts");
        specta()
            .export(specta_typescript::Typescript::default(), &path)
            .expect("bindings export");
        let ts = std::fs::read_to_string(path).unwrap();
        assert!(ts.contains("barSnapshot"));
        assert!(ts.contains("BarStateChanged") || ts.contains("barStateChanged"));
    }
}
