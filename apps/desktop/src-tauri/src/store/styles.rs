//! The tone for each kind of app, and the user's choices of which apps are
//! which kind.

use rusqlite::params;

use super::Db;
use crate::pipeline::style::{self, Category, Tone};

impl Db {
    pub fn tones(&self) -> anyhow::Result<Vec<(Category, Tone)>> {
        let db = self.lock();
        let mut stmt = db.prepare("SELECT category, tone FROM styles")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut tones = Vec::new();
        for row in rows {
            let (category, tone) = row?;
            if let (Some(category), Some(tone)) = (Category::parse(&category), Tone::parse(&tone)) {
                tones.push((category, tone));
            }
        }
        Ok(tones)
    }

    pub fn set_tone(&self, category: Category, tone: Tone) -> anyhow::Result<()> {
        self.lock().execute(
            "INSERT INTO styles (category, tone) VALUES (?1, ?2)
             ON CONFLICT (category) DO UPDATE SET tone = excluded.tone",
            params![category.as_str(), tone.as_str()],
        )?;
        Ok(())
    }

    /// Apps and websites the user put in a category, by program or name.
    pub fn app_categories(&self) -> anyhow::Result<Vec<(String, Category)>> {
        let db = self.lock();
        let mut stmt = db.prepare("SELECT app, category FROM app_categories ORDER BY app")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut apps = Vec::new();
        for row in rows {
            let (app, category) = row?;
            if let Some(category) = Category::parse(&category) {
                apps.push((app, category));
            }
        }
        Ok(apps)
    }

    /// Puts `app` in `category`, or back to Joona's choice with `None`.
    pub fn set_app_category(&self, app: &str, category: Option<Category>) -> anyhow::Result<()> {
        let key = style::app_key(app)?;
        let mut db = self.lock();
        let tx = db.transaction()?;
        for alias in style::aliases(&key) {
            tx.execute(
                "DELETE FROM app_categories WHERE app = ?1 COLLATE NOCASE",
                [&alias],
            )?;
            if let Some(category) = category {
                tx.execute(
                    "INSERT INTO app_categories (app, category) VALUES (?1, ?2)",
                    params![alias, category.as_str()],
                )?;
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
    fn saves_tones_and_app_choices() {
        let db = Db::in_memory().unwrap();
        db.set_tone(Category::Work, Tone::Formal).unwrap();
        db.set_tone(Category::Work, Tone::Excited).unwrap();
        assert_eq!(db.tones().unwrap(), [(Category::Work, Tone::Excited)]);

        db.set_app_category("Teams.exe", Some(Category::Personal))
            .unwrap();
        db.set_app_category("notion", Some(Category::Email))
            .unwrap();
        db.set_app_category("Notion", Some(Category::Work)).unwrap();
        assert_eq!(
            db.app_categories().unwrap(),
            [
                ("Notion".to_owned(), Category::Work),
                ("ms-teams.exe".to_owned(), Category::Personal),
                ("teams.exe".to_owned(), Category::Personal),
            ]
        );
        db.set_app_category("ms-teams.exe", None).unwrap();
        assert_eq!(db.app_categories().unwrap().len(), 1);
        assert!(db.set_app_category("  ", Some(Category::Work)).is_err());
    }
}
