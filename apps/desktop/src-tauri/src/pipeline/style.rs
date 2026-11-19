//! Styles: how polished text is for the kind of app it goes into. These are
//! the rule-based styles, used when no AI model polishes the text.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::bail;
use regex::Regex;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::cleanup::{abbreviation, sentence_starts};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    /// Chats with friends and family.
    Personal,
    /// Chats at work.
    Work,
    Email,
    /// Documents, notes, code and everything else.
    Other,
}

impl Category {
    pub const ALL: [Self; 4] = [Self::Personal, Self::Work, Self::Email, Self::Other];

    /// How it's saved in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Work => "work",
            Self::Email => "email",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    pub fn default_tone(self) -> Tone {
        match self {
            Self::Personal | Self::Work => Tone::Casual,
            Self::Email | Self::Other => Tone::Formal,
        }
    }

    /// Something a dictation in this kind of app could say, after cleanup.
    fn example(self) -> &'static str {
        match self {
            Self::Personal => {
                "Hey, are you free for lunch tomorrow? Let's do 12 if that works for you."
            }
            Self::Work => {
                "Hi team, the deck is ready for review. Let me know what you think by Friday."
            }
            Self::Email => {
                "Hi Priya, thanks for sending the report. I'll go through it today and get back to you."
            }
            Self::Other => "Remember to book the train tickets for Saturday morning.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename = "StyleTone", rename_all = "camelCase")]
pub enum Tone {
    /// Capitals and full punctuation.
    Formal,
    /// Capitals and lighter punctuation.
    Casual,
    /// Lower case and lighter punctuation.
    VeryCasual,
    /// An exclamation mark to finish.
    Excited,
}

impl Tone {
    pub const ALL: [Self; 4] = [Self::Formal, Self::Casual, Self::VeryCasual, Self::Excited];

    /// How it's saved in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Formal => "formal",
            Self::Casual => "casual",
            Self::VeryCasual => "very_casual",
            Self::Excited => "excited",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

/// An app or website, and the kind it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppRule {
    /// The app's program, such as `slack.exe`, or the website's name as its
    /// tab title shows it, such as `Gmail`.
    pub app: String,
    pub name: String,
    pub website: bool,
    pub category: Category,
    /// On Joona's own list.
    pub built_in: bool,
    /// The user picked the category.
    pub changed: bool,
}

/// One category's tone, with a preview of each tone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryStyle {
    pub category: Category,
    pub tone: Tone,
    pub previews: Vec<TonePreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TonePreview {
    pub tone: Tone,
    pub text: String,
}

struct App {
    name: &'static str,
    /// The programs it runs as. The first one is how it's saved.
    exes: &'static [&'static str],
    category: Category,
}

const APPS: &[App] = &[
    App {
        name: "WhatsApp",
        exes: &["whatsapp.exe", "whatsapp.root.exe"],
        category: Category::Personal,
    },
    App {
        name: "Telegram",
        exes: &["telegram.exe"],
        category: Category::Personal,
    },
    App {
        name: "Signal",
        exes: &["signal.exe"],
        category: Category::Personal,
    },
    App {
        name: "Discord",
        exes: &["discord.exe"],
        category: Category::Personal,
    },
    App {
        name: "Messenger",
        exes: &["messenger.exe"],
        category: Category::Personal,
    },
    App {
        name: "Phone Link",
        exes: &["phoneexperiencehost.exe"],
        category: Category::Personal,
    },
    App {
        name: "Slack",
        exes: &["slack.exe"],
        category: Category::Work,
    },
    App {
        name: "Microsoft Teams",
        exes: &["ms-teams.exe", "teams.exe"],
        category: Category::Work,
    },
    App {
        name: "Zoom",
        exes: &["zoom.exe"],
        category: Category::Work,
    },
    App {
        name: "Mattermost",
        exes: &["mattermost.exe"],
        category: Category::Work,
    },
    App {
        name: "Outlook",
        exes: &["olk.exe", "outlook.exe"],
        category: Category::Email,
    },
    App {
        name: "Thunderbird",
        exes: &["thunderbird.exe"],
        category: Category::Email,
    },
    App {
        name: "Mailbird",
        exes: &["mailbird.exe"],
        category: Category::Email,
    },
    App {
        name: "eM Client",
        exes: &["mailclient.exe"],
        category: Category::Email,
    },
];

/// Websites, found by name in the title of the browser tab.
const SITES: &[(&str, Category)] = &[
    ("WhatsApp", Category::Personal),
    ("Messenger", Category::Personal),
    ("Instagram", Category::Personal),
    ("Telegram", Category::Personal),
    ("Discord", Category::Personal),
    ("Slack", Category::Work),
    ("Microsoft Teams", Category::Work),
    ("Google Chat", Category::Work),
    ("LinkedIn", Category::Work),
    ("Gmail", Category::Email),
    ("Outlook", Category::Email),
    ("Proton Mail", Category::Email),
    ("Yahoo Mail", Category::Email),
    ("Zoho Mail", Category::Email),
];

const BROWSERS: &[&str] = &[
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "brave.exe",
    "opera.exe",
    "vivaldi.exe",
    "arc.exe",
    "zen.exe",
    "librewolf.exe",
    "waterfox.exe",
    "floorp.exe",
    "chromium.exe",
    "thorium.exe",
];

/// Names for programs whose file names read badly.
const NAMES: &[(&str, &str)] = &[
    ("winword.exe", "Word"),
    ("excel.exe", "Excel"),
    ("powerpnt.exe", "PowerPoint"),
    ("onenote.exe", "OneNote"),
    ("code.exe", "VS Code"),
    ("msedge.exe", "Edge"),
    ("windowsterminal.exe", "Terminal"),
    ("explorer.exe", "File Explorer"),
];

const MAX_NAME: usize = 60;

/// How an app is shown: `ms-teams.exe` is "Microsoft Teams", and a program
/// Joona doesn't know is its file name, with a capital.
pub fn app_name(exe: &str) -> String {
    let known = APPS
        .iter()
        .find(|a| a.exes.iter().any(|e| e.eq_ignore_ascii_case(exe)))
        .map(|a| a.name)
        .or_else(|| {
            NAMES
                .iter()
                .find(|(e, _)| e.eq_ignore_ascii_case(exe))
                .map(|(_, name)| *name)
        });
    if let Some(name) = known {
        return name.to_owned();
    }
    let stem = exe
        .get(exe.len().saturating_sub(4)..)
        .filter(|end| end.eq_ignore_ascii_case(".exe"))
        .map_or(exe, |_| &exe[..exe.len() - 4]);
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => exe.to_owned(),
    }
}

fn is_browser(exe: &str) -> bool {
    BROWSERS.iter().any(|b| b.eq_ignore_ascii_case(exe))
}

fn is_program(key: &str) -> bool {
    key.to_ascii_lowercase().ends_with(".exe")
}

/// How an app or website the user names is saved: programs by their lower
/// case file name, websites as the user wrote them.
pub fn app_key(input: &str) -> anyhow::Result<String> {
    let key = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let key = key.rsplit(['\\', '/']).next().unwrap_or_default();
    if !key.chars().any(char::is_alphanumeric) {
        bail!("Type the name of an app or website.");
    }
    if key.chars().count() > MAX_NAME {
        bail!("Keep the name under {MAX_NAME} characters.");
    }
    if is_program(key) {
        return Ok(key.to_lowercase());
    }
    let site = SITES.iter().find(|(s, _)| s.eq_ignore_ascii_case(key));
    Ok(site.map_or(key, |(s, _)| *s).to_owned())
}

/// Every program that `app` runs as: Teams is `ms-teams.exe` or `teams.exe`.
pub fn aliases(app: &str) -> Vec<String> {
    APPS.iter()
        .find(|a| a.exes.iter().any(|e| e.eq_ignore_ascii_case(app)))
        .map_or_else(
            || vec![app.to_owned()],
            |a| a.exes.iter().map(|e| (*e).to_owned()).collect(),
        )
}

fn built_in(key: &str) -> bool {
    APPS.iter()
        .any(|a| a.exes.iter().any(|e| e.eq_ignore_ascii_case(key)))
        || SITES.iter().any(|(s, _)| s.eq_ignore_ascii_case(key))
}

/// Whether `needle` is in `haystack` as whole words.
fn contains_words(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(i, _)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + needle.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Which apps are which kind, and the tone for each kind.
#[derive(Debug, Clone)]
pub struct StyleMap {
    tones: HashMap<Category, Tone>,
    /// By lower-case program name.
    apps: HashMap<String, Category>,
    /// By lower-case name, longest first.
    sites: Vec<(String, Category)>,
    rules: Vec<AppRule>,
}

impl Default for StyleMap {
    fn default() -> Self {
        Self::new(&[], &[])
    }
}

impl StyleMap {
    /// `chosen` are the user's choices, by program or website name.
    pub fn new(tones: &[(Category, Tone)], chosen: &[(String, Category)]) -> Self {
        let choice = |key: &str| {
            chosen
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, c)| *c)
        };
        let mut map = Self {
            tones: tones.iter().copied().collect(),
            apps: HashMap::new(),
            sites: Vec::new(),
            rules: Vec::new(),
        };
        for app in APPS {
            let user = app.exes.iter().find_map(|exe| choice(exe));
            let category = user.unwrap_or(app.category);
            for exe in app.exes {
                map.apps.insert((*exe).to_owned(), category);
            }
            map.rules.push(AppRule {
                app: app.exes[0].to_owned(),
                name: app.name.to_owned(),
                website: false,
                category,
                built_in: true,
                changed: user.is_some(),
            });
        }
        for &(site, default) in SITES {
            let user = choice(site);
            let category = user.unwrap_or(default);
            map.sites.push((site.to_lowercase(), category));
            map.rules.push(AppRule {
                app: site.to_owned(),
                name: site.to_owned(),
                website: true,
                category,
                built_in: true,
                changed: user.is_some(),
            });
        }
        for (key, category) in chosen {
            if built_in(key) {
                continue;
            }
            let website = !is_program(key);
            if website {
                map.sites.push((key.to_lowercase(), *category));
            } else {
                map.apps.insert(key.to_lowercase(), *category);
            }
            map.rules.push(AppRule {
                app: key.clone(),
                name: if website { key.clone() } else { app_name(key) },
                website,
                category: *category,
                built_in: false,
                changed: true,
            });
        }
        map.sites.sort_by_key(|(name, _)| Reverse(name.len()));
        map.rules
            .sort_by_cached_key(|r| (r.name.to_lowercase(), r.website));
        map
    }

    /// The kind of app `exe` is. In a browser, it's the kind of website
    /// the tab's `title` names.
    pub fn category(&self, exe: &str, title: &str) -> Category {
        let exe = exe.to_lowercase();
        if is_browser(&exe) {
            let title = title.to_lowercase();
            if let Some((_, category)) = self
                .sites
                .iter()
                .find(|(site, _)| contains_words(&title, site))
            {
                return *category;
            }
        }
        self.apps.get(&exe).copied().unwrap_or(Category::Other)
    }

    pub fn tone(&self, category: Category) -> Tone {
        self.tones
            .get(&category)
            .copied()
            .unwrap_or(category.default_tone())
    }

    pub fn rules(&self) -> &[AppRule] {
        &self.rules
    }

    pub fn is_listed(&self, app: &str) -> bool {
        self.apps.contains_key(&app.to_lowercase())
    }

    pub fn overview(&self) -> Vec<CategoryStyle> {
        Category::ALL
            .into_iter()
            .map(|category| CategoryStyle {
                category,
                tone: self.tone(category),
                previews: Tone::ALL
                    .into_iter()
                    .map(|tone| TonePreview {
                        tone,
                        text: apply(category.example(), tone, |_| false),
                    })
                    .collect(),
            })
            .collect()
    }
}

/// Greetings and short replies that casual writing doesn't follow with a
/// comma, at the start of the text or of a line.
static OPENING_COMMA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?mi)^(\s*(?:hey|hi|hello|hiya|yo|oh|ok|okay|yeah|yes|yep|no|nope|well|so|thanks|cool|sure))\b,",
    )
    .unwrap()
});

/// Applies `tone` to text that cleanup already capitalised and punctuated.
/// `is_name` says whether a word that starts a sentence is a name, which
/// keeps its capital even when very casual.
pub fn apply(text: &str, tone: Tone, is_name: impl Fn(&str) -> bool) -> String {
    let body = text.trim_end();
    let tail = &text[body.len()..];
    let styled = match tone {
        Tone::Formal if open_end(body) => format!("{body}."),
        Tone::Formal => body.to_owned(),
        Tone::Casual => drop_stop(&OPENING_COMMA.replace_all(body, "$1")).to_owned(),
        Tone::VeryCasual => {
            let loose = OPENING_COMMA.replace_all(body, "$1");
            lower_starts(drop_stop(&loose), &is_name)
        }
        Tone::Excited => match final_stop(body) {
            Some(rest) => format!("{rest}!"),
            None if open_end(body) => format!("{body}!"),
            None => body.to_owned(),
        },
    };
    styled + tail
}

/// Whether the text ends mid-sentence: on a letter of a script with
/// capitals, or a digit. Other scripts have their own full stops.
fn open_end(body: &str) -> bool {
    body.chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_digit() || c.is_lowercase() || c.is_uppercase())
}

/// The text without its last full stop, if it ends a sentence.
fn final_stop(body: &str) -> Option<&str> {
    let rest = body.strip_suffix('.')?;
    let ends_word = rest.chars().next_back().is_some_and(char::is_alphanumeric);
    (ends_word && !abbreviation(rest)).then_some(rest)
}

fn drop_stop(body: &str) -> &str {
    final_stop(body).unwrap_or(body)
}

fn lower_starts(body: &str, is_name: &impl Fn(&str) -> bool) -> String {
    let mut out = body.to_owned();
    for at in sentence_starts(body).into_iter().rev() {
        let word = body[at..]
            .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’'))
            .next()
            .unwrap_or_default();
        let mut chars = word.chars();
        let Some(first) = chars.next() else {
            continue;
        };
        let plain = first.is_uppercase() && !chars.as_str().chars().any(char::is_uppercase);
        let pronoun = word == "I" || word.starts_with("I'") || word.starts_with("I’");
        if plain && !pronoun && !is_name(word) {
            out.replace_range(at..at + first.len_utf8(), &first.to_lowercase().to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_apps_and_websites() {
        let map = StyleMap::default();
        assert_eq!(map.category("Slack.exe", ""), Category::Work);
        assert_eq!(map.category("teams.exe", ""), Category::Work);
        assert_eq!(map.category("olk.exe", "Mail - Outlook"), Category::Email);
        assert_eq!(map.category("notepad.exe", "Gmail notes"), Category::Other);
        assert_eq!(
            map.category(
                "chrome.exe",
                "Inbox (3) - priya@gmail.com - Gmail - Google Chrome"
            ),
            Category::Email
        );
        assert_eq!(
            map.category("msedge.exe", "WhatsApp - Personal - Microsoft Edge"),
            Category::Personal
        );
        assert_eq!(
            map.category("firefox.exe", "Gmailify review"),
            Category::Other
        );
        assert_eq!(map.category("unknown.exe", ""), Category::Other);
    }

    #[test]
    fn follows_the_users_choices() {
        let map = StyleMap::new(
            &[(Category::Work, Tone::Formal)],
            &[
                ("ms-teams.exe".into(), Category::Personal),
                ("teams.exe".into(), Category::Personal),
                ("Notion".into(), Category::Work),
                ("obsidian.exe".into(), Category::Personal),
            ],
        );
        assert_eq!(map.category("teams.exe", ""), Category::Personal);
        assert_eq!(
            map.category("chrome.exe", "Roadmap | Notion"),
            Category::Work
        );
        assert_eq!(map.category("Obsidian.exe", ""), Category::Personal);
        assert_eq!(map.tone(Category::Work), Tone::Formal);
        assert_eq!(map.tone(Category::Personal), Tone::Casual);

        let rule = |app: &str| map.rules().iter().find(|r| r.app == app).unwrap();
        assert!(rule("ms-teams.exe").changed && rule("ms-teams.exe").built_in);
        assert!(!rule("slack.exe").changed);
        assert_eq!(rule("obsidian.exe").name, "Obsidian");
        assert!(rule("Notion").website && !rule("Notion").built_in);
        assert!(map.rules().iter().all(|r| r.app != "teams.exe"));
    }

    #[test]
    fn names_apps_and_reads_what_users_type() {
        assert_eq!(app_name("ms-teams.exe"), "Microsoft Teams");
        assert_eq!(app_name("WINWORD.EXE"), "Word");
        assert_eq!(app_name("slack.exe"), "Slack");
        assert_eq!(app_name("notepad"), "Notepad");
        assert_eq!(app_key(r"C:\Apps\Obsidian.EXE").unwrap(), "obsidian.exe");
        assert_eq!(app_key("  gmail ").unwrap(), "Gmail");
        assert_eq!(app_key("My  Wiki").unwrap(), "My Wiki");
        assert!(app_key(" -- ").is_err());
        assert_eq!(aliases("teams.exe"), ["ms-teams.exe", "teams.exe"]);
        assert_eq!(aliases("Notion"), ["Notion"]);
    }

    #[test]
    fn applies_tones() {
        let casual = |text| apply(text, Tone::Casual, |_| false);
        let very = |text| apply(text, Tone::VeryCasual, |w: &str| w == "Priya");
        let excited = |text| apply(text, Tone::Excited, |_| false);
        let formal = |text| apply(text, Tone::Formal, |_| false);

        assert_eq!(formal("See you soon"), "See you soon.");
        assert_eq!(formal("Is it done?"), "Is it done?");
        assert_eq!(formal("कल मिलते हैं"), "कल मिलते हैं");
        assert_eq!(
            casual("Hey, are you free? Let's go."),
            "Hey are you free? Let's go"
        );
        assert_eq!(casual("Thanks.\n"), "Thanks\n");
        assert_eq!(casual("See you at 5 p.m."), "See you at 5 p.m.");
        assert_eq!(
            very("Hey, are you free? I think so."),
            "hey are you free? I think so"
        );
        assert_eq!(very("Priya said OK. NASA too."), "Priya said OK. NASA too");
        assert_eq!(excited("That's great."), "That's great!");
        assert_eq!(excited("That's great"), "That's great!");
        assert_eq!(excited("Really?"), "Really?");
        assert_eq!(formal("Regards \u{F0000}"), "Regards \u{F0000}");
    }

    #[test]
    fn previews_every_tone() {
        let overview = StyleMap::default().overview();
        assert_eq!(overview.len(), 4);
        let personal = &overview[0];
        assert_eq!(personal.tone, Tone::Casual);
        let text = |tone| {
            &personal
                .previews
                .iter()
                .find(|p| p.tone == tone)
                .unwrap()
                .text
        };
        assert_eq!(
            text(Tone::VeryCasual),
            "hey are you free for lunch tomorrow? let's do 12 if that works for you"
        );
        assert!(text(Tone::Excited).ends_with("for you!"));
    }
}
