//! Which language a copy of the game speaks, and where its language files are.
//!
//! The original exe has no language setting: the English Pharaoh.exe opens
//! `Pharaoh_Text.eng` and `Pharaoh_MM.eng` by fixed name, `EVENTMSG.TXT` and
//! `campaign.txt` beside them, and each localized edition shipped its own exe and
//! files. `Language.inf` holds only the title (`[Ident] Title=Pharaoh`) and
//! `Pharaoh.inf` is the saved options block. So Osiris finds the string tables by
//! looking: every `Pharaoh_Text.<ext>` with a `Pharaoh_MM.<ext>` beside it, in the
//! game folder or a folder inside it, and tells their language from the words in
//! them rather than from the extension.

use crate::TextTable;
use std::path::{Path, PathBuf};

/// A language Osiris has its own words for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    English,
    German,
    French,
    Spanish,
    Italian,
}

impl Language {
    pub const ALL: [Language; 5] = [Language::English, Language::German, Language::French, Language::Spanish, Language::Italian];

    /// The two-letter code, as stored in the player's settings.
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::German => "de",
            Language::French => "fr",
            Language::Spanish => "es",
            Language::Italian => "it",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The language's name in itself, as a language menu lists it.
    pub fn native_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::German => "Deutsch",
            Language::French => "Français",
            Language::Spanish => "Español",
            Language::Italian => "Italiano",
        }
    }

    /// The language a file extension suggests (`eng`, `ger`, `fra`, ...), used
    /// only when the text itself doesn't say.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "eng" | "en" | "uk" | "us" => Some(Language::English),
            "ger" | "deu" | "de" | "gem" => Some(Language::German),
            "fra" | "fre" | "fr" => Some(Language::French),
            "spa" | "esp" | "es" | "spn" => Some(Language::Spanish),
            "ita" | "it" => Some(Language::Italian),
            _ => None,
        }
    }

    /// Short words common in each language's running text and rare in the others'.
    fn markers(self) -> &'static [&'static str] {
        match self {
            Language::English => &["the", "and", "of", "your", "you", "is", "to", "with", "city", "have"],
            Language::German => &["der", "die", "das", "und", "ist", "nicht", "eure", "euer", "mit", "stadt", "ein", "eine", "sie", "zu"],
            Language::French => &["le", "les", "et", "des", "est", "vous", "votre", "vos", "une", "pas", "du", "au", "ville"],
            Language::Spanish => &["el", "los", "las", "y", "del", "su", "sus", "una", "ciudad", "para", "con", "por", "es"],
            Language::Italian => &["il", "gli", "della", "di", "è", "non", "che", "una", "città", "per", "con", "sono", "vostra"],
        }
    }
}

/// The language of `text` as told by its words, or `None` if it has too few words
/// to tell (an almost empty table).
pub fn detect(text: &TextTable) -> Option<Language> {
    let mut counts = [0usize; 5];
    let mut words = 0usize;
    for group in 0..1000 {
        for s in text.group(group) {
            detect_words(s, &mut counts, &mut words);
        }
        if words > 40_000 {
            break;
        }
    }
    decide(counts, words)
}

/// The language of free text (a phrase file, say).
pub fn detect_str(s: &str) -> Option<Language> {
    let mut counts = [0usize; 5];
    let mut words = 0usize;
    detect_words(s, &mut counts, &mut words);
    decide(counts, words)
}

fn detect_words(s: &str, counts: &mut [usize; 5], words: &mut usize) {
    for w in s.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()) {
        *words += 1;
        let w = w.to_lowercase();
        for (i, l) in Language::ALL.iter().enumerate() {
            if l.markers().contains(&w.as_str()) {
                counts[i] += 1;
            }
        }
    }
}

fn decide(counts: [usize; 5], words: usize) -> Option<Language> {
    let (best, &n) = counts.iter().enumerate().max_by_key(|&(_, n)| *n)?;
    (words >= 20 && n >= 3 && n * 25 >= words).then_some(Language::ALL[best])
}

/// One set of string tables found in the game folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSet {
    /// The folder they're in: the game folder or one inside it.
    pub dir: PathBuf,
    pub text: PathBuf,
    pub messages: PathBuf,
    /// Their extension as found (`eng`, say).
    pub extension: String,
    /// The language of their words, or else their extension's, if either says.
    pub language: Option<Language>,
}

impl TextSet {
    /// A companion file of these tables (`eventmsg.txt`, `campaign.txt`): the one
    /// in their own folder, else the game folder's `data` one. Names match without
    /// regard to case, as they do on Windows (the exe asks for `EVENTMSG.TXT`).
    pub fn companion(&self, data: &Path, name: &str) -> PathBuf {
        find_file(&self.dir, name).or_else(|| find_file(data, name)).unwrap_or_else(|| data.join(name))
    }

    /// Whether these are the game folder's own tables rather than a copy in a folder inside it.
    pub fn is_root(&self, data: &Path) -> bool {
        self.dir == data
    }
}

/// `dir`'s entry named `name`, matched without regard to case.
pub fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// Every `Pharaoh_Text.<ext>` with its `Pharaoh_MM.<ext>` in `data` and in the
/// folders directly inside it, the game folder's own first, each with its
/// language. Reads each text table to tell its language.
pub fn find_text_sets(data: &Path) -> Vec<TextSet> {
    let mut dirs = vec![data.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(data) {
        let mut subs: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        subs.sort();
        dirs.extend(subs);
    }
    let mut sets = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect();
        files.sort();
        for text in &files {
            let Some(name) = text.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            let Some((stem, ext)) = name.rsplit_once('.') else { continue };
            // `Pharaoh_Text.txt` is the English source the table was built from.
            if !stem.eq_ignore_ascii_case("Pharaoh_Text") || ext.eq_ignore_ascii_case("txt") {
                continue;
            }
            let Some(messages) = find_file(&dir, &format!("Pharaoh_MM.{ext}")) else { continue };
            let Some(table) = std::fs::read(text).ok().and_then(|b| TextTable::parse(&b).ok()) else { continue };
            let language = detect(&table).or_else(|| Language::from_extension(ext));
            sets.push(TextSet { dir: dir.clone(), text: text.clone(), messages, extension: ext.to_owned(), language });
        }
    }
    // The English exe's own names first among the game folder's.
    sets.sort_by_key(|s| (s.dir != data, !s.extension.eq_ignore_ascii_case("eng")));
    sets
}

/// The tables to play in: those in `want`'s language if any are there (the game
/// folder's own first), else the game folder's own, else the first found.
pub fn choose(sets: &[TextSet], want: Option<Language>) -> Option<&TextSet> {
    want.and_then(|l| sets.iter().find(|s| s.language == Some(l))).or_else(|| sets.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(strings: &[&str]) -> TextTable {
        let mut t = TextTable::default();
        t.set_group(5, strings.iter().map(|s| s.to_string()).collect());
        t
    }

    #[test]
    fn tells_languages_apart() {
        let en = table(&["The city needs more workers, and your people have no food.", "Build a granary to store the food of the city.", "You have been promoted to the rank of Architect by Pharaoh.", "The Nile floods each year and waters the fields of your farms."]);
        let de = table(&["Die Stadt braucht mehr Arbeiter, und Eure Bürger haben nichts zu essen.", "Baut einen Kornspeicher, um die Nahrung der Stadt zu lagern.", "Der Pharao hat Euch zum Baumeister ernannt, und das ist eine Ehre.", "Der Nil tritt jedes Jahr über die Ufer und bewässert die Felder."]);
        let fr = table(&["La ville a besoin de travailleurs, et votre peuple n'a pas de nourriture.", "Construisez un grenier pour stocker la nourriture de la ville.", "Vous avez été promu au rang d'architecte par Pharaon.", "Le Nil déborde chaque année et arrose les champs de vos fermes."]);
        let es = table(&["La ciudad necesita más trabajadores y su pueblo no tiene comida.", "Construya un granero para guardar la comida de la ciudad.", "El faraón le ha ascendido al rango de arquitecto por sus logros.", "El Nilo se desborda cada año y riega los campos de las granjas."]);
        let it = table(&["La città ha bisogno di lavoratori e la vostra gente non ha cibo.", "Costruite un granaio per conservare il cibo della città.", "Il faraone vi ha promosso al rango di architetto per i vostri meriti.", "Il Nilo straripa ogni anno e irriga i campi delle fattorie."]);
        assert_eq!(detect(&en), Some(Language::English));
        assert_eq!(detect(&de), Some(Language::German));
        assert_eq!(detect(&fr), Some(Language::French));
        assert_eq!(detect(&es), Some(Language::Spanish));
        assert_eq!(detect(&it), Some(Language::Italian));
        assert_eq!(detect(&table(&["Ok"])), None);
    }

    #[test]
    fn finds_sets_in_the_folder_and_below() {
        let dir = std::env::temp_dir().join(format!("osiris-lang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Deutsch")).unwrap();
        let en = table(&["The city needs more workers, and your people have no food.", "You have been promoted to the rank of Architect by the Pharaoh of the land.", "The Nile floods each year and waters the fields of your farms and the city."]);
        let de = table(&["Die Stadt braucht mehr Arbeiter, und Eure Bürger haben nichts zu essen.", "Der Pharao hat Euch zum Baumeister ernannt, und das ist eine große Ehre.", "Der Nil tritt jedes Jahr über die Ufer und bewässert die Felder der Stadt."]);
        let mm = crate::MessageTable::default().to_bytes();
        std::fs::write(dir.join("Pharaoh_Text.eng"), en.to_bytes()).unwrap();
        std::fs::write(dir.join("Pharaoh_MM.eng"), &mm).unwrap();
        std::fs::write(dir.join("Pharaoh_Text.txt"), "source").unwrap();
        std::fs::write(dir.join("eventmsg.txt"), "").unwrap();
        // The German edition's own names, whatever they were, are found by content.
        std::fs::write(dir.join("Deutsch/pharaoh_text.eng"), de.to_bytes()).unwrap();
        std::fs::write(dir.join("Deutsch/PHARAOH_MM.ENG"), &mm).unwrap();
        std::fs::write(dir.join("Deutsch/EVENTMSG.TXT"), "").unwrap();
        let sets = find_text_sets(&dir);
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].language, Some(Language::English));
        assert!(sets[0].is_root(&dir));
        assert_eq!(sets[1].language, Some(Language::German));
        assert_eq!(choose(&sets, Some(Language::German)).unwrap().dir, dir.join("Deutsch"));
        assert_eq!(choose(&sets, Some(Language::French)).unwrap().dir, dir);
        assert_eq!(choose(&sets, None).unwrap().dir, dir);
        // (The file system may or may not match names regardless of case.)
        assert_eq!(sets[1].companion(&dir, "eventmsg.txt").parent(), Some(dir.join("Deutsch").as_path()));
        assert_eq!(sets[1].companion(&dir, "campaign.txt"), dir.join("campaign.txt"));
        assert_eq!(sets[0].companion(&dir, "eventmsg.txt"), dir.join("eventmsg.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
