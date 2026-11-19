//! The notification-area icon and its menu.

use std::sync::Arc;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::languages;
use crate::session::Command;
use crate::settings::Settings;
use crate::state::AppState;

const TRAY_ID: &str = "joona";
const IDLE: &[u8] = include_bytes!("../icons/tray/idle.png");
const LISTENING: &[u8] = include_bytes!("../icons/tray/listening.png");
const PAUSED: &[u8] = include_bytes!("../icons/tray/paused.png");

pub fn create(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(icon_for(false, settings.general.paused))?)
        .tooltip(tooltip(settings))
        .menu(&menu(app, settings)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_hub(tray.app_handle(), None);
            }
        })
        .build(app)?;
    Ok(())
}

/// Rebuilds the menu and icon after a settings change.
pub fn refresh(app: &AppHandle, settings: &Settings, listening: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = menu(app, settings) {
        let _ = tray.set_menu(Some(menu));
    }
    let _ = tray.set_tooltip(Some(tooltip(settings)));
    set_listening(app, listening, settings.general.paused);
}

/// Rebuilds the menu after something it shows changed, such as an update
/// turning up.
pub fn refresh_menu(app: &AppHandle) {
    let (Some(tray), Some(state)) = (app.tray_by_id(TRAY_ID), app.try_state::<Arc<AppState>>())
    else {
        return;
    };
    if let Ok(menu) = menu(app, &state.settings.get()) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn set_listening(app: &AppHandle, listening: bool, paused: bool) {
    if let Some(tray) = app.tray_by_id(TRAY_ID)
        && let Ok(icon) = Image::from_bytes(icon_for(listening, paused))
    {
        let _ = tray.set_icon(Some(icon));
    }
}

fn icon_for(listening: bool, paused: bool) -> &'static [u8] {
    match (listening, paused) {
        (true, _) => LISTENING,
        (false, true) => PAUSED,
        (false, false) => IDLE,
    }
}

fn tooltip(settings: &Settings) -> String {
    match settings.general.paused {
        true => "Joona (paused)".into(),
        false => format!(
            "Joona: hold {} to dictate",
            settings.shortcuts.trigger.label()
        ),
    }
}

fn menu(app: &AppHandle, settings: &Settings) -> tauri::Result<Menu<Wry>> {
    let trigger = settings.shortcuts.trigger.label();
    let hands_free = match settings.shortcuts.space_for_hands_free {
        true => format!("Start hands-free\t{trigger}+Space"),
        false => "Start hands-free".into(),
    };
    let paste_last = match settings.shortcuts.paste_last {
        Some(shortcut) => format!("Paste last transcript\t{}", shortcut.label()),
        None => "Paste last transcript".into(),
    };
    let update = match app
        .try_state::<Arc<AppState>>()
        .and_then(|state| state.updates.available())
    {
        Some(version) => format!("Update to Joona {version}…"),
        None => "Check for updates…".into(),
    };

    let languages = &settings.languages;
    let mut language_items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    if languages.speak.len() > 1 {
        language_items.push(Box::new(CheckMenuItem::with_id(
            app,
            "lang:auto",
            "Detect automatically",
            true,
            languages.current.is_none(),
            None::<&str>,
        )?));
    }
    for choice in &languages.speak {
        let checked = languages.current.as_deref() == Some(choice.code.as_str())
            || languages.speak.len() == 1;
        language_items.push(Box::new(CheckMenuItem::with_id(
            app,
            format!("lang:{}", choice.code),
            languages::name(&choice.code),
            true,
            checked,
            None::<&str>,
        )?));
    }
    language_items.push(Box::new(PredefinedMenuItem::separator(app)?));
    language_items.push(Box::new(MenuItem::with_id(
        app,
        "lang:settings",
        "Language settings…",
        true,
        None::<&str>,
    )?));
    let language_refs: Vec<&dyn IsMenuItem<Wry>> =
        language_items.iter().map(|i| i.as_ref()).collect();
    let language_menu = Submenu::with_items(app, "Language", true, &language_refs)?;

    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open Joona", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "hands_free",
                hands_free,
                !settings.general.paused,
                None::<&str>,
            )?,
            &MenuItem::with_id(app, "paste_last", paste_last, true, None::<&str>)?,
            &language_menu,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(
                app,
                "pause",
                "Pause Joona",
                true,
                settings.general.paused,
                None::<&str>,
            )?,
            &MenuItem::with_id(app, "updates", update, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Joona", true, None::<&str>)?,
        ],
    )
}

fn on_menu(app: &AppHandle, id: &str) {
    let state = app.state::<Arc<AppState>>();
    let update = |change: Box<dyn FnOnce(&mut Settings)>| {
        if let Err(e) = state.settings.update(change) {
            tracing::warn!("couldn't save settings from the tray: {e:#}");
        }
    };
    match id {
        "open" => crate::show_hub(app, None),
        "hands_free" => state.session.send(Command::StartHandsFree),
        "paste_last" => state.session.send(Command::PasteLast),
        "lang:settings" => crate::show_hub(app, Some("languages")),
        "lang:auto" => update(Box::new(|s| s.languages.current = None)),
        "pause" => update(Box::new(|s| s.general.paused = !s.general.paused)),
        "updates" => {
            crate::show_hub(app, Some("settings/about"));
            if state.updates.available().is_none() {
                let (app, state) = (app.clone(), state.inner().clone());
                tauri::async_runtime::spawn(async move {
                    crate::updates::check(&app, &state, false).await;
                });
            }
        }
        "quit" => app.exit(0),
        other => {
            if let Some(code) = other.strip_prefix("lang:") {
                let code = code.to_owned();
                update(Box::new(move |s| s.languages.current = Some(code)));
            }
        }
    }
}
