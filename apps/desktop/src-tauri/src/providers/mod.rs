//! Cloud services the user can opt into, and their API keys. Nothing here is
//! used until the user agrees to send a service their recordings or text.

pub mod chunk;
pub mod keys;
pub mod llm;
pub mod sarvam;
pub mod stt;
#[cfg(test)]
mod testing;
mod text;

use std::sync::{Arc, LazyLock};
use std::time::Duration;

pub use keys::Keys;
use llm::KeyNeed;

pub const SARVAM: &str = "sarvam";

/// Providers that take an API key.
pub const KNOWN: &[&str] = &[
    SARVAM,
    llm::OPENAI,
    llm::ANTHROPIC,
    llm::GEMINI,
    llm::GROQ,
    llm::OPENROUTER,
    llm::LM_STUDIO,
    llm::CUSTOM,
    stt::DEEPGRAM,
    stt::ELEVENLABS,
    stt::CUSTOM,
];

/// The provider's name as people know it.
pub fn name(provider: &str) -> &'static str {
    match provider {
        SARVAM => sarvam::NAME,
        _ => llm::spec(provider)
            .map(|spec| spec.name)
            .or_else(|| stt::spec(provider).map(|spec| spec.name))
            .unwrap_or("The provider"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("Add your {0} API key first.")]
    NoKey(&'static str),
    #[error("{0} didn't accept your API key.")]
    BadKey(&'static str),
    #[error("Your {0} account is out of credits.")]
    NoCredit(&'static str),
    #[error("{0} is getting too many requests. Try again in a moment.")]
    RateLimited(&'static str),
    #[error("Can't reach {0}. Check your internet connection.")]
    Offline(&'static str),
    /// A server on this PC, such as Ollama, isn't answering.
    #[error("{0} isn't running. Start it, then try again.")]
    NotRunning(&'static str),
    #[error("{0} took too long to answer.")]
    Timeout(&'static str),
    #[error("{0} can't do that: {1}")]
    Rejected(&'static str, String),
    #[error("{0} had a problem: {1}")]
    Failed(&'static str, String),
}

impl ProviderError {
    /// The fix is on the Providers page: a missing or wrong key, or credits.
    pub fn needs_account(&self) -> bool {
        matches!(
            self,
            ProviderError::NoKey(_) | ProviderError::BadKey(_) | ProviderError::NoCredit(_)
        )
    }

    fn network(provider: &'static str, error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            ProviderError::Timeout(provider)
        } else if error.is_connect() || error.is_request() {
            ProviderError::Offline(provider)
        } else {
            ProviderError::Failed(provider, error.to_string())
        }
    }
}

/// One client for every provider, so connections are reused between dictations.
fn http() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> =
        LazyLock::new(|| client().build().unwrap_or_default());
    &CLIENT
}

/// The client for `url`. Servers on this PC are reached directly, never
/// through a proxy.
pub(crate) fn http_for(url: &str) -> &'static reqwest::Client {
    static LOCAL: LazyLock<reqwest::Client> =
        LazyLock::new(|| client().no_proxy().build().unwrap_or_default());
    match llm::is_local_url(url) {
        true => &LOCAL,
        false => http(),
    }
}

fn client() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(concat!(
            "Joona/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/saqib40/joona)"
        ))
        .connect_timeout(Duration::from_secs(10))
}

/// Checks that `key` works with `provider`. `endpoint` is the address of a
/// server the user runs, for providers that have one.
pub async fn check(
    provider: &str,
    key: Option<String>,
    endpoint: Option<String>,
) -> Result<(), ProviderError> {
    if provider == SARVAM {
        let key = key.ok_or(ProviderError::NoKey(sarvam::NAME))?;
        return sarvam::Sarvam::new(key).check().await;
    }
    let needs_address =
        |name| ProviderError::Rejected(name, "it needs the server's address first".into());
    // OpenAI and Groq keys work for both, so their chat check covers speech.
    if let Some(spec) = llm::spec(provider) {
        let base = endpoint
            .filter(|_| spec.endpoint)
            .unwrap_or_else(|| spec.base.to_owned());
        if base.is_empty() {
            return Err(needs_address(spec.name));
        }
        return llm::Llm::new(spec, base, key, "").check().await;
    }
    let Some(spec) = stt::spec(provider) else {
        return Err(ProviderError::Rejected(
            name(provider),
            "Joona doesn't know this provider".into(),
        ));
    };
    let base = endpoint
        .filter(|_| spec.endpoint)
        .unwrap_or_else(|| spec.base.to_owned());
    if base.is_empty() {
        return Err(needs_address(spec.name));
    }
    stt::Stt::new(spec, base, key, "").check().await
}

/// What `provider` needs in the way of an API key.
pub fn key_need(provider: &str) -> KeyNeed {
    llm::spec(provider)
        .map(|spec| spec.key)
        .or_else(|| stt::spec(provider).map(|spec| spec.key))
        .unwrap_or(KeyNeed::Required)
}

/// The saved key for `provider`, read from Credential Manager off the async runtime.
pub async fn key(keys: &Arc<Keys>, provider: &str) -> Result<String, ProviderError> {
    let name = name(provider);
    let keys = keys.clone();
    let provider = provider.to_owned();
    let found = tauri::async_runtime::spawn_blocking(move || keys.get(&provider)).await;
    match found {
        Ok(Ok(Some(key))) => Ok(key),
        Ok(Ok(None)) => Err(ProviderError::NoKey(name)),
        Ok(Err(e)) => {
            tracing::warn!("couldn't read the {name} key from Credential Manager: {e:#}");
            Err(ProviderError::NoKey(name))
        }
        Err(e) => Err(ProviderError::Failed(name, e.to_string())),
    }
}
