//! Parsing and tier selection for the original `music.txt` city playlist.
//!
//! `music.txt` (shipped at the root of `PharaohData`) is one of the original game's data
//! files. Facts about its format and how the game uses it come from reading the Akhenaten
//! reimplementation (`src/sound/music.cpp`, read for facts only, no code copied):
//!
//! - Each non-comment, non-blank line is `<file> <M|A> <mission> <post-tune-delay-sec>
//!   <min-pop> <max-pop>`. `M` ("main") tracks are a mission's primary tunes; `A`
//!   ("alternate") tracks fill the gaps between them. A bare line that reads `AMBIENT` is a
//!   no-op marker (the original engine skips it; it does not name a file).
//! - `mission` ties a block of entries to one campaign scenario. Osiris has no campaign/mission
//!   system yet, so this crate does not filter by it: all rows across the whole file are
//!   pooled and selected purely by population range. This is a deliberate simplification of
//!   the original per-mission soundtrack sequencing, not a verified fact about the original
//!   engine's freeplay behavior.
//! - `min_pop`/`max_pop` bound the city population for which a row is eligible. The original
//!   engine keeps the current tune playing as long as it still fits the population bracket,
//!   and otherwise picks a new one at random from the eligible pool (excluding the track that
//!   just finished). [`pick_track`] reproduces that selection rule; the crossfade / precise
//!   post-tune delay timing is intentionally not reproduced (not required by spec).

use std::path::Path;

/// One parsed row of `music.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicRow {
    /// Filename as it appears in `music.txt` (e.g. `"Ra.mp3"`), resolved against
    /// `PharaohData/AUDIO/Music/` at playback time.
    pub file: String,
    /// `true` for an `M` (main tune) row, `false` for an `A` (alternate) row.
    pub main: bool,
    /// The mission/scenario number the row belongs to in the original file. Unused for
    /// selection (see module docs) but kept for debugging and future use.
    pub mission: i32,
    pub delay_sec: u32,
    pub min_pop: i32,
    pub max_pop: i32,
}

/// A request to play a specific music file under `AUDIO/Music/`.
///
/// Constructed from a bare filename, e.g. `Track::new("Ra.mp3")` or `Track::new("Ra")`
/// (the `.mp3` extension is optional and added if missing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track(pub(crate) String);

impl Track {
    pub fn new(name: impl Into<String>) -> Self {
        let mut name = name.into();
        if Path::new(&name).extension().is_none() {
            name.push_str(".mp3");
        }
        Track(name)
    }

    /// The filename this track resolves to under `AUDIO/Music/`.
    pub fn filename(&self) -> &str {
        &self.0
    }
}

/// Parses the text of `music.txt`. Malformed lines are skipped, matching the original
/// engine's tolerant parser.
pub fn parse_music_txt(text: &str) -> Vec<MusicRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.eq_ignore_ascii_case("AMBIENT") {
            continue;
        }

        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }

        let main = match fields[1].chars().next().map(|c| c.to_ascii_uppercase()) {
            Some('M') => true,
            Some('A') => false,
            _ => continue,
        };

        let (Ok(mission), Ok(delay_sec), Ok(min_pop), Ok(max_pop)) = (
            fields[2].parse::<i32>(),
            fields[3].parse::<u32>(),
            fields[4].parse::<i32>(),
            fields[5].parse::<i32>(),
        ) else {
            continue;
        };

        rows.push(MusicRow {
            file: fields[0].to_string(),
            main,
            mission,
            delay_sec,
            min_pop,
            max_pop,
        });
    }
    rows
}

/// Picks the next city track for `population`, matching the original's population-tier
/// selection: gather every row whose `[min_pop, max_pop]` contains `population`, then choose
/// randomly among them, excluding `exclude` (normally the track that is currently playing)
/// when there is a choice.
pub fn pick_track<'a>(
    rows: &'a [MusicRow],
    population: i32,
    exclude: Option<&str>,
) -> Option<&'a MusicRow> {
    let pool: Vec<&MusicRow> = rows
        .iter()
        .filter(|r| population >= r.min_pop && population <= r.max_pop)
        .collect();

    let candidates: Vec<&&MusicRow> = pool
        .iter()
        .filter(|r| exclude.is_none_or(|ex| !r.file.eq_ignore_ascii_case(ex)))
        .collect();

    if !candidates.is_empty() {
        let idx = rand::random_range(0..candidates.len());
        return Some(candidates[idx]);
    }

    // Every row in the tier was excluded (e.g. only one track fits): fall back to the full pool.
    if !pool.is_empty() {
        let idx = rand::random_range(0..pool.len());
        return Some(pool[idx]);
    }

    None
}

/// `true` if `file` (a bare filename) still falls within a population-eligible row, i.e. the
/// currently playing track still "fits" and should be left alone rather than interrupted.
pub fn track_fits(rows: &[MusicRow], file: &str, population: i32) -> bool {
    rows.iter().any(|r| {
        r.file.eq_ignore_ascii_case(file) && population >= r.min_pop && population <= r.max_pop
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rows_and_skips_ambient_markers() {
        let text = "; comment\nagbj.mp3 M 0 20 0 100000\nAMBIENT\nsstj.mp3 A 0 20 0 100000\n";
        let rows = parse_music_txt(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].file, "agbj.mp3");
        assert!(rows[0].main);
        assert_eq!(rows[1].file, "sstj.mp3");
        assert!(!rows[1].main);
    }

    #[test]
    fn picks_track_within_population_tier() {
        let rows = vec![
            MusicRow {
                file: "low.mp3".into(),
                main: true,
                mission: 0,
                delay_sec: 20,
                min_pop: 0,
                max_pop: 299,
            },
            MusicRow {
                file: "high.mp3".into(),
                main: true,
                mission: 0,
                delay_sec: 20,
                min_pop: 300,
                max_pop: 100000,
            },
        ];
        let picked = pick_track(&rows, 500, None).unwrap();
        assert_eq!(picked.file, "high.mp3");
    }

    #[test]
    fn track_extension_defaults_to_mp3() {
        assert_eq!(Track::new("Ra").filename(), "Ra.mp3");
        assert_eq!(Track::new("Ra.mp3").filename(), "Ra.mp3");
    }
}
