//! Splits long text for services that take a limited number of characters.

/// Splits `text` into consecutive pieces of at most `max` characters, after
/// a sentence where it can, otherwise between words. Joined, the pieces give
/// back `text` exactly.
pub fn split(text: &str, max: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while rest.chars().count() > max && max > 0 {
        let limit = rest.char_indices().nth(max).map_or(rest.len(), |(i, _)| i);
        let head = &rest[..limit];
        let cut = after_last_sentence(head)
            .or_else(|| after_last_space(head))
            .unwrap_or(limit);
        pieces.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    if !rest.is_empty() {
        pieces.push(rest);
    }
    pieces
}

/// The text worth sending from each piece, without the spaces around it.
/// Pieces that are only spaces are left out.
pub fn bodies<'a>(pieces: &[&'a str]) -> Vec<&'a str> {
    pieces
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Puts the rewritten [`bodies`] back in place of the originals, keeping
/// the spaces and line breaks between pieces.
pub fn join(pieces: &[&str], rewritten: Vec<String>) -> String {
    let mut out = String::new();
    let mut rewritten = rewritten.into_iter();
    for piece in pieces {
        let body = piece.trim();
        if body.is_empty() {
            out.push_str(piece);
            continue;
        }
        out.push_str(&piece[..piece.len() - piece.trim_start().len()]);
        out.push_str(rewritten.next().as_deref().unwrap_or(body).trim());
        out.push_str(&piece[piece.trim_end().len()..]);
    }
    out
}

/// Just after the last sentence end followed by a space or line break.
fn after_last_sentence(text: &str) -> Option<usize> {
    let mut best = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\n' {
            best = Some(i + 1);
        } else if matches!(c, '.' | '!' | '?' | '।' | '॥' | '۔' | '؟')
            && let Some(&(j, next)) = chars.peek()
            && next.is_whitespace()
        {
            best = Some(j + next.len_utf8());
        }
    }
    best.filter(|&i| i > 0)
}

fn after_last_space(text: &str) -> Option<usize> {
    text.char_indices()
        .rfind(|(_, c)| c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8())
        .filter(|&i| i > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_piece() {
        assert_eq!(split("Hello there.", 1000), ["Hello there."]);
        assert!(split("", 1000).is_empty());
    }

    #[test]
    fn splits_after_sentences_then_between_words() {
        let text = "One two. Three four five. Six seven";
        let pieces = split(text, 20);
        assert_eq!(pieces, ["One two. ", "Three four five. ", "Six seven"]);
        assert_eq!(pieces.concat(), text);

        let words = split("alpha beta gamma delta", 12);
        assert_eq!(words, ["alpha beta ", "gamma delta"]);
        assert_eq!(split("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn counts_characters_not_bytes() {
        let text = "नमस्ते दोस्तों। आप कैसे हैं?";
        let pieces = split(text, 16);
        assert_eq!(pieces.concat(), text);
        assert!(pieces.iter().all(|p| p.chars().count() <= 16), "{pieces:?}");
        assert_eq!(pieces[0], "नमस्ते दोस्तों। ");
    }

    #[test]
    fn rewritten_pieces_keep_their_line_breaks() {
        let pieces = split("Pehla.\n\nDoosra hai. ", 8);
        assert_eq!(bodies(&pieces), ["Pehla.", "Doosra", "hai."]);
        let out = join(
            &pieces,
            vec!["First.".into(), " Second".into(), "one.".into()],
        );
        assert_eq!(out, "First.\n\nSecond one. ");
    }
}
