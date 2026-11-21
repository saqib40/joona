//! User settings, kept as JSON in `%APPDATA%\Joona\settings.json`.
//!
//! Every field has a default, so older files load after an upgrade and
//! unknown fields from newer versions are ignored. Saves go through a
//! temporary file, so a crash mid-write can't leave a truncated file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::watch;

use crate::hotkeys::{GestureConfig, Shortcut};
use crate::insert::InsertSettings;
use crate::providers::{llm, stt};

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    /// Onboarding has been completed (or skipped).
    pub onboarded: bool,
    pub general: General,
    pub shortcuts: Shortcuts,
    pub languages: Languages,
    pub models: ModelSettings,
    pub insertion: InsertSettings,
    pub polish: Polish,
    pub transcription: Transcription,
    pub privacy: Privacy,
    pub experimental: Experimental,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            onboarded: false,
            general: General::default(),
            shortcuts: Shortcuts::default(),
            languages: Languages::default(),
            models: ModelSettings::default(),
            insertion: InsertSettings::default(),
            polish: Polish::default(),
            transcription: Transcription::default(),
            privacy: Privacy::default(),
            experimental: Experimental::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BarVisibility {
    /// A thin pill at rest, the full bar while dictating.
    #[default]
    Always,
    /// Only while dictating.
    Dictating,
    Hidden,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct General {
    pub launch_at_login: bool,
    pub bar: BarVisibility,
    pub sounds: bool,
    pub theme: Theme,
    /// Input device id. `None` follows the Windows default.
    pub microphone: Option<String>,
    /// Shortcuts do nothing until resumed from the tray.
    pub paused: bool,
    /// Asks GitHub about new versions now and then.
    pub check_updates: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            launch_at_login: true,
            bar: BarVisibility::Always,
            sounds: true,
            theme: Theme::System,
            microphone: None,
            paused: false,
            check_updates: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Shortcuts {
    /// Hold to dictate; double-tap for hands-free.
    pub trigger: Shortcut,
    /// Trigger+Space starts hands-free.
    pub space_for_hands_free: bool,
    pub double_tap_for_hands_free: bool,
    pub paste_last: Option<Shortcut>,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            trigger: Shortcut::CTRL_WIN,
            space_for_hands_free: true,
            double_tap_for_hands_free: true,
            paste_last: Some(Shortcut::ALT_SHIFT_Z),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Script {
    /// The language's own script, such as Devanagari for Hindi.
    #[default]
    Native,
    /// English words in Latin letters and the rest in the language's own
    /// script, such as "मेरा phone number है". Only Sarvam writes this.
    Mixed,
    /// Latin letters, such as "kal milte hain".
    Roman,
}

/// Where a spoken language is transcribed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LanguageEngine {
    /// On this PC with the active Whisper model.
    #[default]
    Local,
    /// Sarvam's API, after the user agreed to send audio there.
    Sarvam,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LanguageChoice {
    /// ISO 639-1 code, for example `hi`.
    pub code: String,
    pub engine: LanguageEngine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Languages {
    /// "I speak": the languages a dictation may be in.
    pub speak: Vec<LanguageChoice>,
    /// "Type in": the language text comes out in. `None` keeps the spoken language.
    pub output: Option<String>,
    pub script: Script,
    /// Output language for the alternate shortcut (trigger+Shift).
    pub alternate: Option<String>,
    /// Quick switch from the bar or tray. `None` detects among `speak`.
    pub current: Option<String>,
}

impl Default for Languages {
    fn default() -> Self {
        Self {
            speak: vec![LanguageChoice {
                code: "en".into(),
                engine: LanguageEngine::Local,
            }],
            output: None,
            script: Script::Native,
            alternate: None,
            current: None,
        }
    }
}

impl Languages {
    pub fn codes(&self) -> Vec<String> {
        self.speak.iter().map(|l| l.code.clone()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelSettings {
    /// Catalog id of the local model for dictation. `None` picks the best installed one.
    pub active: Option<String>,
    /// A faster model for English-only dictation, such as Parakeet.
    pub english: Option<String>,
    /// Run models on the GPU when the GPU engine is installed.
    pub use_gpu: bool,
    /// Free models from memory after this many idle minutes. 0 keeps them loaded.
    pub idle_unload_minutes: u32,
    /// Load the model at startup so the first dictation is fast.
    pub preload: bool,
    /// Whisper beam width, 1 (fastest) to 8. `None` uses the engine default.
    pub beam_size: Option<u32>,
    pub threads: Option<u32>,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            active: None,
            english: None,
            use_gpu: false,
            idle_unload_minutes: 10,
            preload: true,
            beam_size: None,
            threads: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PolishLevel {
    Off,
    /// Rule-based cleanup: fillers, stutters, spacing, capitals.
    #[default]
    Basic,
    /// One LLM pass for grammar, self-corrections, lists and style.
    Smart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Polish {
    pub level: PolishLevel,
    /// The AI model's provider for Smart polish and translating text, such as `openai`.
    pub provider: Option<String>,
    /// The model to ask. `None` takes the provider's suggestion.
    pub model: Option<String>,
    /// The address of a server the user runs, such as Ollama. `None` uses its usual one.
    pub endpoint: Option<String>,
    /// The AI model translates text when your languages need it.
    pub translate: bool,
}

impl Default for Polish {
    fn default() -> Self {
        Self {
            level: PolishLevel::Basic,
            provider: None,
            model: None,
            endpoint: None,
            translate: true,
        }
    }
}

/// A speech-to-text service the user brought a key for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Transcription {
    /// The service that hears dictations in place of the model on this PC,
    /// such as `groq`. `None` keeps them here. Languages sent to Sarvam
    /// still go there.
    pub provider: Option<String>,
    /// The model to ask. `None` takes the service's suggestion.
    pub model: Option<String>,
    /// The address of a server the user runs. `None` uses its usual one.
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Privacy {
    /// Keep a history of dictations on this PC. Insights are built from it.
    pub keep_history: bool,
    /// Delete history older than this many days. 0 keeps everything.
    pub history_days: u32,
    /// Keep recordings next to history entries. Off by default.
    pub save_audio: bool,
    /// When a cloud provider fails, transcribe locally instead.
    pub fallback_to_local: bool,
    /// Say so when that fallback happens.
    pub fallback_toast: bool,
    /// Cloud services the user agreed to send recordings or text to, such as
    /// `sarvam`. Nothing goes to a service that isn't listed.
    pub consented: Vec<String>,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            keep_history: true,
            history_days: 0,
            save_audio: false,
            fallback_to_local: true,
            fallback_toast: true,
            consented: Vec::new(),
        }
    }
}

impl Privacy {
    pub fn consented_to(&self, provider: &str) -> bool {
        self.consented.iter().any(|p| p == provider)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Experimental {
    /// Trigger+Alt rewrites selected text by voice.
    pub command_mode: bool,
    /// Saying "press enter" presses Enter after the text.
    pub press_enter: bool,
}

impl Settings {
    pub fn gesture_config(&self) -> GestureConfig {
        GestureConfig {
            trigger: self.shortcuts.trigger,
            space_for_hands_free: self.shortcuts.space_for_hands_free,
            double_tap_for_hands_free: self.shortcuts.double_tap_for_hands_free,
            command_mode: self.experimental.command_mode,
            alternate_output: self.languages.alternate.is_some(),
            paste_last: self.shortcuts.paste_last,
        }
    }

    /// Fixes values a hand-edited or older file may get wrong.
    fn sanitize(&mut self) {
        self.version = SETTINGS_VERSION;
        if !self.shortcuts.trigger.is_valid_trigger() {
            self.shortcuts.trigger = Shortcut::CTRL_WIN;
        }
        if self.languages.speak.is_empty() {
            self.languages.speak = Languages::default().speak;
        }
        let mut seen = Vec::new();
        self.languages.speak.retain(|l| {
            let fresh = !seen.contains(&l.code);
            seen.push(l.code.clone());
            fresh
        });
        if let Some(current) = &self.languages.current
            && !seen.contains(current)
        {
            self.languages.current = None;
        }
        self.privacy.consented.sort();
        self.privacy.consented.dedup();
        // Audio only goes to Sarvam after the user agreed to it, and only in
        // languages Sarvam knows.
        let sarvam = self.privacy.consented_to(crate::providers::SARVAM);
        for choice in &mut self.languages.speak {
            if choice.engine == LanguageEngine::Sarvam
                && (!sarvam || !crate::languages::find(&choice.code).is_some_and(|l| l.sarvam))
            {
                choice.engine = LanguageEngine::Local;
            }
        }
        if let Some(beam) = self.models.beam_size {
            self.models.beam_size = Some(beam.clamp(1, 8));
        }
        let polish = &mut self.polish;
        polish.provider.take_if(|p| llm::spec(p).is_none());
        polish.model = trimmed(polish.model.take());
        // Sarvam-M and Sarvam-30B were removed from the API. Drop a saved
        // model ID from an older version so the provider's current default is
        // used instead of failing every request.
        if polish.provider.as_deref() == Some(crate::providers::SARVAM)
            && matches!(polish.model.as_deref(), Some("sarvam-m" | "sarvam-30b"))
        {
            polish.model = None;
        }
        polish.endpoint = polish
            .endpoint
            .take()
            .and_then(|typed| llm::endpoint(&typed).ok());
        let transcription = &mut self.transcription;
        transcription.provider.take_if(|p| stt::spec(p).is_none());
        transcription.model = trimmed(transcription.model.take());
        transcription.endpoint = transcription
            .endpoint
            .take()
            .and_then(|typed| llm::endpoint(&typed).ok());
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

pub struct SettingsStore {
    path: PathBuf,
    tx: watch::Sender<Settings>,
}

impl SettingsStore {
    /// Loads settings, falling back to defaults. A file that can't be parsed
    /// is kept as `settings.json.bad` rather than overwritten silently.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let settings = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(mut s) => {
                    s.sanitize();
                    s
                }
                Err(e) => {
                    tracing::warn!("settings file is invalid ({e}); starting from defaults");
                    let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                    Settings::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                tracing::warn!("couldn't read settings ({e}); starting from defaults");
                Settings::default()
            }
        };
        Self {
            path,
            tx: watch::channel(settings).0,
        }
    }

    pub fn get(&self) -> Settings {
        self.tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Settings> {
        self.tx.subscribe()
    }

    /// Replaces the settings, saves them and notifies subscribers.
    pub fn set(&self, mut settings: Settings) -> anyhow::Result<Settings> {
        settings.sanitize();
        save(&self.path, &settings)?;
        self.tx.send_replace(settings.clone());
        Ok(settings)
    }

    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> anyhow::Result<Settings> {
        let mut settings = self.get();
        change(&mut settings);
        self.set(settings)
    }
}

fn save(path: &Path, settings: &Settings) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("joona-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = temp_dir("missing");
        let store = SettingsStore::load(dir.join("settings.json"));
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = temp_dir("round-trip");
        let path = dir.join("settings.json");
        let store = SettingsStore::load(&path);
        store
            .update(|s| {
                s.general.sounds = false;
                s.languages.alternate = Some("en".into());
            })
            .unwrap();
        let reloaded = SettingsStore::load(&path).get();
        assert!(!reloaded.general.sounds);
        assert_eq!(reloaded.languages.alternate.as_deref(), Some("en"));
        assert!(reloaded.gesture_config().alternate_output);
    }

    #[test]
    fn partial_and_unknown_fields_load() {
        let dir = temp_dir("partial");
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"general":{"sounds":false},"futureThing":42}"#).unwrap();
        let s = SettingsStore::load(&path).get();
        assert!(!s.general.sounds);
        assert!(
            s.general.launch_at_login,
            "other fields keep their defaults"
        );
        assert_eq!(s.shortcuts.trigger, Shortcut::CTRL_WIN);
    }

    #[test]
    fn a_broken_file_is_kept_aside() {
        let dir = temp_dir("broken");
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        let store = SettingsStore::load(&path);
        assert_eq!(store.get(), Settings::default());
        assert!(dir.join("settings.json.bad").exists());
    }

    #[test]
    fn sanitize_repairs_bad_values() {
        let mut s = Settings::default();
        s.shortcuts.trigger = Shortcut {
            ctrl: false,
            win: false,
            alt: false,
            shift: false,
            key: Some(0x41),
        };
        s.languages.speak.push(LanguageChoice {
            code: "en".into(),
            engine: LanguageEngine::Sarvam,
        });
        s.languages.current = Some("ta".into());
        s.models.beam_size = Some(40);
        s.polish.provider = Some("skynet".into());
        s.polish.model = Some("  ".into());
        s.polish.endpoint = Some(" localhost:8080 ".into());
        s.transcription.provider = Some("anthropic".into());
        s.transcription.model = Some(" nova-3 ".into());
        s.transcription.endpoint = Some("localhost:8000/v1/audio/transcriptions".into());
        s.sanitize();
        assert_eq!(s.shortcuts.trigger, Shortcut::CTRL_WIN);
        assert_eq!(s.languages.speak.len(), 1);
        assert_eq!(s.languages.current, None);
        assert_eq!(s.models.beam_size, Some(8));
        assert_eq!(s.polish.provider, None);
        assert_eq!(s.polish.model, None);
        assert_eq!(
            s.polish.endpoint.as_deref(),
            Some("http://localhost:8080/v1")
        );
        // Anthropic has no speech to text.
        assert_eq!(s.transcription.provider, None);
        assert_eq!(s.transcription.model.as_deref(), Some("nova-3"));
        assert_eq!(
            s.transcription.endpoint.as_deref(),
            Some("http://localhost:8000/v1")
        );
    }

    #[test]
    fn older_files_translate_with_the_ai_model() {
        let dir = temp_dir("polish");
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"polish":{"level":"smart","provider":"openai"}}"#).unwrap();
        let polish = SettingsStore::load(&path).get().polish;
        assert_eq!(polish.level, PolishLevel::Smart);
        assert_eq!(polish.provider.as_deref(), Some("openai"));
        assert!(polish.translate);
    }

    #[test]
    fn removed_sarvam_models_fall_back_to_the_current_default() {
        let mut s = Settings::default();
        s.polish.provider = Some(crate::providers::SARVAM.into());
        s.polish.model = Some("sarvam-m".into());
        s.sanitize();
        assert_eq!(s.polish.model, None);

        s.polish.model = Some("sarvam-30b".into());
        s.sanitize();
        assert_eq!(s.polish.model, None);

        s.polish.model = Some("sarvam-105b".into());
        s.sanitize();
        assert_eq!(s.polish.model.as_deref(), Some("sarvam-105b"));
    }

    #[test]
    fn sarvam_needs_consent_and_a_language_it_knows() {
        let mut s = Settings::default();
        s.languages.speak = [
            ("hi", LanguageEngine::Sarvam),
            ("fr", LanguageEngine::Sarvam),
        ]
        .into_iter()
        .map(|(code, engine)| LanguageChoice {
            code: code.into(),
            engine,
        })
        .collect();
        s.sanitize();
        assert!(
            s.languages
                .speak
                .iter()
                .all(|l| l.engine == LanguageEngine::Local)
        );

        s.languages.speak[0].engine = LanguageEngine::Sarvam;
        s.languages.speak[1].engine = LanguageEngine::Sarvam;
        s.privacy.consented = vec!["sarvam".into(), "sarvam".into()];
        s.sanitize();
        assert_eq!(s.privacy.consented, ["sarvam"]);
        assert_eq!(s.languages.speak[0].engine, LanguageEngine::Sarvam);
        assert_eq!(s.languages.speak[1].engine, LanguageEngine::Local);
    }
}
