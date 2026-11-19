//! Chat models for Smart polish and translating text: OpenAI and the many
//! services that copy its API, Anthropic's own API, and models people run
//! themselves with Ollama, LM Studio or llama.cpp.

use std::time::Duration;

use reqwest::{RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

use super::{ProviderError, SARVAM, http_for, sarvam};

pub const OPENAI: &str = "openai";
pub const ANTHROPIC: &str = "anthropic";
pub const GEMINI: &str = "gemini";
pub const GROQ: &str = "groq";
pub const OPENROUTER: &str = "openrouter";
pub const OLLAMA: &str = "ollama";
pub const LM_STUDIO: &str = "lmstudio";
pub const CUSTOM: &str = "custom";
/// The small model Joona runs on this PC itself.
pub const LOCAL: &str = "local";

const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const ANTHROPIC_VERSION: &str = "2023-06-01";
const TEMPERATURE: f32 = 0.2;
/// Reasoning models spend tokens thinking before they write the answer.
const THINKING_TOKENS: u32 = 4096;
/// More than anyone scrolls through in a model picker.
const MAX_MODELS: usize = 400;
/// Words in model names that aren't for chat: embeddings, speech, images.
const NOT_CHAT: &[&str] = &[
    "embed",
    "whisper",
    "tts",
    "dall-e",
    "davinci",
    "babbage",
    "moderation",
    "transcribe",
    "realtime",
    "audio",
    "image",
    "search",
    "guard",
    "rerank",
    "imagen",
    "veo",
    "aqa",
    "sora",
    "computer-use",
    "turbo-instruct",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Api {
    /// `/chat/completions`, which most providers offer.
    OpenAi,
    /// `/messages`.
    Anthropic,
}

/// Whether a provider needs an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum KeyNeed {
    Required,
    /// Servers people run themselves may or may not ask for one.
    Optional,
    Never,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Spec {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    api: Api,
    pub key: KeyNeed,
    /// Runs on this PC unless the user points it elsewhere.
    pub local: bool,
    /// The API's address. Empty when only the user knows it.
    pub base: &'static str,
    /// The user can change the address, for servers they run themselves.
    pub endpoint: bool,
    /// Good models for dictation, the default first.
    pub models: &'static [&'static str],
    pub keys_url: Option<&'static str>,
    pub privacy_url: Option<&'static str>,
    /// Something to know before sending it text.
    pub note: Option<&'static str>,
}

pub static SPECS: &[Spec] = &[
    Spec {
        id: OPENAI,
        name: "OpenAI",
        summary: "GPT models. Quick, and good at following your style.",
        api: Api::OpenAi,
        key: KeyNeed::Required,
        local: false,
        base: "https://api.openai.com/v1",
        endpoint: false,
        models: &["gpt-4.1-mini", "gpt-4.1-nano", "gpt-4.1", "gpt-5-mini"],
        keys_url: Some("https://platform.openai.com/api-keys"),
        privacy_url: Some("https://openai.com/policies/privacy-policy/"),
        note: None,
    },
    Spec {
        id: ANTHROPIC,
        name: "Anthropic",
        summary: "Claude models. Careful with meaning and tone.",
        api: Api::Anthropic,
        key: KeyNeed::Required,
        local: false,
        base: "https://api.anthropic.com/v1",
        endpoint: false,
        models: &["claude-haiku-4-5", "claude-sonnet-4-5"],
        keys_url: Some("https://console.anthropic.com/settings/keys"),
        privacy_url: Some("https://www.anthropic.com/legal/privacy"),
        note: None,
    },
    Spec {
        id: GEMINI,
        name: "Google Gemini",
        summary: "Gemini models, with a free tier.",
        api: Api::OpenAi,
        key: KeyNeed::Required,
        local: false,
        base: "https://generativelanguage.googleapis.com/v1beta/openai",
        endpoint: false,
        models: &["gemini-2.5-flash-lite", "gemini-2.5-flash"],
        keys_url: Some("https://aistudio.google.com/apikey"),
        privacy_url: Some("https://ai.google.dev/gemini-api/terms"),
        note: Some("On the free tier, Google may use what you send to improve its products."),
    },
    Spec {
        id: GROQ,
        name: "Groq",
        summary: "Open models on very fast chips, with a free tier.",
        api: Api::OpenAi,
        key: KeyNeed::Required,
        local: false,
        base: "https://api.groq.com/openai/v1",
        endpoint: false,
        models: &["llama-3.3-70b-versatile", "llama-3.1-8b-instant"],
        keys_url: Some("https://console.groq.com/keys"),
        privacy_url: Some("https://groq.com/privacy-policy/"),
        note: None,
    },
    Spec {
        id: OPENROUTER,
        name: "OpenRouter",
        summary: "One key for hundreds of models from every lab.",
        api: Api::OpenAi,
        key: KeyNeed::Required,
        local: false,
        base: "https://openrouter.ai/api/v1",
        endpoint: false,
        models: &[
            "openai/gpt-4.1-mini",
            "anthropic/claude-haiku-4.5",
            "google/gemini-2.5-flash-lite",
            "meta-llama/llama-3.3-70b-instruct",
        ],
        keys_url: Some("https://openrouter.ai/settings/keys"),
        privacy_url: Some("https://openrouter.ai/privacy"),
        note: Some("OpenRouter passes your text on to the company behind the model you pick."),
    },
    Spec {
        id: SARVAM,
        name: sarvam::NAME,
        summary: "Sarvam-M, made for Indian languages. Uses your Sarvam key.",
        api: Api::OpenAi,
        key: KeyNeed::Required,
        local: false,
        base: "https://api.sarvam.ai/v1",
        endpoint: false,
        models: &["sarvam-m"],
        keys_url: Some(sarvam::KEYS_URL),
        privacy_url: Some(sarvam::PRIVACY_URL),
        note: None,
    },
    Spec {
        id: LOCAL,
        name: "Local AI",
        summary: "A free model that Joona runs on this PC. No account needed.",
        api: Api::OpenAi,
        key: KeyNeed::Never,
        local: true,
        base: "",
        endpoint: false,
        models: &["qwen3-4b-instruct"],
        keys_url: None,
        privacy_url: None,
        note: None,
    },
    Spec {
        id: OLLAMA,
        name: "Ollama",
        summary: "Models you run yourself with Ollama.",
        api: Api::OpenAi,
        key: KeyNeed::Never,
        local: true,
        base: "http://localhost:11434/v1",
        endpoint: true,
        models: &[],
        keys_url: None,
        privacy_url: None,
        note: None,
    },
    Spec {
        id: LM_STUDIO,
        name: "LM Studio",
        summary: "Models you run yourself with LM Studio.",
        api: Api::OpenAi,
        key: KeyNeed::Optional,
        local: true,
        base: "http://localhost:1234/v1",
        endpoint: true,
        models: &[],
        keys_url: None,
        privacy_url: None,
        note: None,
    },
    Spec {
        id: CUSTOM,
        name: "Custom server",
        summary: "Any server with an OpenAI-compatible API, such as llama.cpp or vLLM.",
        api: Api::OpenAi,
        key: KeyNeed::Optional,
        local: false,
        base: "",
        endpoint: true,
        models: &[],
        keys_url: None,
        privacy_url: None,
        note: None,
    },
];

pub fn spec(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id == id)
}

/// What `privacy.consented` lists once the user agreed to send text to a
/// provider's AI model. Sarvam's speech consent doesn't cover it.
pub fn consent(provider: &str) -> String {
    format!("ai:{provider}")
}

/// Whether text sent to `spec` at `endpoint` leaves this PC.
pub fn sends_away(spec: &Spec, endpoint: &str) -> bool {
    match spec.endpoint {
        true => !is_local_url(endpoint),
        false => !spec.local,
    }
}

/// Whether `url` points at this PC.
pub fn is_local_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| {
            host == "localhost"
                || host.ends_with(".localhost")
                || host == "[::1]"
                || host.starts_with("127.")
        })
}

/// Turns an address the user typed, such as `localhost:11434`, into the
/// API's base address, such as `http://localhost:11434/v1`.
pub fn endpoint(typed: &str) -> Result<String, String> {
    let typed = typed.trim().trim_end_matches('/');
    let typed = ["/chat/completions", "/audio/transcriptions"]
        .iter()
        .find_map(|path| typed.strip_suffix(path))
        .unwrap_or(typed)
        .trim_end_matches('/');
    let full = match typed.contains("://") {
        true => typed.to_owned(),
        false => format!("http://{typed}"),
    };
    let url = reqwest::Url::parse(&full)
        .map_err(|_| "That doesn't look like a web address.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err("Use an address that starts with http:// or https://.".into());
    }
    Ok(match url.path() {
        "" | "/" => format!("{full}/v1"),
        _ => full,
    })
}

#[derive(Deserialize)]
struct ChatReply {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicReply {
    #[serde(default)]
    content: Vec<AnthropicBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

/// What a chat model is asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    /// Messages with the answers they should get, shown before `user`.
    pub examples: Vec<(String, String)>,
    pub user: String,
}

impl Prompt {
    /// The examples and the message, without the system prompt.
    fn turns(&self) -> Vec<Value> {
        let mut turns = Vec::with_capacity(self.examples.len() * 2 + 1);
        for (asked, answered) in &self.examples {
            turns.push(json!({ "role": "user", "content": asked }));
            turns.push(json!({ "role": "assistant", "content": answered }));
        }
        turns.push(json!({ "role": "user", "content": self.user }));
        turns
    }
}

pub struct Llm {
    spec: &'static Spec,
    base: String,
    key: Option<String>,
    model: String,
}

impl Llm {
    pub fn new(
        spec: &'static Spec,
        base: impl Into<String>,
        key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            spec,
            base: base.into().trim_end_matches('/').to_owned(),
            key,
            model: model.into(),
        }
    }

    /// The model's answer to the prompt.
    pub async fn chat(
        &self,
        prompt: &Prompt,
        max_tokens: u32,
        timeout: Duration,
    ) -> Result<String, ProviderError> {
        let text = match self.spec.api {
            Api::OpenAi => self.openai_chat(prompt, max_tokens, timeout).await?,
            Api::Anthropic => self.anthropic_chat(prompt, max_tokens, timeout).await?,
        };
        match text.trim().is_empty() {
            true => Err(self.failed("it sent back an empty answer")),
            false => Ok(text),
        }
    }

    async fn openai_chat(
        &self,
        prompt: &Prompt,
        max_tokens: u32,
        timeout: Duration,
    ) -> Result<String, ProviderError> {
        let mut messages = vec![json!({ "role": "system", "content": prompt.system })];
        messages.extend(prompt.turns());
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "temperature": TEMPERATURE,
            "max_tokens": max_tokens,
            "stream": false,
        });
        let reply: ChatReply = match self.send(self.completion(&body, timeout)).await {
            // Reasoning models refuse a temperature and want `max_completion_tokens`.
            Err(ProviderError::Rejected(_, message)) if about_sampling(&message) => {
                if let Some(fields) = body.as_object_mut() {
                    fields.remove("temperature");
                    fields.remove("max_tokens");
                    fields.insert(
                        "max_completion_tokens".into(),
                        json!(max_tokens.saturating_add(THINKING_TOKENS)),
                    );
                }
                self.send(self.completion(&body, timeout)).await?
            }
            reply => reply?,
        };
        let choice = reply
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| self.failed("it sent back no answer"))?;
        if choice.finish_reason.as_deref() == Some("length") {
            return Err(self.failed("its answer was cut off"));
        }
        Ok(choice.message.content.unwrap_or_default())
    }

    async fn anthropic_chat(
        &self,
        prompt: &Prompt,
        max_tokens: u32,
        timeout: Duration,
    ) -> Result<String, ProviderError> {
        let body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "temperature": TEMPERATURE,
            "system": prompt.system,
            "messages": prompt.turns(),
        });
        let reply: AnthropicReply = self
            .send(self.post("messages").json(&body).timeout(timeout))
            .await?;
        if reply.stop_reason.as_deref() == Some("max_tokens") {
            return Err(self.failed("its answer was cut off"));
        }
        Ok(reply
            .content
            .into_iter()
            .filter(|block| block.kind == "text")
            .filter_map(|block| block.text)
            .collect())
    }

    /// The provider's models for chat: its suggestions first, then the rest by name.
    pub async fn models(&self) -> Result<Vec<String>, ProviderError> {
        // Neither lists models: Sarvam has a few, and Joona's own has one.
        if matches!(self.spec.id, SARVAM | LOCAL) {
            return Ok(self.spec.models.iter().map(|m| (*m).to_owned()).collect());
        }
        let path = match self.spec.api {
            Api::OpenAi => "models",
            Api::Anthropic => "models?limit=1000",
        };
        let list: ModelList = self.send(self.get(path).timeout(CHECK_TIMEOUT)).await?;
        Ok(chat_models(self.spec, list.data.into_iter().map(|m| m.id)))
    }

    /// Checks the key, or that the server is running, with a request that
    /// costs nothing.
    pub async fn check(&self) -> Result<(), ProviderError> {
        match self.spec.id {
            // Its model list is public, so it can't tell a good key from a bad one.
            OPENROUTER => self
                .send::<Value>(self.get("key").timeout(CHECK_TIMEOUT))
                .await
                .map(drop),
            _ => self.models().await.map(drop),
        }
    }

    fn completion(&self, body: &Value, timeout: Duration) -> RequestBuilder {
        self.post("chat/completions").json(body).timeout(timeout)
    }

    fn post(&self, path: &str) -> RequestBuilder {
        let url = format!("{}/{path}", self.base);
        self.headers(http_for(&url).post(url))
    }

    fn get(&self, path: &str) -> RequestBuilder {
        let url = format!("{}/{path}", self.base);
        self.headers(http_for(&url).get(url))
    }

    fn headers(&self, mut request: RequestBuilder) -> RequestBuilder {
        if self.spec.api == Api::Anthropic {
            request = request.header("anthropic-version", ANTHROPIC_VERSION);
        }
        if self.spec.id == OPENROUTER {
            request = request
                .header("HTTP-Referer", "https://github.com/saqib40/joona")
                .header("X-Title", "Joona");
        }
        let Some(key) = &self.key else {
            return request;
        };
        match (self.spec.api, self.spec.id) {
            (Api::Anthropic, _) => request.header("x-api-key", key),
            (_, SARVAM) => request.header("api-subscription-key", key).bearer_auth(key),
            _ => request.bearer_auth(key),
        }
    }

    async fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, ProviderError> {
        let response = request.send().await.map_err(|e| self.network(&e))?;
        let status = response.status();
        let body = response.text().await.map_err(|e| self.network(&e))?;
        if !status.is_success() {
            return Err(error_from(self.spec.name, status, &body));
        }
        serde_json::from_str(&body)
            .map_err(|e| self.failed(&format!("its answer didn't make sense ({e})")))
    }

    fn network(&self, error: &reqwest::Error) -> ProviderError {
        match ProviderError::network(self.spec.name, error) {
            ProviderError::Offline(name) if is_local_url(&self.base) => {
                ProviderError::NotRunning(name)
            }
            other => other,
        }
    }

    fn failed(&self, why: &str) -> ProviderError {
        ProviderError::Failed(self.spec.name, why.to_owned())
    }
}

/// Whether a refusal is about the temperature or token limit.
fn about_sampling(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("temperature") || message.contains("max_tokens")
}

/// Every provider words its errors a little differently.
pub(super) fn error_from(name: &'static str, status: StatusCode, body: &str) -> ProviderError {
    let parsed = match serde_json::from_str::<Value>(body).unwrap_or(Value::Null) {
        Value::Array(mut items) if !items.is_empty() => items.swap_remove(0),
        other => other,
    };
    let error = &parsed["error"];
    let detail = &parsed["detail"];
    let message = [
        error["message"].as_str(),
        error.as_str(),
        parsed["message"].as_str(),
        detail.as_str(),
        detail["message"].as_str(),
        parsed["err_msg"].as_str(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|m| !m.is_empty())
    .map_or_else(
        || {
            status
                .canonical_reason()
                .unwrap_or("unknown error")
                .to_owned()
        },
        str::to_owned,
    );
    let code = [
        error["code"].as_str(),
        error["type"].as_str(),
        error["status"].as_str(),
        detail["status"].as_str(),
        parsed["err_code"].as_str(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_ascii_lowercase();
    let lower = message.to_lowercase();
    let bad_key = code.contains("invalid_api_key")
        || code.contains("authentication_error")
        || code.contains("invalid_auth")
        || (lower.contains("api key")
            && ["invalid", "not valid", "incorrect"]
                .iter()
                .any(|w| lower.contains(w)));
    let no_credit = ["insufficient_quota", "quota_exceeded", "payment_required"]
        .iter()
        .any(|c| code.contains(c))
        || [
            "insufficient credits",
            "credit balance",
            "exceeded your current quota",
        ]
        .iter()
        .any(|w| lower.contains(w));
    match status.as_u16() {
        // Some services answer 401 when the account, not the key, is the problem.
        _ if no_credit => ProviderError::NoCredit(name),
        401 => ProviderError::BadKey(name),
        _ if bad_key => ProviderError::BadKey(name),
        402 => ProviderError::NoCredit(name),
        429 => ProviderError::RateLimited(name),
        500.. => ProviderError::Failed(name, message),
        _ => ProviderError::Rejected(name, message),
    }
}

/// The ids in a model list that are for chat, suggestions first.
fn chat_models(spec: &Spec, ids: impl Iterator<Item = String>) -> Vec<String> {
    let mut ids: Vec<String> = ids
        .map(|id| match id.strip_prefix("models/") {
            Some(bare) => bare.to_owned(),
            None => id,
        })
        .filter(|id| {
            let lower = id.to_ascii_lowercase();
            !NOT_CHAT.iter().any(|w| lower.contains(w))
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids.sort_by_key(|id| {
        spec.models
            .iter()
            .position(|m| m == id)
            .unwrap_or(usize::MAX)
    });
    ids.truncate(MAX_MODELS);
    ids
}

#[cfg(test)]
mod tests {
    use super::super::testing::{closed_port, json_body, serve, serve_once};
    use super::*;

    const ANSWER: &str = r#"{"id":"c1","choices":[{"index":0,"message":{"role":"assistant","content":"Hello there."},"finish_reason":"stop"}]}"#;

    fn client(id: &str, base: String) -> Llm {
        Llm::new(
            spec(id).unwrap(),
            base,
            Some("test-key".into()),
            "test-model",
        )
    }

    fn brief() -> Prompt {
        Prompt {
            system: "Be brief.".into(),
            examples: vec![("Hello".into(), "Hi.".into())],
            user: "Hi".into(),
        }
    }

    #[test]
    fn every_provider_has_a_name_and_an_address_or_a_way_to_get_one() {
        for spec in SPECS {
            assert!(!spec.name.is_empty());
            assert!(
                !spec.base.is_empty() || spec.endpoint || spec.id == LOCAL,
                "{}",
                spec.id
            );
            assert_eq!(
                spec.key == KeyNeed::Required,
                spec.keys_url.is_some(),
                "{}",
                spec.id
            );
            assert!(
                !sends_away(spec, spec.base) || spec.privacy_url.is_some() || spec.id == CUSTOM
            );
        }
        assert_eq!(spec(SARVAM).unwrap().name, "Sarvam");
        assert_eq!(consent(OPENAI), "ai:openai");
    }

    #[tokio::test]
    async fn chat_sends_openai_messages_and_reads_the_answer() {
        let (base, server) = serve_once(200, ANSWER);
        let answer = client(OPENAI, base)
            .chat(&brief(), 300, CHECK_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(answer, "Hello there.");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /chat/completions "), "{request}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-key")
        );
        let body = json_body(&request);
        assert_eq!(body["model"], "test-model");
        let roles: Vec<&str> = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, ["system", "user", "assistant", "user"]);
        assert_eq!(body["messages"][0]["content"], "Be brief.");
        assert_eq!(body["messages"][2]["content"], "Hi.");
        assert_eq!(body["messages"][3]["content"], "Hi");
        assert_eq!(body["max_tokens"], 300);
        assert_eq!(body["stream"], false);
    }

    #[tokio::test]
    async fn reasoning_models_are_asked_again_without_a_temperature() {
        let refusal = r#"{"error":{"message":"Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead.","type":"invalid_request_error","param":"max_tokens","code":"unsupported_parameter"}}"#;
        let (base, server) = serve(vec![(400, refusal), (200, ANSWER)]);
        let answer = client(OPENAI, base)
            .chat(&brief(), 300, CHECK_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(answer, "Hello there.");
        let requests = server.join().unwrap();
        let retry = json_body(&requests[1]);
        assert_eq!(retry.get("temperature"), None);
        assert_eq!(retry.get("max_tokens"), None);
        assert_eq!(retry["max_completion_tokens"], 300 + THINKING_TOKENS);
    }

    #[tokio::test]
    async fn anthropic_has_its_own_api() {
        let (base, server) = serve_once(
            200,
            r#"{"id":"m1","type":"message","content":[{"type":"text","text":"Hi "},{"type":"text","text":"there."}],"stop_reason":"end_turn"}"#,
        );
        let answer = client(ANTHROPIC, base)
            .chat(&brief(), 300, CHECK_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(answer, "Hi there.");
        let request = server.join().unwrap();
        let head = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /messages "));
        assert!(head.contains("x-api-key: test-key"));
        assert!(head.contains("anthropic-version: 2023-06-01"));
        assert!(!head.contains("authorization:"));
        let body = json_body(&request);
        assert_eq!(body["system"], "Be brief.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][2]["content"], "Hi");
    }

    #[tokio::test]
    async fn answers_that_were_cut_off_or_empty_fail() {
        let (base, _server) = serve_once(
            200,
            r#"{"choices":[{"message":{"content":"Half an ans"},"finish_reason":"length"}]}"#,
        );
        let cut = client(OPENAI, base).chat(&brief(), 5, CHECK_TIMEOUT).await;
        assert_eq!(
            cut,
            Err(ProviderError::Failed(
                "OpenAI",
                "its answer was cut off".into()
            ))
        );
        let (base, _server) = serve_once(
            200,
            r#"{"choices":[{"message":{"content":null},"finish_reason":"stop"}]}"#,
        );
        let empty = client(GROQ, base).chat(&brief(), 5, CHECK_TIMEOUT).await;
        assert!(matches!(empty, Err(ProviderError::Failed("Groq", _))));
    }

    #[test]
    fn errors_say_what_to_do() {
        let error = |status: u16, body: &str| {
            error_from("OpenAI", StatusCode::from_u16(status).unwrap(), body)
        };
        assert_eq!(
            error(
                401,
                r#"{"error":{"message":"Incorrect API key provided.","code":"invalid_api_key"}}"#
            ),
            ProviderError::BadKey("OpenAI")
        );
        // Gemini says so with a 400, in a list.
        assert_eq!(
            error(
                400,
                r#"[{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT"}}]"#
            ),
            ProviderError::BadKey("OpenAI")
        );
        assert_eq!(
            error(
                429,
                r#"{"error":{"message":"You exceeded your current quota.","type":"insufficient_quota"}}"#
            ),
            ProviderError::NoCredit("OpenAI")
        );
        assert_eq!(error(402, "{}"), ProviderError::NoCredit("OpenAI"));
        assert_eq!(error(429, "{}"), ProviderError::RateLimited("OpenAI"));
        assert_eq!(
            error(
                404,
                r#"{"error":{"message":"The model `gpt-9` does not exist.","code":"model_not_found"}}"#
            ),
            ProviderError::Rejected("OpenAI", "The model `gpt-9` does not exist.".into())
        );
        // Ollama's errors are plain strings.
        assert_eq!(
            error(
                404,
                r#"{"error":"model \"llama9\" not found, try pulling it first"}"#
            ),
            ProviderError::Rejected(
                "OpenAI",
                "model \"llama9\" not found, try pulling it first".into()
            )
        );
        assert_eq!(
            error(
                401,
                r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#
            ),
            ProviderError::BadKey("OpenAI")
        );
        assert_eq!(
            error(503, "<html>down</html>"),
            ProviderError::Failed("OpenAI", "Service Unavailable".into())
        );
    }

    #[tokio::test]
    async fn model_lists_keep_chat_models_with_suggestions_first() {
        let (base, _server) = serve_once(
            200,
            r#"{"object":"list","data":[{"id":"whisper-1"},{"id":"gpt-4o"},{"id":"gpt-4.1-nano"},{"id":"text-embedding-3-small"},{"id":"gpt-4.1-mini"},{"id":"dall-e-3"},{"id":"chatgpt-4o-latest"}]}"#,
        );
        let models = client(OPENAI, base).models().await.unwrap();
        assert_eq!(
            models,
            [
                "gpt-4.1-mini",
                "gpt-4.1-nano",
                "chatgpt-4o-latest",
                "gpt-4o"
            ]
        );
        let gemini = chat_models(
            spec(GEMINI).unwrap(),
            [
                "models/gemini-2.5-flash",
                "models/text-embedding-004",
                "models/aqa",
            ]
            .into_iter()
            .map(String::from),
        );
        assert_eq!(gemini, ["gemini-2.5-flash"]);
    }

    #[tokio::test]
    async fn openrouter_checks_the_key_itself() {
        let (base, server) = serve_once(200, r#"{"data":{"label":"sk-or-v1-abc"}}"#);
        client(OPENROUTER, base).check().await.unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /key "), "{request}");
        assert!(request.to_ascii_lowercase().contains("x-title: joona"));
    }

    #[tokio::test]
    async fn servers_on_this_pc_that_are_down_are_not_running() {
        let base = closed_port();
        let ollama = Llm::new(
            spec(OLLAMA).unwrap(),
            format!("{base}/v1"),
            None,
            "llama3.2",
        );
        assert_eq!(
            ollama.check().await,
            Err(ProviderError::NotRunning("Ollama"))
        );
    }

    #[test]
    fn typed_addresses_are_cleaned_up() {
        assert_eq!(
            endpoint("localhost:11434").unwrap(),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            endpoint(" http://192.168.1.5:8080/v1/ ").unwrap(),
            "http://192.168.1.5:8080/v1"
        );
        assert_eq!(
            endpoint("https://api.example.com/v1/chat/completions").unwrap(),
            "https://api.example.com/v1"
        );
        assert!(endpoint("ftp://example.com").is_err());
        assert!(endpoint("").is_err());

        assert!(is_local_url("http://localhost:1234/v1"));
        assert!(is_local_url("http://127.0.0.1:8080"));
        assert!(is_local_url("http://[::1]:8080/v1"));
        assert!(!is_local_url("http://192.168.1.5:8080/v1"));
        assert!(!is_local_url("https://api.openai.com/v1"));

        let ollama = spec(OLLAMA).unwrap();
        assert!(!sends_away(ollama, "http://localhost:11434/v1"));
        assert!(sends_away(ollama, "http://192.168.1.5:11434/v1"));
        assert!(sends_away(
            spec(OPENAI).unwrap(),
            "https://api.openai.com/v1"
        ));
        assert!(!sends_away(spec(LOCAL).unwrap(), ""));
    }
}
