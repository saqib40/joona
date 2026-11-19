//! Suggests a model for the languages the user speaks and the PC they have.

use serde::Serialize;
use specta::Type;

use super::catalog::{Catalog, CatalogModel};
use super::hardware::Hardware;
use crate::settings::{LanguageEngine, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    /// The suggestion for this PC and these languages.
    Recommended,
    Good,
    /// Runs, but slowly on this PC.
    Slow,
    /// Needs more memory than this PC can spare.
    TooBig,
    /// Doesn't understand one of the languages.
    WrongLanguages,
}

/// What the user needs from a local model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Need {
    /// Languages transcribed on this PC. Those sent to Sarvam don't count.
    pub languages: Vec<String>,
    /// Speech in another language should come out as English.
    pub translate: bool,
}

impl Need {
    pub fn from_settings(s: &Settings) -> Self {
        let languages: Vec<String> = s
            .languages
            .speak
            .iter()
            .filter(|l| l.engine == LanguageEngine::Local)
            .map(|l| l.code.clone())
            .collect();
        let translate =
            s.languages.output.as_deref() == Some("en") && languages.iter().any(|l| l != "en");
        Self {
            languages,
            translate,
        }
    }

    fn english_only(&self) -> bool {
        self.languages.iter().all(|l| l == "en")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Low,
    Mid,
    High,
}

fn tier(hw: &Hardware) -> Tier {
    let gpu = hw.best_gpu().is_some_and(|g| g.vram_mb >= 4096);
    if hw.ram_mb < 6 * 1024 || hw.threads < 4 || !hw.avx2 {
        Tier::Low
    } else if hw.ram_mb >= 15 * 1024 && (hw.threads >= 8 || gpu) {
        // "16 GB" PCs report a little under 16 GB.
        Tier::High
    } else {
        Tier::Mid
    }
}

/// How well a model suits this PC and these languages, before picking one.
pub fn fit(model: &CatalogModel, hw: &Hardware, need: &Need) -> Fit {
    if !need.languages.iter().all(|l| model.speaks(l)) {
        return Fit::WrongLanguages;
    }
    // Leave room for the apps the user is dictating into.
    if hw.ram_mb > 0 && u64::from(model.ram_mb) * 10 > hw.ram_mb * 6 {
        return Fit::TooBig;
    }
    match tier(hw) {
        Tier::Low if model.speed <= 2 => Fit::Slow,
        Tier::Mid if model.speed <= 1 => Fit::Slow,
        _ => Fit::Good,
    }
}

/// The model to suggest, in order of preference for each kind of user and PC.
pub fn recommend<'a>(catalog: &'a Catalog, hw: &Hardware, need: &Need) -> Option<&'a CatalogModel> {
    let tier = tier(hw);
    let preferred: &[&str] = match (need.english_only(), need.translate, tier) {
        (true, _, Tier::Low) => &["moonshine-base", "whisper-base", "whisper-tiny"],
        (true, _, _) => &["parakeet-tdt-0.6b-v3", "moonshine-base", "whisper-small"],
        (false, true, Tier::High) => &["whisper-large-v3-q5_0", "whisper-small", "whisper-base"],
        (false, true, Tier::Mid) => &["whisper-small", "whisper-base"],
        (false, true, Tier::Low) => &["whisper-base", "whisper-tiny"],
        // Parakeet only qualifies when it knows every language, which rules it out for Indian ones.
        (false, false, Tier::High) => &[
            "parakeet-tdt-0.6b-v3",
            "whisper-large-v3-turbo-q5_0",
            "whisper-small",
        ],
        (false, false, Tier::Mid) => &["parakeet-tdt-0.6b-v3", "whisper-small", "whisper-base"],
        (false, false, Tier::Low) => &["whisper-base", "whisper-tiny"],
    };
    let usable = |m: &&CatalogModel| {
        matches!(fit(m, hw, need), Fit::Good | Fit::Slow) && (!need.translate || m.translate)
    };
    preferred
        .iter()
        .filter_map(|id| catalog.get(id))
        .find(usable)
        .or_else(|| {
            catalog
                .models
                .iter()
                .filter(usable)
                .max_by_key(|m| (fit(m, hw, need) == Fit::Good, m.accuracy, m.speed))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::catalog::catalog;

    fn pc(ram_gb: u64, threads: u32) -> Hardware {
        Hardware {
            cpu: "Test CPU".into(),
            cores: threads / 2,
            threads,
            ram_mb: ram_gb * 1024 - 300,
            available_ram_mb: ram_gb * 512,
            avx2: true,
            gpus: Vec::new(),
        }
    }

    fn need(languages: &[&str], translate: bool) -> Need {
        Need {
            languages: languages.iter().map(|l| (*l).to_owned()).collect(),
            translate,
        }
    }

    fn pick(hw: &Hardware, need: &Need) -> Option<String> {
        recommend(catalog(), hw, need).map(|m| m.id.clone())
    }

    #[test]
    fn english_speakers_get_parakeet_or_moonshine() {
        assert_eq!(
            pick(&pc(16, 8), &need(&["en"], false)).as_deref(),
            Some("parakeet-tdt-0.6b-v3")
        );
        assert_eq!(
            pick(&pc(4, 4), &need(&["en"], false)).as_deref(),
            Some("moonshine-base")
        );
    }

    #[test]
    fn translating_needs_a_model_that_can() {
        assert_eq!(
            pick(&pc(16, 12), &need(&["hi"], true)).as_deref(),
            Some("whisper-large-v3-q5_0")
        );
        assert_eq!(
            pick(&pc(8, 8), &need(&["hi", "en"], true)).as_deref(),
            Some("whisper-small")
        );
        assert!(recommend(catalog(), &pc(8, 8), &need(&["ta"], true)).is_some_and(|m| m.translate));
    }

    #[test]
    fn indian_languages_skip_parakeet() {
        assert_eq!(
            pick(&pc(16, 8), &need(&["hi", "en"], false)).as_deref(),
            Some("whisper-large-v3-turbo-q5_0")
        );
        assert_eq!(
            pick(&pc(8, 4), &need(&["de", "en"], false)).as_deref(),
            Some("parakeet-tdt-0.6b-v3")
        );
    }

    #[test]
    fn languages_whisper_lacks_have_no_local_model() {
        assert_eq!(pick(&pc(16, 8), &need(&["kok"], false)), None);
        let konkani = need(&["kok"], false);
        assert!(
            catalog()
                .models
                .iter()
                .all(|m| fit(m, &pc(16, 8), &konkani) == Fit::WrongLanguages)
        );
    }

    #[test]
    fn big_models_dont_fit_small_pcs() {
        let large = catalog().get("whisper-large-v3-q5_0").unwrap();
        assert_eq!(fit(large, &pc(2, 4), &need(&["hi"], false)), Fit::TooBig);
    }

    #[test]
    fn sarvam_languages_dont_need_a_local_model() {
        let mut s = Settings::default();
        s.languages.speak = vec![
            crate::settings::LanguageChoice {
                code: "en".into(),
                engine: LanguageEngine::Local,
            },
            crate::settings::LanguageChoice {
                code: "kok".into(),
                engine: LanguageEngine::Sarvam,
            },
        ];
        s.languages.output = Some("en".into());
        assert_eq!(Need::from_settings(&s), need(&["en"], false));
    }
}
