//! What the Models page and onboarding show: catalog and installed models
//! side by side, rated for this PC and the user's languages.

use joona_protocol::EngineKind;
use serde::Serialize;
use specta::Type;

use super::catalog::{Catalog, Engine};
use super::download::Progress;
use super::hardware::Hardware;
use super::library::InstalledModel;
use super::recommend::{self, Fit, Need};
use crate::settings::Settings;
use crate::supervisor::Variant;

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelCard {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    pub summary: String,
    /// ISO 639-1 codes. Empty means every language the engine knows.
    pub languages: Vec<String>,
    pub multilingual: bool,
    pub translate: bool,
    /// 1 (slowest) to 5 (fastest). 0 for a model that isn't in the catalog.
    pub speed: u8,
    /// 1 to 5. 0 for a model that isn't in the catalog.
    pub accuracy: u8,
    /// On disk, once installed.
    pub size: u64,
    pub download_size: u64,
    pub ram_mb: u32,
    pub license: String,
    pub credit: String,
    pub installed: bool,
    /// Copied into the models folder by hand rather than downloaded.
    pub custom: bool,
    pub fit: Fit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub running: bool,
    pub variant: Variant,
    /// The GPU build is installed, so "Use the GPU" can work.
    pub gpu_build: bool,
    /// The graphics card the running engine uses.
    pub gpu_name: Option<String>,
    /// Models in memory now.
    pub loaded: Vec<String>,
    /// Silero VAD is installed, so silence is trimmed before transcribing.
    pub vad: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelsOverview {
    pub models: Vec<ModelCard>,
    /// The catalog model to suggest for this PC and these languages.
    pub recommended: Option<String>,
    /// The installed model the next dictation will use.
    pub current: Option<String>,
    pub downloads: Vec<Progress>,
    pub hardware: Hardware,
    pub engine: EngineState,
}

impl Engine {
    pub fn from_kind(kind: EngineKind) -> Self {
        match kind {
            EngineKind::Whisper => Engine::Whisper,
            EngineKind::Parakeet => Engine::Parakeet,
            EngineKind::Moonshine => Engine::Moonshine,
        }
    }
}

/// Every catalog model, then the installed models the catalog doesn't know,
/// and the id of the one to recommend. Graphics cards only count when `gpu`
/// says the GPU engine is the one running models.
pub fn cards(
    catalog: &Catalog,
    installed: &[InstalledModel],
    hardware: &Hardware,
    settings: &Settings,
    gpu: bool,
) -> (Vec<ModelCard>, Option<String>) {
    let need = Need::from_settings(settings);
    let mut hardware = hardware.clone();
    if !gpu {
        hardware.gpus.clear();
    }
    let hardware = &hardware;
    let recommended = recommend::recommend(catalog, hardware, &need).map(|m| m.id.clone());
    let mut cards: Vec<ModelCard> = catalog
        .models
        .iter()
        .map(|m| ModelCard {
            id: m.id.clone(),
            name: m.name.clone(),
            engine: m.engine,
            summary: m.summary.clone(),
            languages: m.languages.clone(),
            multilingual: m.multilingual,
            translate: m.translate,
            speed: m.speed,
            accuracy: m.accuracy,
            size: m.size,
            download_size: m.download_size(),
            ram_mb: m.ram_mb,
            license: m.license.clone(),
            credit: m.credit.clone(),
            installed: installed.iter().any(|i| i.id() == m.id),
            custom: false,
            fit: if recommended.as_deref() == Some(&m.id) {
                Fit::Recommended
            } else {
                recommend::fit(m, hardware, &need)
            },
        })
        .collect();
    for model in installed.iter().filter(|i| catalog.get(i.id()).is_none()) {
        let m = &model.manifest;
        cards.push(ModelCard {
            id: m.id.clone(),
            name: m.id.clone(),
            engine: Engine::from_kind(m.engine),
            summary: "Added to the models folder by hand.".into(),
            languages: m.languages.clone(),
            multilingual: m.multilingual,
            translate: m.translate,
            speed: 0,
            accuracy: 0,
            size: m.bytes,
            download_size: 0,
            ram_mb: 0,
            license: String::new(),
            credit: String::new(),
            installed: true,
            custom: true,
            fit: if need.languages.iter().all(|l| model.speaks(l)) {
                Fit::Good
            } else {
                Fit::WrongLanguages
            },
        });
    }
    (cards, recommended)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Manifest;
    use crate::models::catalog::catalog;
    use crate::models::hardware::Gpu;
    use crate::settings::{LanguageChoice, LanguageEngine};

    #[test]
    fn lists_the_catalog_and_hand_copied_models() {
        let hardware = Hardware {
            cpu: "Test CPU".into(),
            cores: 4,
            threads: 8,
            ram_mb: 16 * 1024 - 300,
            available_ram_mb: 8 * 1024,
            avx2: true,
            gpus: Vec::new(),
        };
        let installed = ["whisper-small", "whisper-medium"].map(|id| InstalledModel {
            manifest: Manifest {
                id: id.into(),
                engine: EngineKind::Whisper,
                path: format!("ggml-{}.bin", id.trim_start_matches("whisper-")),
                variant: None,
                multilingual: true,
                translate: true,
                languages: Vec::new(),
                bytes: 1,
                sha256: None,
                source: None,
            },
            dir: std::path::PathBuf::from(id),
        });
        let settings = Settings::default();
        let (cards, recommended) = cards(catalog(), &installed, &hardware, &settings, false);

        assert_eq!(cards.len(), catalog().models.len() + 1);
        let small = cards.iter().find(|c| c.id == "whisper-small").unwrap();
        assert!(small.installed && !small.custom);
        let medium = cards.last().unwrap();
        assert!(medium.id == "whisper-medium" && medium.custom && medium.fit == Fit::Good);

        let recommended = recommended.expect("an English speaker on this PC gets a suggestion");
        let card = cards.iter().find(|c| c.id == recommended).unwrap();
        assert_eq!(card.fit, Fit::Recommended);
        assert_eq!(
            cards.iter().filter(|c| c.fit == Fit::Recommended).count(),
            1
        );
    }

    #[test]
    fn a_graphics_card_counts_only_while_the_gpu_engine_runs() {
        let hardware = Hardware {
            cpu: "Test CPU".into(),
            cores: 6,
            threads: 6,
            ram_mb: 16 * 1024 - 300,
            available_ram_mb: 8 * 1024,
            avx2: true,
            gpus: vec![Gpu {
                name: "Test GPU".into(),
                vram_mb: 8 * 1024,
                dedicated: true,
            }],
        };
        let mut settings = Settings::default();
        settings.languages.speak = vec![LanguageChoice {
            code: "hi".into(),
            engine: LanguageEngine::Local,
        }];
        settings.languages.output = Some("en".into());

        let pick = |gpu| cards(catalog(), &[], &hardware, &settings, gpu).1;
        assert_eq!(pick(false).as_deref(), Some("whisper-small"));
        assert_eq!(pick(true).as_deref(), Some("whisper-large-v3-q5_0"));
    }
}
