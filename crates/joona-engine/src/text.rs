//! Removes text that speech models produce without anyone saying it.

use std::sync::LazyLock;

use regex::Regex;

/// Whisper learned these from video subtitles and emits them for silence or noise.
const INVENTED: &[&str] = &[
    "thanks for watching",
    "thank you for watching",
    "thanks for watching and see you next time",
    "please subscribe",
    "like and subscribe",
    "dont forget to like and subscribe",
    "subtitles by the amaraorg community",
    "subtitles by",
    "transcribed by",
    "transcription by castingwords",
    "translated by",
];

/// Only suspicious when there was no voice detection to vouch for real speech.
const INVENTED_WITHOUT_VAD: &[&str] =
    &["thank you", "thanks", "you", "bye", "okay", "so", "the end"];

static BRACKETED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]").unwrap());
static SOUND_TAGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[(*]\s*(music|upbeat music|soft music|applause|laughs?|laughter|sighs?|coughs?|clears throat|silence|inaudible|background noise|static|no audio|blank audio)\s*[)*]",
    )
    .unwrap()
});
static MUSIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[♪♫♬]+").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());

pub fn clean(text: &str, had_vad: bool) -> String {
    let t = BRACKETED.replace_all(text, " ");
    let t = SOUND_TAGS.replace_all(&t, " ");
    let t = MUSIC.replace_all(&t, " ");
    let t = SPACES.replace_all(t.trim(), " ").into_owned();
    let key = comparison_key(&t);
    if key.is_empty()
        || INVENTED.contains(&key.as_str())
        || (!had_vad && INVENTED_WITHOUT_VAD.contains(&key.as_str()))
    {
        return String::new();
    }
    collapse_repeats(&t)
}

/// Lowercase letters, digits and single spaces only.
fn comparison_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if c.is_whitespace() && !out.ends_with(' ') && !out.is_empty() {
            out.push(' ');
        }
    }
    out.trim_end().to_string()
}

/// Whisper sometimes loops on a phrase. Three or more identical sentences in a
/// row, or a word repeated four or more times, are cut down to one.
fn collapse_repeats(text: &str) -> String {
    let mut sentences: Vec<&str> = Vec::new();
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '।') {
            let end = i + c.len_utf8();
            sentences.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        sentences.push(&text[start..]);
    }

    let mut kept: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < sentences.len() {
        let key = comparison_key(sentences[i]);
        let mut run = 1;
        while i + run < sentences.len()
            && !key.is_empty()
            && comparison_key(sentences[i + run]) == key
        {
            run += 1;
        }
        let keep = if run >= 3 { 1 } else { run };
        kept.extend(&sentences[i..i + keep]);
        i += run;
    }
    let joined = kept.concat();

    let words: Vec<&str> = joined.split(' ').collect();
    let mut out: Vec<&str> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let key = comparison_key(words[i]);
        let mut run = 1;
        while i + run < words.len() && !key.is_empty() && comparison_key(words[i + run]) == key {
            run += 1;
        }
        if run >= 4 {
            // Keep the last copy, which carries any trailing punctuation.
            out.push(words[i + run - 1]);
        } else {
            out.extend(&words[i..i + run]);
        }
        i += run;
    }
    out.join(" ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_speech_tags_disappear() {
        assert_eq!(clean("[BLANK_AUDIO]", true), "");
        assert_eq!(clean(" (upbeat music) ", true), "");
        assert_eq!(clean("♪ ♪", true), "");
        assert_eq!(clean("Hello [Music] there.", true), "Hello there.");
        assert_eq!(clean("*laughs* That is funny.", true), "That is funny.");
    }

    #[test]
    fn parentheses_people_dictate_are_kept() {
        assert_eq!(
            clean("Bring the forms (signed) tomorrow.", true),
            "Bring the forms (signed) tomorrow."
        );
    }

    #[test]
    fn subtitle_hallucinations_are_dropped() {
        assert_eq!(clean(" Thanks for watching!", true), "");
        assert_eq!(clean("Subtitles by the Amara.org community", true), "");
    }

    #[test]
    fn thank_you_is_real_when_voice_detection_heard_speech() {
        assert_eq!(clean("Thank you.", true), "Thank you.");
        assert_eq!(clean("Thank you.", false), "");
    }

    #[test]
    fn looping_sentences_collapse() {
        assert_eq!(
            clean(
                "I will be there. I will be there. I will be there. I will be there.",
                true
            ),
            "I will be there."
        );
        assert_eq!(clean("Yes. Yes. Okay.", true), "Yes. Yes. Okay.");
    }

    #[test]
    fn stuttering_loops_collapse() {
        assert_eq!(clean("the the the the the end", true), "the end");
        assert_eq!(clean("no no no", true), "no no no");
    }

    #[test]
    fn other_scripts_survive() {
        assert_eq!(clean("नमस्ते, आप कैसे हैं?", true), "नमस्ते, आप कैसे हैं?");
        assert_eq!(clean("வணக்கம்", true), "வணக்கம்");
    }
}
