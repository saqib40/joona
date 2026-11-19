//! Joona's database, `%LOCALAPPDATA%\Joona\joona.db`: the dictation history, the
//! dictionary, snippets and styles.

mod dictionary;
mod snippets;
mod styles;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

pub use self::dictionary::{DictionaryEntry, Imported};
pub use self::snippets::{MAX_SNIPPET, Snippet};

/// Step `i` brings the schema from version `i` to `i + 1`.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS dictations (
        id INTEGER PRIMARY KEY,
        created_at INTEGER NOT NULL,
        app TEXT,
        app_name TEXT,
        raw TEXT NOT NULL,
        text TEXT NOT NULL,
        language TEXT,
        output TEXT,
        words INTEGER NOT NULL,
        audio_ms INTEGER NOT NULL,
        model TEXT NOT NULL,
        cloud TEXT,
        mode TEXT NOT NULL,
        delivery TEXT NOT NULL,
        fixes INTEGER NOT NULL DEFAULT 0,
        category TEXT,
        audio INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS dictations_created ON dictations (created_at);",
    "CREATE TABLE dictionary (
        id INTEGER PRIMARY KEY,
        word TEXT NOT NULL,
        heard TEXT,
        starred INTEGER NOT NULL DEFAULT 0,
        uses INTEGER NOT NULL DEFAULT 0,
        last_used INTEGER,
        created_at INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX dictionary_words ON dictionary (lower(word)) WHERE heard IS NULL;
    CREATE UNIQUE INDEX dictionary_heard ON dictionary (lower(heard)) WHERE heard IS NOT NULL;
    CREATE TABLE snippets (
        id INTEGER PRIMARY KEY,
        phrase TEXT NOT NULL,
        text TEXT NOT NULL,
        uses INTEGER NOT NULL DEFAULT 0,
        last_used INTEGER,
        created_at INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX snippets_phrase ON snippets (lower(phrase));
    CREATE TABLE app_categories (
        app TEXT PRIMARY KEY,
        category TEXT NOT NULL
    );
    CREATE TABLE styles (
        category TEXT PRIMARY KEY,
        tone TEXT NOT NULL
    );",
];

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::new(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn in_memory() -> anyhow::Result<Self> {
        Self::new(Connection::open_in_memory()?)
    }

    fn new(mut conn: Connection) -> anyhow::Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, step) in MIGRATIONS.iter().enumerate().skip(version.max(0) as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(step)?;
            tx.pragma_update(None, "user_version", i as i64 + 1)?;
            tx.commit()?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Trims `text` and turns every run of whitespace into one space.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_a_database_from_the_first_version() {
        let path = std::env::temp_dir().join(format!("joona-db-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO dictations (created_at, raw, text, words, audio_ms, model, mode, delivery)
                 VALUES (1, 'hi', 'Hi', 1, 900, 'whisper-base', 'dictate', 'pasted')",
                [],
            )
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        {
            let conn = db.lock();
            let version: i64 = conn
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .unwrap();
            assert_eq!(version, MIGRATIONS.len() as i64);
            let kept: i64 = conn
                .query_row("SELECT COUNT(*) FROM dictations", [], |r| r.get(0))
                .unwrap();
            assert_eq!(kept, 1);
        }
        assert!(db.dictionary().unwrap().is_empty());
        drop(db);
        // Opening it again runs nothing twice.
        Db::open(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn one_line_collapses_whitespace() {
        assert_eq!(one_line("  my \n home\taddress "), "my home address");
    }
}
