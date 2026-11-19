//! Sarvam AI: speech-to-text for 22 Indian languages and English, and text
//! translation and romanization between them.

use std::time::Duration;

use futures_util::{StreamExt, TryStreamExt, stream};
use reqwest::multipart::{Form, Part};
use reqwest::{RequestBuilder, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use super::{ProviderError, chunk, http, text};
use crate::audio::{SAMPLE_RATE, encode_wav, level};
use crate::languages;
use crate::settings::Script;

pub const NAME: &str = "Sarvam";
pub const STT_MODEL: &str = "saaras:v4";
pub const KEYS_URL: &str = "https://dashboard.sarvam.ai/key-management";
pub const PRIVACY_URL: &str = "https://www.sarvam.ai/privacy-policy";
const API: &str = "https://api.sarvam.ai";
/// The REST endpoint takes at most 30 s of audio per request.
const MAX_PIECE: usize = 25 * SAMPLE_RATE as usize;
/// Requests in flight at once for a long recording or text.
const PARALLEL: usize = 4;
const STT_TIMEOUT: Duration = Duration::from_secs(30);
const TEXT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_KEYTERMS: usize = 50;
const MAX_KEYTERM_CHARS: usize = 64;
/// A piece this quiet is a muted stretch with nothing to transcribe.
const SILENT_PEAK: f32 = 0.002;
/// What `mayura:v1` translates. It can also write romanized or code-mixed
/// text, and `/transliterate` takes the same languages.
const MAYURA: &[&str] = &[
    "bn", "en", "gu", "hi", "kn", "ml", "mr", "or", "pa", "ta", "te",
];
const MAYURA_CHARS: usize = 1000;
const SARVAM_TRANSLATE_CHARS: usize = 2000;
const TRANSLITERATE_CHARS: usize = 1000;

/// What Sarvam writes for the speech it hears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The spoken language in its own script.
    Transcribe,
    /// English.
    Translate,
    /// The spoken language in Latin letters.
    Translit,
    /// English words in Latin letters, the rest in the language's own script.
    Codemix,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Transcribe => "transcribe",
            Mode::Translate => "translate",
            Mode::Translit => "translit",
            Mode::Codemix => "codemix",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    pub text: String,
    /// The language Sarvam heard, as an ISO code.
    pub language: Option<String>,
}

/// Sarvam's code for a language, such as `hi-IN`. Sarvam writes Odia as `od`.
pub fn code(language: &str) -> Option<String> {
    languages::find(language)
        .filter(|l| l.sarvam)
        .map(|_| match language {
            "or" => "od-IN".to_owned(),
            _ => format!("{language}-IN"),
        })
}

/// The other way round: `od-IN` becomes `or`.
fn iso(code: &str) -> String {
    let base = code.split('-').next().unwrap_or(code).to_ascii_lowercase();
    if base == "od" { "or".to_owned() } else { base }
}

/// Whether Sarvam translates text from one language into the other.
pub fn can_translate(from: &str, to: &str) -> bool {
    from != to && code(from).is_some() && code(to).is_some()
}

/// Whether Sarvam can write text in `language` in Latin letters.
pub fn can_romanize(language: &str) -> bool {
    language != "en" && MAYURA.contains(&language)
}

#[derive(Deserialize)]
struct SttResponse {
    transcript: String,
    #[serde(default)]
    language_code: Option<String>,
}

#[derive(Deserialize)]
struct TranslateResponse {
    translated_text: String,
}

#[derive(Deserialize)]
struct TransliterateResponse {
    transliterated_text: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetails,
}

#[derive(Deserialize)]
struct ErrorDetails {
    #[serde(default)]
    message: String,
    #[serde(default)]
    code: String,
}

pub struct Sarvam {
    key: String,
    base: String,
}

impl Sarvam {
    pub fn new(key: String) -> Self {
        Self {
            key,
            base: API.to_owned(),
        }
    }

    /// Turns a 16 kHz recording into text. `language` is an ISO code, or
    /// `None` to let Sarvam work it out. `keyterms` are names and terms it
    /// should listen for.
    pub async fn transcribe(
        &self,
        mut samples: Vec<f32>,
        language: Option<&str>,
        mode: Mode,
        keyterms: &[String],
    ) -> Result<Heard, ProviderError> {
        let language_code = match language {
            Some(l) => code(l).ok_or_else(|| {
                ProviderError::Rejected(NAME, format!("it doesn't know {}", languages::name(l)))
            })?,
            None => "unknown".to_owned(),
        };
        level::normalize(&mut samples);
        let keyterms = keyterms_field(keyterms);
        let pieces: Vec<Vec<u8>> = chunk::split_at_pauses(&samples, MAX_PIECE)
            .into_iter()
            .map(|range| &samples[range])
            .filter(|piece| piece.iter().any(|s| s.abs() >= SILENT_PEAK))
            .map(encode_wav)
            .collect();
        let heard: Vec<SttResponse> = stream::iter(pieces)
            .map(|wav| self.transcribe_piece(wav, &language_code, mode, keyterms.as_deref()))
            .buffered(PARALLEL)
            .try_collect()
            .await?;
        let text = heard
            .iter()
            .map(|h| h.transcript.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let detected = most_common(heard.iter().filter_map(|h| h.language_code.as_deref()));
        Ok(Heard {
            text,
            language: language.map(str::to_owned).or(detected),
        })
    }

    async fn transcribe_piece(
        &self,
        wav: Vec<u8>,
        language_code: &str,
        mode: Mode,
        keyterms: Option<&str>,
    ) -> Result<SttResponse, ProviderError> {
        let file = Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| ProviderError::Failed(NAME, e.to_string()))?;
        let mut form = Form::new()
            .part("file", file)
            .text("model", STT_MODEL)
            .text("mode", mode.as_str())
            .text("language_code", language_code.to_owned());
        if let Some(keyterms) = keyterms {
            form = form.text("keyterms", keyterms.to_owned());
        }
        send(
            self.post("speech-to-text")
                .multipart(form)
                .timeout(STT_TIMEOUT),
        )
        .await
    }

    /// Translates text from one of Sarvam's languages into another. With
    /// [`Script::Roman`] or [`Script::Mixed`] the result is written that way
    /// where Sarvam can.
    pub async fn translate(
        &self,
        input: &str,
        from: &str,
        to: &str,
        script: Script,
    ) -> Result<String, ProviderError> {
        let (Some(source), Some(target)) = (code(from), code(to)) else {
            return Err(ProviderError::Rejected(
                NAME,
                format!(
                    "it doesn't translate {} into {}",
                    languages::name(from),
                    languages::name(to)
                ),
            ));
        };
        let mayura = MAYURA.contains(&from) && MAYURA.contains(&to);
        let (model, limit) = match mayura {
            true => ("mayura:v1", MAYURA_CHARS),
            false => ("sarvam-translate:v1", SARVAM_TRANSLATE_CHARS),
        };
        let mode = match (mayura, script) {
            (true, Script::Mixed) => "code-mixed",
            _ => "formal",
        };
        let mut body = json!({
            "source_language_code": source,
            "target_language_code": target,
            "model": model,
            "mode": mode,
        });
        let latin = languages::find(to).is_none_or(|l| l.latin);
        if mayura && script == Script::Roman && !latin {
            body["output_script"] = json!("roman");
        }
        self.rewrite(input, limit, "translate", body, |r: TranslateResponse| {
            r.translated_text
        })
        .await
    }

    /// Writes text in `language` in Latin letters, such as "kal milte hain".
    pub async fn romanize(&self, input: &str, language: &str) -> Result<String, ProviderError> {
        let source = code(language)
            .filter(|_| can_romanize(language))
            .ok_or_else(|| {
                ProviderError::Rejected(
                    NAME,
                    format!(
                        "it can't write {} in English letters",
                        languages::name(language)
                    ),
                )
            })?;
        let body = json!({
            "source_language_code": source,
            "target_language_code": "en-IN",
        });
        self.rewrite(
            input,
            TRANSLITERATE_CHARS,
            "transliterate",
            body,
            |r: TransliterateResponse| r.transliterated_text,
        )
        .await
    }

    /// Checks the key with about the smallest request that needs one.
    pub async fn check(&self) -> Result<(), ProviderError> {
        self.translate("Hello", "en", "hi", Script::Native)
            .await
            .map(|_| ())
    }

    /// Sends `input` in pieces Sarvam accepts, each as `body` plus `input`,
    /// and joins what comes back.
    async fn rewrite<R: DeserializeOwned>(
        &self,
        input: &str,
        limit: usize,
        path: &str,
        body: serde_json::Value,
        text_of: fn(R) -> String,
    ) -> Result<String, ProviderError> {
        let pieces = text::split(input, limit);
        let requests: Vec<RequestBuilder> = text::bodies(&pieces)
            .into_iter()
            .map(|piece| {
                let mut body = body.clone();
                body["input"] = json!(piece);
                self.post(path).json(&body).timeout(TEXT_TIMEOUT)
            })
            .collect();
        let rewritten: Vec<String> = stream::iter(requests)
            .map(|request| async move { send::<R>(request).await.map(text_of) })
            .buffered(PARALLEL)
            .try_collect()
            .await?;
        Ok(text::join(&pieces, rewritten))
    }

    fn post(&self, path: &str) -> RequestBuilder {
        http()
            .post(format!("{}/{path}", self.base))
            .header("api-subscription-key", &self.key)
    }
}

async fn send<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, ProviderError> {
    let response = request
        .send()
        .await
        .map_err(|e| ProviderError::network(NAME, &e))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| ProviderError::network(NAME, &e))?;
    if !status.is_success() {
        return Err(error_from(status, &body));
    }
    serde_json::from_str(&body)
        .map_err(|e| ProviderError::Failed(NAME, format!("its answer didn't make sense ({e})")))
}

fn error_from(status: StatusCode, body: &str) -> ProviderError {
    let details = serde_json::from_str::<ErrorBody>(body)
        .ok()
        .map(|b| b.error);
    let code = details.as_ref().map_or("", |d| d.code.as_str());
    let message = details
        .as_ref()
        .map(|d| d.message.trim())
        .filter(|m| !m.is_empty())
        .map_or_else(
            || {
                status
                    .canonical_reason()
                    .unwrap_or("unknown error")
                    .to_owned()
            },
            str::to_owned,
        );
    match (status.as_u16(), code) {
        (_, "invalid_api_key_error" | "authentication_error") | (401, _) | (403, "") => {
            ProviderError::BadKey(NAME)
        }
        (_, "insufficient_quota_error") => ProviderError::NoCredit(NAME),
        (_, "rate_limit_exceeded_error") | (429, _) => ProviderError::RateLimited(NAME),
        (500.., _) => ProviderError::Failed(NAME, message),
        _ => ProviderError::Rejected(NAME, message),
    }
}

/// Sarvam takes up to 50 terms of up to 64 characters, as a JSON array.
fn keyterms_field(keyterms: &[String]) -> Option<String> {
    let terms: Vec<&str> = keyterms
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty() && t.chars().count() <= MAX_KEYTERM_CHARS)
        .take(MAX_KEYTERMS)
        .collect();
    (!terms.is_empty()).then(|| serde_json::to_string(&terms).unwrap_or_default())
}

/// The language most pieces of a long recording were in.
fn most_common<'a>(codes: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for language in codes.map(iso) {
        match counts.iter_mut().find(|(l, _)| *l == language) {
            Some((_, n)) => *n += 1,
            None => counts.push((language, 1)),
        }
    }
    counts
        .into_iter()
        .rev()
        .max_by_key(|(_, n)| *n)
        .map(|(l, _)| l)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{closed_port, json_body, serve_once};
    use super::*;

    fn client(base: String) -> Sarvam {
        Sarvam {
            key: "test-key".into(),
            base,
        }
    }

    #[test]
    fn language_codes() {
        assert_eq!(code("hi").as_deref(), Some("hi-IN"));
        assert_eq!(code("or").as_deref(), Some("od-IN"));
        assert_eq!(code("kok").as_deref(), Some("kok-IN"));
        assert_eq!(code("fr"), None);
        assert_eq!(iso("od-IN"), "or");
        assert_eq!(iso("kok-IN"), "kok");
        assert!(can_translate("ta", "hi"));
        assert!(!can_translate("en", "fr"));
        assert!(!can_translate("hi", "hi"));
        assert!(can_romanize("ta"));
        assert!(!can_romanize("kok"));
        assert!(!can_romanize("en"));
    }

    #[test]
    fn keyterms_are_a_json_list_within_limits() {
        assert_eq!(keyterms_field(&[]), None);
        let terms = [
            "Joona".to_owned(),
            " ".into(),
            "New Delhi".into(),
            "x".repeat(65),
        ];
        assert_eq!(
            keyterms_field(&terms).as_deref(),
            Some(r#"["Joona","New Delhi"]"#)
        );
        let many: Vec<String> = (0..80).map(|i| format!("term {i}")).collect();
        let field: Vec<String> = serde_json::from_str(&keyterms_field(&many).unwrap()).unwrap();
        assert_eq!(field.len(), MAX_KEYTERMS);
    }

    #[test]
    fn errors_say_what_to_do() {
        let bad_key = r#"{"error":{"code":"invalid_api_key_error","message":"Invalid API key","request_id":null}}"#;
        assert_eq!(
            error_from(StatusCode::FORBIDDEN, bad_key),
            ProviderError::BadKey(NAME)
        );
        assert_eq!(
            error_from(
                StatusCode::TOO_MANY_REQUESTS,
                r#"{"error":{"code":"insufficient_quota_error","message":"Quota exceeded"}}"#
            ),
            ProviderError::NoCredit(NAME)
        );
        assert_eq!(
            error_from(StatusCode::TOO_MANY_REQUESTS, "{}"),
            ProviderError::RateLimited(NAME)
        );
        assert_eq!(
            error_from(
                StatusCode::BAD_REQUEST,
                r#"{"error":{"code":"invalid_request_error","message":"Audio is too long"}}"#
            ),
            ProviderError::Rejected(NAME, "Audio is too long".into())
        );
        assert_eq!(
            error_from(StatusCode::SERVICE_UNAVAILABLE, "<html>busy</html>"),
            ProviderError::Failed(NAME, "Service Unavailable".into())
        );
        assert!(ProviderError::BadKey(NAME).needs_account());
        assert!(!ProviderError::Offline(NAME).needs_account());
    }

    #[test]
    fn the_most_common_language_wins() {
        let codes = ["hi-IN", "en-IN", "hi-IN"];
        assert_eq!(most_common(codes.into_iter()).as_deref(), Some("hi"));
        assert_eq!(most_common(["od-IN"].into_iter()).as_deref(), Some("or"));
        assert_eq!(most_common(std::iter::empty()), None);
    }

    #[tokio::test]
    async fn transcribe_sends_the_documented_form() {
        let (base, server) = serve_once(
            200,
            r#"{"request_id":"r1","transcript":" नमस्ते दोस्तों ","language_code":"hi-IN"}"#,
        );
        let terms = ["Joona".to_owned(), "Priya".into()];
        let heard = client(base)
            .transcribe(vec![0.2; 16_000], None, Mode::Codemix, &terms)
            .await
            .unwrap();
        assert_eq!(
            heard,
            Heard {
                text: "नमस्ते दोस्तों".into(),
                language: Some("hi".into()),
            }
        );
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /speech-to-text "), "{request}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("api-subscription-key: test-key")
        );
        assert!(request.contains("filename=\"audio.wav\""));
        for (name, value) in [
            ("model", "saaras:v4"),
            ("mode", "codemix"),
            ("language_code", "unknown"),
            ("keyterms", r#"["Joona","Priya"]"#),
        ] {
            assert!(
                request.contains(&format!("name=\"{name}\"\r\n\r\n{value}\r\n")),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn silence_is_never_sent() {
        let heard = client("http://127.0.0.1:9".into())
            .transcribe(vec![0.0; 16_000], Some("ta"), Mode::Transcribe, &[])
            .await
            .unwrap();
        assert_eq!(heard.text, "");
        assert_eq!(heard.language.as_deref(), Some("ta"));
    }

    #[tokio::test]
    async fn translate_picks_the_model_and_script() {
        let (base, server) = serve_once(
            200,
            r#"{"request_id":"r2","translated_text":"kal milte hain.","source_language_code":"en-IN"}"#,
        );
        let text = client(base)
            .translate("See you tomorrow.", "en", "hi", Script::Roman)
            .await
            .unwrap();
        assert_eq!(text, "kal milte hain.");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /translate "));
        let body = json_body(&request);
        assert_eq!(body["input"], "See you tomorrow.");
        assert_eq!(body["source_language_code"], "en-IN");
        assert_eq!(body["target_language_code"], "hi-IN");
        assert_eq!(body["model"], "mayura:v1");
        assert_eq!(body["output_script"], "roman");
    }

    #[tokio::test]
    async fn bad_keys_and_missing_networks_are_told_apart() {
        let (base, _server) = serve_once(
            403,
            r#"{"error":{"code":"invalid_api_key_error","message":"Invalid API key"}}"#,
        );
        assert_eq!(client(base).check().await, Err(ProviderError::BadKey(NAME)));

        let offline = client(closed_port()).check().await;
        assert_eq!(offline, Err(ProviderError::Offline(NAME)));
    }
}
