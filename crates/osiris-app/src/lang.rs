//! The player's language: which of the game's own string tables Osiris reads, and
//! Osiris's own words (the Rules window, the Mission Editor, its messages) in it.
//!
//! Osiris's words are written in English in the code, each wrapped in [`tr`], and
//! looked up in `lang/<code>.toml`, whose keys are those English strings. A string
//! missing from a table stays English. The language itself is the player's choice
//! from the Options menu (`language.txt` in the user folder), or automatically the
//! language of the string tables in the game folder.

use anyhow::{Context, Result};
use osiris_formats::language::{self, TextSet};
use osiris_formats::{Campaign, Language, MessageTable, Phrases, TextTable};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

/// The translations, by language, as shipped.
const TABLES: [(Language, &str); 4] = [
    (Language::German, include_str!("../lang/de.toml")),
    (Language::French, include_str!("../lang/fr.toml")),
    (Language::Spanish, include_str!("../lang/es.toml")),
    (Language::Italian, include_str!("../lang/it.toml")),
];

/// The language Osiris's words are shown in, as an index into [`Language::ALL`].
static CURRENT: AtomicU8 = AtomicU8::new(0);
/// The player's choice: an index into [`Language::ALL`], or `AUTO`.
static CHOICE: AtomicU8 = AtomicU8::new(AUTO);
const AUTO: u8 = u8::MAX;

type Table = HashMap<String, String>;

fn tables() -> &'static HashMap<Language, Table> {
    static T: OnceLock<HashMap<Language, Table>> = OnceLock::new();
    T.get_or_init(|| TABLES.iter().map(|&(l, src)| (l, parse_table(src).unwrap_or_else(|e| panic!("lang/{}.toml: {e}", l.code())))).collect())
}

fn parse_table(src: &str) -> Result<Table, toml::de::Error> {
    toml::from_str(src)
}

pub fn current() -> Language {
    Language::ALL[CURRENT.load(Ordering::Relaxed) as usize % Language::ALL.len()]
}

pub fn set_current(l: Language) {
    CURRENT.store(Language::ALL.iter().position(|&x| x == l).unwrap_or(0) as u8, Ordering::Relaxed);
}

/// `en` in the current language: Osiris's own words, written in English in the code.
pub fn tr(en: &str) -> &str {
    tr_in(current(), en)
}

pub fn tr_in(l: Language, en: &str) -> &str {
    if l == Language::English {
        return en;
    }
    tables().get(&l).and_then(|t| t.get(en)).map_or(en, String::as_str)
}

/// Marks `en` for translation where it is written (a table of names, say), to be
/// passed through [`tr`] where it is shown.
pub const fn n_(en: &str) -> &str {
    en
}

/// The simulation's reasons an order can't be carried out (placement rules, a
/// festival that can't be held, a route that can't open), which it gives in
/// English and Osiris shows through [`tr`]. The tests check this list against the
/// simulation's source.
#[allow(dead_code)] // Read by the tests.
pub const SIM_MESSAGES: [&str; 34] = [
    n_("A bridge must reach straight across to the far bank"),
    n_("A festival can't be held now"),
    n_("Bridges start at the water's edge"),
    n_("Build a temple complex first"),
    n_("Can't build there"),
    n_("Must be built at the water's edge, or on the floodplain's bank"),
    n_("Must be built near water"),
    n_("Must be built next to ore-bearing rock"),
    n_("Must be built next to rock"),
    n_("Must be built on meadow"),
    n_("Must be built on the floodplain"),
    n_("Must be built on the shore, facing the water"),
    n_("Needs the app"),
    n_("No room for the parade ground"),
    n_("No such building"),
    n_("No such city"),
    n_("Not available yet"),
    n_("Not available"),
    n_("Not enough beer"),
    n_("Not enough granite in storage"),
    n_("Not modeled in Osiris"),
    n_("Only one obelisk at a time"),
    n_("Only one tomb of this size may be cut at a time"),
    n_("Outside the map"),
    n_("People are in the way"),
    n_("Place it on your temple complex"),
    n_("The city does not know this god"),
    n_("This city does not trade with you"),
    n_("Unknown building"),
    n_("Unknown cheat"),
    n_("You can only have one sun temple under construction at a time"),
    n_("You do not have enough debens to open a trade route."),
    n_("You need 240 blocks of sandstone to build a mausoleum"),
    n_("You need 220 blocks of sandstone to build a sun temple"),
];

/// [`tr`] of a template, with `{0}`, `{1}`... filled in from `args` (a translation
/// may put them in another order).
pub fn trf(en: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut s = tr(en).to_owned();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}

/// The player's choice of language, `None` meaning "as the game data".
pub fn choice() -> Option<Language> {
    Language::ALL.get(CHOICE.load(Ordering::Relaxed) as usize).copied()
}

pub fn set_choice(l: Option<Language>) {
    CHOICE.store(l.and_then(|l| Language::ALL.iter().position(|&x| x == l)).map_or(AUTO, |i| i as u8), Ordering::Relaxed);
}

fn choice_path() -> std::path::PathBuf {
    crate::user_dir().join("language.txt")
}

/// Reads the saved choice: `auto`, or a language's code.
pub fn load_choice() {
    let s = std::fs::read_to_string(choice_path()).unwrap_or_default();
    set_choice(Language::from_code(&s));
}

pub fn save_choice() {
    let _ = std::fs::write(choice_path(), format!("{}\n", choice().map_or("auto", Language::code)));
}

/// The choice after `c` in the Options menu's cycle: automatic, then each language.
pub fn next_choice(c: Option<Language>) -> Option<Language> {
    match c {
        None => Some(Language::ALL[0]),
        Some(l) => Language::ALL.iter().position(|&x| x == l).and_then(|i| Language::ALL.get(i + 1)).copied(),
    }
}

/// The Options menu's label for the language, e.g. "Language - Deutsch" or, when
/// it follows the game's text, "Language - English (auto)".
pub fn choice_label() -> String {
    match choice() {
        Some(l) => format!("{} - {}", tr("Language"), l.native_name()),
        None => format!("{} - {} ({})", tr("Language"), current().native_name(), tr("auto")),
    }
}

/// The game's own words in the chosen language, as far as the game folder has them.
pub struct GameText {
    pub text: Arc<TextTable>,
    pub messages: Arc<MessageTable>,
    pub phrases: Arc<Phrases>,
    pub campaign: Campaign,
    /// The set the tables came from.
    pub set: TextSet,
}

/// Finds the string tables in `data` for the player's choice of language (see
/// [`language::choose`]), reads them with their `eventmsg.txt` and `campaign.txt`,
/// and sets Osiris's own words to the choice, or to the tables' language if the
/// choice is automatic.
pub fn load_game_text(data: &Path) -> Result<GameText> {
    let (g, words) = read_game_text(data, choice())?;
    set_current(words);
    log::info!("game text from {} ({:?}); Osiris's own words in {:?}", g.set.text.display(), g.set.language, words);
    Ok(g)
}

/// The game's text for a choice of language, and the language Osiris's words
/// should then be in.
fn read_game_text(data: &Path, choice: Option<Language>) -> Result<(GameText, Language)> {
    let sets = language::find_text_sets(data);
    let set = language::choose(&sets, choice).cloned().with_context(|| trf("Pharaoh's string tables (Pharaoh_Text.eng and Pharaoh_MM.eng) are not in {0}.", &[&data.display()]))?;
    let words = choice.or(set.language).unwrap_or(Language::English);
    let text = TextTable::parse(&std::fs::read(&set.text).with_context(|| set.text.display().to_string())?)?;
    let messages = MessageTable::parse(&std::fs::read(&set.messages).with_context(|| set.messages.display().to_string())?)?;
    let read = |name: &str| std::fs::read(set.companion(data, name)).map(|b| osiris_formats::text::decode_cp1252(&b)).unwrap_or_default();
    let phrases = Phrases::parse(&read("eventmsg.txt"));
    let campaign = Campaign::parse(&read("campaign.txt")).unwrap_or_default();
    Ok((GameText { text: Arc::new(text), messages: Arc::new(messages), phrases: Arc::new(phrases), campaign, set }, words))
}

/// The English string tables of `data`, for tests that read the original's words.
#[cfg(test)]
pub fn english_text(data: &Path) -> Arc<TextTable> {
    read_game_text(data, Some(Language::English)).expect("string tables").0.text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `tr("...")`/`trf("...")` literal in the source, unescaped.
    fn keys_in_source() -> std::collections::BTreeSet<String> {
        fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut keys = std::collections::BTreeSet::new();
        for f in files {
            // Comments may quote a call without being one.
            let src: String = std::fs::read_to_string(&f).unwrap().lines().filter(|l| !l.trim_start().starts_with("//")).map(|l| format!("{l}\n")).collect();
            for opener in ["tr(\"", "trf(\"", "n_(\""] {
                let mut rest = src.as_str();
                while let Some(at) = rest.find(opener) {
                    // Only the calls, not `ends_with_tr("` and the like.
                    let before = rest[..at].chars().next_back();
                    rest = &rest[at + opener.len()..];
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '"') {
                        continue;
                    }
                    let mut key = String::new();
                    let mut chars = rest.char_indices();
                    while let Some((i, c)) = chars.next() {
                        match c {
                            '"' => {
                                rest = &rest[i + 1..];
                                break;
                            }
                            '\\' => match chars.next().map(|(_, c)| c) {
                                Some('n') => key.push('\n'),
                                Some('t') => key.push('\t'),
                                Some(c) => key.push(c),
                                None => {}
                            },
                            c => key.push(c),
                        }
                    }
                    keys.insert(key);
                }
            }
        }
        keys
    }

    /// Every reason the simulation gives (`Outcome::Invalid("...")`, `Err("...")`
    /// outside its tests) is in [`SIM_MESSAGES`], so it gets translated.
    #[test]
    fn simulation_messages_are_listed() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../osiris-sim/src");
        let mut missing = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
            let src = std::fs::read_to_string(e.path()).unwrap();
            let src = src.split("#[cfg(test)]").next().unwrap();
            for opener in ["Invalid(\"", "Err(\""] {
                for (at, _) in src.match_indices(opener) {
                    let rest = &src[at + opener.len()..];
                    let msg = &rest[..rest.find('"').unwrap()];
                    // Lower-case ones are file errors, not reasons shown in play.
                    if msg.starts_with(char::is_uppercase) && !SIM_MESSAGES.contains(&msg) {
                        missing.push(format!("{}: {msg}", e.path().display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "not in SIM_MESSAGES:\n{}", missing.join("\n"));
    }

    fn placeholders(s: &str) -> Vec<String> {
        let mut v: Vec<String> = s.match_indices('{').filter_map(|(i, _)| s[i..].find('}').map(|j| s[i..i + j + 1].to_owned())).collect();
        v.extend(s.match_indices("@P").map(|_| "@P".to_owned()));
        v.extend(s.match_indices("@L").map(|_| "@L".to_owned()));
        v.extend(s.match_indices('\n').map(|_| "\\n".to_owned()));
        v.sort();
        v
    }

    #[test]
    fn every_language_has_every_string() {
        let keys = keys_in_source();
        assert!(keys.len() > 50, "found only {} keys", keys.len());
        let mut problems = Vec::new();
        for (l, t) in tables() {
            for k in &keys {
                match t.get(k) {
                    None => problems.push(format!("{}: missing {k:?}", l.code())),
                    Some(v) if placeholders(v) != placeholders(k) => problems.push(format!("{}: placeholders differ in {k:?}", l.code())),
                    Some(v) if v.trim().is_empty() => problems.push(format!("{}: empty {k:?}", l.code())),
                    _ => {}
                }
            }
            for k in t.keys().filter(|k| !keys.contains(*k)) {
                problems.push(format!("{}: unused {k:?}", l.code()));
            }
        }
        problems.sort();
        assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
    }

    #[test]
    fn translations_draw_in_the_games_font() {
        // Every character must be one the bitmap fonts have (Windows-1252, less the
        // few marks they lack, which Osiris swaps for straight quotes).
        for (l, t) in tables() {
            for v in t.values() {
                for c in v.chars() {
                    assert!(osiris_formats::text::cp1252_byte(c) != b'?' || c == '?', "{}: {c:?} in {v:?}", l.code());
                    assert!(!matches!(c, '„' | '“' | '”' | '‚' | '‘' | '’'), "{}: {c:?} draws as a letter in the game's font, in {v:?}", l.code());
                }
            }
        }
    }

    #[test]
    fn untranslated_strings_stay_english() {
        assert_eq!(tr_in(Language::German, "no such string"), "no such string");
        assert_eq!(tr_in(Language::English, "Language"), "Language");
    }

    #[test]
    fn reads_a_localized_copy() {
        // A made-up German copy: its tables, an eventmsg.txt and campaign.txt in
        // Windows-1252, in a folder inside an English install.
        let dir = std::env::temp_dir().join(format!("osiris-app-lang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("German")).unwrap();
        let mut en = TextTable::default();
        en.set_group(1, vec!["File".into(), "New game".into()]);
        en.set_group(5, vec!["The city needs more workers, and your people have no food to eat.".into(), "You have been promoted to the rank of Architect by the Pharaoh.".into(), "The Nile floods each year and waters the fields of your farms.".into()]);
        let mut de = TextTable::default();
        de.set_group(1, vec!["Datei".into(), "Neues Spiel".into()]);
        de.set_group(5, vec!["Die Stadt braucht mehr Arbeiter, und Eure Bürger haben nichts zu essen.".into(), "Der Pharao hat Euch zum Baumeister ernannt, und das ist eine große Ehre.".into(), "Der Nil tritt jedes Jahr über die Ufer und bewässert die Felder.".into()]);
        let mm = |title: &str| MessageTable::from_messages(vec![osiris_formats::Message { title: title.into(), ..Default::default() }]).to_bytes();
        std::fs::write(dir.join("Pharaoh_Text.eng"), en.to_bytes()).unwrap();
        std::fs::write(dir.join("Pharaoh_MM.eng"), mm("Welcome")).unwrap();
        std::fs::write(dir.join("eventmsg.txt"), "PHRASE_x \"Greetings\"\n").unwrap();
        std::fs::write(dir.join("German/Pharaoh_Text.eng"), de.to_bytes()).unwrap();
        std::fs::write(dir.join("German/Pharaoh_MM.eng"), mm("Willkommen in Ägypten")).unwrap();
        std::fs::write(dir.join("German/eventmsg.txt"), osiris_formats::text::encode_cp1252("PHRASE_x \"Grüße, Statthalter\"\n")).unwrap();
        std::fs::write(dir.join("German/campaign.txt"), osiris_formats::text::encode_cp1252("[MISSION_NAMES]\nNubt (Naqada)\nThinis\nMen-nefer (Memphis)\n")).unwrap();

        let (g, words) = read_game_text(&dir, None).unwrap();
        assert_eq!(g.text.get(1, 1), Some("New game"));
        assert_eq!(words, Language::English);
        assert_eq!(g.phrases.get("PHRASE_x"), Some("Greetings"));

        let (g, words) = read_game_text(&dir, Some(Language::German)).unwrap();
        assert_eq!(g.text.get(1, 1), Some("Neues Spiel"));
        assert_eq!(g.messages.get(0).unwrap().title, "Willkommen in Ägypten");
        assert_eq!(g.phrases.get("PHRASE_x"), Some("Grüße, Statthalter"));
        assert_eq!(g.campaign.mission_names.get(2).map(String::as_str), Some("Men-nefer (Memphis)"));
        assert_eq!(words, Language::German);
        assert_eq!(tr_in(words, "Language"), "Sprache");

        // A language the folder lacks keeps the installed text, Osiris's words switch.
        let (g, words) = read_game_text(&dir, Some(Language::Italian)).unwrap();
        assert_eq!(g.text.get(1, 0), Some("File"));
        assert_eq!(words, Language::Italian);

        // With only the German copy in the folder itself, automatic means German.
        let only = dir.join("German");
        let (g, words) = read_game_text(&only, None).unwrap();
        assert_eq!((g.text.get(1, 0), words), (Some("Datei"), Language::German));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
