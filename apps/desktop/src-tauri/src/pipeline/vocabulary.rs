//! The user's dictionary and snippets, ready for the text pipeline.

use std::collections::HashSet;

use super::matcher::Matcher;
use crate::store::{DictionaryEntry, Snippet};

/// How many dictionary words go to the speech model as hints.
pub const MAX_TERMS: usize = 50;

/// Stands in for a snippet's text while the rest is cleaned up: the first
/// code point of a private use plane, plus the snippet's place in the text.
const PLACEHOLDER: u32 = 0xF0000;
/// The plane's last code points aren't characters.
const PLACEHOLDERS: u32 = 0xFFFE;

/// The placeholder for the `n`th snippet in a text, counting from 0.
pub fn placeholder(n: usize) -> char {
    u32::try_from(n)
        .ok()
        .filter(|&n| n < PLACEHOLDERS)
        .and_then(|n| char::from_u32(PLACEHOLDER + n))
        .unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// Which snippet `c` stands for, when it's a placeholder.
pub fn placeholder_index(c: char) -> Option<usize> {
    (c as u32)
        .checked_sub(PLACEHOLDER)
        .filter(|&n| n < PLACEHOLDERS)
        .map(|n| n as usize)
}

#[derive(Default)]
pub struct Vocabulary {
    /// What the speech model writes, and how the dictionary spells it.
    fixes: Matcher<Fix>,
    /// Dictionary words without a "heard as", to count how often they come up.
    words: Matcher<i64>,
    snippets: Matcher<usize>,
    /// The id and text of each snippet.
    texts: Vec<(i64, String)>,
    /// Lower-case first words of dictionary entries that start with a capital.
    names: HashSet<String>,
    terms: Vec<String>,
}

struct Fix {
    id: i64,
    word: String,
}

/// Text with snippet phrases swapped for placeholders.
pub struct Shielded {
    pub text: String,
    used: Vec<usize>,
}

impl Shielded {
    /// How many placeholders the text has.
    pub fn placeholders(&self) -> usize {
        self.used.len()
    }
}

/// Whether a capital follows a small letter, as in `iPhone`, `GitHub` or
/// `Priya Sharma`. Such words can't be ordinary words written another way,
/// so it's safe to fix their case wherever they're found.
fn mixed_case(word: &str) -> bool {
    let mut lower = false;
    for c in word.chars() {
        if c.is_lowercase() {
            lower = true;
        } else if c.is_uppercase() && lower {
            return true;
        }
    }
    false
}

impl Vocabulary {
    /// `entries` come most important first: starred, then most used.
    pub fn new(entries: &[DictionaryEntry], snippets: &[Snippet]) -> Self {
        let mut v = Self::default();
        for e in entries {
            let fix = || Fix {
                id: e.id,
                word: e.word.clone(),
            };
            match &e.heard {
                Some(heard) => {
                    v.fixes.add(heard, fix());
                }
                None => {
                    if mixed_case(&e.word) {
                        v.fixes.add(&e.word, fix());
                    }
                    v.words.add(&e.word, e.id);
                }
            }
            if let Some(first) = e.word.split_whitespace().next()
                && first.chars().next().is_some_and(char::is_uppercase)
            {
                v.names.insert(first.to_lowercase());
            }
        }
        let mut seen = HashSet::new();
        v.terms = entries
            .iter()
            .filter(|e| seen.insert(e.word.to_lowercase()))
            .take(MAX_TERMS)
            .map(|e| e.word.clone())
            .collect();
        for (i, s) in snippets.iter().enumerate() {
            v.snippets.add(&s.phrase, i);
            v.texts.push((s.id, s.text.clone()));
        }
        v
    }

    /// Spells dictionary words the dictionary's way. Returns the text, how
    /// many words changed and the entries that changed them.
    pub fn fix(&self, text: &str) -> (String, u32, Vec<i64>) {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        let mut fixes = 0;
        let mut used = Vec::new();
        for found in self.fixes.find(text) {
            out.push_str(&text[last..found.range.start]);
            out.push_str(&found.value.word);
            if text[found.range.clone()] != found.value.word {
                fixes += 1;
                used.push(found.value.id);
            }
            last = found.range.end;
        }
        out.push_str(&text[last..]);
        (out, fixes, used)
    }

    /// Plain dictionary words in `text`, to count how often they come up.
    pub fn mentioned(&self, text: &str) -> Vec<i64> {
        self.words
            .find(text)
            .into_iter()
            .map(|f| *f.value)
            .collect()
    }

    /// Swaps snippet phrases for placeholders, so the steps after this one
    /// leave the snippets' text alone.
    pub fn shield(&self, text: &str) -> Shielded {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        let mut used = Vec::new();
        for found in self.snippets.find(text) {
            out.push_str(&text[last..found.range.start]);
            out.push(placeholder(used.len()));
            used.push(*found.value);
            last = found.range.end;
        }
        out.push_str(&text[last..]);
        Shielded { text: out, used }
    }

    /// The ids of the snippets in `shielded`.
    pub fn snippets_in(&self, shielded: &Shielded) -> Vec<i64> {
        shielded.used.iter().map(|&i| self.texts[i].0).collect()
    }

    /// Puts the snippets' text back into `text`, which came from `shielded`
    /// and may have been changed since. A full stop or comma that's all that
    /// follows a snippet on its line is dropped: a snippet ends the way it's
    /// written.
    pub fn unshield(&self, text: &str, shielded: &Shielded) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some((at, c)) = rest
            .char_indices()
            .find(|&(_, c)| self.slot(c, shielded).is_some())
        {
            out.push_str(&rest[..at]);
            if let Some(i) = self.slot(c, shielded) {
                out.push_str(&self.texts[i].1);
            }
            rest = &rest[at + c.len_utf8()..];
            let line = rest.split('\n').next().unwrap_or_default();
            if matches!(line.trim(), "." | "," | "।") {
                rest = &rest[line.trim_end().len()..];
            }
        }
        out.push_str(rest);
        out
    }

    fn slot(&self, c: char, shielded: &Shielded) -> Option<usize> {
        shielded.used.get(placeholder_index(c)?).copied()
    }

    /// Whether `word`, at the start of a sentence, is the first word of a
    /// name in the dictionary, so it keeps its capital.
    pub fn is_name(&self, word: &str) -> bool {
        self.names.contains(&word.to_lowercase())
    }

    /// Words for the speech model to expect, most important first.
    pub fn terms(&self) -> &[String] {
        &self.terms
    }

    /// The terms as a hint for Whisper, which reads them as text it heard
    /// before the recording.
    pub fn prompt(&self) -> Option<String> {
        (!self.terms.is_empty()).then(|| self.terms.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, word: &str, heard: Option<&str>) -> DictionaryEntry {
        DictionaryEntry {
            id,
            word: word.into(),
            heard: heard.map(Into::into),
            starred: false,
            uses: 0,
            last_used: None,
            created_at: 0,
        }
    }

    fn snippet(id: i64, phrase: &str, text: &str) -> Snippet {
        Snippet {
            id,
            phrase: phrase.into(),
            text: text.into(),
            uses: 0,
            last_used: None,
            created_at: 0,
        }
    }

    #[test]
    fn fixes_spellings_and_case() {
        let v = Vocabulary::new(
            &[
                entry(1, "Joona", Some("Juna")),
                entry(2, "GitHub", None),
                entry(3, "Priya", None),
                entry(4, "Kubernetes", Some("cuber netes")),
            ],
            &[],
        );
        let (text, fixes, used) =
            v.fix("juna is on github, and so is GitHub. priya uses Cuber Netes.");
        assert_eq!(
            text,
            "Joona is on GitHub, and so is GitHub. priya uses Kubernetes."
        );
        assert_eq!(fixes, 3);
        assert_eq!(used, [1, 2, 4]);
        assert_eq!(v.mentioned(&text), [2, 2, 3]);
        assert!(v.is_name("PRIYA") && v.is_name("joona") && !v.is_name("uses"));
        assert_eq!(
            v.prompt().as_deref(),
            Some("Joona, GitHub, Priya, Kubernetes")
        );
    }

    #[test]
    fn shields_snippets_until_the_end() {
        let v = Vocabulary::new(
            &[],
            &[
                snippet(7, "my address", "12 MG Road\nBengaluru"),
                snippet(8, "sign off", "Thanks,\nPriya"),
            ],
        );
        let shielded = v.shield("send it to my address. Sign off.");
        assert_eq!(shielded.text, "send it to \u{F0000}. \u{F0001}.");
        assert_eq!(v.snippets_in(&shielded), [7, 8]);
        let polished = shielded.text.replace("send", "Send");
        assert_eq!(
            v.unshield(&polished, &shielded),
            "Send it to 12 MG Road\nBengaluru. Thanks,\nPriya"
        );
        assert_eq!(
            v.unshield("\u{F0001} \nnext", &shielded),
            "Thanks,\nPriya \nnext"
        );
        assert_eq!(v.unshield("\u{F0005}", &shielded), "\u{F0005}");
    }

    #[test]
    fn keeps_the_most_important_terms() {
        let entries: Vec<_> = (0..80)
            .map(|i| entry(i, &format!("Word{i}"), None))
            .chain([entry(99, "word0", Some("word zero"))])
            .collect();
        let v = Vocabulary::new(&entries, &[]);
        assert_eq!(v.terms().len(), MAX_TERMS);
        assert_eq!(v.terms()[0], "Word0");
        assert!(Vocabulary::default().prompt().is_none());
    }
}
