//! Rule-based cleanup for Basic polish: drops fillers and stutters, tidies
//! spacing and puts capitals where sentences start.

use std::sync::LazyLock;

use regex::Regex;

/// Quotes and brackets that can open a word.
const OPENERS: &[char] = &['"', '\'', '“', '‘', '(', '['];
/// Punctuation that can close a word.
const CLOSERS: &[char] = &[
    '.', ',', ';', ':', '!', '?', '…', '।', '"', '\'', '”', '’', ')', ']',
];
const ENDS: &[char] = &['.', '!', '?', '…', '।'];

/// Words that end with a full stop without ending the sentence.
const ABBREVIATIONS: &[&str] = &[
    "e.g", "i.e", "etc", "vs", "mr", "mrs", "ms", "dr", "prof", "st", "jr", "sr", "a.m", "p.m",
    "approx", "dept", "inc", "ltd",
];

/// Words people repeat when they stumble, as in "I I think", and never twice
/// on purpose. "That that" and "so-so" aren't here for that reason.
const STUMBLES: &[&str] = &[
    "i", "i'm", "i'll", "i've", "i'd", "a", "an", "the", "and", "but", "or", "to", "of", "with",
    "if", "it", "it's", "we", "we're", "you're", "they", "they're", "she", "this", "there's",
    "let's",
];

static FILLER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:u+m+|u+h+m*|e+r+m+|a+h+m+|h+m+)$").unwrap());

pub struct Cleaned {
    pub text: String,
    /// Fillers and stutters taken out.
    pub fixes: u32,
}

pub fn clean(text: &str) -> Cleaned {
    let mut fixes = 0;
    let (head, words) = split(text);
    let words = drop_stumbles(drop_fillers(words, &mut fixes), &mut fixes);
    let mut joined = String::from(head);
    for w in &words {
        w.push_to(&mut joined);
    }
    let text = capitalize(&tidy_spacing(&joined));
    Cleaned { text, fixes }
}

/// A word, the punctuation stuck to it and the whitespace after it.
#[derive(Debug, Clone)]
struct Word {
    open: String,
    core: String,
    close: String,
    space: String,
}

impl Word {
    fn push_to(&self, out: &mut String) {
        out.push_str(&self.open);
        out.push_str(&self.core);
        out.push_str(&self.close);
        out.push_str(&self.space);
    }

    fn folded(&self) -> String {
        self.core.to_lowercase().replace('’', "'")
    }
}

/// The whitespace the text starts with, and its words.
fn split(text: &str) -> (&str, Vec<Word>) {
    let start = text.len() - text.trim_start().len();
    let mut words = Vec::new();
    let mut i = start;
    while i < text.len() {
        let end = text[i..]
            .find(char::is_whitespace)
            .map_or(text.len(), |n| i + n);
        let next = text[end..]
            .find(|c: char| !c.is_whitespace())
            .map_or(text.len(), |n| end + n);
        let run = &text[i..end];
        let body = run.trim_start_matches(OPENERS);
        let core = body.trim_end_matches(CLOSERS);
        words.push(Word {
            open: run[..run.len() - body.len()].to_owned(),
            core: core.to_owned(),
            close: body[core.len()..].to_owned(),
            space: text[end..next].to_owned(),
        });
        i = next;
    }
    (&text[..start], words)
}

fn drop_fillers(words: Vec<Word>, fixes: &mut u32) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::with_capacity(words.len());
    for w in words {
        if !w.open.is_empty() || !FILLER.is_match(&w.core) {
            out.push(w);
            continue;
        }
        *fixes += 1;
        let Some(prev) = out.last_mut() else {
            continue;
        };
        // "go, um." ends the sentence at "go".
        if let Some(at) = w.close.find(ENDS) {
            prev.close = format!(
                "{}{}",
                prev.close.trim_end_matches([',', ';', ':']),
                &w.close[at..]
            );
            prev.space = w.space;
        } else if w.space.contains('\n') {
            prev.space = w.space;
        }
    }
    out
}

/// Parts of a word joined by hyphens, when they're all the same stumble,
/// as in "I-I".
fn hyphen_stumble(core: &str) -> Option<&str> {
    let mut parts = core.split('-');
    let first = parts.next()?;
    let folded = first.to_lowercase().replace('’', "'");
    let mut repeats = 0;
    for part in parts {
        if !part.eq_ignore_ascii_case(first) {
            return None;
        }
        repeats += 1;
    }
    (repeats > 0 && STUMBLES.contains(&folded.as_str())).then_some(first)
}

fn drop_stumbles(words: Vec<Word>, fixes: &mut u32) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::with_capacity(words.len());
    for mut w in words {
        if w.open.is_empty()
            && let Some(first) = hyphen_stumble(&w.core)
        {
            w.core = first.to_owned();
            *fixes += 1;
        }
        if let Some(prev) = out.last_mut()
            && prev.close.is_empty()
            && w.open.is_empty()
            && !prev.space.contains('\n')
            && prev.folded() == w.folded()
            && STUMBLES.contains(&w.folded().as_str())
        {
            prev.close = w.close;
            prev.space = w.space;
            *fixes += 1;
            continue;
        }
        out.push(w);
    }
    out
}

static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").unwrap());
static BEFORE_PUNCTUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r" +([,.;:!?…।])").unwrap());
static AROUND_NEWLINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" *\n *").unwrap());
static COMMAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",+([,.!?])").unwrap());

fn tidy_spacing(text: &str) -> String {
    let text = SPACES.replace_all(text, " ");
    let text = BEFORE_PUNCTUATION.replace_all(&text, "$1");
    let text = AROUND_NEWLINES.replace_all(&text, "\n");
    let text = COMMAS.replace_all(&text, "$1");
    text.trim_matches([' ', '\t']).to_owned()
}

/// Whether the text before a full stop ends with an abbreviation or an
/// initial, so the full stop doesn't end the sentence.
pub(super) fn abbreviation(before: &str) -> bool {
    let start = before
        .char_indices()
        .rev()
        .find(|&(_, c)| !(c.is_alphanumeric() || c == '.'))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let token = &before[start..];
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_uppercase()
        && c != 'I'
    {
        return true;
    }
    ABBREVIATIONS.contains(&token.to_lowercase().as_str())
}

/// Byte offsets of the first letter of each sentence and line.
pub(super) fn sentence_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut at_start = true;
    for (i, c) in text.char_indices() {
        if at_start {
            if c.is_whitespace() || OPENERS.contains(&c) {
                continue;
            }
            if c.is_alphabetic() {
                starts.push(i);
            }
        }
        at_start = match c {
            '\n' => true,
            '.' => {
                let before = &text[..i];
                ends_here(text, i + 1) && !before.ends_with('.') && !abbreviation(before)
            }
            '!' | '?' | '।' => ends_here(text, i + c.len_utf8()),
            _ => false,
        };
    }
    starts
}

/// Whether whitespace follows `at`, past any closing quotes and brackets.
fn ends_here(text: &str, at: usize) -> bool {
    let rest = text[at..].trim_start_matches(['"', '\'', '”', '’', ')', ']']);
    rest.starts_with(char::is_whitespace)
}

/// Web addresses and email addresses keep their case.
fn address(word: &str) -> bool {
    if word.contains('@') || word.contains("://") {
        return true;
    }
    let word = word.trim_end_matches(CLOSERS);
    word.rsplit_once('.').is_some_and(|(name, end)| {
        !name.is_empty() && end.len() >= 2 && end.chars().all(|c| c.is_ascii_lowercase())
    })
}

static LONE_I: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bi\b").unwrap());

fn capitalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for m in LONE_I.find_iter(text) {
        let before = text[..m.start()].chars().next_back();
        let after = text[m.end()..].chars().next();
        if before.is_some_and(|c| matches!(c, '.' | '-' | '\'' | '’' | '@' | '/'))
            || after.is_some_and(|c| matches!(c, '.' | '-' | '@'))
        {
            continue;
        }
        out.push_str(&text[last..m.start()]);
        out.push('I');
        last = m.end();
    }
    out.push_str(&text[last..]);

    for at in sentence_starts(&out).into_iter().rev() {
        let word = out[at..]
            .split(char::is_whitespace)
            .next()
            .unwrap_or_default();
        let Some(c) = word.chars().next() else {
            continue;
        };
        if c.is_lowercase() && !address(word) {
            out.replace_range(at..at + c.len_utf8(), &c.to_uppercase().to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleaned(text: &str) -> (String, u32) {
        let c = clean(text);
        (c.text, c.fixes)
    }

    #[test]
    fn drops_fillers() {
        assert_eq!(
            cleaned("Um, I think we should go."),
            ("I think we should go.".into(), 1)
        );
        assert_eq!(
            cleaned("So, uh, I think, umm, yes"),
            ("So, I think, yes".into(), 2)
        );
        assert_eq!(
            cleaned("We should go, um. Then eat"),
            ("We should go. Then eat".into(), 1)
        );
        assert_eq!(cleaned("Hmm."), (String::new(), 1));
        assert_eq!(
            cleaned("Uh-huh, and \"um\" is a word"),
            ("Uh-huh, and \"um\" is a word".into(), 0)
        );
    }

    #[test]
    fn drops_stumbles() {
        assert_eq!(
            cleaned("I I think the the plan works"),
            ("I think the plan works".into(), 2)
        );
        assert_eq!(cleaned("I-I don't know"), ("I don't know".into(), 1));
        assert_eq!(
            cleaned("and and then it it broke"),
            ("And then it broke".into(), 2)
        );
        for kept in [
            "It's very very good. Bye bye",
            "I know that that works",
            "It was so-so",
        ] {
            assert_eq!(cleaned(kept), (kept.into(), 0));
        }
        assert_eq!(cleaned("the, the end"), ("The, the end".into(), 0));
    }

    #[test]
    fn tidies_spacing() {
        assert_eq!(cleaned("Hello ,  world !"), ("Hello, world!".into(), 0));
        assert_eq!(
            cleaned("Dear Sam,\n  how are you,."),
            ("Dear Sam,\nHow are you.".into(), 0)
        );
        assert_eq!(cleaned("\n\nnew start"), ("\n\nNew start".into(), 0));
    }

    #[test]
    fn capitalizes_sentences() {
        assert_eq!(
            cleaned("i think i'm done. see you at 5 p.m. tomorrow! ok? \"yes,\" she said"),
            (
                "I think I'm done. See you at 5 p.m. tomorrow! Ok? \"Yes,\" she said".into(),
                0
            )
        );
        assert_eq!(
            cleaned("e.g. this. i.e. that"),
            ("E.g. this. I.e. that".into(), 0)
        );
        assert_eq!(
            cleaned("joona.app is live. priya@mail.com too"),
            ("joona.app is live. priya@mail.com too".into(), 0)
        );
        assert_eq!(
            cleaned("J. K. Rowling wrote it"),
            ("J. K. Rowling wrote it".into(), 0)
        );
        assert_eq!(cleaned("कल मिलते हैं। ठीक है"), ("कल मिलते हैं। ठीक है".into(), 0));
    }
}
