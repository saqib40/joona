pub mod ai;
pub mod audio;
pub mod history;
pub mod hotkeys;
pub mod hub;
pub mod insert;
pub mod ipc;
pub mod languages;
pub mod models;
pub mod overlay;
pub mod paths;
pub mod pipeline;
pub mod providers;
pub mod router;
pub mod session;
pub mod settings;
pub mod sounds;
pub mod state;
pub mod store;
pub mod supervisor;
mod tray;
pub mod updates;

use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use joona_protocol::ConfigureParams;
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_specta::Event;

use crate::ai::LocalAi;
use crate::audio::MicTest;
use crate::history::History;
use crate::hotkeys::Hotkeys;
use crate::insert::Inserter;
use crate::ipc::{
    AiChanged, BarConfig, BarConfigChanged, DownloadEvents, HistoryChanged, HubNavigate,
    ModelsChanged, OverlayBridge, SettingsChanged, TauriUi,
};
use crate::models::Library;
use crate::models::catalog::catalog;
use crate::models::download::Downloads;
use crate::overlay::Overlay;
use crate::pipeline::Rules;
use crate::providers::{Keys, llm};
use crate::session::{BarState, Command, SessionHandle, Toast, ToastAction, Tone};
use crate::settings::{Settings, SettingsStore};
use crate::state::AppState;
use crate::store::Db;
use crate::supervisor::{EngineEvent, Supervisor, Variant};
use crate::updates::Updates;

/// Passed by the login item: start in the tray without opening the Hub.
pub const BACKGROUND_FLAG: &str = "--background";
/// Passed by the uninstaller when the user deletes Joona's data.
const FORGET_KEYS_FLAG: &str = "--forget-keys";
const IDLE_CHECK: Duration = Duration::from_secs(60);
/// Model loading waits this long after start-up, so the windows come first.
const WARM_UP_DELAY: Duration = Duration::from_millis(1500);
/// How often history older than the retention setting is deleted.
const PRUNE_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

pub fn run() {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        // NSApplicationActivationPolicyAccessory = 1: Must be set before any windows
        // are constructed so that overlay windows can join all Spaces and full-screen apps.
        let _: () = msg_send![app, setActivationPolicy: 1i64];
    }

    let _log = init_logging();
    if std::env::args().any(|a| a == FORGET_KEYS_FLAG) {
        forget_keys();
        return;
    }
    #[cfg(debug_assertions)]
    if let Err(e) = write_bindings() {
        tracing::warn!("couldn't export TypeScript bindings: {e:#}");
    }
    let specta = ipc::specta();
    let background = std::env::args().any(|a| a == BACKGROUND_FLAG);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_hub(app, None);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![BACKGROUND_FLAG]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            specta.mount_events(app);
            setup(app.handle(), background).map_err(|e| {
                tracing::error!("start-up failed: {e:#}");
                e.into()
            })
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "menu_settings" {
                show_hub(app, Some("settings"));
            }
        })
        .on_window_event(|window, event| {
            if window.label() != "hub" {
                return;
            }
            match event {
                // Closing the Hub keeps Joona running in the tray.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                    if let Some(state) = window.try_state::<Arc<AppState>>() {
                        hub_closed(state.inner().clone());
                    }
                }
                // Mica in the system theme has to be applied again.
                WindowEvent::ThemeChanged(_) => {
                    if let (Some(state), Some(hub)) = (
                        window.try_state::<Arc<AppState>>(),
                        window.get_webview_window("hub"),
                    ) {
                        hub::style(&hub, state.settings.get().general.theme);
                    }
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Joona");

    app.run(|app, event| match event {
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            show_hub(app, None);
        }
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        RunEvent::Exit => shutdown(app),
        _ => {}
    });
}

fn setup(app: &AppHandle, background: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let pkg_info = app.package_info();
        let config = app.config();
        let about_metadata = tauri::menu::AboutMetadata {
            name: Some(pkg_info.name.clone()),
            version: Some(pkg_info.version.to_string()),
            copyright: config.bundle.copyright.clone(),
            authors: config.bundle.publisher.clone().map(|p| vec![p]),
            ..Default::default()
        };

        let app_submenu = tauri::menu::Submenu::with_items(
            app,
            pkg_info.name.clone(),
            true,
            &[
                &tauri::menu::PredefinedMenuItem::about(app, None, Some(about_metadata))?,
                &tauri::menu::MenuItem::with_id(
                    app,
                    "menu_settings",
                    "Settings…",
                    true,
                    Some("CmdOrCtrl+,"),
                )?,
                &tauri::menu::PredefinedMenuItem::separator(app)?,
                &tauri::menu::PredefinedMenuItem::services(app, None)?,
                &tauri::menu::PredefinedMenuItem::separator(app)?,
                &tauri::menu::PredefinedMenuItem::hide(app, None)?,
                &tauri::menu::PredefinedMenuItem::hide_others(app, None)?,
                &tauri::menu::PredefinedMenuItem::show_all(app, None)?,
                &tauri::menu::PredefinedMenuItem::separator(app)?,
                &tauri::menu::PredefinedMenuItem::quit(app, None)?,
            ],
        )?;

        let file_submenu = tauri::menu::Submenu::with_items(
            app,
            "File",
            true,
            &[&tauri::menu::PredefinedMenuItem::close_window(app, None)?],
        )?;

        let edit_submenu = tauri::menu::Submenu::with_items(
            app,
            "Edit",
            true,
            &[
                &tauri::menu::PredefinedMenuItem::undo(app, None)?,
                &tauri::menu::PredefinedMenuItem::redo(app, None)?,
                &tauri::menu::PredefinedMenuItem::separator(app)?,
                &tauri::menu::PredefinedMenuItem::cut(app, None)?,
                &tauri::menu::PredefinedMenuItem::copy(app, None)?,
                &tauri::menu::PredefinedMenuItem::paste(app, None)?,
                &tauri::menu::PredefinedMenuItem::select_all(app, None)?,
            ],
        )?;

        let window_submenu = tauri::menu::Submenu::with_items(
            app,
            "Window",
            true,
            &[
                &tauri::menu::PredefinedMenuItem::minimize(app, None)?,
                &tauri::menu::PredefinedMenuItem::maximize(app, None)?,
                &tauri::menu::PredefinedMenuItem::separator(app)?,
                &tauri::menu::PredefinedMenuItem::close_window(app, None)?,
            ],
        )?;

        let menu = tauri::menu::Menu::with_items(
            app,
            &[&app_submenu, &file_submenu, &edit_submenu, &window_submenu],
        )?;
        let _ = app.set_menu(menu);
    }

    let settings = SettingsStore::load(paths::settings_file());
    let initial = settings.get();

    let bar = app
        .get_webview_window("bar")
        .ok_or_else(|| anyhow::anyhow!("the bar window is missing"))?;
    let overlay = Overlay::start(
        native_handle(&bar)?,
        initial.general.bar,
        OverlayBridge(app.clone()),
    );

    let inserter = match Inserter::new() {
        Ok(inserter) => Some(Mutex::new(inserter)),
        Err(e) => {
            tracing::error!("the clipboard helper didn't start: {e:#}");
            None
        }
    };
    let db = match Db::open(&paths::database_file()) {
        Ok(db) => Some(Arc::new(db)),
        Err(e) => {
            tracing::error!("couldn't open the database: {e:#}");
            None
        }
    };
    let history = db
        .clone()
        .map(|db| Arc::new(History::new(db, paths::recordings_dir())));
    let (session, commands) = SessionHandle::channel();
    let library = Library::new(paths::models_dir());
    let downloads = Downloads::new(library.clone(), catalog(), DownloadEvents(app.clone()))?;
    let state = Arc::new(AppState {
        settings,
        engine: Supervisor::new(engine_variant(&initial)),
        library,
        downloads,
        inserter,
        db,
        history,
        rules: RwLock::new(Arc::new(Rules::default())),
        hotkeys: Mutex::new(None),
        ui: Arc::new(TauriUi {
            app: app.clone(),
            overlay: overlay.clone(),
        }),
        session,
        bar: Mutex::new(BarState::Resting),
        hardware: OnceLock::new(),
        mic_test: MicTest::default(),
        keys: Arc::new(Keys::default()),
        local_ai: Arc::new(LocalAi::new(paths::local_ai_dir())),
        updates: Updates::default(),
    });
    if let Err(e) = state.reload_rules() {
        tracing::error!("couldn't read the dictionary, snippets and styles: {e:#}");
    }
    app.manage(state.clone());
    app.manage(overlay);
    // Pages call commands that need the state as soon as they load, and pages load
    // while any window is being made. So Tauri makes only the bar before this setup,
    // and the Hub (`create: false`) is made after the state is managed.
    let hub = create_window(app, "hub")?;
    hub::style(&hub, initial.general.theme);

    sounds::prepare();
    crate::audio::ensure_mic_access();
    session::spawn(state.clone(), commands);
    start_hotkeys(&state, &initial);
    tray::create(app, &initial)?;
    watch_settings(app.clone(), state.clone());
    tauri::async_runtime::spawn(warm_up(state.clone()));
    tauri::async_runtime::spawn_blocking({
        let state = state.clone();
        move || {
            let hardware = state.hardware();
            tracing::info!(cpu = %hardware.cpu, ram_mb = hardware.ram_mb, gpu = ?hardware.best_gpu().map(|g| &g.name), "detected hardware");
        }
    });
    tauri::async_runtime::spawn(unload_when_idle(state.clone()));
    tauri::async_runtime::spawn(watch_engine(app.clone(), state.clone()));
    tauri::async_runtime::spawn(prune_history_regularly(app.clone(), state.clone()));
    updates::spawn(app.clone(), state.clone());
    apply_launch_at_login(app, initial.general.launch_at_login);

    // Until onboarding is done, even the login item opens the Hub.
    if !background || !initial.onboarded {
        show_hub(app, None);
    }
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        background,
        "Joona started"
    );
    Ok(())
}

fn shutdown(app: &AppHandle) {
    if let Some(overlay) = app.try_state::<Arc<Overlay>>() {
        overlay.stop();
    }
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state
            .hotkeys
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        tauri::async_runtime::block_on(async {
            state.engine.stop().await;
            state.local_ai.stop().await;
        });
    }
    tracing::info!("Joona stopped");
}

/// Shows the Hub and brings it to the front, optionally on a page.
pub fn show_hub(app: &AppHandle, page: Option<&str>) {
    let Some(hub) = app.get_webview_window("hub") else {
        return;
    };
    let _ = hub.unminimize();
    let _ = hub.show();
    let _ = hub.set_focus();
    if let Some(page) = page {
        let _ = HubNavigate(page.to_owned()).emit_to(app, "hub");
    }
}

/// Makes a window that tauri.conf.json describes but leaves to Joona to create.
fn create_window(app: &AppHandle, label: &str) -> anyhow::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .ok_or_else(|| anyhow::anyhow!("tauri.conf.json has no {label} window"))?;
    #[allow(unused_mut)]
    let mut builder = WebviewWindowBuilder::from_config(app, config)?;
    #[cfg(target_os = "macos")]
    if label == "hub" {
        builder = builder
            .decorations(true)
            .hidden_title(true)
            .title_bar_style(tauri::TitleBarStyle::Overlay);
    }
    Ok(builder.build()?)
}

#[cfg(windows)]
fn native_handle(window: &WebviewWindow) -> anyhow::Result<isize> {
    Ok(window.hwnd()?.0 as isize)
}

#[cfg(target_os = "macos")]
fn native_handle(window: &WebviewWindow) -> anyhow::Result<isize> {
    Ok(window.ns_window()? as isize)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn native_handle(_window: &WebviewWindow) -> anyhow::Result<isize> {
    Ok(0)
}

fn start_hotkeys(state: &Arc<AppState>, settings: &Settings) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    match Hotkeys::start(settings.gesture_config(), tx.clone()) {
        Ok(hotkeys) => {
            hotkeys.set_paused(settings.general.paused);
            *state.hotkeys.lock().unwrap_or_else(|e| e.into_inner()) = Some(hotkeys);
        }
        Err(e) => {
            tracing::warn!("couldn't install the keyboard shortcut on startup: {e:#}");
            #[cfg(target_os = "macos")]
            spawn_accessibility_retry(state.clone(), tx);
        }
    }
    let session = state.session.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(action) = rx.recv().await {
            session.send(Command::Gesture(action));
        }
    });
}

#[cfg(target_os = "macos")]
fn spawn_accessibility_retry(
    state: Arc<AppState>,
    tx: tokio::sync::mpsc::UnboundedSender<crate::hotkeys::Action>,
) {
    tauri::async_runtime::spawn(async move {
        tracing::info!("waiting for macOS Accessibility permission in System Settings...");
        let mut interval = tokio::time::interval(Duration::from_millis(1500));
        loop {
            interval.tick().await;

            if state
                .hotkeys
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
            {
                break;
            }

            if crate::hotkeys::macos::is_accessibility_trusted(false) {
                tracing::info!("macOS Accessibility permission granted, installing hotkey tap...");
                let current_settings = state.settings.get();
                match Hotkeys::start(current_settings.gesture_config(), tx.clone()) {
                    Ok(hotkeys) => {
                        hotkeys.set_paused(current_settings.general.paused);
                        *state.hotkeys.lock().unwrap_or_else(|e| e.into_inner()) = Some(hotkeys);
                        tracing::info!("macOS Accessibility hotkey tap successfully installed");
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("Accessibility was trusted but hotkeys start failed: {e:#}");
                    }
                }
            }
        }
    });
}

fn engine_config(state: &AppState, settings: &Settings) -> ConfigureParams {
    ConfigureParams {
        vad_model: state.library.vad(),
        threads: settings.models.threads,
        beam_size: settings.models.beam_size,
    }
}

/// The GPU build when the user wants it and it's installed.
fn engine_variant(settings: &Settings) -> Variant {
    if settings.models.use_gpu && Variant::Vulkan.is_installed() {
        Variant::Vulkan
    } else {
        Variant::Cpu
    }
}

fn watch_settings(app: AppHandle, state: Arc<AppState>) {
    let mut rx = state.settings.subscribe();
    tauri::async_runtime::spawn(async move {
        let mut last = rx.borrow().clone();
        while rx.changed().await.is_ok() {
            let now = rx.borrow_and_update().clone();
            if let Some(hotkeys) = state
                .hotkeys
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                hotkeys.update_config(now.gesture_config());
                hotkeys.set_paused(now.general.paused);
            }
            if let Some(overlay) = app.try_state::<Arc<Overlay>>() {
                overlay.set_visibility(now.general.bar);
            }
            let listening = matches!(
                *state.bar.lock().unwrap_or_else(|e| e.into_inner()),
                BarState::Listening(_)
            );
            tray::refresh(&app, &now, listening);
            if now.models != last.models
                && let Err(e) = state.engine.set_config(engine_config(&state, &now)).await
            {
                tracing::warn!("couldn't apply engine settings: {e}");
            }
            let gpu_changed = now.models.use_gpu != last.models.use_gpu;
            let choice_changed = now.models.active != last.models.active
                || now.models.english != last.models.english;
            if gpu_changed || choice_changed {
                // Free what the old choice loaded; the new default loads below.
                if let Err(e) = state.engine.unload(None).await {
                    tracing::debug!("couldn't unload the models: {e}");
                }
            }
            if gpu_changed {
                state.engine.set_variant(engine_variant(&now)).await;
                let _ = ModelsChanged.emit_to(&app, "hub");
            } else if choice_changed {
                let _ = ModelsChanged.emit_to(&app, "hub");
            }
            if gpu_changed
                || choice_changed
                || router::default_model(&now, &state.library)
                    != router::default_model(&last, &state.library)
            {
                preload_default(state.clone());
            }
            if now.general.launch_at_login != last.general.launch_at_login {
                apply_launch_at_login(&app, now.general.launch_at_login);
            }
            if now.general.check_updates && !last.general.check_updates {
                let (app, state) = (app.clone(), state.clone());
                tauri::async_runtime::spawn(async move {
                    updates::check(&app, &state, true).await;
                });
            }
            if now.general.theme != last.general.theme
                && let Some(hub) = app.get_webview_window("hub")
            {
                hub::style(&hub, now.general.theme);
            }
            if now.privacy.history_days != last.privacy.history_days {
                prune_history(&app, &state);
            }
            let local = |s: &Settings| s.polish.provider.as_deref() == Some(llm::LOCAL);
            if local(&last) && !local(&now) {
                state.local_ai.stop().await;
            }
            let _ = SettingsChanged(now.clone()).emit(&app);
            let _ = BarConfigChanged(BarConfig::from_settings(&now)).emit(&app);
            last = now;
        }
    });
}

/// Loads the default model shortly after start-up, so the first dictation is quick.
async fn warm_up(state: Arc<AppState>) {
    let settings = state.settings.get();
    if let Err(e) = state
        .engine
        .set_config(engine_config(&state, &settings))
        .await
    {
        tracing::warn!("couldn't store engine settings: {e}");
    }
    // Models copied in by hand arrive without Silero VAD.
    if state.library.vad().is_none() && !state.library.installed().is_empty() {
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            match state.downloads.ensure_vad().await {
                Ok(_) => {
                    let config = engine_config(&state, &state.settings.get());
                    if let Err(e) = state.engine.set_config(config).await {
                        tracing::warn!("couldn't give the engine Silero VAD: {e}");
                    }
                }
                Err(e) => tracing::warn!("couldn't download Silero VAD: {e:#}"),
            }
        });
    }
    tokio::time::sleep(WARM_UP_DELAY).await;
    preload_default(state);
}

/// Loads the model the next dictation would use, unless the user turned
/// preloading off.
pub(crate) fn preload_default(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let settings = state.settings.get();
        if !settings.models.preload {
            return;
        }
        let Some(model) = router::default_model(&settings, &state.library) else {
            tracing::info!("no speech model installed yet");
            return;
        };
        match state
            .engine
            .load(model.clone(), settings.models.use_gpu)
            .await
        {
            Ok(loaded) => {
                tracing::info!(model = %model.id, ms = loaded.load_ms, device = ?loaded.device, "model ready")
            }
            Err(e) => tracing::warn!(model = %model.id, "couldn't preload the model: {e}"),
        }
    });
}

/// A download finished. The engine may now have Silero VAD, and dictation
/// may use the new model; with the Hub closed, the bar says it's ready.
pub(crate) fn model_installed(app: &AppHandle, id: &str) {
    let Some(state) = app.try_state::<Arc<AppState>>().map(|s| s.inner().clone()) else {
        return;
    };
    let hub_open = app
        .get_webview_window("hub")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    let settings = state.settings.get();
    if id == ai::local::DOWNLOAD_ID {
        let _ = AiChanged.emit_to(app, "hub");
        if !hub_open {
            state.session.send(Command::Toast(
                Toast::new(Tone::Success, "Local AI is ready")
                    .detail("Smart polish can use it now. Your text stays on this PC.")
                    .action(ToastAction::OpenProviders, "Providers"),
            ));
        }
        return;
    }
    if !hub_open {
        let name = catalog().get(id).map_or(id, |m| m.name.as_str());
        state.session.send(Command::Toast(
            Toast::new(Tone::Success, format!("{name} is ready")).detail(format!(
                "Hold {} to dictate.",
                settings.shortcuts.trigger.label()
            )),
        ));
    }
    tauri::async_runtime::spawn(async move {
        if let Err(e) = state
            .engine
            .set_config(engine_config(&state, &settings))
            .await
        {
            tracing::warn!("couldn't apply engine settings: {e}");
        }
        preload_default(state);
    });
}

/// Frees models nobody has used for a while.
async fn unload_when_idle(state: Arc<AppState>) {
    let mut every = tokio::time::interval(IDLE_CHECK);
    every.tick().await;
    loop {
        every.tick().await;
        let minutes = state.settings.get().models.idle_unload_minutes;
        if minutes == 0 {
            continue;
        }
        let idle = Duration::from_secs(u64::from(minutes) * 60);
        state.local_ai.stop_if_idle(idle);
        if state.engine.info().await.is_none() {
            continue;
        }
        match state.engine.unload_idle(idle).await {
            Ok(freed) if !freed.is_empty() => tracing::info!(?freed, "unloaded idle models"),
            Ok(_) => {}
            Err(e) => tracing::debug!("idle check failed: {e}"),
        }
    }
}

async fn watch_engine(app: AppHandle, state: Arc<AppState>) {
    let mut events = state.engine.subscribe();
    loop {
        match events.recv().await {
            Ok(EngineEvent::Crashed {
                message,
                restart_in: None,
            }) => {
                tracing::error!("the engine keeps crashing: {message}");
                state.session.send(Command::Toast(
                    Toast::new(Tone::Error, "The local engine keeps crashing")
                        .detail(message)
                        .action(ToastAction::OpenModels, "Models"),
                ));
            }
            Ok(EngineEvent::FellBackToCpu { reason }) => {
                tracing::warn!("switched the engine to the CPU: {reason}");
                state.session.send(Command::Toast(
                    Toast::new(Tone::Warning, "Switched to the CPU")
                        .detail(format!("The GPU engine failed: {reason}"))
                        .action(ToastAction::OpenModels, "Models"),
                ));
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
        let _ = ModelsChanged.emit_to(&app, "hub");
    }
}

/// Deletes history older than the retention setting.
fn prune_history(app: &AppHandle, state: &Arc<AppState>) {
    let Some(history) = state.history.clone() else {
        return;
    };
    let days = state.settings.get().privacy.history_days;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        match history.prune(days, crate::history::now_ms()) {
            Ok(0) => {}
            Ok(removed) => {
                tracing::info!(removed, days, "deleted old history");
                let _ = HistoryChanged.emit_to(&app, "hub");
            }
            Err(e) => tracing::warn!("couldn't delete old history: {e:#}"),
        }
    });
}

async fn prune_history_regularly(app: AppHandle, state: Arc<AppState>) {
    let mut every = tokio::time::interval(PRUNE_EVERY);
    loop {
        every.tick().await;
        prune_history(&app, &state);
    }
}

/// The microphone test and shortcut recording end with the Hub.
fn hub_closed(state: Arc<AppState>) {
    if let Some(hotkeys) = state
        .hotkeys
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        hotkeys.stop_recording();
    }
    tauri::async_runtime::spawn_blocking(move || state.mic_test.stop(None));
}

/// Deletes every saved API key from Credential Manager.
fn forget_keys() {
    let keys = Keys::default();
    for provider in providers::KNOWN {
        if let Err(e) = keys.delete(provider) {
            tracing::warn!(provider, "couldn't delete the API key: {e:#}");
        }
    }
    tracing::info!("deleted the saved API keys");
}

fn apply_launch_at_login(app: &AppHandle, enabled: bool) {
    // Development builds would register the debug binary at login.
    if cfg!(debug_assertions) {
        return;
    }
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    let result = match enabled {
        true => autolaunch.enable(),
        false => autolaunch.disable(),
    };
    if let Err(e) = result {
        tracing::warn!("couldn't change launch at login: {e}");
    }
}

fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, Layer, fmt};

    let filter = || {
        EnvFilter::try_from_env("JOONA_LOG")
            .unwrap_or_else(|_| EnvFilter::new("info,joona_lib=debug,joona::engine=info"))
    };
    let dir = paths::logs_dir();
    let file = std::fs::create_dir_all(&dir).ok().and_then(|_| {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("joona")
            .filename_suffix("log")
            .max_log_files(7)
            .build(&dir)
            .ok()
    });
    let (file_layer, guard) = match file {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_filter(filter());
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };
    let console = cfg!(debug_assertions).then(|| fmt::layer().with_filter(filter()));
    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(console)
        .try_init();
    guard
}

/// Writes the TypeScript bindings for the UI to `src/lib/bindings.ts`.
pub fn write_bindings() -> anyhow::Result<std::path::PathBuf> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts");
    ipc::specta().export(specta_typescript::Typescript::default(), &path)?;
    Ok(path)
}
