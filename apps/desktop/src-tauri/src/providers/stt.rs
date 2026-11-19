//! Speech-to-text services the user brings a key for: OpenAI and the servers
//! that copy its API (Groq, or one the user runs), Deepgram and ElevenLabs.
//! Each hears a whole recording once the user stops speaking, in place of the
//! model on this PC.

use std::time::Duration;

use futures_util::{StreamExt, TryStreamExt, stream};
use reqwest::multipart::{Form, Part};
use reqwest::{RequestBuilder, Url};
use serde_json::Value;

use super::llm::{self, KeyNeed};
use super::{ProviderError, chunk, http_for};
use crate::audio::{SAMPLE_RATE, encode_pcm16, encode_wav, level};
use crate::languages;
use crate::settings::Settings;

pub const OPENAI: &str = llm::OPENAI;
pub const GROQ: &str = llm::GROQ;
pub const DEEPGRAM: &str = "deepgram";
pub const ELEVENLABS: &str = "elevenlabs";
/// A Whisper server the user runs with an OpenAI-compatible API.
pub const CUSTOM: &str = "custom-stt";

/// OpenAI-compatible APIs take files of up to 25 MB, about 13 minutes here.
const MAX_PIECE: usize = 10 * 60 * SAMPLE_RATE as usize;
/// Requests in flight at once for a long recording.
const PARALLEL: usize = 4;
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
/// Most answers come in a second or two; a long recording takes longer.
const BASE_TIMEOUT: Duration = Duration::from_secs(30);
/// Whisper reads about 224 tokens of prompt.
const MAX_PROMPT_CHARS: usize = 600;
const MAX_TERMS: usize = 50;
const MAX_TERM_CHARS: usize = 50;
/// A piece this quiet is a muted stretch with nothing to transcribe.
const SILENT_PEAK: f32 = 0.002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Api {
    /// `/audio/transcriptions`.
    OpenAi,
    /// `/listen`.
    Deepgram,
    /// `/speech-to-text`.
    ElevenLabs,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Spec {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    api: Api,
    pub key: KeyNeed,
    /// The API's address. Empty when only the user knows it.
    pub base: &'static str,
    /// The user can change the address, for servers they run themselves.
    pub endpoint: bool,
    /// Good models for dictation, the default first.
    pub models: &'static [&'static str],
    /// Models that can turn speech straight into English.
    translates: &'static [&'static str],
    pub keys_url: Option<&'static str>,
    pub privacy_url: Option<&'static str>,
    /// Something to know before sending it recordings.
    pub note: Option<&'static str>,
}

pub static SPECS: &[Spec] = &[
    Spec {
        id: OPENAI,
        name: "OpenAI",
        summary: "GPT Transcribe. Accurate in many languages, and good with names.",
        api: Api::OpenAi,
        key: KeyNeed::Required,
        base: "https://api.openai.com/v1",
        endpoint: false,
        models: &["gpt-transcribe", "gpt-4o-mini-transcribe"],
        translates: &[],
        keys_url: Some("https://platform.openai.com/api-keys"),
        privacy_url: Some("https://openai.com/policies/privacy-policy/"),
        note: None,
    },
    Spec {
        id: GROQ,
        name: "Groq",
        summary: "Whisper Large v3 on very fast chips, with a free tier.",
        api: Api::OpenAi,
        key: KeyNeed::Required,
        base: "https://api.groq.com/openai/v1",
        endpoint: false,
        models: &["whisper-large-v3-turbo", "whisper-large-v3"],
        translates: &["whisper-large-v3"],
        keys_url: Some("https://console.groq.com/keys"),
        privacy_url: Some("https://groq.com/privacy-policy/"),
        note: None,
    },
    Spec {
        id: DEEPGRAM,
        name: "Deepgram",
        summary: "Nova-3. Quick, and listens for the words in your dictionary.",
        api: Api::Deepgram,
        key: KeyNeed::Required,
        base: "https://api.deepgram.com/v1",
        endpoint: false,
        models: &["nova-3", "nova-2"],
        translates: &[],
        keys_url: Some("https://console.deepgram.com/"),
        privacy_url: Some("https://deepgram.com/privacy"),
        note: None,
    },
    Spec {
        id: ELEVENLABS,
        name: "ElevenLabs",
        summary: "Scribe. Strong in more than 90 languages, Indian ones included.",
        api: Api::ElevenLabs,
        key: KeyNeed::Required,
        base: "https://api.elevenlabs.io/v1",
        endpoint: false,
        models: &["scribe_v2"],
        translates: &[],
        keys_url: Some("https://elevenlabs.io/app/settings/api-keys"),
        privacy_url: Some("https://elevenlabs.io/privacy-policy"),
        note: None,
    },
    Spec {
        id: CUSTOM,
        name: "Your own server",
        summary: "A Whisper server with an OpenAI-compatible API, such as Speaches or LocalAI.",
        api: Api::OpenAi,
        key: KeyNeed::Optional,
        base: "",
        endpoint: true,
        models: &[],
        translates: &[],
        keys_url: None,
        privacy_url: None,
        note: None,
    },
];

pub fn spec(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id == id)
}

/// What `privacy.consented` lists once the user agreed to send a service
/// their recordings. Agreeing to send it text doesn't cover this.
pub fn consent(provider: &str) -> String {
    format!("stt:{provider}")
}

/// Whether recordings sent to `spec` at `base` leave this PC.
pub fn sends_away(spec: &Spec, base: &str) -> bool {
    !spec.endpoint || !llm::is_local_url(base)
}

impl Spec {
    /// Whether `model` turns speech straight into English.
    pub fn translates(&self, model: &str) -> bool {
        self.translates.contains(&model)
    }

    /// Whether it gets the dictionary's words, to spell them the user's way.
    pub fn takes_terms(&self) -> bool {
        self.api != Api::ElevenLabs
    }
}

/// The service the settings pick to hear dictations, when it's set up.
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    pub spec: &'static Spec,
    pub model: String,
    pub base: String,
    /// Recordings go off this PC.
    pub cloud: bool,
}

/// Why the service the user picked can't hear dictations yet. Its key is
/// checked when a recording is sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotReady {
    #[error("Joona doesn't have your permission to send recordings to {0}.")]
    NoConsent(&'static str),
    #[error("Your server's address is missing.")]
    NoEndpoint,
    #[error("No model is picked.")]
    NoModel,
}

/// The speech-to-text service in the settings. `None` when dictations stay
/// on this PC.
pub fn chosen(settings: &Settings) -> Option<Result<Chosen, NotReady>> {
    let spec = settings.transcription.provider.as_deref().and_then(spec)?;
    Some(ready(spec, settings))
}

fn ready(spec: &'static Spec, settings: &Settings) -> Result<Chosen, NotReady> {
    let picked = &settings.transcription;
    let base = match (spec.endpoint, &picked.endpoint) {
        (true, Some(endpoint)) => endpoint.clone(),
        _ => spec.base.to_owned(),
    };
    if base.is_empty() {
        return Err(NotReady::NoEndpoint);
    }
    let cloud = sends_away(spec, &base);
    if cloud && !settings.privacy.consented_to(&consent(spec.id)) {
        return Err(NotReady::NoConsent(spec.name));
    }
    let model = picked
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

/// What a service heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    pub text: String,
    /// The spoken language as Joona's code, when known.
    pub language: Option<String>,
    /// The text is in English because the service translated the speech.
    pub translated: bool,
}

/// What to listen for.
#[derive(Debug, Clone, Copy, Default)]
pub struct Listen<'a> {
    /// The spoken language, when it's known.
    pub language: Option<&'a str>,
    /// The languages it may be, when it isn't.
    pub candidates: &'a [String],
    /// Write English, whatever was spoken, when the model can.
    pub to_english: bool,
    /// Dictionary words to listen for.
    pub terms: &'a [String],
}

pub struct Stt {
    spec: &'static Spec,
    base: String,
    key: Option<String>,
    model: String,
}

impl Stt {
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

    /// Turns a 16 kHz recording into text.
    pub async fn transcribe(
        &self,
        mut samples: Vec<f32>,
        listen: Listen<'_>,
    ) -> Result<Heard, ProviderError> {
        level::normalize(&mut samples);
        let ranges = match self.spec.api {
            Api::OpenAi => chunk::split_at_pauses(&samples, MAX_PIECE),
            // These take a long recording in one go.
            Api::Deepgram | Api::ElevenLabs => std::iter::once(0..samples.len()).collect(),
        };
        let translated = listen.to_english && self.spec.translates(&self.model);
        let requests: Vec<RequestBuilder> = ranges
            .into_iter()
            .map(|range| &samples[range])
            .filter(|piece| piece.iter().any(|s| s.abs() >= SILENT_PEAK))
            .map(|piece| self.request(piece, listen, translated))
            .collect::<Result<_, _>>()?;
        let heard: Vec<Heard> = stream::iter(requests)
            .map(|request| self.heard(request, translated))
            .buffered(PARALLEL)
            .try_collect()
            .await?;
        let text = heard
            .iter()
            .map(|h| h.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let detected = heard.iter().find_map(|h| h.language.clone());
        Ok(Heard {
            text,
            language: listen.language.map(str::to_owned).or(detected),
            translated,
        })
    }

    /// The request for one piece of the recording.
    fn request(
        &self,
        samples: &[f32],
        listen: Listen<'_>,
        translate: bool,
    ) -> Result<RequestBuilder, ProviderError> {
        let seconds = samples.len() as u64 / u64::from(SAMPLE_RATE);
        let timeout = BASE_TIMEOUT + Duration::from_secs(seconds / 4);
        let request = match self.spec.api {
            Api::OpenAi => self.openai(samples, listen, translate)?,
            Api::Deepgram => self.deepgram(samples, listen)?,
            Api::ElevenLabs => self.elevenlabs(samples, listen)?,
        };
        Ok(request.timeout(timeout))
    }

    /// What the service heard in one piece.
    async fn heard(
        &self,
        request: RequestBuilder,
        translate: bool,
    ) -> Result<Heard, ProviderError> {
        let reply: Value = self.send(request).await?;
        let (text, language) = match self.spec.api {
            Api::OpenAi => (
                reply["text"].as_str(),
                reply["language"]
                    .as_str()
                    .or_else(|| reply["languages"][0]["code"].as_str()),
            ),
            Api::Deepgram => {
                let channel = &reply["results"]["channels"][0];
                (
                    channel["alternatives"][0]["transcript"].as_str(),
                    channel["detected_language"].as_str(),
                )
            }
            Api::ElevenLabs => (reply["text"].as_str(), reply["language_code"].as_str()),
        };
        let Some(text) = text else {
            return Err(self.failed("its answer had no text"));
        };
        Ok(Heard {
            text: text.trim().to_owned(),
            language: language.and_then(languages::recognize).map(str::to_owned),
            translated: translate,
        })
    }

    fn openai(
        &self,
        samples: &[f32],
        listen: Listen<'_>,
        translate: bool,
    ) -> Result<RequestBuilder, ProviderError> {
        let model = self.model.to_ascii_lowercase();
        // Whisper models say which language they heard only in verbose_json;
        // the GPT ones offer just json, with the language in it.
        let format = match model.contains("whisper") {
            true => "verbose_json",
            false => "json",
        };
        let mut form = Form::new()
            .part(
                "file",
                self.file(encode_wav(samples), "audio.wav", "audio/wav")?,
            )
            .text("model", self.model.clone())
            .text("response_format", format);
        // A translation is always into English, and takes no language.
        if !translate && let Some(language) = listen.language {
            form = form.text("language", language.to_owned());
        }
        if model.starts_with("gpt-transcribe") {
            if listen.language.is_none() {
                for language in listen.candidates {
                    form = form.text("languages[]", language.clone());
                }
            }
            for term in terms(listen.terms) {
                form = form.text("keywords[]", term.to_owned());
            }
        } else if let Some(prompt) = prompt(listen.terms) {
            form = form.text("prompt", prompt);
        }
        let path = match translate {
            true => "audio/translations",
            false => "audio/transcriptions",
        };
        Ok(self.post(path).multipart(form))
    }

    fn deepgram(
        &self,
        samples: &[f32],
        listen: Listen<'_>,
    ) -> Result<RequestBuilder, ProviderError> {
        let mut query: Vec<(&str, String)> = vec![
            ("model", self.model.clone()),
            ("smart_format", "true".into()),
            ("punctuate", "true".into()),
        ];
        match listen.language {
            Some(language) => query.push(("language", language.to_owned())),
            None => query.push(("detect_language", "true".into())),
        }
        // Nova-3 takes key terms; older models take keywords.
        let boost = match self.model.starts_with("nova-3") {
            true => "keyterm",
            false => "keywords",
        };
        query.extend(terms(listen.terms).map(|t| (boost, t.to_owned())));
        let url = Url::parse_with_params(&format!("{}/listen", self.base), &query)
            .map_err(|e| self.failed(&e.to_string()))?;
        Ok(self
            .headers(http_for(url.as_str()).post(url))
            .header("content-type", "audio/wav")
            .body(encode_wav(samples)))
    }

    fn elevenlabs(
        &self,
        samples: &[f32],
        listen: Listen<'_>,
    ) -> Result<RequestBuilder, ProviderError> {
        // Raw 16 kHz PCM is quicker for it than a WAV file. Key terms cost
        // extra there, so they aren't sent.
        let mut form = Form::new()
            .part(
                "file",
                self.file(
                    encode_pcm16(samples),
                    "audio.pcm",
                    "application/octet-stream",
                )?,
            )
            .text("model_id", self.model.clone())
            .text("file_format", "pcm_s16le_16")
            .text("tag_audio_events", "false");
        if let Some(language) = listen.language {
            form = form.text("language_code", language.to_owned());
        }
        Ok(self.post("speech-to-text").multipart(form))
    }

    /// Checks the key, or that the server is running, with a request that
    /// costs nothing.
    pub async fn check(&self) -> Result<(), ProviderError> {
        let path = match self.spec.api {
            Api::OpenAi => "models",
            Api::Deepgram => "projects",
            Api::ElevenLabs => "user",
        };
        let response = self
            .get(path)
            .timeout(CHECK_TIMEOUT)
            .send()
            .await
            .map_err(|e| self.network(&e))?;
        let status = response.status();
        let body = response.text().await.map_err(|e| self.network(&e))?;
        let fine = status.is_success()
            // A key made only for speech to text may not read the account.
            || (self.spec.api == Api::ElevenLabs && body.contains("missing_permissions"))
            // A server the user runs may not list models, but it answered.
            || (self.spec.endpoint && !matches!(status.as_u16(), 401 | 403));
        match fine {
            true => Ok(()),
            false => Err(llm::error_from(self.spec.name, status, &body)),
        }
    }

    fn file(&self, bytes: Vec<u8>, name: &str, mime: &str) -> Result<Part, ProviderError> {
        Part::bytes(bytes)
            .file_name(name.to_owned())
            .mime_str(mime)
            .map_err(|e| self.failed(&e.to_string()))
    }

    fn post(&self, path: &str) -> RequestBuilder {
        let url = format!("{}/{path}", self.base);
        self.headers(http_for(&url).post(url))
    }

    fn get(&self, path: &str) -> RequestBuilder {
        let url = format!("{}/{path}", self.base);
        self.headers(http_for(&url).get(url))
    }

    fn headers(&self, request: RequestBuilder) -> RequestBuilder {
        let Some(key) = &self.key else {
            return request;
        };
        match self.spec.api {
            Api::OpenAi => request.bearer_auth(key),
            Api::Deepgram => request.header("authorization", format!("Token {key}")),
            Api::ElevenLabs => request.header("xi-api-key", key),
        }
    }

    async fn send(&self, request: RequestBuilder) -> Result<Value, ProviderError> {
        let response = request.send().await.map_err(|e| self.network(&e))?;
        let status = response.status();
        let body = response.text().await.map_err(|e| self.network(&e))?;
        if !status.is_success() {
            return Err(llm::error_from(self.spec.name, status, &body));
        }
        serde_json::from_str(&body)
            .map_err(|e| self.failed(&format!("its answer didn't make sense ({e})")))
    }

    fn network(&self, error: &reqwest::Error) -> ProviderError {
        match ProviderError::network(self.spec.name, error) {
            ProviderError::Offline(name) if llm::is_local_url(&self.base) => {
                ProviderError::NotRunning(name)
            }
            other => other,
        }
    }

    fn failed(&self, why: &str) -> ProviderError {
        ProviderError::Failed(self.spec.name, why.to_owned())
    }
}

/// Dictionary words short enough to send, most important first.
fn terms(terms: &[String]) -> impl Iterator<Item = &str> {
    terms
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty() && t.chars().count() <= MAX_TERM_CHARS)
        .take(MAX_TERMS)
}

/// Whisper spells words the way its prompt does.
fn prompt(words: &[String]) -> Option<String> {
    let mut prompt = String::new();
    for term in terms(words) {
        if prompt.chars().count() + term.chars().count() + 2 > MAX_PROMPT_CHARS {
            break;
        }
        if !prompt.is_empty() {
            prompt.push_str(", ");
        }
        prompt.push_str(term);
    }
    (!prompt.is_empty()).then_some(prompt)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{closed_port, serve_once};
    use super::*;

    fn client(id: &str, base: String, model: &str) -> Stt {
        Stt::new(spec(id).unwrap(), base, Some("test-key".into()), model)
    }

    /// Half a second of a tone, loud enough not to count as silence.
    fn speech() -> Vec<f32> {
        (0..8_000).map(|i| (i as f32 / 8.0).sin() * 0.3).collect()
    }

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn every_provider_is_known_and_has_a_model_or_an_address() {
        for spec in SPECS {
            assert!(super::super::KNOWN.contains(&spec.id), "{}", spec.id);
            assert!(!spec.models.is_empty() || spec.endpoint, "{}", spec.id);
            for model in spec.translates {
                assert!(spec.models.contains(model), "{model}");
            }
        }
    }

    #[test]
    fn a_service_needs_permission_and_an_address() {
        let mut s = Settings::default();
        assert_eq!(chosen(&s), None);

        s.transcription.provider = Some(GROQ.into());
        assert_eq!(chosen(&s), Some(Err(NotReady::NoConsent("Groq"))));
        s.privacy.consented.push(consent(GROQ));
        let groq = chosen(&s).unwrap().unwrap();
        assert!(groq.cloud);
        assert_eq!(groq.model, "whisper-large-v3-turbo");
        assert_eq!(groq.base, "https://api.groq.com/openai/v1");

        // Agreeing to send Groq text isn't agreeing to send it recordings.
        s.privacy.consented = vec![llm::consent(GROQ)];
        assert_eq!(chosen(&s), Some(Err(NotReady::NoConsent("Groq"))));

        s.transcription.provider = Some(CUSTOM.into());
        assert_eq!(chosen(&s), Some(Err(NotReady::NoEndpoint)));
        s.transcription.endpoint = Some("http://localhost:8000/v1".into());
        assert_eq!(chosen(&s), Some(Err(NotReady::NoModel)));
        s.transcription.model = Some("Systran/faster-whisper-small".into());
        let own = chosen(&s).unwrap().unwrap();
        assert!(!own.cloud, "a server on this PC needs no permission");
    }

    #[tokio::test]
    async fn whisper_on_groq_says_which_language_it_heard() {
        let (base, request) = serve_once(200, r#"{"text":" Kal milte hain. ","language":"Hindi"}"#);
        let terms = words(&["Priya", "Joona"]);
        let heard = client(GROQ, base, "whisper-large-v3-turbo")
            .transcribe(
                speech(),
                Listen {
                    candidates: &words(&["en", "hi"]),
                    terms: &terms,
                    ..Listen::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(heard.text, "Kal milte hain.");
        assert_eq!(heard.language.as_deref(), Some("hi"));
        assert!(!heard.translated);

        let request = request.join().unwrap();
        assert!(
            request.starts_with("POST /audio/transcriptions"),
            "{request}"
        );
        assert!(request.contains("authorization: Bearer test-key"));
        assert!(request.contains("whisper-large-v3-turbo"));
        assert!(request.contains("verbose_json"));
        assert!(request.contains("Priya, Joona"));
        assert!(!request.contains("name=\"language\""));
    }

    #[tokio::test]
    async fn whisper_large_v3_turns_speech_into_english() {
        let (base, request) = serve_once(200, r#"{"text":"See you tomorrow."}"#);
        let heard = client(GROQ, base, "whisper-large-v3")
            .transcribe(
                speech(),
                Listen {
                    language: Some("hi"),
                    to_english: true,
                    ..Listen::default()
                },
            )
            .await
            .unwrap();
        assert!(heard.translated);
        assert_eq!(heard.language.as_deref(), Some("hi"));
        let request = request.join().unwrap();
        assert!(request.starts_with("POST /audio/translations"), "{request}");
        assert!(!request.contains("name=\"language\""));
    }

    #[tokio::test]
    async fn gpt_transcribe_gets_keywords_and_the_possible_languages() {
        let (base, request) = serve_once(
            200,
            r#"{"text":"Hello Priya.","languages":[{"code":"en"}]}"#,
        );
        let terms = words(&["Priya"]);
        let heard = client(OPENAI, base, "gpt-transcribe")
            .transcribe(
                speech(),
                Listen {
                    candidates: &words(&["en", "ta"]),
                    terms: &terms,
                    ..Listen::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(heard.language.as_deref(), Some("en"));
        let request = request.join().unwrap();
        assert!(request.contains("name=\"keywords[]\"\r\n\r\nPriya"));
        assert!(request.contains("name=\"languages[]\"\r\n\r\nta"));
        assert!(request.contains("name=\"response_format\"\r\n\r\njson"));
        assert!(!request.contains("name=\"prompt\""));
    }

    #[tokio::test]
    async fn deepgram_gets_raw_audio_and_key_terms() {
        let (base, request) = serve_once(
            200,
            r#"{"results":{"channels":[{"detected_language":"hi","alternatives":[{"transcript":"Namaste."}]}]}}"#,
        );
        let terms = words(&["Priya"]);
        let heard = client(DEEPGRAM, base, "nova-3")
            .transcribe(
                speech(),
                Listen {
                    terms: &terms,
                    ..Listen::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(heard.text, "Namaste.");
        assert_eq!(heard.language.as_deref(), Some("hi"));
        let request = request.join().unwrap();
        let line = request.lines().next().unwrap();
        assert!(line.starts_with("POST /listen?model=nova-3"), "{line}");
        assert!(line.contains("detect_language=true"));
        assert!(line.contains("keyterm=Priya"));
        assert!(request.contains("authorization: Token test-key"));
        assert!(request.contains("content-type: audio/wav"));
    }

    #[tokio::test]
    async fn elevenlabs_gets_pcm_and_writes_three_letter_codes() {
        let (base, request) = serve_once(200, r#"{"text":"Vanakkam.","language_code":"tam"}"#);
        let heard = client(ELEVENLABS, base, "scribe_v2")
            .transcribe(speech(), Listen::default())
            .await
            .unwrap();
        assert_eq!(heard.language.as_deref(), Some("ta"));
        let request = request.join().unwrap();
        assert!(request.starts_with("POST /speech-to-text"));
        assert!(request.contains("xi-api-key: test-key"));
        assert!(request.contains("pcm_s16le_16"));
        assert!(request.contains("name=\"tag_audio_events\"\r\n\r\nfalse"));
    }

    #[tokio::test]
    async fn silence_isnt_sent() {
        let heard = client(GROQ, closed_port(), "whisper-large-v3-turbo")
            .transcribe(vec![0.0; 16_000], Listen::default())
            .await
            .unwrap();
        assert_eq!(heard.text, "");
    }

    #[tokio::test]
    async fn errors_say_what_to_fix() {
        let (base, _) = serve_once(
            401,
            r#"{"err_code":"INVALID_AUTH","err_msg":"Invalid credentials."}"#,
        );
        let error = client(DEEPGRAM, base, "nova-3")
            .transcribe(speech(), Listen::default())
            .await
            .unwrap_err();
        assert_eq!(error, ProviderError::BadKey("Deepgram"));

        let (base, _) = serve_once(
            401,
            r#"{"detail":{"status":"quota_exceeded","message":"This request exceeds your quota."}}"#,
        );
        let error = client(ELEVENLABS, base, "scribe_v2")
            .transcribe(speech(), Listen::default())
            .await
            .unwrap_err();
        assert_eq!(error, ProviderError::NoCredit("ElevenLabs"));

        let error = Stt::new(spec(CUSTOM).unwrap(), closed_port(), None, "whisper")
            .transcribe(speech(), Listen::default())
            .await
            .unwrap_err();
        assert_eq!(error, ProviderError::NotRunning("Your own server"));
    }

    #[tokio::test]
    async fn checking_a_key() {
        let (base, request) = serve_once(200, r#"{"projects":[]}"#);
        client(DEEPGRAM, base, "nova-3").check().await.unwrap();
        assert!(request.join().unwrap().starts_with("GET /projects"));

        let (base, _) = serve_once(
            401,
            r#"{"detail":{"status":"missing_permissions","message":"The API key is missing the permission user_read."}}"#,
        );
        client(ELEVENLABS, base, "scribe_v2").check().await.unwrap();

        let (base, _) = serve_once(404, r#"{"detail":"Not Found"}"#);
        Stt::new(spec(CUSTOM).unwrap(), base, None, "whisper")
            .check()
            .await
            .unwrap();

        let (base, _) = serve_once(401, r#"{"error":{"message":"Invalid API Key"}}"#);
        let error = client(GROQ, base, "whisper-large-v3-turbo")
            .check()
            .await
            .unwrap_err();
        assert_eq!(error, ProviderError::BadKey("Groq"));
    }

    #[test]
    fn the_prompt_fits_whisper() {
        let many: Vec<String> = (0..200).map(|i| format!("Term{i}")).collect();
        let prompt = prompt(&many).unwrap();
        assert!(prompt.chars().count() <= MAX_PROMPT_CHARS);
        assert!(prompt.starts_with("Term0, Term1"));
        assert_eq!(super::prompt(&[]), None);
    }
}
