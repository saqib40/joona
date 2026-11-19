//! Decides how each dictation is transcribed: which model, which spoken
//! language, whether speech is translated, and whether audio leaves the PC.

use joona_protocol::{EngineKind, ModelSpec, OpenParams, Task};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::hotkeys::Mode;
use crate::languages;
use crate::models::{InstalledModel, Library};
use crate::providers::{SARVAM, sarvam, stt};
use crate::settings::{LanguageEngine, Script, Settings};

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The languages listened for.
    pub spoken: Vec<String>,
    /// The model on this PC. It transcribes, tells which language was spoken
    /// before a service is asked, or stands in when one can't be reached.
    pub local: Option<Local>,
    pub sarvam: Option<SarvamRoute>,
    /// The speech-to-text service the user brought a key for. It hears the
    /// recordings that don't go to Sarvam.
    pub byok: Option<ByokRoute>,
    /// The language to type in, when it isn't the spoken one.
    pub output: Option<String>,
    pub script: Script,
    pub label: RouteLabel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ByokRoute {
    pub spec: &'static stt::Spec,
    pub model: String,
    pub base: String,
    /// Recordings go off this PC.
    pub cloud: bool,
    /// It turns the speech straight into the output language, English.
    pub translate: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    pub model: ModelSpec,
    pub open: OpenParams,
    pub use_gpu: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SarvamRoute {
    /// Spoken languages Sarvam turns into text.
    pub languages: Vec<String>,
    /// Spoken languages that stay on this PC. With some, the model here
    /// listens first, and Sarvam only hears recordings in one of `languages`.
    pub kept: Vec<String>,
    pub mode: sarvam::Mode,
}

impl SarvamRoute {
    /// Sarvam hears every recording.
    pub fn direct(&self) -> bool {
        self.kept.is_empty()
    }
}

/// Who hears a recording the model here doesn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listener {
    Sarvam,
    /// The service the user brought a key for.
    Byok,
}

/// A recording a service should hear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    pub to: Listener,
    /// The language to tell the service. `None` lets it work that out.
    pub language: Option<String>,
}

/// What still has to happen to a transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rewrite {
    Translate {
        from: String,
        to: String,
    },
    /// Write it in Latin letters.
    Romanize {
        language: String,
    },
}

/// What the bar shows about a route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RouteLabel {
    /// For example `EN`, `Auto` or `HI → EN`.
    pub text: String,
    /// Speech is turned into another language.
    pub cross_language: bool,
    /// The cloud service audio is sent to, if any.
    pub cloud: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("No speech model is installed yet.")]
    NoModel,
    #[error("None of your installed models understands {}.", languages::name(.0))]
    Unsupported(String),
}

/// What the Languages page says about a route, without dictating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoutePreview {
    /// For example `HI → EN`.
    pub label: String,
    /// The installed model that listens, or stands in for Sarvam.
    pub model: Option<String>,
    pub translation: Translation,
    /// Spoken languages whose recordings go to Sarvam.
    pub sarvam: Vec<String>,
    /// The cloud service that hears every recording, if one does.
    pub cloud: Option<String>,
    /// The speech-to-text service the user brought a key for, when it hears
    /// recordings in place of the model here.
    pub service: Option<String>,
    /// Who translates the text afterwards, when someone can.
    pub translator: Option<String>,
    /// Why dictating this way doesn't work yet.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Translation {
    /// The text stays in the spoken language.
    None,
    /// The model or Sarvam turns speech straight into English.
    Speech,
    /// The transcript is translated afterwards, which needs a provider.
    Text,
}

pub fn route(settings: &Settings, mode: Mode, library: &Library) -> Result<Route, RouteError> {
    let plan = plan(settings, mode);
    let installed = library.installed();
    let to_english = plan.output.as_deref() == Some("en");
    let listen = |languages: &[String]| local_route(settings, &installed, languages, to_english);
    let byok = stt::chosen(settings)
        .and_then(Result::ok)
        .map(|chosen| ByokRoute {
            translate: to_english && chosen.spec.translates(&chosen.model),
            spec: chosen.spec,
            model: chosen.model,
            base: chosen.base,
            cloud: chosen.cloud,
        });
    let make = |spoken: &[String],
                local: Option<Local>,
                sarvam: Option<SarvamRoute>,
                byok: Option<ByokRoute>| {
        let cloud = match (&sarvam, &byok) {
            (Some(s), _) if s.direct() => Some(sarvam::NAME.to_owned()),
            (None, Some(b)) if b.cloud => Some(b.spec.name.to_owned()),
            _ => None,
        };
        Route {
            label: RouteLabel {
                text: label_text(mode, spoken, plan.output.as_deref()),
                cross_language: plan.output.is_some(),
                cloud,
            },
            spoken: spoken.to_vec(),
            local,
            sarvam,
            byok,
            output: plan.output.clone(),
            script: settings.languages.script,
        }
    };

    if plan.sarvam.is_empty() {
        // The service hears everything; the model here, if any, stands in.
        if byok.is_some() {
            return Ok(make(&plan.local, listen(&plan.local).ok(), None, byok));
        }
        return Ok(make(&plan.local, Some(listen(&plan.local)?), None, None));
    }
    let sarvam = SarvamRoute {
        languages: plan.sarvam.clone(),
        kept: plan.local.clone(),
        mode: sarvam_mode(plan.output.as_deref(), settings.languages.script),
    };
    if plan.local.is_empty() {
        let known: Vec<String> = plan
            .sarvam
            .iter()
            .filter(|l| languages::whisper_knows(l))
            .cloned()
            .collect();
        let stand_in = match known.is_empty() {
            true => None,
            false => listen(&known).ok(),
        };
        return Ok(make(&plan.sarvam, stand_in, Some(sarvam), None));
    }
    // The model here listens first to tell the languages apart. It hears
    // a language it doesn't know as that language's stand-in.
    let mut candidates = plan.local.clone();
    for language in &plan.sarvam {
        let heard_as = match languages::whisper_knows(language) {
            true => Some(language.as_str()),
            false => languages::stand_in(language),
        };
        if let Some(heard_as) = heard_as
            && !candidates.iter().any(|c| c == heard_as)
        {
            candidates.push(heard_as.to_owned());
        }
    }
    match listen(&candidates) {
        Ok(local) => Ok(make(&plan.spoken, Some(local), Some(sarvam), byok)),
        // No model here tells these languages apart. The ones that stay here
        // still work, and picking a language on the bar reaches Sarvam.
        Err(_) if byok.is_some() => Ok(make(&plan.local, listen(&plan.local).ok(), None, byok)),
        Err(_) => Ok(make(&plan.local, Some(listen(&plan.local)?), None, None)),
    }
}

impl Route {
    /// Whether the model here listens while the user speaks.
    pub fn listens_locally(&self) -> bool {
        self.local.is_some() && self.direct().is_none()
    }

    /// The service that hears every recording, if one does.
    pub fn direct(&self) -> Option<Listener> {
        match (&self.sarvam, &self.byok) {
            (Some(sarvam), _) if sarvam.direct() => Some(Listener::Sarvam),
            (None, Some(_)) => Some(Listener::Byok),
            _ => None,
        }
    }

    /// Which service should hear a recording that the model here heard as
    /// `detected`, if one should. On a direct route it hears every recording.
    pub fn handoff(&self, detected: Option<&str>) -> Option<Handoff> {
        let to_byok = |language: Option<&str>| {
            self.byok.as_ref().map(|_| Handoff {
                to: Listener::Byok,
                language: language.map(str::to_owned),
            })
        };
        let Some(sarvam) = self.sarvam.as_ref() else {
            let only = match self.spoken.as_slice() {
                [only] => Some(only.as_str()),
                _ => None,
            };
            return to_byok(only);
        };
        let sarvam_hears = |language: Option<String>| Handoff {
            to: Listener::Sarvam,
            language,
        };
        if sarvam.direct() {
            let language = match sarvam.languages.as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            };
            return Some(sarvam_hears(language));
        }
        let detected = detected?;
        if sarvam.kept.iter().any(|l| l == detected) {
            return to_byok(Some(detected));
        }
        let meant: Vec<&String> = sarvam
            .languages
            .iter()
            .filter(|l| *l == detected || languages::stand_in(l) == Some(detected))
            .collect();
        match meant.as_slice() {
            [] => to_byok(None),
            [only] => Some(sarvam_hears(Some((*only).clone()))),
            // Whisper can't tell these apart, but Sarvam can.
            _ => Some(sarvam_hears(None)),
        }
    }

    /// What still has to happen to a transcript in `language`. `in_output`
    /// means the speech was already turned into the output language, and
    /// `romanized` that the text is already in Latin letters.
    pub fn rewrite(
        &self,
        language: Option<&str>,
        in_output: bool,
        romanized: bool,
    ) -> Option<Rewrite> {
        let language = language?;
        if let Some(to) = &self.output
            && !in_output
            && to != language
        {
            return Some(Rewrite::Translate {
                from: language.to_owned(),
                to: to.clone(),
            });
        }
        let now = match in_output {
            true => self.output.as_deref().unwrap_or(language),
            false => language,
        };
        let latin = languages::find(now).is_none_or(|l| l.latin);
        (self.script == Script::Roman && !romanized && !latin).then(|| Rewrite::Romanize {
            language: now.to_owned(),
        })
    }
}

/// The model to preload at startup: the one the default route listens with.
pub fn default_model(settings: &Settings, library: &Library) -> Option<ModelSpec> {
    route(settings, Mode::Dictate, library)
        .ok()
        .filter(Route::listens_locally)
        .and_then(|r| r.local)
        .map(|l| l.model)
}

/// Who can translate the text once it's written down.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Translators {
    /// A Sarvam key is saved.
    pub sarvam_key: bool,
    /// The AI model's provider, when one is ready and may translate.
    pub ai: Option<String>,
    /// The AI model goes first, because Smart polish translates as it polishes.
    pub ai_first: bool,
}

/// How dictating would go, for the Languages page.
pub fn preview(
    settings: &Settings,
    mode: Mode,
    library: &Library,
    translators: &Translators,
) -> RoutePreview {
    let route = match route(settings, mode, library) {
        Ok(route) => route,
        Err(e) => {
            return RoutePreview {
                label: label(settings, mode),
                model: None,
                translation: Translation::None,
                sarvam: Vec::new(),
                cloud: None,
                service: None,
                translator: None,
                problem: Some(e.to_string()),
            };
        }
    };
    let sarvam_translates_speech = route
        .sarvam
        .as_ref()
        .is_some_and(|s| s.mode == sarvam::Mode::Translate);
    let local_translates = route
        .local
        .as_ref()
        .is_some_and(|l| l.open.task == Task::Translate);
    let translation = match (&route.output, route.direct(), &route.byok) {
        (None, _, _) => Translation::None,
        (Some(_), Some(Listener::Sarvam), _) if sarvam_translates_speech => Translation::Speech,
        (Some(_), Some(Listener::Byok) | None, Some(byok)) if byok.translate => Translation::Speech,
        (Some(_), None, None) if local_translates => Translation::Speech,
        _ => Translation::Text,
    };
    let sarvam_translates = route.output.as_deref().is_some_and(|to| {
        route
            .spoken
            .iter()
            .all(|from| from == to || sarvam::can_translate(from, to))
    });
    let sarvam =
        (translators.sarvam_key && settings.privacy.consented_to(SARVAM) && sarvam_translates)
            .then(|| sarvam::NAME.to_owned());
    let translator = match (translation, translators.ai_first) {
        (Translation::Text, true) => translators.ai.clone().or(sarvam),
        (Translation::Text, false) => sarvam.or_else(|| translators.ai.clone()),
        _ => None,
    };
    RoutePreview {
        label: route.label.text,
        model: route.local.map(|l| l.open.model),
        translation,
        sarvam: route.sarvam.map(|s| s.languages).unwrap_or_default(),
        cloud: route.label.cloud,
        service: route.byok.map(|b| b.spec.name.to_owned()),
        translator,
        problem: None,
    }
}

/// What the bar shows next to its language picker, such as `HI → EN`.
pub fn label(settings: &Settings, mode: Mode) -> String {
    let plan = plan(settings, mode);
    label_text(mode, &plan.spoken, plan.output.as_deref())
}

/// Who turns each spoken language into text, before looking at the models.
struct Plan {
    /// Every language listened for, in the user's order.
    spoken: Vec<String>,
    /// The ones Sarvam hears.
    sarvam: Vec<String>,
    /// The ones that stay on this PC.
    local: Vec<String>,
    /// The language to type in, when it isn't the spoken one.
    output: Option<String>,
}

fn plan(settings: &Settings, mode: Mode) -> Plan {
    let languages = &settings.languages;
    let listen = match &languages.current {
        Some(code) => vec![code.clone()],
        None => languages.codes(),
    };
    let consented = settings.privacy.consented_to(SARVAM);
    let to_sarvam = |code: &String| {
        consented
            && sarvam::code(code).is_some()
            && languages
                .speak
                .iter()
                .any(|l| l.code == *code && l.engine == LanguageEngine::Sarvam)
    };
    let sarvam: Vec<String> = listen.iter().filter(|c| to_sarvam(c)).cloned().collect();
    let mut local: Vec<String> = listen.iter().filter(|c| !to_sarvam(c)).cloned().collect();
    // A language the models here don't know can't hold up the others.
    if !sarvam.is_empty() || local.iter().any(|c| languages::whisper_knows(c)) {
        local.retain(|c| languages::whisper_knows(c));
    }
    let spoken: Vec<String> = listen
        .into_iter()
        .filter(|c| sarvam.contains(c) || local.contains(c))
        .collect();
    let output = match mode {
        Mode::Dictate => languages.output.clone(),
        Mode::Alternate => languages
            .alternate
            .clone()
            .or_else(|| languages.output.clone()),
        Mode::Command => None,
    }
    .filter(|out| spoken.len() != 1 || spoken[0] != *out);
    Plan {
        spoken,
        sarvam,
        local,
        output,
    }
}

fn sarvam_mode(output: Option<&str>, script: Script) -> sarvam::Mode {
    match (output, script) {
        (Some("en"), _) => sarvam::Mode::Translate,
        // Transcribed here, then translated into the output language.
        (Some(_), _) | (None, Script::Native) => sarvam::Mode::Transcribe,
        (None, Script::Mixed) => sarvam::Mode::Codemix,
        (None, Script::Roman) => sarvam::Mode::Translit,
    }
}

fn local_route(
    settings: &Settings,
    installed: &[InstalledModel],
    listen: &[String],
    to_english: bool,
) -> Result<Local, RouteError> {
    let model = pick_model(settings, installed, listen, to_english)?;
    let task = match to_english && model.manifest.translate {
        true => Task::Translate,
        false => Task::Transcribe,
    };
    let (language, candidates) = match listen {
        [only] => (Some(only.clone()), Vec::new()),
        _ if !model.manifest.multilingual => (Some("en".to_owned()), Vec::new()),
        many => (None, many.to_vec()),
    };
    Ok(Local {
        open: OpenParams {
            model: model.id().to_owned(),
            task,
            language,
            candidates,
            prompt: None,
            vad: true,
        },
        model: model.spec(),
        use_gpu: settings.models.use_gpu,
    })
}

fn pick_model<'a>(
    settings: &Settings,
    installed: &'a [InstalledModel],
    sources: &[String],
    to_english: bool,
) -> Result<&'a InstalledModel, RouteError> {
    if installed.is_empty() {
        return Err(RouteError::NoModel);
    }
    let able: Vec<&InstalledModel> = installed
        .iter()
        .filter(|m| sources.iter().all(|s| m.speaks(s)))
        .collect();
    let chosen = |id: &Option<String>| {
        let id = id.as_deref()?;
        able.iter().copied().find(|m| m.id() == id)
    };
    // Without a choice, the largest model is usually the most accurate one
    // this PC was willing to download.
    let largest = |fits: fn(&InstalledModel) -> bool| {
        able.iter()
            .copied()
            .filter(|m| fits(m))
            .max_by_key(|m| m.manifest.bytes)
    };

    let picked = if sources.iter().all(|s| s == "en") {
        chosen(&settings.models.english)
            .or_else(|| chosen(&settings.models.active))
            // Parakeet matches large Whisper models in English and is much faster.
            .or_else(|| largest(|m| m.manifest.engine == EngineKind::Parakeet))
            .or_else(|| largest(|_| true))
    } else if to_english {
        // Whisper can turn speech into English itself, with no provider.
        chosen(&settings.models.active)
            .filter(|m| m.manifest.translate)
            .or_else(|| largest(|m| m.manifest.translate))
            .or_else(|| chosen(&settings.models.active))
            .or_else(|| largest(|_| true))
    } else {
        chosen(&settings.models.active).or_else(|| largest(|_| true))
    };
    if let Some(m) = picked {
        return Ok(m);
    }
    let missing = sources
        .iter()
        .find(|s| !installed.iter().any(|m| m.speaks(s)))
        .or(sources.first())
        .cloned()
        .unwrap_or_else(|| "en".into());
    Err(RouteError::Unsupported(missing))
}

fn label_text(mode: Mode, sources: &[String], output: Option<&str>) -> String {
    if mode == Mode::Command {
        return "Command".into();
    }
    let from = match sources {
        [only] => only.to_uppercase(),
        _ => "Auto".into(),
    };
    match output {
        Some(out) => format!("{from} → {}", out.to_uppercase()),
        None => from,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::models::Manifest;
    use crate::settings::LanguageChoice;

    /// Whisper models as (id, multilingual, can translate, bytes).
    fn library(name: &str, models: &[(&str, bool, bool, u64)]) -> Library {
        let manifests = models
            .iter()
            .map(|(id, multilingual, translate, bytes)| Manifest {
                id: (*id).into(),
                engine: EngineKind::Whisper,
                path: "model.bin".into(),
                variant: None,
                multilingual: *multilingual,
                translate: *translate,
                languages: Vec::new(),
                bytes: *bytes,
                sha256: None,
                source: None,
            });
        install(name, manifests.collect())
    }

    fn install(name: &str, manifests: Vec<Manifest>) -> Library {
        let root = std::env::temp_dir().join(format!("joona-router-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for manifest in manifests {
            let dir = root.join(&manifest.id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(crate::models::library::MANIFEST),
                serde_json::to_string(&manifest).unwrap(),
            )
            .unwrap();
        }
        Library::new(root)
    }

    fn parakeet() -> Manifest {
        Manifest {
            id: "parakeet-tdt-0.6b-v3".into(),
            engine: EngineKind::Parakeet,
            path: ".".into(),
            variant: Some("int8".into()),
            multilingual: true,
            translate: false,
            languages: vec!["en".into(), "de".into()],
            bytes: 640,
            sha256: None,
            source: None,
        }
    }

    fn whisper(id: &str, translate: bool, bytes: u64) -> Manifest {
        Manifest {
            id: id.into(),
            engine: EngineKind::Whisper,
            path: "model.bin".into(),
            variant: None,
            multilingual: true,
            translate,
            languages: Vec::new(),
            bytes,
            sha256: None,
            source: None,
        }
    }

    fn speak(codes: &[&str]) -> Vec<LanguageChoice> {
        codes
            .iter()
            .map(|c| LanguageChoice {
                code: (*c).into(),
                engine: LanguageEngine::Local,
            })
            .collect()
    }

    /// Languages as (code, sent to Sarvam), with Sarvam agreed to.
    fn speak_with_sarvam(s: &mut Settings, languages: &[(&str, bool)]) {
        s.privacy.consented = vec![SARVAM.into()];
        s.languages.speak = languages
            .iter()
            .map(|(code, sarvam)| LanguageChoice {
                code: (*code).into(),
                engine: match sarvam {
                    true => LanguageEngine::Sarvam,
                    false => LanguageEngine::Local,
                },
            })
            .collect();
    }

    fn local(route: &Route) -> &Local {
        route.local.as_ref().expect("a local model")
    }

    fn handoff(language: Option<&str>) -> Option<Handoff> {
        Some(Handoff {
            to: Listener::Sarvam,
            language: language.map(str::to_owned),
        })
    }

    fn to_byok(language: Option<&str>) -> Option<Handoff> {
        Some(Handoff {
            to: Listener::Byok,
            language: language.map(str::to_owned),
        })
    }

    /// Recordings go to `provider`, which the user agreed to.
    fn use_service(s: &mut Settings, provider: &str) {
        s.transcription.provider = Some(provider.into());
        s.privacy.consented.push(stt::consent(provider));
    }

    #[test]
    fn english_dictation_uses_the_model_as_is() {
        let lib = library("english", &[("whisper-base.en", false, false, 100)]);
        let route = route(&Settings::default(), Mode::Dictate, &lib).unwrap();
        assert_eq!(local(&route).open.task, Task::Transcribe);
        assert_eq!(local(&route).open.language.as_deref(), Some("en"));
        assert_eq!(route.label.text, "EN");
        assert!(!route.label.cross_language);
        assert!(route.listens_locally());
        assert_eq!(route.handoff(Some("en")), None);
        assert_eq!(
            local(&route).model.path,
            PathBuf::from(lib.root())
                .join("whisper-base.en")
                .join("model.bin")
        );
    }

    #[test]
    fn speaking_hindi_to_type_english_uses_whisper_translate() {
        let lib = library(
            "hi-en",
            &[
                ("whisper-base.en", false, false, 100),
                ("whisper-small", true, true, 400),
            ],
        );
        let mut s = Settings::default();
        s.languages.speak = speak(&["hi"]);
        s.languages.output = Some("en".into());
        let route = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(local(&route).open.model, "whisper-small");
        assert_eq!(local(&route).open.task, Task::Translate);
        assert_eq!(local(&route).open.language.as_deref(), Some("hi"));
        assert_eq!(route.rewrite(Some("hi"), true, false), None);
        assert_eq!(route.label.text, "HI → EN");
    }

    #[test]
    fn several_languages_detect_among_them() {
        let lib = library("multi", &[("whisper-small", true, true, 400)]);
        let mut s = Settings::default();
        s.languages.speak = speak(&["hi", "en"]);
        let route = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(local(&route).open.language, None);
        assert_eq!(local(&route).open.candidates, ["hi", "en"]);
        assert_eq!(route.label.text, "Auto");

        s.languages.current = Some("hi".into());
        let picked = super::route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(local(&picked).open.language.as_deref(), Some("hi"));
        assert!(local(&picked).open.candidates.is_empty());
    }

    #[test]
    fn turbo_cannot_translate_so_the_text_is_translated_afterwards() {
        let lib = library("turbo", &[("whisper-large-v3-turbo", true, false, 1600)]);
        let mut s = Settings::default();
        s.languages.speak = speak(&["ta"]);
        s.languages.output = Some("en".into());
        let route = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(local(&route).open.task, Task::Transcribe);
        assert_eq!(
            route.rewrite(Some("ta"), false, false),
            Some(Rewrite::Translate {
                from: "ta".into(),
                to: "en".into()
            })
        );
    }

    #[test]
    fn alternate_output_and_command_mode() {
        let lib = library("alternate", &[("whisper-small", true, true, 400)]);
        let mut s = Settings::default();
        s.languages.speak = speak(&["hi"]);
        s.languages.alternate = Some("en".into());
        let task = |mode| local(&route(&s, mode, &lib).unwrap()).open.task;
        assert_eq!(task(Mode::Dictate), Task::Transcribe);
        assert_eq!(task(Mode::Alternate), Task::Translate);
        assert_eq!(
            route(&s, Mode::Command, &lib).unwrap().label.text,
            "Command"
        );
    }

    #[test]
    fn explicit_choices_win_and_missing_models_are_reported() {
        let lib = library(
            "choice",
            &[
                ("whisper-base.en", false, false, 100),
                ("whisper-small", true, true, 400),
                ("whisper-tiny", true, true, 50),
            ],
        );
        let mut s = Settings::default();
        let model = |s: &Settings, lib: &Library| {
            local(&route(s, Mode::Dictate, lib).unwrap())
                .open
                .model
                .clone()
        };
        assert_eq!(model(&s, &lib), "whisper-small");
        s.models.english = Some("whisper-base.en".into());
        assert_eq!(model(&s, &lib), "whisper-base.en");
        s.languages.speak = speak(&["hi"]);
        s.models.active = Some("whisper-tiny".into());
        assert_eq!(model(&s, &lib), "whisper-tiny");

        let english_only = library("english-only", &[("whisper-base.en", false, false, 100)]);
        assert_eq!(
            route(&s, Mode::Dictate, &english_only).unwrap_err(),
            RouteError::Unsupported("hi".into())
        );
        assert_eq!(
            route(&s, Mode::Dictate, &library("empty", &[])).unwrap_err(),
            RouteError::NoModel
        );
        assert_eq!(
            RouteError::Unsupported("hi".into()).to_string(),
            "None of your installed models understands Hindi."
        );
    }

    #[test]
    fn english_goes_to_parakeet_unless_another_model_was_chosen() {
        let lib = install(
            "parakeet",
            vec![parakeet(), whisper("whisper-medium", true, 1500)],
        );
        let mut s = Settings::default();
        let model = |s: &Settings| {
            local(&route(s, Mode::Dictate, &lib).unwrap())
                .open
                .model
                .clone()
        };
        assert_eq!(model(&s), "parakeet-tdt-0.6b-v3");

        s.languages.speak = speak(&["hi"]);
        assert_eq!(model(&s), "whisper-medium");

        s.languages.speak = speak(&["en"]);
        s.models.active = Some("whisper-medium".into());
        assert_eq!(model(&s), "whisper-medium");
    }

    #[test]
    fn typing_english_prefers_a_model_that_translates_speech() {
        let lib = install(
            "translate",
            vec![
                whisper("whisper-large-v3-turbo", false, 1600),
                whisper("whisper-small", true, 480),
            ],
        );
        let mut s = Settings::default();
        s.languages.speak = speak(&["hi"]);
        assert_eq!(
            local(&route(&s, Mode::Dictate, &lib).unwrap()).open.model,
            "whisper-large-v3-turbo"
        );

        s.languages.output = Some("en".into());
        s.models.active = Some("whisper-large-v3-turbo".into());
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(local(&r).open.model, "whisper-small");
        assert_eq!(local(&r).open.task, Task::Translate);

        let turbo_only = install(
            "turbo-only",
            vec![whisper("whisper-large-v3-turbo", false, 1600)],
        );
        let r = route(&s, Mode::Dictate, &turbo_only).unwrap();
        assert_eq!(local(&r).open.task, Task::Transcribe);
        assert!(matches!(
            r.rewrite(Some("hi"), false, false),
            Some(Rewrite::Translate { to, .. }) if to == "en"
        ));
    }

    #[test]
    fn previews_say_how_the_text_is_made() {
        let lib = install(
            "preview",
            vec![parakeet(), whisper("whisper-small", true, 480)],
        );
        let mut s = Settings::default();
        let english = preview(&s, Mode::Dictate, &lib, &Translators::default());
        assert_eq!(english.label, "EN");
        assert_eq!(english.model.as_deref(), Some("parakeet-tdt-0.6b-v3"));
        assert_eq!(english.translation, Translation::None);

        s.languages.speak = speak(&["hi", "en"]);
        s.languages.output = Some("en".into());
        let speech = preview(&s, Mode::Dictate, &lib, &Translators::default());
        assert_eq!(speech.label, "Auto → EN");
        assert_eq!(speech.translation, Translation::Speech);

        s.languages.output = Some("ta".into());
        let text = preview(&s, Mode::Dictate, &lib, &Translators::default());
        assert_eq!(text.translation, Translation::Text);
        assert_eq!(text.translator, None);

        s.languages.speak = speak(&["hi", "kok"]);
        assert_eq!(
            preview(&s, Mode::Dictate, &lib, &Translators::default()).label,
            "HI → TA"
        );

        s.languages.speak = speak(&["kok"]);
        let unsupported = preview(&s, Mode::Dictate, &lib, &Translators::default());
        assert_eq!(unsupported.label, "KOK → TA");
        assert_eq!(unsupported.model, None);
        assert_eq!(
            unsupported.problem.as_deref(),
            Some("None of your installed models understands Konkani.")
        );
    }

    #[test]
    fn sarvam_languages_go_straight_to_sarvam() {
        let lib = install("sarvam", vec![whisper("whisper-small", true, 480)]);
        let mut s = Settings::default();
        speak_with_sarvam(&mut s, &[("hi", true)]);
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        let sarvam = r.sarvam.as_ref().unwrap();
        assert!(sarvam.direct());
        assert_eq!(sarvam.mode, sarvam::Mode::Transcribe);
        assert_eq!(r.label.cloud.as_deref(), Some("Sarvam"));
        assert!(!r.listens_locally());
        assert_eq!(r.handoff(None), handoff(Some("hi")));
        // The model here only stands in.
        assert_eq!(local(&r).open.language.as_deref(), Some("hi"));
        assert_eq!(default_model(&s, &lib), None);

        let mode = |s: &Settings| route(s, Mode::Dictate, &lib).unwrap().sarvam.unwrap().mode;
        s.languages.script = Script::Mixed;
        assert_eq!(mode(&s), sarvam::Mode::Codemix);
        s.languages.script = Script::Roman;
        assert_eq!(mode(&s), sarvam::Mode::Translit);
        s.languages.output = Some("en".into());
        assert_eq!(mode(&s), sarvam::Mode::Translate);
        s.languages.output = Some("ta".into());
        assert_eq!(mode(&s), sarvam::Mode::Transcribe);
    }

    #[test]
    fn sarvam_works_without_a_model_here() {
        let mut s = Settings::default();
        speak_with_sarvam(&mut s, &[("or", true), ("hi", true)]);
        let r = route(&s, Mode::Dictate, &library("none", &[])).unwrap();
        assert_eq!(r.local, None);
        assert_eq!(r.handoff(None), handoff(None));
        assert_eq!(r.label.text, "Auto");

        // Whisper doesn't know Odia, so nothing here can stand in.
        let lib = install("odia", vec![whisper("whisper-small", true, 480)]);
        s.languages.current = Some("or".into());
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.local, None);
        assert_eq!(r.handoff(None), handoff(Some("or")));
    }

    #[test]
    fn mixed_languages_are_told_apart_here_first() {
        let lib = install("mixed", vec![whisper("whisper-small", true, 480)]);
        let mut s = Settings::default();
        speak_with_sarvam(&mut s, &[("en", false), ("hi", true)]);
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert!(r.listens_locally());
        assert_eq!(r.label.cloud, None);
        assert_eq!(local(&r).open.candidates, ["en", "hi"]);
        assert_eq!(r.handoff(Some("en")), None);
        assert_eq!(r.handoff(Some("hi")), handoff(Some("hi")));
        assert_eq!(r.handoff(None), None);

        // Whisper hears Odia as Bengali.
        speak_with_sarvam(&mut s, &[("en", false), ("or", true)]);
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(local(&r).open.candidates, ["en", "bn"]);
        assert_eq!(r.handoff(Some("bn")), handoff(Some("or")));

        // Maithili also sounds like Hindi to Whisper, so Sarvam decides.
        speak_with_sarvam(&mut s, &[("en", false), ("hi", true), ("mai", true)]);
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(local(&r).open.candidates, ["en", "hi"]);
        assert_eq!(r.handoff(Some("hi")), handoff(None));

        // Picking Hindi on the bar sends it straight to Sarvam.
        s.languages.current = Some("hi".into());
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert!(r.sarvam.as_ref().is_some_and(SarvamRoute::direct));
    }

    #[test]
    fn without_a_model_to_tell_languages_apart_the_local_ones_still_work() {
        let lib = install("parakeet-only", vec![parakeet()]);
        let mut s = Settings::default();
        speak_with_sarvam(&mut s, &[("en", false), ("hi", true)]);
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.sarvam, None);
        assert_eq!(local(&r).open.model, "parakeet-tdt-0.6b-v3");
        assert_eq!(r.label.text, "EN");
    }

    #[test]
    fn sarvam_needs_consent() {
        let lib = install("consent", vec![whisper("whisper-small", true, 480)]);
        let mut s = Settings::default();
        speak_with_sarvam(&mut s, &[("hi", true)]);
        s.privacy.consented.clear();
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.sarvam, None);
        assert!(r.listens_locally());
    }

    #[test]
    fn transcripts_are_translated_or_romanized_afterwards() {
        let lib = install("rewrite", vec![whisper("whisper-small", true, 480)]);
        let mut s = Settings::default();
        s.languages.speak = speak(&["en", "hi"]);
        s.languages.output = Some("ta".into());
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(
            r.rewrite(Some("en"), false, false),
            Some(Rewrite::Translate {
                from: "en".into(),
                to: "ta".into()
            })
        );
        assert_eq!(r.rewrite(Some("ta"), false, false), None);
        assert_eq!(r.rewrite(None, false, false), None);

        s.languages.output = None;
        s.languages.script = Script::Roman;
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(
            r.rewrite(Some("hi"), false, false),
            Some(Rewrite::Romanize {
                language: "hi".into()
            })
        );
        assert_eq!(r.rewrite(Some("hi"), false, true), None);
        assert_eq!(r.rewrite(Some("en"), false, false), None);

        s.languages.script = Script::Mixed;
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.rewrite(Some("hi"), false, false), None);
    }

    #[test]
    fn previews_mention_sarvam() {
        let lib = install("preview-sarvam", vec![whisper("whisper-small", true, 480)]);
        let mut s = Settings::default();
        let sarvam = Translators {
            sarvam_key: true,
            ..Translators::default()
        };
        speak_with_sarvam(&mut s, &[("hi", true)]);
        s.languages.output = Some("en".into());
        let p = preview(&s, Mode::Dictate, &lib, &sarvam);
        assert_eq!(p.translation, Translation::Speech);
        assert_eq!(p.sarvam, ["hi"]);
        assert_eq!(p.cloud.as_deref(), Some("Sarvam"));
        assert_eq!(p.model.as_deref(), Some("whisper-small"));

        speak_with_sarvam(&mut s, &[("en", false)]);
        s.languages.output = Some("ta".into());
        let p = preview(&s, Mode::Dictate, &lib, &sarvam);
        assert_eq!(p.translation, Translation::Text);
        assert_eq!(p.translator.as_deref(), Some("Sarvam"));
        assert_eq!(
            preview(&s, Mode::Dictate, &lib, &Translators::default()).translator,
            None
        );

        s.languages.output = Some("fr".into());
        assert_eq!(preview(&s, Mode::Dictate, &lib, &sarvam).translator, None);
    }

    #[test]
    fn an_ai_model_translates_what_sarvam_cant() {
        let lib = install("preview-ai", vec![whisper("whisper-small", true, 480)]);
        let mut s = Settings::default();
        speak_with_sarvam(&mut s, &[("en", false)]);
        s.languages.output = Some("ta".into());
        let mut both = Translators {
            sarvam_key: true,
            ai: Some("OpenAI".into()),
            ai_first: false,
        };
        let translator =
            |s: &Settings, t: &Translators| preview(s, Mode::Dictate, &lib, t).translator;
        assert_eq!(translator(&s, &both).as_deref(), Some("Sarvam"));
        // Smart polish translates as it polishes.
        both.ai_first = true;
        assert_eq!(translator(&s, &both).as_deref(), Some("OpenAI"));
        both.ai_first = false;
        s.languages.output = Some("fr".into());
        assert_eq!(translator(&s, &both).as_deref(), Some("OpenAI"));
    }

    #[test]
    fn a_service_with_your_key_hears_every_recording() {
        let lib = library("byok", &[("whisper-small", true, true, 480)]);
        let mut s = Settings::default();
        use_service(&mut s, stt::GROQ);
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.direct(), Some(Listener::Byok));
        assert!(!r.listens_locally());
        assert_eq!(r.label.cloud.as_deref(), Some("Groq"));
        assert_eq!(r.handoff(None), to_byok(Some("en")));
        // The model here stands in when Groq can't be reached.
        assert_eq!(local(&r).open.model, "whisper-small");
        assert_eq!(default_model(&s, &lib), None, "nothing to preload");

        // Without permission, recordings stay here.
        s.privacy.consented.clear();
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.byok, None);
        assert!(r.listens_locally());
    }

    #[test]
    fn a_service_works_without_a_model_here() {
        let lib = library("byok-alone", &[]);
        let mut s = Settings::default();
        s.languages.speak = speak(&["en", "hi"]);
        assert_eq!(route(&s, Mode::Dictate, &lib), Err(RouteError::NoModel));

        use_service(&mut s, stt::DEEPGRAM);
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.local, None);
        assert_eq!(r.handoff(None), to_byok(None));
        assert_eq!(r.label.text, "Auto");
    }

    #[test]
    fn a_service_can_turn_speech_into_english() {
        let lib = library("byok-english", &[("whisper-small", true, true, 480)]);
        let mut s = Settings::default();
        s.languages.speak = speak(&["hi"]);
        s.languages.output = Some("en".into());
        use_service(&mut s, stt::GROQ);
        let translates = |s: &Settings| {
            route(s, Mode::Dictate, &lib)
                .unwrap()
                .byok
                .is_some_and(|b| b.translate)
        };
        // Turbo only transcribes, so the text is translated afterwards.
        assert!(!translates(&s));
        let p = preview(&s, Mode::Dictate, &lib, &Translators::default());
        assert_eq!(p.translation, Translation::Text);
        assert_eq!(p.service.as_deref(), Some("Groq"));

        s.transcription.model = Some("whisper-large-v3".into());
        assert!(translates(&s));
        let p = preview(&s, Mode::Dictate, &lib, &Translators::default());
        assert_eq!(p.translation, Translation::Speech);
        assert_eq!(p.cloud.as_deref(), Some("Groq"));
    }

    #[test]
    fn sarvam_keeps_its_languages_and_the_service_hears_the_rest() {
        let lib = library("byok-sarvam", &[("whisper-small", true, true, 480)]);
        let mut s = Settings::default();
        speak_with_sarvam(&mut s, &[("en", false), ("ta", true)]);
        use_service(&mut s, stt::ELEVENLABS);
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.direct(), None);
        assert!(r.listens_locally(), "the model here tells them apart");
        assert_eq!(r.label.cloud, None);
        assert_eq!(r.handoff(Some("ta")), handoff(Some("ta")));
        assert_eq!(r.handoff(Some("en")), to_byok(Some("en")));

        // Only Sarvam's languages: the service isn't needed.
        speak_with_sarvam(&mut s, &[("ta", true)]);
        s.privacy.consented.push(stt::consent(stt::ELEVENLABS));
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.direct(), Some(Listener::Sarvam));
        assert_eq!(r.byok, None);
    }

    #[test]
    fn your_own_speech_server_needs_no_permission() {
        let lib = library("byok-own", &[("whisper-small", true, true, 480)]);
        let mut s = Settings::default();
        s.transcription.provider = Some(stt::CUSTOM.into());
        s.transcription.endpoint = Some("http://localhost:8000/v1".into());
        s.transcription.model = Some("Systran/faster-whisper-small".into());
        let r = route(&s, Mode::Dictate, &lib).unwrap();
        assert_eq!(r.direct(), Some(Listener::Byok));
        assert_eq!(r.label.cloud, None, "no cloud badge for this PC");
    }
}
