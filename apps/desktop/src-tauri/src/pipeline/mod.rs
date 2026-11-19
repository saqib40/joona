//! Turns a raw transcript into the text that gets typed. The steps run in
//! this order:
//! 1. dictionary spellings,
//! 2. spoken commands such as "new line",
//! 3. snippets, swapped for placeholders so later steps leave them alone,
//! 4. cleanup: fillers, stutters, spacing and capitals,
//! 5. the style for the kind of app the text goes into.
//!
//! With polish off, steps 4 and 5 are skipped. With Smart polish, an AI model
//! does them instead, and may translate the text as well.

mod cleanup;
mod commands;
mod matcher;
pub mod style;
pub mod vocabulary;

use std::sync::LazyLock;

use regex::Regex;

pub use self::style::{Category, StyleMap, Tone};
pub use self::vocabulary::{Shielded, Vocabulary};
use crate::settings::PolishLevel;
use crate::store::Db;

/// Tags Whisper emits for sounds that aren't speech, such as `[BLANK_AUDIO]`,
/// `(music)` or `*laughs*`.
static NON_SPEECH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[\[(*♪]\s*(blank_audio|silence|music|applause|laughter|laughs|noise|inaudible|no speech|background noise|typing|sound)\s*[\])*♪]",
    )
    .unwrap()
});

/// The dictionary, snippets and styles, read from the database once and
/// kept until they change.
#[derive(Default)]
pub struct Rules {
    pub vocabulary: Vocabulary,
    pub styles: StyleMap,
}

impl Rules {
    pub fn load(db: &Db) -> anyhow::Result<Self> {
        Ok(Self {
            vocabulary: Vocabulary::new(&db.dictionary()?, &db.snippets()?),
            styles: StyleMap::new(&db.tones()?, &db.app_categories()?),
        })
    }
}

pub struct Options {
    pub polish: PolishLevel,
    pub tone: Tone,
    /// Listen for "press enter" at the end.
    pub press_enter: bool,
    pub trailing_space: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub text: String,
    /// Press Enter once the text is in.
    pub enter: bool,
    /// Words the dictionary and cleanup changed.
    pub fixes: u32,
    /// The dictionary entries that came up.
    pub words: Vec<i64>,
    /// The snippets that were said.
    pub snippets: Vec<i64>,
}

/// A transcript after steps 1 to 3, which every polish level runs.
pub struct Prepared {
    shielded: Shielded,
    enter: bool,
    /// Words the dictionary changed.
    fixes: u32,
    words: Vec<i64>,
}

impl Prepared {
    /// The text so far, with a placeholder for each snippet.
    pub fn text(&self) -> &str {
        &self.shielded.text
    }

    /// How many snippets the text has placeholders for.
    pub fn snippets(&self) -> usize {
        self.shielded.placeholders()
    }

    /// There's something to polish besides snippets.
    pub fn has_words(&self) -> bool {
        self.shielded.text.chars().any(char::is_alphanumeric)
    }
}

pub fn process(raw: &str, vocabulary: &Vocabulary, options: &Options) -> Output {
    complete(
        prepare(raw, vocabulary, options.press_enter),
        vocabulary,
        options,
    )
}

/// Steps 1 to 3.
pub fn prepare(raw: &str, vocabulary: &Vocabulary, press_enter: bool) -> Prepared {
    let text = tidy_whitespace(&NON_SPEECH.replace_all(raw, " "));
    let (text, fixes, words) = vocabulary.fix(&text);
    let spoken = commands::apply(&text, press_enter);
    Prepared {
        shielded: vocabulary.shield(&spoken.text),
        enter: spoken.enter,
        fixes,
        words,
    }
}

/// Steps 4 and 5 by rule, as `options` say, then the snippets go back in.
pub fn complete(prepared: Prepared, vocabulary: &Vocabulary, options: &Options) -> Output {
    let mut text = prepared.shielded.text.clone();
    let mut fixes = 0;
    if options.polish != PolishLevel::Off {
        let cleaned = cleanup::clean(&text);
        fixes = cleaned.fixes;
        text = style::apply(&cleaned.text, options.tone, |w| vocabulary.is_name(w));
    }
    finish(prepared, text, fixes, vocabulary, options.trailing_space)
}

/// Finishes with `polished`, which an AI model wrote from the prepared text
/// and which still has its placeholders. A translation's changes aren't
/// counted as fixes.
pub fn complete_polished(
    prepared: Prepared,
    polished: &str,
    translated: bool,
    vocabulary: &Vocabulary,
    trailing_space: bool,
) -> Output {
    let fixes = match translated {
        true => 0,
        false => changed_words(prepared.text(), polished),
    };
    let text = tidy_whitespace(polished);
    finish(prepared, text, fixes, vocabulary, trailing_space)
}

fn finish(
    prepared: Prepared,
    text: String,
    fixes: u32,
    vocabulary: &Vocabulary,
    trailing_space: bool,
) -> Output {
    let Prepared {
        shielded,
        enter,
        fixes: spelled,
        mut words,
    } = prepared;
    words.extend(vocabulary.mentioned(&text));
    words.sort_unstable();
    words.dedup();
    let mut snippets = vocabulary.snippets_in(&shielded);
    snippets.sort_unstable();
    snippets.dedup();

    let mut text = vocabulary
        .unshield(&text, &shielded)
        .trim_matches([' ', '\t'])
        .to_owned();
    if snippets.is_empty() && !text.chars().any(|c| c.is_alphanumeric() || c == '\n') {
        text.clear();
    }
    if trailing_space && !enter && !text.is_empty() && !text.ends_with('\n') {
        text.push(' ');
    }
    Output {
        text,
        enter,
        fixes: spelled + fixes,
        words,
        snippets,
    }
}

/// Roughly how many words differ between `before` and `after`: whichever is
/// more, the words taken out or the words put in. Capitals and punctuation
/// don't count.
fn changed_words(before: &str, after: &str) -> u32 {
    let words = |text: &str| -> Vec<String> {
        text.split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };
    let (before, after) = (words(before), words(after));
    let common = match before.len().saturating_mul(after.len()) {
        // Too long to compare word by word; the difference in length will do.
        n if n > 4_000_000 => before.len().min(after.len()),
        _ => common_words(&before, &after),
    };
    let changed = (before.len() - common).max(after.len() - common);
    u32::try_from(changed).unwrap_or(u32::MAX)
}

/// The length of the longest sequence of words both have, in order.
fn common_words(a: &[String], b: &[String]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut diagonal = 0;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = match x == y {
                true => diagonal + 1,
                false => above.max(row[j]),
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Collapses runs of spaces and trims every line, keeping line breaks.
fn tidy_whitespace(text: &str) -> String {
    let lines: Vec<String> = text
        .trim()
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    lines.join("\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{DictionaryEntry, Snippet};

    fn options(polish: PolishLevel) -> Options {
        Options {
            polish,
            tone: Tone::Formal,
            press_enter: true,
            trailing_space: true,
        }
    }

    fn finish(raw: &str, trailing_space: bool) -> String {
        let options = Options {
            trailing_space,
            ..options(PolishLevel::Off)
        };
        process(raw, &Vocabulary::default(), &options).text
    }

    #[test]
    fn trims_and_adds_one_trailing_space() {
        assert_eq!(finish("  Hello   world. ", true), "Hello world. ");
        assert_eq!(finish(" Hello world.", false), "Hello world.");
    }

    #[test]
    fn drops_non_speech_tags() {
        assert_eq!(finish("[BLANK_AUDIO]", true), "");
        assert_eq!(
            finish(" (Music) Okay, let's start. [ Silence ]", true),
            "Okay, let's start. "
        );
        assert_eq!(finish("♪ music ♪", true), "");
        assert_eq!(finish("[Ravi] said hi", true), "[Ravi] said hi ");
        assert_eq!(finish(" . ", true), "");
    }

    #[test]
    fn keeps_line_breaks_and_other_scripts() {
        assert_eq!(
            finish("नमस्ते  दोस्तों\n  second   line ", true),
            "नमस्ते दोस्तों\nsecond line "
        );
    }

    fn vocabulary() -> Vocabulary {
        let entry = DictionaryEntry {
            id: 1,
            word: "Joona".into(),
            heard: Some("Juna".into()),
            starred: false,
            uses: 0,
            last_used: None,
            created_at: 0,
        };
        let snippet = Snippet {
            id: 7,
            phrase: "my signature".into(),
            text: "Thanks,\nPriya".into(),
            uses: 0,
            last_used: None,
            created_at: 0,
        };
        Vocabulary::new(&[entry], &[snippet])
    }

    #[test]
    fn runs_every_step_in_order() {
        let raw = "Um, juna is ready. New paragraph. I I think it works. My signature.";
        let out = process(raw, &vocabulary(), &options(PolishLevel::Basic));
        assert_eq!(
            out,
            Output {
                text: "Joona is ready.\n\nI think it works. Thanks,\nPriya ".into(),
                enter: false,
                fixes: 3,
                words: vec![1],
                snippets: vec![7],
            }
        );

        let off = process(raw, &vocabulary(), &options(PolishLevel::Off));
        assert_eq!(
            off.text,
            "Um, Joona is ready.\n\nI I think it works. Thanks,\nPriya "
        );
        assert_eq!(off.fixes, 1);
    }

    #[test]
    fn presses_enter_without_a_trailing_space() {
        let v = Vocabulary::default();
        let out = process(
            "See you soon. Press enter.",
            &v,
            &options(PolishLevel::Basic),
        );
        assert_eq!((out.text.as_str(), out.enter), ("See you soon.", true));
        let alone = process("Press enter.", &v, &options(PolishLevel::Basic));
        assert_eq!((alone.text.as_str(), alone.enter), ("", true));
        let filler = process("Hmm.", &v, &options(PolishLevel::Basic));
        assert_eq!((filler.text.as_str(), filler.enter), ("", false));
    }

    #[test]
    fn text_an_ai_model_polished_gets_its_snippets_back() {
        let v = vocabulary();
        let prepared = prepare("Um, so juna works. My signature.", &v, true);
        assert_eq!(prepared.snippets(), 1);
        let marker = vocabulary::placeholder(0);
        assert!(prepared.text().ends_with(&format!("{marker}.")));
        let polished = format!("So Joona works.\n{marker}");
        let out = complete_polished(prepared, &polished, false, &v, true);
        assert_eq!(out.text, "So Joona works.\nThanks,\nPriya ");
        assert_eq!(out.snippets, vec![7]);
        assert_eq!(out.words, vec![1]);
        // "juna" spelled the dictionary's way, then "Um" taken out.
        assert_eq!(out.fixes, 2);

        let translated = prepare("juna works", &v, true);
        let out = complete_polished(translated, "जूना काम करता है।", true, &v, false);
        assert_eq!((out.text.as_str(), out.fixes), ("जूना काम करता है।", 1));
    }

    #[test]
    fn counts_the_words_that_changed() {
        assert_eq!(changed_words("um I I think so", "I think so."), 2);
        assert_eq!(changed_words("Hello there", "hello, there!"), 0);
        assert_eq!(changed_words("at two no three", "at three"), 2);
        assert_eq!(changed_words("", "Hi"), 1);
    }
}
