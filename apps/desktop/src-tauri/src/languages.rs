//! Languages Joona knows about, and which engines understand them.

use serde::Serialize;
use specta::Type;

use LocalQuality::{Fair, Good, None as Unknown, Weak};

/// How well Whisper on this PC understands a language. A rough guide from
/// published FLEURS error rates for the larger models; small models do worse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LocalQuality {
    Good,
    /// Usable, with more mistakes.
    Fair,
    /// Often wrong.
    Weak,
    /// Whisper doesn't know the language.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    /// ISO 639-1 code, or ISO 639-2/3 where there is no two-letter code.
    pub code: &'static str,
    pub name: &'static str,
    /// The language's own name for itself.
    pub native: &'static str,
    /// Whisper models can transcribe it.
    pub whisper: bool,
    pub local: LocalQuality,
    /// Sarvam's speech-to-text supports it.
    pub sarvam: bool,
    /// Written in Latin letters, so there's nothing to romanize.
    pub latin: bool,
}

const fn lang(
    code: &'static str,
    name: &'static str,
    native: &'static str,
    local: LocalQuality,
    sarvam: bool,
) -> Language {
    Language {
        code,
        name,
        native,
        whisper: !matches!(local, LocalQuality::None),
        local,
        sarvam,
        latin: latin(native),
    }
}

/// Whether a name is written in Latin letters, judged by its first letter.
const fn latin(native: &str) -> bool {
    // ASCII, or the UTF-8 lead byte of U+00C0 to U+02BF: accented Latin
    // letters and modifier letters such as the ʻokina in ʻŌlelo Hawaiʻi.
    matches!(
        native.as_bytes(),
        [] | [0x00..=0x7F, ..] | [0xC3..=0xCA, ..]
    )
}

/// Indian languages first, then the rest of Whisper's languages by name.
pub const LANGUAGES: &[Language] = &[
    lang("en", "English", "English", Good, true),
    lang("hi", "Hindi", "हिन्दी", Fair, true),
    lang("bn", "Bengali", "বাংলা", Weak, true),
    lang("ta", "Tamil", "தமிழ்", Fair, true),
    lang("te", "Telugu", "తెలుగు", Weak, true),
    lang("mr", "Marathi", "मराठी", Weak, true),
    lang("gu", "Gujarati", "ગુજરાતી", Weak, true),
    lang("kn", "Kannada", "ಕನ್ನಡ", Weak, true),
    lang("ml", "Malayalam", "മലയാളം", Weak, true),
    lang("pa", "Punjabi", "ਪੰਜਾਬੀ", Weak, true),
    lang("or", "Odia", "ଓଡ଼ିଆ", Unknown, true),
    lang("as", "Assamese", "অসমীয়া", Weak, true),
    lang("ur", "Urdu", "اردو", Fair, true),
    lang("ne", "Nepali", "नेपाली", Weak, true),
    lang("sa", "Sanskrit", "संस्कृतम्", Weak, true),
    lang("sd", "Sindhi", "سنڌي", Weak, true),
    lang("kok", "Konkani", "कोंकणी", Unknown, true),
    lang("ks", "Kashmiri", "کٲشُر", Unknown, true),
    lang("mai", "Maithili", "मैथिली", Unknown, true),
    lang("doi", "Dogri", "डोगरी", Unknown, true),
    lang("mni", "Manipuri", "ꯃꯩꯇꯩꯂꯣꯟ", Unknown, true),
    lang("brx", "Bodo", "बड़ो", Unknown, true),
    lang("sat", "Santali", "ᱥᱟᱱᱛᱟᱲᱤ", Unknown, true),
    lang("af", "Afrikaans", "Afrikaans", Fair, false),
    lang("sq", "Albanian", "Shqip", Weak, false),
    lang("am", "Amharic", "አማርኛ", Weak, false),
    lang("ar", "Arabic", "العربية", Fair, false),
    lang("hy", "Armenian", "Հայերեն", Fair, false),
    lang("az", "Azerbaijani", "Azərbaycan", Fair, false),
    lang("ba", "Bashkir", "Башҡорт", Weak, false),
    lang("eu", "Basque", "Euskara", Fair, false),
    lang("be", "Belarusian", "Беларуская", Fair, false),
    lang("bs", "Bosnian", "Bosanski", Fair, false),
    lang("br", "Breton", "Brezhoneg", Weak, false),
    lang("bg", "Bulgarian", "Български", Fair, false),
    lang("my", "Burmese", "မြန်မာ", Weak, false),
    lang("yue", "Cantonese", "粵語", Fair, false),
    lang("ca", "Catalan", "Català", Good, false),
    lang("zh", "Chinese", "中文", Good, false),
    lang("hr", "Croatian", "Hrvatski", Fair, false),
    lang("cs", "Czech", "Čeština", Good, false),
    lang("da", "Danish", "Dansk", Good, false),
    lang("nl", "Dutch", "Nederlands", Good, false),
    lang("et", "Estonian", "Eesti", Fair, false),
    lang("fo", "Faroese", "Føroyskt", Weak, false),
    lang("fi", "Finnish", "Suomi", Good, false),
    lang("fr", "French", "Français", Good, false),
    lang("gl", "Galician", "Galego", Fair, false),
    lang("ka", "Georgian", "ქართული", Weak, false),
    lang("de", "German", "Deutsch", Good, false),
    lang("el", "Greek", "Ελληνικά", Good, false),
    lang("ht", "Haitian Creole", "Kreyòl ayisyen", Weak, false),
    lang("ha", "Hausa", "Hausa", Weak, false),
    lang("haw", "Hawaiian", "ʻŌlelo Hawaiʻi", Weak, false),
    lang("he", "Hebrew", "עברית", Fair, false),
    lang("hu", "Hungarian", "Magyar", Good, false),
    lang("is", "Icelandic", "Íslenska", Fair, false),
    lang("id", "Indonesian", "Bahasa Indonesia", Good, false),
    lang("it", "Italian", "Italiano", Good, false),
    lang("ja", "Japanese", "日本語", Good, false),
    lang("jw", "Javanese", "Basa Jawa", Weak, false),
    lang("kk", "Kazakh", "Қазақ", Fair, false),
    lang("km", "Khmer", "ខ្មែរ", Weak, false),
    lang("ko", "Korean", "한국어", Good, false),
    lang("lo", "Lao", "ລາວ", Weak, false),
    lang("la", "Latin", "Latina", Weak, false),
    lang("lv", "Latvian", "Latviešu", Fair, false),
    lang("ln", "Lingala", "Lingála", Weak, false),
    lang("lt", "Lithuanian", "Lietuvių", Fair, false),
    lang("lb", "Luxembourgish", "Lëtzebuergesch", Weak, false),
    lang("mk", "Macedonian", "Македонски", Fair, false),
    lang("mg", "Malagasy", "Malagasy", Weak, false),
    lang("ms", "Malay", "Bahasa Melayu", Good, false),
    lang("mt", "Maltese", "Malti", Weak, false),
    lang("mi", "Maori", "Te Reo Māori", Weak, false),
    lang("mn", "Mongolian", "Монгол", Weak, false),
    lang("no", "Norwegian", "Norsk", Good, false),
    lang("nn", "Norwegian Nynorsk", "Nynorsk", Weak, false),
    lang("oc", "Occitan", "Occitan", Weak, false),
    lang("ps", "Pashto", "پښتو", Weak, false),
    lang("fa", "Persian", "فارسی", Fair, false),
    lang("pl", "Polish", "Polski", Good, false),
    lang("pt", "Portuguese", "Português", Good, false),
    lang("ro", "Romanian", "Română", Good, false),
    lang("ru", "Russian", "Русский", Good, false),
    lang("sr", "Serbian", "Српски", Fair, false),
    lang("sn", "Shona", "chiShona", Weak, false),
    lang("si", "Sinhala", "සිංහල", Weak, false),
    lang("sk", "Slovak", "Slovenčina", Fair, false),
    lang("sl", "Slovenian", "Slovenščina", Fair, false),
    lang("so", "Somali", "Soomaali", Weak, false),
    lang("es", "Spanish", "Español", Good, false),
    lang("su", "Sundanese", "Basa Sunda", Weak, false),
    lang("sw", "Swahili", "Kiswahili", Fair, false),
    lang("sv", "Swedish", "Svenska", Good, false),
    lang("tl", "Tagalog", "Tagalog", Fair, false),
    lang("tg", "Tajik", "Тоҷикӣ", Weak, false),
    lang("tt", "Tatar", "Татар", Weak, false),
    lang("th", "Thai", "ไทย", Fair, false),
    lang("bo", "Tibetan", "བོད་སྐད་", Weak, false),
    lang("tr", "Turkish", "Türkçe", Good, false),
    lang("tk", "Turkmen", "Türkmençe", Weak, false),
    lang("uk", "Ukrainian", "Українська", Good, false),
    lang("uz", "Uzbek", "Oʻzbek", Weak, false),
    lang("vi", "Vietnamese", "Tiếng Việt", Good, false),
    lang("cy", "Welsh", "Cymraeg", Fair, false),
    lang("yi", "Yiddish", "ייִדיש", Weak, false),
    lang("yo", "Yoruba", "Yorùbá", Weak, false),
];

pub fn find(code: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|l| l.code == code)
}

/// Three-letter codes (ISO 639-2 and 639-3) for languages that also have a
/// two-letter one. Services that write those use these.
const THREE_LETTER: &[(&str, &str)] = &[
    ("eng", "en"),
    ("hin", "hi"),
    ("ben", "bn"),
    ("tam", "ta"),
    ("tel", "te"),
    ("mar", "mr"),
    ("guj", "gu"),
    ("kan", "kn"),
    ("mal", "ml"),
    ("pan", "pa"),
    ("ori", "or"),
    ("ory", "or"),
    ("asm", "as"),
    ("urd", "ur"),
    ("nep", "ne"),
    ("npi", "ne"),
    ("san", "sa"),
    ("snd", "sd"),
    ("kas", "ks"),
    ("ara", "ar"),
    ("zho", "zh"),
    ("cmn", "zh"),
    ("chi", "zh"),
    ("nld", "nl"),
    ("dut", "nl"),
    ("fra", "fr"),
    ("fre", "fr"),
    ("deu", "de"),
    ("ger", "de"),
    ("ell", "el"),
    ("gre", "el"),
    ("ind", "id"),
    ("ita", "it"),
    ("jpn", "ja"),
    ("kor", "ko"),
    ("msa", "ms"),
    ("zlm", "ms"),
    ("fas", "fa"),
    ("per", "fa"),
    ("pol", "pl"),
    ("por", "pt"),
    ("ron", "ro"),
    ("rum", "ro"),
    ("rus", "ru"),
    ("spa", "es"),
    ("swe", "sv"),
    ("tgl", "tl"),
    ("fil", "tl"),
    ("tha", "th"),
    ("tur", "tr"),
    ("ukr", "uk"),
    ("vie", "vi"),
    ("heb", "he"),
    ("ces", "cs"),
    ("cze", "cs"),
    ("dan", "da"),
    ("fin", "fi"),
    ("hun", "hu"),
    ("nor", "no"),
    ("nob", "no"),
    ("swa", "sw"),
    ("swh", "sw"),
];

/// Joona's code for a language as a speech service wrote it: a code such as
/// `hi` or `hi-IN`, a three-letter code such as `hin`, or a name such as
/// `hindi`.
pub fn recognize(written: &str) -> Option<&'static str> {
    let written = written.trim();
    let base = written
        .split(['-', '_'])
        .next()
        .unwrap_or(written)
        .to_ascii_lowercase();
    if let Some(language) = find(&base) {
        return Some(language.code);
    }
    if let Some((_, code)) = THREE_LETTER.iter().find(|(three, _)| *three == base) {
        return Some(code);
    }
    LANGUAGES
        .iter()
        .find(|l| l.name.eq_ignore_ascii_case(written))
        .map(|l| l.code)
}

/// The English name, or the code itself for a language Joona doesn't know.
pub fn name(code: &str) -> String {
    find(code).map_or_else(|| code.to_uppercase(), |l| l.name.to_owned())
}

/// Whisper knows the language, or Joona doesn't and leaves it to the model.
pub fn whisper_knows(code: &str) -> bool {
    find(code).is_none_or(|l| l.whisper)
}

/// The closest language Whisper knows, for one it doesn't. Whisper hears
/// Odia as Bengali, for example, which is enough to notice that the speech
/// is meant for Sarvam rather than for a language that stays on this PC.
pub fn stand_in(code: &str) -> Option<&'static str> {
    match code {
        "or" | "mni" => Some("bn"),
        "kok" => Some("mr"),
        "ks" => Some("ur"),
        "doi" => Some("pa"),
        "mai" | "brx" | "sat" => Some("hi"),
        _ => None,
    }
}

/// The one language among `candidates` written in the script `text` is in,
/// for a service that doesn't say which language it heard.
pub fn guess(text: &str, candidates: &[String]) -> Option<String> {
    let first = script(text.chars().find(|c| c.is_alphabetic())?);
    let mut matching = candidates.iter().filter(|code| {
        find(code)
            .and_then(|l| l.native.chars().next())
            .is_some_and(|c| script(c) == first)
    });
    match (matching.next(), matching.next()) {
        (Some(only), None) => Some(only.clone()),
        _ => None,
    }
}

/// A rough stand-in for the script `c` is in: its block of 128 characters,
/// with Latin, Cyrillic, Arabic and Chinese characters as one block each.
fn script(c: char) -> u32 {
    match u32::from(c) {
        0..=0x2AF | 0x1E00..=0x1EFF => 0,
        0x400..=0x52F => 0x400,
        0x600..=0x6FF | 0x750..=0x77F => 0x600,
        0x3400..=0x9FFF => 0x3400,
        code => code & !0x7F,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transcripts_script_tells_its_language() {
        let spoken: Vec<String> = ["en", "hi", "ta"].map(String::from).into();
        assert_eq!(guess("नमस्ते, कैसे हो?", &spoken).as_deref(), Some("hi"));
        assert_eq!(guess("வணக்கம்", &spoken).as_deref(), Some("ta"));
        assert_eq!(guess("Hello there", &spoken).as_deref(), Some("en"));
        assert_eq!(guess("123 नमस्ते", &spoken).as_deref(), Some("hi"));
        // Hindi and Marathi share a script.
        assert_eq!(guess("नमस्कार", &["hi".into(), "mr".into()]), None);
        assert_eq!(guess("...", &spoken), None);
        assert_eq!(guess("Привет", &spoken), None);
    }

    #[test]
    fn codes_are_unique() {
        for (i, a) in LANGUAGES.iter().enumerate() {
            assert!(
                LANGUAGES[i + 1..].iter().all(|b| b.code != a.code),
                "{} twice",
                a.code
            );
        }
    }

    #[test]
    fn whisper_knows_about_a_hundred_languages() {
        let whisper = LANGUAGES.iter().filter(|l| l.whisper).count();
        assert!((95..=100).contains(&whisper), "{whisper}");
        assert!(
            LANGUAGES
                .iter()
                .all(|l| l.whisper == (l.local != LocalQuality::None))
        );
    }

    #[test]
    fn names() {
        assert_eq!(name("hi"), "Hindi");
        assert_eq!(name("xx"), "XX");
        assert!(find("or").is_some_and(|l| l.sarvam && !l.whisper));
    }

    #[test]
    fn languages_as_services_write_them() {
        for (written, code) in [
            ("hi", "hi"),
            ("hi-IN", "hi"),
            ("EN", "en"),
            ("eng", "en"),
            ("ory", "or"),
            ("kok", "kok"),
            ("hindi", "hi"),
            ("Tamil", "ta"),
            ("pt-BR", "pt"),
        ] {
            assert_eq!(recognize(written), Some(code), "{written}");
        }
        assert_eq!(recognize("klingon"), None);
        assert_eq!(recognize(""), None);
        for (three, two) in THREE_LETTER {
            assert!(find(two).is_some(), "{three} maps to unknown {two}");
        }
    }

    #[test]
    fn languages_whisper_lacks_have_a_stand_in_it_knows() {
        for language in LANGUAGES.iter().filter(|l| !l.whisper) {
            let stand_in = stand_in(language.code)
                .unwrap_or_else(|| panic!("{} has no stand-in", language.code));
            assert!(whisper_knows(stand_in), "{stand_in}");
        }
        assert_eq!(stand_in("hi"), None);
        assert!(whisper_knows("xx"));
    }

    #[test]
    fn scripts() {
        let latin: Vec<_> = ["en", "fr", "cs", "tr", "vi", "haw", "uz"]
            .iter()
            .filter(|c| !find(c).unwrap().latin)
            .collect();
        assert!(latin.is_empty(), "not Latin: {latin:?}");
        let other: Vec<_> = ["hi", "ta", "ur", "ru", "el", "zh", "ja", "mni", "sat"]
            .iter()
            .filter(|c| find(c).unwrap().latin)
            .collect();
        assert!(other.is_empty(), "Latin: {other:?}");
    }
}
