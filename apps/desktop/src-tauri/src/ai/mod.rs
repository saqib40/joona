//! Smart polish, translating text and Command Mode with an AI model: the one
//! the user set up on the Providers page, or Joona's own on this PC.

pub mod command;
pub mod local;
pub mod prompt;

use std::sync::Arc;
use std::time::{Duration, Instant};

pub use self::command::Command;
pub use self::local::LocalAi;
pub use self::prompt::Request;
use crate::providers::llm::{self, KeyNeed, Llm, Prompt, Spec};
use crate::providers::{self, Keys, ProviderError};
use crate::settings::Settings;

/// Most answers take a few seconds; reasoning models take longer.
const CLOUD_TIMEOUT: Duration = Duration::from_secs(30);
/// Models on this PC may have to load first, and can be slow on a CPU.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(45);

/// The AI model the settings pick, ready to use.
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    pub spec: &'static Spec,
    pub model: String,
    /// The API's address. Empty for Joona's own model, whose server picks one.
    pub base: String,
    /// Text goes off this PC.
    pub cloud: bool,
}

impl Chosen {
    /// The provider's name for the cloud badge, when the text leaves this PC.
    pub fn cloud_name(&self) -> Option<&'static str> {
        self.cloud.then_some(self.spec.name)
    }
}

/// Something to ask the AI model, and how to check its answer.
pub trait Ask {
    /// The rules, worked examples, then the text.
    fn prompt(&self) -> Prompt;
    /// The longest answer that makes sense, in tokens.
    fn max_tokens(&self) -> u32;
    /// The answer without anything wrapped around it, or `None` when it
    /// isn't what was asked for.
    fn accept(&self, answer: &str) -> Option<String>;
    /// How many words the model gets, which sets how long to wait.
    fn words(&self) -> usize;
    /// Why an answer `accept` turned down can't be used.
    fn refused(&self) -> &'static str;
}

/// Why there's no AI model to use.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotReady {
    #[error("No AI model is set up.")]
    NoProvider,
    #[error("Your {0} API key is missing.")]
    NoKey(&'static str),
    #[error("Joona doesn't have your permission to send text to {0}.")]
    NoConsent(&'static str),
    #[error("Your server's address is missing.")]
    NoEndpoint,
    #[error("No model is picked.")]
    NoModel,
    #[error("The local AI model isn't downloaded.")]
    NotInstalled,
}

/// The AI model the settings pick, when everything it needs is there.
pub fn chosen(settings: &Settings, keys: &Keys, local: &LocalAi) -> Result<Chosen, NotReady> {
    let polish = &settings.polish;
    let spec = polish
        .provider
        .as_deref()
        .and_then(llm::spec)
        .ok_or(NotReady::NoProvider)?;
    let base = match (spec.endpoint, &polish.endpoint) {
        (true, Some(endpoint)) if !endpoint.trim().is_empty() => endpoint.clone(),
        _ => spec.base.to_owned(),
    };
    if base.is_empty() && (spec.endpoint || !spec.local) {
        return Err(NotReady::NoEndpoint);
    }
    let cloud = llm::sends_away(spec, &base);
    if cloud && !settings.privacy.consented_to(&llm::consent(spec.id)) {
        return Err(NotReady::NoConsent(spec.name));
    }
    if spec.key == KeyNeed::Required && !keys.has(spec.id) {
        return Err(NotReady::NoKey(spec.name));
    }
    if spec.id == llm::LOCAL && !local.installed() {
        return Err(NotReady::NotInstalled);
    }
    let model = polish
        .model
        .clone()
        .or_else(|| spec.models.first().map(|m| (*m).to_owned()))
        .ok_or(NotReady::NoModel)?;
    Ok(Chosen {
        spec,
        model,
        base,
        cloud,
    })
}

/// Has the chosen model do `request`, and checks its answer. Only lengths
/// are logged, never the text.
pub async fn run(
    chosen: &Chosen,
    keys: &Arc<Keys>,
    local: &LocalAi,
    request: &(impl Ask + Sync),
) -> Result<String, ProviderError> {
    let name = chosen.spec.name;
    let (base, key) = match chosen.spec.id {
        llm::LOCAL => {
            let (base, key) = local.start().await.map_err(|e| {
                tracing::warn!("couldn't start the local AI model: {e:#}");
                ProviderError::Failed(name, format!("{e:#}"))
            })?;
            (base, Some(key))
        }
        id => {
            let key = match chosen.spec.key {
                KeyNeed::Never => None,
                KeyNeed::Optional => providers::key(keys, id).await.ok(),
                KeyNeed::Required => Some(providers::key(keys, id).await?),
            };
            (chosen.base.clone(), key)
        }
    };
    let words = request.words() as u64;
    let timeout = match chosen.cloud {
        true => CLOUD_TIMEOUT + Duration::from_secs(words / 10),
        false => LOCAL_TIMEOUT + Duration::from_secs(words / 2),
    };
    let started = Instant::now();
    let answer = Llm::new(chosen.spec, base, key, chosen.model.clone())
        .chat(&request.prompt(), request.max_tokens(), timeout)
        .await;
    if chosen.spec.id == llm::LOCAL {
        local.touch();
    }
    let answer = answer?;
    let accepted = request.accept(&answer);
    tracing::info!(
        provider = chosen.spec.id,
        model = %chosen.model,
        words,
        answered = answer.chars().count(),
        accepted = accepted.is_some(),
        ms = started.elapsed().as_millis() as u64,
        "the AI model answered"
    );
    accepted.ok_or_else(|| ProviderError::Failed(name, request.refused().into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: &str) -> Settings {
        let mut s = Settings::default();
        s.polish.provider = Some(provider.into());
        s
    }

    fn local() -> LocalAi {
        LocalAi::new(std::env::temp_dir().join("joona-no-local-ai"))
    }

    #[test]
    fn cloud_models_need_permission_first() {
        let keys = Keys::default();
        assert_eq!(
            chosen(&Settings::default(), &keys, &local()),
            Err(NotReady::NoProvider)
        );
        // Ollama has no models of its own to suggest.
        let mut s = settings(llm::OLLAMA);
        assert_eq!(chosen(&s, &keys, &local()), Err(NotReady::NoModel));
        s.polish.model = Some("llama3.2".into());
        let ollama = chosen(&s, &keys, &local()).unwrap();
        assert!(!ollama.cloud);
        assert_eq!(ollama.base, "http://localhost:11434/v1");
        assert_eq!(ollama.cloud_name(), None);

        s.polish.endpoint = Some("http://192.168.1.20:11434/v1".into());
        assert_eq!(
            chosen(&s, &keys, &local()),
            Err(NotReady::NoConsent("Ollama"))
        );
        s.privacy.consented.push(llm::consent(llm::OLLAMA));
        let remote = chosen(&s, &keys, &local()).unwrap();
        assert!(remote.cloud);
        assert_eq!(remote.cloud_name(), Some("Ollama"));
    }

    #[test]
    fn a_custom_server_needs_its_address_and_joonas_own_model_its_download() {
        let keys = Keys::default();
        assert_eq!(
            chosen(&settings(llm::CUSTOM), &keys, &local()),
            Err(NotReady::NoEndpoint)
        );
        assert_eq!(
            chosen(&settings(llm::LOCAL), &keys, &local()),
            Err(NotReady::NotInstalled)
        );
    }
}
