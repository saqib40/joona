//! Snippets: a short phrase the user says, and the text Joona types for it.

use anyhow::bail;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use specta::Type;

use super::{Db, one_line};

/// The longest text a snippet can type, in characters.
pub const MAX_SNIPPET: usize = 4_000;
const MAX_PHRASE: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: i64,
    /// What the user says, for example `my address`.
    pub phrase: String,
    /// What Joona types instead.
    pub text: String,
    pub uses: u32,
    pub last_used: Option<i64>,
    pub created_at: i64,
}

const COLUMNS: &str = "id, phrase, text, uses, last_used, created_at";

fn row_to_snippet(r: &rusqlite::Row<'_>) -> rusqlite::Result<Snippet> {
    Ok(Snippet {
        id: r.get(0)?,
        phrase: r.get(1)?,
        text: r.get(2)?,
        uses: r.get::<_, i64>(3)? as u32,
        last_used: r.get(4)?,
        created_at: r.get(5)?,
    })
}

/// The snippet as it's saved, or why it can't be.
fn clean(phrase: &str, text: &str) -> anyhow::Result<(String, String)> {
    let phrase = one_line(phrase);
    let phrase = phrase.trim_matches(|c: char| !c.is_alphanumeric());
    if phrase.is_empty() {
        bail!("Type the phrase you'll say.");
    }
    if phrase.chars().count() > MAX_PHRASE {
        bail!("Keep the phrase under {MAX_PHRASE} characters.");
    }
    let text = text.replace("\r\n", "\n");
    let text = text.trim_start_matches('\n').trim_end();
    if text.is_empty() {
        bail!("Type the text Joona should type.");
    }
    if text.chars().count() > MAX_SNIPPET {
        bail!("A snippet can be up to {MAX_SNIPPET} characters long.");
    }
    Ok((phrase.to_owned(), text.to_owned()))
}

fn check_unique(db: &Connection, id: Option<i64>, phrase: &str) -> anyhow::Result<()> {
    let taken: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM snippets WHERE lower(phrase) = lower(?1) AND id != ?2)",
        params![phrase, id.unwrap_or(-1)],
        |r| r.get(0),
    )?;
    if taken {
        bail!("You already have a snippet for “{phrase}”.");
    }
    Ok(())
}

fn get(db: &Connection, id: i64) -> anyhow::Result<Snippet> {
    let snippet = db
        .query_row(
            &format!("SELECT {COLUMNS} FROM snippets WHERE id = ?1"),
            [id],
            row_to_snippet,
        )
        .optional()?;
    match snippet {
        Some(snippet) => Ok(snippet),
        None => bail!("That snippet was deleted."),
    }
}

impl Db {
    pub fn snippets(&self) -> anyhow::Result<Vec<Snippet>> {
        let db = self.lock();
        let mut stmt = db.prepare(&format!(
            "SELECT {COLUMNS} FROM snippets ORDER BY lower(phrase)"
        ))?;
        let rows = stmt.query_map([], row_to_snippet)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn snippet_add(&self, phrase: &str, text: &str, now: i64) -> anyhow::Result<Snippet> {
        let (phrase, text) = clean(phrase, text)?;
        let db = self.lock();
        check_unique(&db, None, &phrase)?;
        db.execute(
            "INSERT INTO snippets (phrase, text, created_at) VALUES (?1, ?2, ?3)",
            params![phrase, text, now],
        )?;
        get(&db, db.last_insert_rowid())
    }

    pub fn snippet_update(&self, id: i64, phrase: &str, text: &str) -> anyhow::Result<Snippet> {
        let (phrase, text) = clean(phrase, text)?;
        let db = self.lock();
        check_unique(&db, Some(id), &phrase)?;
        db.execute(
            "UPDATE snippets SET phrase = ?2, text = ?3 WHERE id = ?1",
            params![id, phrase, text],
        )?;
        get(&db, id)
    }

    pub fn snippet_delete(&self, id: i64) -> anyhow::Result<()> {
        self.lock()
            .execute("DELETE FROM snippets WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Counts one more use for each snippet in `ids`.
    pub fn snippets_used(&self, ids: &[i64], now: i64) -> anyhow::Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut db = self.lock();
        let tx = db.transaction()?;
        {
            let mut stmt =
                tx.prepare("UPDATE snippets SET uses = uses + 1, last_used = ?2 WHERE id = ?1")?;
            for id in ids {
                stmt.execute(params![id, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_checks_snippets() {
        let db = Db::in_memory().unwrap();
        let home = db
            .snippet_add(
                " My   address. ",
                "\r\n12 MG Road\r\nBengaluru 560001\n\n",
                1,
            )
            .unwrap();
        assert_eq!(home.phrase, "My address");
        assert_eq!(home.text, "12 MG Road\nBengaluru 560001");

        let err = |r: anyhow::Result<Snippet>| r.unwrap_err().to_string();
        assert_eq!(
            err(db.snippet_add("my ADDRESS", "x", 2)),
            "You already have a snippet for “my ADDRESS”."
        );
        assert_eq!(
            err(db.snippet_add("...", "x", 2)),
            "Type the phrase you'll say."
        );
        assert_eq!(
            err(db.snippet_add("sign off", " \n ", 2)),
            "Type the text Joona should type."
        );
        assert!(err(db.snippet_add("long one", &"a".repeat(MAX_SNIPPET + 1), 2)).contains("4000"));
        assert!(
            db.snippet_add("long one", &"a".repeat(MAX_SNIPPET), 2)
                .is_ok()
        );

        let edited = db
            .snippet_update(home.id, "my home address", "12 MG Road")
            .unwrap();
        assert_eq!(edited.phrase, "my home address");
        db.snippets_used(&[home.id, home.id], 9).unwrap();
        let listed = db.snippets().unwrap();
        assert_eq!(listed[1].phrase, "my home address");
        assert_eq!(listed[1].uses, 2);
        assert_eq!(listed[1].last_used, Some(9));
        db.snippet_delete(home.id).unwrap();
        assert_eq!(db.snippets().unwrap().len(), 1);
    }
}
