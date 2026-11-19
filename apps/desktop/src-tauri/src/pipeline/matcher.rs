//! Finds phrases in text word by word, ignoring case and how the words are
//! spaced. Thousands of phrases cost no more than a few, because each word of
//! the text only looks at the phrases that start with it.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

/// Letters, digits and the marks of scripts such as Devanagari.
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\w+").unwrap());

pub struct Matcher<T> {
    /// Phrases by their first word, longest first.
    phrases: HashMap<String, Vec<Phrase<T>>>,
}

struct Phrase<T> {
    words: Vec<String>,
    /// What separates each pair of words: `None` for any whitespace, or the
    /// exact characters, such as the `.` in `Node.js`.
    gaps: Vec<Option<String>>,
    /// Characters before the first word and after the last, such as the
    /// `++` in `C++`.
    prefix: String,
    suffix: String,
    value: T,
}

pub struct Found<'a, T> {
    pub range: Range<usize>,
    pub value: &'a T,
}

impl<T> Default for Matcher<T> {
    fn default() -> Self {
        Self {
            phrases: HashMap::new(),
        }
    }
}

fn fold(word: &str) -> String {
    word.to_lowercase()
}

fn gap(between: &str) -> String {
    between.trim().replace(['’', '‘'], "'")
}

impl<T> Matcher<T> {
    /// Returns `false` when the phrase has no words to look for.
    pub fn add(&mut self, phrase: &str, value: T) -> bool {
        let found: Vec<_> = WORD.find_iter(phrase).collect();
        let (Some(first), Some(last)) = (found.first(), found.last()) else {
            return false;
        };
        let gaps = found
            .windows(2)
            .map(|w| {
                let between = &phrase[w[0].end()..w[1].start()];
                (!between.trim().is_empty()).then(|| gap(between))
            })
            .collect();
        let phrase = Phrase {
            words: found.iter().map(|m| fold(m.as_str())).collect(),
            gaps,
            prefix: phrase[..first.start()].trim().to_owned(),
            suffix: phrase[last.end()..].trim().to_owned(),
            value,
        };
        let list = self.phrases.entry(phrase.words[0].clone()).or_default();
        list.push(phrase);
        list.sort_by_key(|p| Reverse((p.words.len(), p.prefix.len() + p.suffix.len())));
        true
    }

    /// Every match, left to right and not overlapping. Where phrases
    /// overlap, the one that starts first wins, and then the longest.
    pub fn find(&self, text: &str) -> Vec<Found<'_, T>> {
        if self.phrases.is_empty() {
            return Vec::new();
        }
        let words: Vec<(Range<usize>, String)> = WORD
            .find_iter(text)
            .map(|m| (m.range(), fold(m.as_str())))
            .collect();
        let mut found = Vec::new();
        let mut taken = 0;
        let mut i = 0;
        while i < words.len() {
            let hit = self.phrases.get(&words[i].1).and_then(|list| {
                list.iter()
                    .find_map(|p| p.at(text, &words, i, taken).map(|range| (p, range)))
            });
            match hit {
                Some((phrase, range)) => {
                    taken = range.end;
                    i += phrase.words.len();
                    found.push(Found {
                        range,
                        value: &phrase.value,
                    });
                }
                None => i += 1,
            }
        }
        found
    }
}

impl<T> Phrase<T> {
    /// Where this phrase is in `text` if it starts at word `i`, not
    /// reaching back before `taken`.
    fn at(
        &self,
        text: &str,
        words: &[(Range<usize>, String)],
        i: usize,
        taken: usize,
    ) -> Option<Range<usize>> {
        let span = words.get(i..i + self.words.len())?;
        if span.iter().zip(&self.words).any(|((_, w), want)| w != want) {
            return None;
        }
        for (pair, want) in span.windows(2).zip(&self.gaps) {
            let between = &text[pair[0].0.end..pair[1].0.start];
            let fits = match want {
                None => !between.is_empty() && between.chars().all(char::is_whitespace),
                Some(want) => gap(between) == *want,
            };
            if !fits {
                return None;
            }
        }
        let (start, end) = (span[0].0.start, span[span.len() - 1].0.end);
        if !text[taken..start].ends_with(&self.prefix) || !text[end..].starts_with(&self.suffix) {
            return None;
        }
        Some(start - self.prefix.len()..end + self.suffix.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(phrases: &[&str], text: &str) -> Vec<String> {
        let mut m = Matcher::default();
        for (i, p) in phrases.iter().enumerate() {
            m.add(p, i);
        }
        m.find(text)
            .into_iter()
            .map(|f| format!("{}={}", &text[f.range], f.value))
            .collect()
    }

    #[test]
    fn matches_whole_words_in_any_case() {
        assert_eq!(
            matches(&["joona"], "Joona, JOONA and joonas"),
            ["Joona=0", "JOONA=0"]
        );
        assert_eq!(matches(&["June a"], "it's June  a\tday"), ["June  a=0"]);
        assert!(matches(&["June a"], "June, a day").is_empty());
    }

    #[test]
    fn prefers_the_longest_phrase() {
        assert_eq!(
            matches(&["New York", "New York City"], "New York City and New York"),
            ["New York City=1", "New York=0"]
        );
    }

    #[test]
    fn keeps_punctuation_inside_and_around_words() {
        assert_eq!(matches(&["C++"], "I like C++ and C"), ["C++=0"]);
        assert_eq!(matches(&["node.js"], "Node.js, not Node js"), ["Node.js=0"]);
        assert_eq!(matches(&["don't"], "Don’t do that"), ["Don’t=0"]);
        assert_eq!(matches(&[".NET"], "ASP.NET and .net"), [".NET=0", ".net=0"]);
        assert!(!Matcher::default().add("...", 0));
    }

    #[test]
    fn works_in_other_scripts() {
        assert_eq!(
            matches(&["प्रिया"], "कल प्रिया आएगी, प्रियाजी नहीं"),
            ["प्रिया=0"]
        );
    }
}
