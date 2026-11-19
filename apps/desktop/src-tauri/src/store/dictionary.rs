//! The dictionary: words Joona should spell right, and what the speech model
//! writes instead of them.

use std::path::Path;

use anyhow::bail;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::{Db, one_line};

/// More than this is a word list rather than a dictionary, and slows every
/// dictation down.
pub const MAX_ENTRIES: usize = 5_000;
const MAX_LEN: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: i64,
    /// How it's written, for example `Joona`.
    pub word: String,
    /// What the speech model writes instead, for example `Juna`. `None` for
    /// a word that is only there to be recognised.
    pub heard: Option<String>,
    pub starred: bool,
    /// Dictations it turned up in.
    pub uses: u32,
    pub last_used: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub added: u32,
    /// Rows that were already in the dictionary, empty or too long.
    pub skipped: u32,
}

const COLUMNS: &str = "id, word, heard, starred, uses, last_used, created_at";

fn row_to_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<DictionaryEntry> {
    Ok(DictionaryEntry {
        id: r.get(0)?,
        word: r.get(1)?,
        heard: r.get(2)?,
        starred: r.get(3)?,
        uses: r.get::<_, i64>(4)? as u32,
        last_used: r.get(5)?,
        created_at: r.get(6)?,
    })
}

fn clean(text: &str) -> anyhow::Result<String> {
    let text = one_line(text);
    if !text.chars().any(char::is_alphanumeric) {
        bail!("Type a word or phrase.");
    }
    if text.chars().count() > MAX_LEN {
        bail!("Keep it under {MAX_LEN} characters.");
    }
    Ok(text)
}

/// The entry as it's saved, or why it can't be.
fn clean_entry(word: &str, heard: Option<&str>) -> anyhow::Result<(String, Option<String>)> {
    let word = clean(word)?;
    let heard = match heard.filter(|h| !h.trim().is_empty()) {
        Some(heard) => {
            let heard = clean(heard)?;
            if heard == word {
                bail!("That's already how “{word}” is written.");
            }
            Some(heard)
        }
        None => None,
    };
    Ok((word, heard))
}

/// SQLite's `lower` only folds ASCII, like the unique indexes it backs.
fn check_unique(
    db: &Connection,
    id: Option<i64>,
    word: &str,
    heard: Option<&str>,
) -> anyhow::Result<()> {
    let id = id.unwrap_or(-1);
    match heard {
        None => {
            let taken: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM dictionary
                 WHERE heard IS NULL AND lower(word) = lower(?1) AND id != ?2)",
                params![word, id],
                |r| r.get(0),
            )?;
            if taken {
                bail!("“{word}” is already in your dictionary.");
            }
        }
        Some(heard) => {
            let other: Option<String> = db
                .query_row(
                    "SELECT word FROM dictionary
                     WHERE heard IS NOT NULL AND lower(heard) = lower(?1) AND id != ?2",
                    params![heard, id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(other) = other {
                bail!("Joona already changes “{heard}” to “{other}”.");
            }
        }
    }
    Ok(())
}

fn get(db: &Connection, id: i64) -> anyhow::Result<DictionaryEntry> {
    let entry = db
        .query_row(
            &format!("SELECT {COLUMNS} FROM dictionary WHERE id = ?1"),
            [id],
            row_to_entry,
        )
        .optional()?;
    match entry {
        Some(entry) => Ok(entry),
        None => bail!("That entry was deleted."),
    }
}

fn count(db: &Connection) -> anyhow::Result<usize> {
    let n: i64 = db.query_row("SELECT COUNT(*) FROM dictionary", [], |r| r.get(0))?;
    Ok(n as usize)
}

impl Db {
    /// Starred entries first, then the most used.
    pub fn dictionary(&self) -> anyhow::Result<Vec<DictionaryEntry>> {
        let db = self.lock();
        let mut stmt = db.prepare(&format!(
            "SELECT {COLUMNS} FROM dictionary
             ORDER BY starred DESC, uses DESC, lower(word), lower(COALESCE(heard, ''))"
        ))?;
        let rows = stmt.query_map([], row_to_entry)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn dictionary_add(
        &self,
        word: &str,
        heard: Option<&str>,
        now: i64,
    ) -> anyhow::Result<DictionaryEntry> {
        let (word, heard) = clean_entry(word, heard)?;
        let db = self.lock();
        if count(&db)? >= MAX_ENTRIES {
            bail!("The dictionary is full at {MAX_ENTRIES} entries. Delete some first.");
        }
        check_unique(&db, None, &word, heard.as_deref())?;
        db.execute(
            "INSERT INTO dictionary (word, heard, created_at) VALUES (?1, ?2, ?3)",
            params![word, heard, now],
        )?;
        get(&db, db.last_insert_rowid())
    }

    pub fn dictionary_update(
        &self,
        id: i64,
        word: &str,
        heard: Option<&str>,
    ) -> anyhow::Result<DictionaryEntry> {
        let (word, heard) = clean_entry(word, heard)?;
        let db = self.lock();
        check_unique(&db, Some(id), &word, heard.as_deref())?;
        db.execute(
            "UPDATE dictionary SET word = ?2, heard = ?3 WHERE id = ?1",
            params![id, word, heard],
        )?;
        get(&db, id)
    }

    pub fn dictionary_star(&self, id: i64, starred: bool) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE dictionary SET starred = ?2 WHERE id = ?1",
            params![id, starred],
        )?;
        Ok(())
    }

    pub fn dictionary_delete(&self, id: i64) -> anyhow::Result<()> {
        self.lock()
            .execute("DELETE FROM dictionary WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Counts one more dictation for each entry in `ids`.
    pub fn dictionary_used(&self, ids: &[i64], now: i64) -> anyhow::Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut db = self.lock();
        let tx = db.transaction()?;
        {
            let mut stmt =
                tx.prepare("UPDATE dictionary SET uses = uses + 1, last_used = ?2 WHERE id = ?1")?;
            for id in ids {
                stmt.execute(params![id, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Adds the rows of a CSV file: the word, then optionally what Joona
    /// hears and whether it's starred. A plain list of words works too.
    pub fn dictionary_import(&self, path: &Path, now: i64) -> anyhow::Result<Imported> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .trim(csv::Trim::All)
            .from_path(path)?;
        let mut db = self.lock();
        let tx = db.transaction()?;
        let mut total = count(&tx)?;
        let mut imported = Imported {
            added: 0,
            skipped: 0,
        };
        for (i, record) in reader.records().enumerate() {
            let record = record?;
            let field = |n: usize| record.get(n).unwrap_or("").trim_start_matches('\u{feff}');
            if i == 0 && field(0).eq_ignore_ascii_case("word") {
                continue;
            }
            if field(0).is_empty() && field(1).is_empty() {
                continue;
            }
            let added = total < MAX_ENTRIES
                && (|| {
                    let (word, heard) = clean_entry(field(0), Some(field(1)))?;
                    check_unique(&tx, None, &word, heard.as_deref())?;
                    let starred = matches!(
                        field(2).to_lowercase().as_str(),
                        "1" | "true" | "yes" | "y" | "starred" | "★"
                    );
                    tx.execute(
                        "INSERT INTO dictionary (word, heard, starred, created_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![word, heard, starred, now],
                    )?;
                    anyhow::Ok(())
                })()
                .is_ok();
            if added {
                imported.added += 1;
                total += 1;
            } else {
                imported.skipped += 1;
            }
        }
        tx.commit()?;
        Ok(imported)
    }

    /// Writes the dictionary as CSV that [`Db::dictionary_import`] reads back.
    pub fn dictionary_export(&self, path: &Path) -> anyhow::Result<usize> {
        let entries = self.dictionary()?;
        let mut w = csv::Writer::from_path(path)?;
        w.write_record(["word", "heard", "starred"])?;
        for e in &entries {
            w.write_record([
                e.word.as_str(),
                e.heard.as_deref().unwrap_or(""),
                if e.starred { "yes" } else { "" },
            ])?;
        }
        w.flush()?;
        Ok(entries.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(db: &Db) -> Vec<String> {
        db.dictionary()
            .unwrap()
            .into_iter()
            .map(|e| match e.heard {
                Some(heard) => format!("{heard}>{}", e.word),
                None => e.word,
            })
            .collect()
    }

    #[test]
    fn adds_edits_and_ranks_entries() {
        let db = Db::in_memory().unwrap();
        let joona = db.dictionary_add("  Joona ", None, 1).unwrap();
        assert_eq!(joona.word, "Joona");
        let priya = db.dictionary_add("Priya", None, 2).unwrap();
        let juna = db.dictionary_add("Joona", Some("Juna"), 3).unwrap();
        assert_eq!(juna.heard.as_deref(), Some("Juna"));

        db.dictionary_used(&[priya.id], 10).unwrap();
        assert_eq!(
            words(&db),
            ["Priya", "Joona", "Juna>Joona"],
            "most used first"
        );
        db.dictionary_star(juna.id, true).unwrap();
        assert_eq!(words(&db)[0], "Juna>Joona", "starred first");

        let edited = db
            .dictionary_update(priya.id, "Priya Sharma", Some(""))
            .unwrap();
        assert_eq!(edited.word, "Priya Sharma");
        assert_eq!(edited.uses, 1);
        assert_eq!(edited.last_used, Some(10));
        db.dictionary_delete(joona.id).unwrap();
        assert_eq!(words(&db), ["Juna>Joona", "Priya Sharma"]);
    }

    #[test]
    fn refuses_duplicates_and_empty_entries() {
        let db = Db::in_memory().unwrap();
        db.dictionary_add("Joona", None, 1).unwrap();
        db.dictionary_add("Joona", Some("Juna"), 1).unwrap();
        let err = |r: anyhow::Result<DictionaryEntry>| r.unwrap_err().to_string();
        assert_eq!(
            err(db.dictionary_add("joona", None, 1)),
            "“joona” is already in your dictionary."
        );
        assert_eq!(
            err(db.dictionary_add("Jun", Some("JUNA"), 1)),
            "Joona already changes “JUNA” to “Joona”."
        );
        assert_eq!(
            err(db.dictionary_add(" ,. ", None, 1)),
            "Type a word or phrase."
        );
        assert_eq!(
            err(db.dictionary_add("GitHub", Some("GitHub"), 1)),
            "That's already how “GitHub” is written."
        );
        assert!(db.dictionary_add("GitHub", Some("github"), 1).is_ok());
        assert!(err(db.dictionary_add(&"a".repeat(101), None, 1)).contains("100"));
    }

    #[test]
    fn imports_and_exports_csv() {
        let dir = std::env::temp_dir().join(format!("joona-dict-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("in.csv");
        std::fs::write(
            &file,
            "\u{feff}Word,Heard,Starred\nJoona,Juna,yes\nPriya,,\n\nJoona,Juna,\n\"Sharma, Priya\",,1\n,,\n",
        )
        .unwrap();
        let db = Db::in_memory().unwrap();
        let imported = db.dictionary_import(&file, 5).unwrap();
        assert_eq!(
            imported,
            Imported {
                added: 3,
                skipped: 1
            }
        );
        let entries = db.dictionary().unwrap();
        assert!(
            entries
                .iter()
                .any(|e| e.word == "Sharma, Priya" && e.starred)
        );

        let out = dir.join("out.csv");
        assert_eq!(db.dictionary_export(&out).unwrap(), 3);
        let again = Db::in_memory().unwrap();
        assert_eq!(again.dictionary_import(&out, 6).unwrap().added, 3);
        assert_eq!(words(&again), words(&db));

        std::fs::write(&file, "Kubernetes\nPostgreSQL\n").unwrap();
        assert_eq!(again.dictionary_import(&file, 7).unwrap().added, 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
