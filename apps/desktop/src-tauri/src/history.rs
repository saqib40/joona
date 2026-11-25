//! Dictation history in SQLite (`joona.db`), and the numbers the Home and
//! Insights pages show. Everything is computed on this PC.

use std::path::{Path, PathBuf};
use std::sync::{Arc, MutexGuard};

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::hotkeys::Mode;
use crate::store::Db;

/// Typing speed used to estimate the time dictation saved.
pub const TYPING_WPM: f64 = 40.0;
/// Clips shorter than this make the words-per-minute figure jumpy.
const MIN_WPM_AUDIO_MS: i64 = 1_500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Delivery {
    Pasted,
    Typed,
    /// Left on the clipboard for the user to paste.
    Copied,
    Failed,
}

impl Delivery {
    fn as_str(self) -> &'static str {
        match self {
            Delivery::Pasted => "pasted",
            Delivery::Typed => "typed",
            Delivery::Copied => "copied",
            Delivery::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "pasted" => Delivery::Pasted,
            "typed" => Delivery::Typed,
            "copied" => Delivery::Copied,
            _ => Delivery::Failed,
        }
    }
}

fn mode_str(mode: Mode) -> &'static str {
    match mode {
        Mode::Dictate => "dictate",
        Mode::Alternate => "alternate",
        Mode::Command => "command",
    }
}

fn parse_mode(s: &str) -> Mode {
    match s {
        "alternate" => Mode::Alternate,
        "command" => Mode::Command,
        _ => Mode::Dictate,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    /// Unix time in milliseconds.
    pub created_at: i64,
    /// The program the text went into, for example `slack.exe`.
    pub app: Option<String>,
    pub app_name: Option<String>,
    /// What the engine heard, before cleanup.
    pub raw: String,
    /// What was inserted.
    pub text: String,
    /// The spoken language, when known.
    pub language: Option<String>,
    /// The language the text came out in, when it differs from the spoken one.
    pub output: Option<String>,
    pub words: u32,
    pub audio_ms: u32,
    pub model: String,
    /// The cloud service the audio went to, if any.
    pub cloud: Option<String>,
    pub mode: Mode,
    pub delivery: Delivery,
    /// Corrections the text pipeline made, such as dictionary replacements.
    pub fixes: u32,
    /// Style category of the app, for example `email`.
    pub category: Option<String>,
    /// The recording was kept (Data & Privacy → Save audio).
    pub audio: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HomeStats {
    pub words_this_week: u32,
    /// Speaking speed over the last 30 days. 0 without enough data.
    pub wpm: u32,
    /// Days in a row with at least one dictation, up to today.
    pub streak: u32,
    pub total_dictations: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub app: String,
    pub name: String,
    pub category: Option<String>,
    pub words: u32,
    pub dictations: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DayActivity {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    pub words: u32,
    pub dictations: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Insights {
    pub total_words: u32,
    pub total_dictations: u32,
    pub wpm: u32,
    /// Compared with typing at [`TYPING_WPM`].
    pub minutes_saved: u32,
    pub fixes: u32,
    pub apps: Vec<AppUsage>,
    /// Days with dictations in the last year, oldest first.
    pub days: Vec<DayActivity>,
    pub current_streak: u32,
    pub longest_streak: u32,
    /// Share of dictations that left the PC, 0 to 100.
    pub cloud_percent: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Csv,
    Json,
}

/// An app the user dictated into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentApp {
    /// Its program, such as `slack.exe`.
    pub app: String,
    pub name: String,
}

pub struct History {
    db: Arc<Db>,
    audio_dir: PathBuf,
}

impl History {
    pub fn new(db: Arc<Db>, audio_dir: PathBuf) -> Self {
        Self { db, audio_dir }
    }

    #[cfg(test)]
    pub fn in_memory() -> anyhow::Result<Self> {
        Ok(Self::new(
            Arc::new(Db::in_memory()?),
            std::env::temp_dir().join("joona-test-audio"),
        ))
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock()
    }

    /// Where a kept recording lives.
    pub fn audio_path(&self, id: i64) -> PathBuf {
        self.audio_dir.join(format!("{id}.wav"))
    }

    /// Saves a dictation and returns it with its id.
    pub fn add(&self, mut entry: Entry) -> anyhow::Result<Entry> {
        let db = self.db();
        db.execute(
            "INSERT INTO dictations (created_at, app, app_name, raw, text, language, output, words,
                audio_ms, model, cloud, mode, delivery, fixes, category, audio)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                entry.created_at,
                entry.app,
                entry.app_name,
                entry.raw,
                entry.text,
                entry.language,
                entry.output,
                entry.words,
                entry.audio_ms,
                entry.model,
                entry.cloud,
                mode_str(entry.mode),
                entry.delivery.as_str(),
                entry.fixes,
                entry.category,
                entry.audio,
            ],
        )?;
        entry.id = db.last_insert_rowid();
        Ok(entry)
    }

    /// Marks an entry as having a kept recording.
    pub fn set_audio(&self, id: i64, audio: bool) -> anyhow::Result<()> {
        self.db().execute(
            "UPDATE dictations SET audio = ?2 WHERE id = ?1",
            params![id, audio],
        )?;
        Ok(())
    }

    pub fn get(&self, id: i64) -> anyhow::Result<Option<Entry>> {
        let db = self.db();
        let mut stmt = db.prepare(&format!("SELECT {COLUMNS} FROM dictations WHERE id = ?1"))?;
        let mut rows = stmt.query_map([id], row_to_entry)?;
        Ok(rows.next().transpose()?)
    }

    /// Newest first. `before` and `before_id` are the `created_at` and `id` of
    /// the last entry already shown.
    pub fn list(
        &self,
        search: &str,
        before: Option<i64>,
        before_id: Option<i64>,
        limit: u32,
    ) -> anyhow::Result<Vec<Entry>> {
        let db = self.db();
        let pattern = format!("%{}%", escape_like(search.trim()));
        let (before_ts, before_row_id) = match before {
            Some(ts) => (ts, before_id.unwrap_or(i64::MIN)),
            None => (i64::MAX, i64::MAX),
        };
        let mut stmt = db.prepare(&format!(
            "SELECT {COLUMNS} FROM dictations
             WHERE (created_at, id) < (?1, ?2)
               AND (?3 = '%%' OR text LIKE ?3 ESCAPE '\\' OR raw LIKE ?3 ESCAPE '\\' OR app_name LIKE ?3 ESCAPE '\\')
             ORDER BY created_at DESC, id DESC
             LIMIT ?4"
        ))?;
        let rows = stmt.query_map(
            params![before_ts, before_row_id, pattern, limit],
            row_to_entry,
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Apps dictated into, most recently used first.
    pub fn recent_apps(&self, limit: u32) -> anyhow::Result<Vec<RecentApp>> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT lower(app), MAX(app_name) FROM dictations
             WHERE app IS NOT NULL AND app != ''
             GROUP BY lower(app) ORDER BY MAX(created_at) DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            let app: String = r.get(0)?;
            let name: Option<String> = r.get(1)?;
            Ok(RecentApp {
                name: name.unwrap_or_else(|| app.clone()),
                app,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn delete(&self, id: i64) -> anyhow::Result<()> {
        self.db()
            .execute("DELETE FROM dictations WHERE id = ?1", [id])?;
        let _ = std::fs::remove_file(self.audio_path(id));
        Ok(())
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        self.db().execute("DELETE FROM dictations", [])?;
        let _ = std::fs::remove_dir_all(&self.audio_dir);
        Ok(())
    }

    /// Deletes entries older than `days`. Returns how many went.
    pub fn prune(&self, days: u32, now_ms: i64) -> anyhow::Result<usize> {
        if days == 0 {
            return Ok(0);
        }
        let cutoff = now_ms - i64::from(days) * 86_400_000;
        let db = self.db();
        let ids: Vec<i64> = db
            .prepare("SELECT id FROM dictations WHERE created_at < ?1 AND audio = 1")?
            .query_map([cutoff], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let removed = db.execute("DELETE FROM dictations WHERE created_at < ?1", [cutoff])?;
        drop(db);
        for id in ids {
            let _ = std::fs::remove_file(self.audio_path(id));
        }
        Ok(removed)
    }

    pub fn home_stats(&self, now_ms: i64) -> anyhow::Result<HomeStats> {
        let today = local_date(now_ms);
        let monday = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
        let db = self.db();
        let words_this_week: i64 = db.query_row(
            "SELECT COALESCE(SUM(words), 0) FROM dictations WHERE created_at >= ?1",
            [local_midnight_ms(monday)],
            |r| r.get(0),
        )?;
        let total: i64 = db.query_row("SELECT COUNT(*) FROM dictations", [], |r| r.get(0))?;
        let wpm = wpm(&db, now_ms - 30 * 86_400_000)?;
        let days = active_days(&db)?;
        Ok(HomeStats {
            words_this_week: words_this_week as u32,
            wpm,
            streak: current_streak(&days, today),
            total_dictations: total as u32,
        })
    }

    pub fn insights(&self, now_ms: i64) -> anyhow::Result<Insights> {
        let today = local_date(now_ms);
        let db = self.db();
        let (total_words, total_dictations, audio_ms, fixes, cloud): (i64, i64, i64, i64, i64) = db.query_row(
            "SELECT COALESCE(SUM(words), 0), COUNT(*), COALESCE(SUM(audio_ms), 0), COALESCE(SUM(fixes), 0),
                    COALESCE(SUM(cloud IS NOT NULL), 0)
             FROM dictations",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        let typing_minutes = total_words as f64 / TYPING_WPM;
        let speaking_minutes = audio_ms as f64 / 60_000.0;
        let apps = db
            .prepare(
                "SELECT app, MAX(app_name), MAX(category), SUM(words), COUNT(*) FROM dictations
                 WHERE app IS NOT NULL GROUP BY lower(app) ORDER BY SUM(words) DESC LIMIT 8",
            )?
            .query_map([], |r| {
                let app: String = r.get(0)?;
                Ok(AppUsage {
                    name: r
                        .get::<_, Option<String>>(1)?
                        .unwrap_or_else(|| app.clone()),
                    app,
                    category: r.get(2)?,
                    words: r.get::<_, i64>(3)? as u32,
                    dictations: r.get::<_, i64>(4)? as u32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let all_days = active_days(&db)?;
        let year_ago = today - Duration::days(364);
        let days = all_days
            .iter()
            .filter(|(date, _)| *date >= year_ago)
            .map(|(date, (words, dictations))| DayActivity {
                date: date.format("%Y-%m-%d").to_string(),
                words: *words,
                dictations: *dictations,
            })
            .collect();
        Ok(Insights {
            total_words: total_words as u32,
            total_dictations: total_dictations as u32,
            wpm: wpm(&db, i64::MIN)?,
            minutes_saved: (typing_minutes - speaking_minutes).max(0.0).round() as u32,
            fixes: fixes as u32,
            apps,
            days,
            current_streak: current_streak(&all_days, today),
            longest_streak: longest_streak(&all_days),
            cloud_percent: if total_dictations > 0 {
                (cloud * 100 / total_dictations) as u32
            } else {
                0
            },
        })
    }

    pub fn export(&self, path: &Path, format: ExportFormat) -> anyhow::Result<usize> {
        let entries = self.list("", None, None, u32::MAX)?;
        match format {
            ExportFormat::Json => std::fs::write(path, serde_json::to_vec_pretty(&entries)?)?,
            ExportFormat::Csv => {
                let mut w = csv::Writer::from_path(path)?;
                w.write_record([
                    "date", "app", "language", "output", "words", "seconds", "model", "cloud",
                    "text", "raw",
                ])?;
                for e in &entries {
                    let date = Local
                        .timestamp_millis_opt(e.created_at)
                        .single()
                        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default();
                    w.write_record([
                        date,
                        e.app_name.clone().unwrap_or_default(),
                        e.language.clone().unwrap_or_default(),
                        e.output.clone().unwrap_or_default(),
                        e.words.to_string(),
                        format!("{:.1}", f64::from(e.audio_ms) / 1000.0),
                        e.model.clone(),
                        e.cloud.clone().unwrap_or_default(),
                        e.text.clone(),
                        e.raw.clone(),
                    ])?;
                }
                w.flush()?;
            }
        }
        Ok(entries.len())
    }
}

const COLUMNS: &str =
    "id, created_at, app, app_name, raw, text, language, output, words, audio_ms, model, cloud,
    mode, delivery, fixes, category, audio";

fn row_to_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        created_at: r.get(1)?,
        app: r.get(2)?,
        app_name: r.get(3)?,
        raw: r.get(4)?,
        text: r.get(5)?,
        language: r.get(6)?,
        output: r.get(7)?,
        words: r.get::<_, i64>(8)? as u32,
        audio_ms: r.get::<_, i64>(9)? as u32,
        model: r.get(10)?,
        cloud: r.get(11)?,
        mode: parse_mode(&r.get::<_, String>(12)?),
        delivery: Delivery::parse(&r.get::<_, String>(13)?),
        fixes: r.get::<_, i64>(14)? as u32,
        category: r.get(15)?,
        audio: r.get(16)?,
    })
}

/// Words in a transcript. Scripts written without spaces count a word per
/// two characters, roughly what a reader would.
pub fn count_words(text: &str) -> u32 {
    let spaced = text.split_whitespace().count();
    let unspaced = text
        .chars()
        .filter(|c| matches!(*c as u32, 0x3040..=0x30FF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0x0E00..=0x0E7F))
        .count();
    if unspaced > spaced * 2 {
        unspaced.div_ceil(2) as u32
    } else {
        spaced as u32
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn wpm(db: &Connection, since_ms: i64) -> rusqlite::Result<u32> {
    let (words, audio_ms): (i64, i64) = db.query_row(
        "SELECT COALESCE(SUM(words), 0), COALESCE(SUM(audio_ms), 0) FROM dictations
         WHERE created_at >= ?1 AND audio_ms >= ?2",
        params![since_ms, MIN_WPM_AUDIO_MS],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(if audio_ms >= 10_000 {
        (words as f64 / (audio_ms as f64 / 60_000.0)).round() as u32
    } else {
        0
    })
}

/// Words and dictations for each local day that had any, oldest first.
fn active_days(db: &Connection) -> rusqlite::Result<Vec<(NaiveDate, (u32, u32))>> {
    let mut stmt = db.prepare(
        "SELECT date(created_at / 1000, 'unixepoch', 'localtime') AS day, SUM(words), COUNT(*)
         FROM dictations GROUP BY day ORDER BY day",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u32,
            r.get::<_, i64>(2)? as u32,
        ))
    })?;
    let mut days = Vec::new();
    for row in rows {
        let (day, words, count) = row?;
        if let Ok(date) = NaiveDate::parse_from_str(&day, "%Y-%m-%d") {
            days.push((date, (words, count)));
        }
    }
    Ok(days)
}

/// Days in a row up to today, or up to yesterday when today has none yet.
fn current_streak(days: &[(NaiveDate, (u32, u32))], today: NaiveDate) -> u32 {
    let mut expected = match days.last() {
        Some((last, _)) if *last == today => today,
        Some((last, _)) if *last == today - Duration::days(1) => *last,
        _ => return 0,
    };
    let mut streak = 0;
    for (date, _) in days.iter().rev() {
        if *date != expected {
            break;
        }
        streak += 1;
        expected -= Duration::days(1);
    }
    streak
}

fn longest_streak(days: &[(NaiveDate, (u32, u32))]) -> u32 {
    let (mut best, mut run) = (0, 0);
    let mut previous: Option<NaiveDate> = None;
    for (date, _) in days {
        run = match previous {
            Some(p) if *date == p + Duration::days(1) => run + 1,
            _ => 1,
        };
        best = best.max(run);
        previous = Some(*date);
    }
    best
}

fn local_date(ms: i64) -> NaiveDate {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map_or_else(|| Local::now().date_naive(), |d| d.date_naive())
}

fn local_midnight_ms(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0)
        .and_then(|t| Local.from_local_datetime(&t).earliest())
        .map_or(0, |d| d.timestamp_millis())
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(created_at: i64, text: &str, app: &str) -> Entry {
        Entry {
            id: 0,
            created_at,
            app: Some(app.into()),
            app_name: Some(app.trim_end_matches(".exe").into()),
            raw: text.into(),
            text: text.into(),
            language: Some("en".into()),
            output: None,
            words: count_words(text),
            audio_ms: 6_000,
            model: "whisper-small".into(),
            cloud: None,
            mode: Mode::Dictate,
            delivery: Delivery::Pasted,
            fixes: 1,
            category: None,
            audio: false,
        }
    }

    fn noon(date: NaiveDate) -> i64 {
        local_midnight_ms(date) + 12 * 3_600_000
    }

    #[test]
    fn saves_lists_searches_and_deletes() {
        let history = History::in_memory().unwrap();
        let today = Local::now().date_naive();
        let a = history
            .add(entry(
                noon(today) - 2_000,
                "Let's meet at three",
                "slack.exe",
            ))
            .unwrap();
        let b = history
            .add(entry(
                noon(today) - 1_000,
                "100% done_with it",
                "outlook.exe",
            ))
            .unwrap();
        assert!(b.id > a.id);

        let all = history.list("", None, None, 10).unwrap();
        assert_eq!(all.iter().map(|e| e.id).collect::<Vec<_>>(), [b.id, a.id]);
        assert_eq!(
            history.list("MEET", None, None, 10).unwrap().len(),
            1,
            "search ignores case"
        );
        assert_eq!(
            history.list("100%", None, None, 10).unwrap().len(),
            1,
            "% is literal"
        );
        assert_eq!(
            history.list("slack", None, None, 10).unwrap().len(),
            1,
            "app names match"
        );
        assert_eq!(
            history.list("", Some(b.created_at), None, 10).unwrap()[0].id,
            a.id,
            "pages by time"
        );

        let recent = history.recent_apps(10).unwrap();
        assert_eq!(
            recent.iter().map(|r| r.app.as_str()).collect::<Vec<_>>(),
            ["outlook.exe", "slack.exe"]
        );

        assert_eq!(history.get(a.id).unwrap().unwrap(), a);
        history.delete(a.id).unwrap();
        assert!(history.get(a.id).unwrap().is_none());
        history.clear().unwrap();
        assert!(history.list("", None, None, 10).unwrap().is_empty());

        // Entries sharing the same created_at paginate without skipping.
        let ts = noon(today);
        let id1 = history.add(entry(ts, "one", "app.exe")).unwrap().id;
        let id2 = history.add(entry(ts, "two", "app.exe")).unwrap().id;
        let id3 = history.add(entry(ts, "three", "app.exe")).unwrap().id;
        let page1 = history.list("", None, None, 2).unwrap();
        assert_eq!(page1.iter().map(|e| e.id).collect::<Vec<_>>(), [id3, id2]);
        let last = page1.last().unwrap();
        let page2 = history
            .list("", Some(last.created_at), Some(last.id), 2)
            .unwrap();
        assert_eq!(page2.iter().map(|e| e.id).collect::<Vec<_>>(), [id1]);
    }

    #[test]
    fn stats_and_streaks() {
        let history = History::in_memory().unwrap();
        let today = Local::now().date_naive();
        for days_ago in [0, 1, 2, 5, 6, 7, 8] {
            history
                .add(entry(
                    noon(today - Duration::days(days_ago)),
                    "one two three four five six",
                    "code.exe",
                ))
                .unwrap();
        }
        let now = noon(today) + 1_000;
        let home = history.home_stats(now).unwrap();
        assert_eq!(home.streak, 3);
        assert_eq!(home.total_dictations, 7);
        assert_eq!(home.wpm, 60, "6 words in 6 seconds");

        let insights = history.insights(now).unwrap();
        assert_eq!(insights.total_words, 42);
        assert_eq!(insights.longest_streak, 4);
        assert_eq!(insights.current_streak, 3);
        assert_eq!(insights.fixes, 7);
        assert_eq!(insights.days.len(), 7);
        assert_eq!(insights.apps[0].dictations, 7);
        assert_eq!(
            insights.minutes_saved, 0,
            "42 words typed take about a minute, as long as speaking them"
        );

        assert_eq!(history.prune(3, now).unwrap(), 4);
        assert_eq!(history.home_stats(now).unwrap().total_dictations, 3);
    }

    #[test]
    fn a_streak_survives_until_today_ends() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let days: Vec<_> = [16, 17]
            .iter()
            .map(|d| (NaiveDate::from_ymd_opt(2026, 11, *d).unwrap(), (1, 1)))
            .collect();
        assert_eq!(current_streak(&days, today), 2);
        assert_eq!(current_streak(&days, today + Duration::days(1)), 0);
    }

    #[test]
    fn counts_words_in_any_script() {
        assert_eq!(count_words("  Hello there, world "), 3);
        assert_eq!(count_words("कल मिलते हैं"), 3);
        assert_eq!(count_words("今日はいい天気ですね"), 5);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn exports_csv_and_json() {
        let history = History::in_memory().unwrap();
        history
            .add(entry(now_ms(), "Hello, \"quoted\" world", "notepad.exe"))
            .unwrap();
        let dir = std::env::temp_dir().join(format!("joona-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            history
                .export(&dir.join("h.csv"), ExportFormat::Csv)
                .unwrap(),
            1
        );
        let csv = std::fs::read_to_string(dir.join("h.csv")).unwrap();
        assert!(csv.contains("\"Hello, \"\"quoted\"\" world\""));
        history
            .export(&dir.join("h.json"), ExportFormat::Json)
            .unwrap();
        let json: Vec<Entry> =
            serde_json::from_slice(&std::fs::read(dir.join("h.json")).unwrap()).unwrap();
        assert_eq!(json[0].app.as_deref(), Some("notepad.exe"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
