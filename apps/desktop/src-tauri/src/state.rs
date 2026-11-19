//! The app's shared state, created once at startup.

use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::ai::LocalAi;
use crate::audio::MicTest;
use crate::history::History;
use crate::hotkeys::Hotkeys;
use crate::insert::Inserter;
use crate::models::Library;
use crate::models::download::Downloads;
use crate::models::hardware::{self, Hardware};
use crate::pipeline::Rules;
use crate::providers::Keys;
use crate::session::{BarState, SessionHandle, Ui};
use crate::settings::SettingsStore;
use crate::store::Db;
use crate::supervisor::Supervisor;
use crate::updates::Updates;

pub struct AppState {
    pub settings: SettingsStore,
    pub engine: Supervisor,
    pub library: Library,
    pub downloads: Arc<Downloads>,
    /// `None` when the clipboard helper couldn't start; dictations then fail
    /// with a clear message instead of the app refusing to run.
    pub inserter: Option<Mutex<Inserter>>,
    /// `None` when the database couldn't be opened; dictation still works.
    pub db: Option<Arc<Db>>,
    pub history: Option<Arc<History>>,
    /// The dictionary, snippets and styles, ready for the text pipeline.
    pub rules: RwLock<Arc<Rules>>,
    pub hotkeys: Mutex<Option<Hotkeys>>,
    pub ui: Arc<dyn Ui>,
    pub session: SessionHandle,
    /// What the bar shows now, for a bar window that (re)loads.
    pub bar: Mutex<BarState>,
    pub hardware: OnceLock<Hardware>,
    pub mic_test: MicTest,
    /// API keys for cloud providers, kept in Windows Credential Manager.
    pub keys: Arc<Keys>,
    /// Joona's own AI model, when the user downloaded it.
    pub local_ai: Arc<LocalAi>,
    pub updates: Updates,
}

impl AppState {
    /// Detected the first time it's needed.
    pub fn hardware(&self) -> &Hardware {
        self.hardware.get_or_init(hardware::detect)
    }

    pub fn rules(&self) -> Arc<Rules> {
        self.rules.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reads the dictionary, snippets and styles again after they change.
    pub fn reload_rules(&self) -> anyhow::Result<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        let rules = Arc::new(Rules::load(db)?);
        *self.rules.write().unwrap_or_else(|e| e.into_inner()) = rules;
        Ok(())
    }
}
