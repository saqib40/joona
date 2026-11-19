//! Spoken commands: "new line", "new paragraph" and, when it's turned on,
//! "press enter" at the end of a dictation.
//!
//! A command counts only when it's set off from the words around it by a
//! pause the speech model wrote as punctuation, or by the start or end of
//! the dictation. "Write it on a new line" is about a new line, so neither
//! counts after words like "a" and "the".

use std::sync::LazyLock;

use regex::Regex;

static BREAK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:new\s+line|next\s+line|new\s+paragraph|न्यू\s+लाइन|नई\s+लाइन|न्यू\s+पैराग्राफ)\b",
    )
    .unwrap()
});
static ENTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bpress\s+enter\b|प्रेस\s+एंटर").unwrap());

/// Pauses, as the speech model writes them.
const PAUSES: &[char] = &[',', '.', ';', ':', '!', '?', '।', '—', '–', '\n'];
/// Words before "new line" that make it a thing, not a command.
const DETERMINERS: &[&str] = &[
    "a", "an", "the", "this", "that", "one", "another", "each", "every", "no", "any", "some",
    "our", "your", "my", "their", "his", "her", "its",
];

pub struct Spoken {
    pub text: String,
    /// "Press enter" ended the dictation.
    pub enter: bool,
}

pub fn apply(text: &str, press_enter: bool) -> Spoken {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for m in BREAK.find_iter(text) {
        if m.start() < last || !set_off(text, m.start(), m.end()) {
            continue;
        }
        out.push_str(text[last..m.start()].trim_end_matches([' ', '\t']));
        let said = m.as_str().to_lowercase();
        out.push_str(match said.contains("paragraph") || said.contains("पैराग्राफ")
        {
            true => "\n\n",
            false => "\n",
        });
        let rest = &text[m.end()..];
        last = text.len()
            - rest
                .trim_start_matches(|c: char| {
                    c == ' ' || c == '\t' || (PAUSES.contains(&c) && c != '\n')
                })
                .len();
    }
    out.push_str(&text[last..]);
    let enter = press_enter && end_with_enter(&mut out);
    Spoken { text: out, enter }
}

fn set_off(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].trim_end_matches([' ', '\t']);
    let after = text[end..].trim_start_matches([' ', '\t']);
    let paused = |c: Option<char>| c.is_none_or(|c| PAUSES.contains(&c));
    (paused(before.chars().next_back()) || paused(after.chars().next()))
        && !after_determiner(before)
}

fn after_determiner(before: &str) -> bool {
    let word = before
        .rsplit(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_lowercase();
    DETERMINERS.contains(&word.as_str())
}

/// Takes "press enter" off the end of `text`, if it's said there after a
/// pause.
fn end_with_enter(text: &mut String) -> bool {
    let Some(m) = ENTER.find_iter(text).last() else {
        return false;
    };
    let tail_is_punctuation = text[m.end()..]
        .chars()
        .all(|c| c.is_whitespace() || PAUSES.contains(&c));
    let before = text[..m.start()].trim_end_matches([' ', '\t']);
    let paused = before
        .chars()
        .next_back()
        .is_none_or(|c| PAUSES.contains(&c));
    if !tail_is_punctuation || !paused {
        return false;
    }
    let keep = before
        .trim_end_matches([' ', '\t', ',', ';', ':', '—', '–'])
        .len();
    text.truncate(keep);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spoken(text: &str) -> (String, bool) {
        let s = apply(text, true);
        (s.text, s.enter)
    }

    #[test]
    fn breaks_lines_where_commands_are_set_off() {
        assert_eq!(
            spoken("Hi Priya, new line, how are you?").0,
            "Hi Priya,\nhow are you?"
        );
        assert_eq!(
            spoken("Thanks. New paragraph. Best, Sam.").0,
            "Thanks.\n\nBest, Sam."
        );
        assert_eq!(spoken("New line").0, "\n");
        assert_eq!(
            spoken("Item one. Next line item two").0,
            "Item one.\nitem two"
        );
        assert_eq!(spoken("नमस्ते। न्यू लाइन, कैसे हो?").0, "नमस्ते।\nकैसे हो?");
    }

    #[test]
    fn leaves_lines_people_talk_about() {
        for kept in [
            "Write that on a new line.",
            "Put it on the next line, please.",
            "We launched a new line of shoes",
            "The new line manager starts Monday",
        ] {
            assert_eq!(spoken(kept).0, kept);
        }
    }

    #[test]
    fn presses_enter_only_at_the_end() {
        assert_eq!(
            spoken("See you soon, press enter."),
            ("See you soon".into(), true)
        );
        assert_eq!(spoken("Press enter"), (String::new(), true));
        assert_eq!(
            spoken("Tell him to press enter."),
            ("Tell him to press enter.".into(), false)
        );
        assert_eq!(
            spoken("Press enter, then wait."),
            ("Press enter, then wait.".into(), false)
        );
        let off = apply("Done. Press enter.", false);
        assert_eq!(
            (off.text.as_str(), off.enter),
            ("Done. Press enter.", false)
        );
    }
}
