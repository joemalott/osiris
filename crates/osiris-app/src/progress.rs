//! Where the player is in the campaign. `campaign.txt` lists the missions in order,
//! with choice screens between some of them: picking a city sets the path, and the
//! campaign then plays the missions of that path (and those every path shares, path 0)
//! until the next choice.

use osiris_formats::Campaign;
use osiris_formats::campaign::{CampaignEntry, Choice, ChoiceScreen};
use osiris_sim::ratings::MissionResult;
use std::collections::BTreeMap;

/// What comes after the missions played so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    Mission(usize),
    /// A choice screen: its index in the campaign's entries.
    Choice(usize),
    /// The campaign is over.
    End,
}

#[derive(Debug, Clone)]
pub struct Progress {
    /// Missions won, in the order played.
    pub done: Vec<usize>,
    pub path: u32,
    pub next: Next,
    /// The best result of each mission won, which Explore History shows.
    pub results: BTreeMap<usize, MissionResult>,
}

/// A campaign entry, with the sections run together.
enum Entry<'a> {
    Mission { id: usize, path: u32, merge: &'a [u32] },
    Choice(&'a ChoiceScreen, &'a [Choice]),
}

fn entries(c: &Campaign) -> Vec<Entry<'_>> {
    c.sections
        .iter()
        .flat_map(|s| &s.entries)
        .map(|e| match e {
            CampaignEntry::Mission(m) => Entry::Mission { id: m.id as usize, path: m.path_id, merge: &m.merge_paths },
            CampaignEntry::ChoiceScreen { screen, choices } => Entry::Choice(screen, choices),
        })
        .collect()
}

fn on_path(path: u32, current: u32, merge: &[u32]) -> bool {
    path == 0 || path == current || merge.contains(&current)
}

impl Progress {
    /// The start of the campaign.
    pub fn new(c: &Campaign) -> Self {
        let mut p = Self { done: Vec::new(), path: 0, next: Next::End, results: BTreeMap::new() };
        p.next = p.scan(c, 0, true);
        p
    }

    /// The first mission or choice from entry `from` on for the current path. After a
    /// choice, the chosen path's first mission is wanted, so choice screens met on the
    /// way (those of other paths) are passed over.
    fn scan(&mut self, c: &Campaign, from: usize, choices: bool) -> Next {
        for (i, e) in entries(c).into_iter().enumerate().skip(from) {
            match e {
                Entry::Choice(..) if choices => return Next::Choice(i),
                Entry::Mission { id, path, merge } if on_path(path, self.path, merge) => {
                    if path == 0 {
                        self.path = 0;
                    }
                    return Next::Mission(id);
                }
                _ => {}
            }
        }
        Next::End
    }

    /// The entry of mission `m` on the current path (or any entry of it).
    fn entry_of(&self, c: &Campaign, m: usize) -> Option<usize> {
        let list = entries(c);
        let pos = |mine: bool| list.iter().position(|e| matches!(e, Entry::Mission { id, path, merge } if *id == m && (!mine || on_path(*path, self.path, merge))));
        pos(true).or_else(|| pos(false))
    }

    /// Mission `m` has been won. Only the next mission moves the campaign on; winning
    /// an earlier one again changes nothing.
    pub fn won(&mut self, c: &Campaign, m: usize) {
        if self.next != Next::Mission(m) {
            return;
        }
        self.done.push(m);
        let Some(i) = self.entry_of(c, m) else {
            self.next = Next::End;
            return;
        };
        self.next = self.scan(c, i + 1, true);
    }

    /// Keeps a won mission's result, as the original does, only when it scores higher
    /// than the one kept.
    pub fn record(&mut self, m: usize, r: MissionResult) {
        if self.results.get(&m).is_none_or(|old| r.score > old.score) {
            self.results.insert(m, r);
        }
    }

    /// The pending choice screen, with its choices.
    pub fn choice<'a>(&self, c: &'a Campaign) -> Option<(&'a ChoiceScreen, &'a [Choice])> {
        let Next::Choice(i) = self.next else { return None };
        match entries(c).into_iter().nth(i)? {
            Entry::Choice(s, ch) => Some((s, ch)),
            Entry::Mission { .. } => None,
        }
    }

    /// The player picked the city on `path`: its first mission is next.
    pub fn choose(&mut self, c: &Campaign, path: u32) {
        let Next::Choice(i) = self.next else { return };
        self.path = path;
        self.next = self.scan(c, i + 1, false);
    }

    /// The missions the player may start: those won, and the next.
    pub fn playable(&self) -> Vec<usize> {
        let mut v = self.done.clone();
        if let Next::Mission(m) = self.next
            && !v.contains(&m)
        {
            v.push(m);
        }
        v
    }

    /// Reads a saved position, or an older save that held only the number of missions
    /// unlocked in a straight line.
    pub fn parse(c: &Campaign, text: &str) -> Self {
        let mut p = Self::new(c);
        let text = text.trim();
        if let Ok(n) = text.parse::<usize>() {
            // Replay the old straight-line progress: every mission below `n` counts
            // as won, and at a choice the path leading to one of them is taken.
            loop {
                match p.next.clone() {
                    Next::Mission(m) if m < n => p.won(c, m),
                    Next::Choice(_) => {
                        let paths: Vec<u32> = p.choice(c).map(|(_, ch)| ch.iter().map(|x| x.path_id).collect()).unwrap_or_default();
                        let pick = paths.into_iter().find(|&path| {
                            let mut q = p.clone();
                            q.choose(c, path);
                            matches!(q.next, Next::Mission(m) if m < n)
                        });
                        match pick {
                            Some(path) => p.choose(c, path),
                            None => break,
                        }
                    }
                    _ => break,
                }
            }
            return p;
        }
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match (words.next(), words.next()) {
                (Some("path"), Some(v)) => p.path = v.parse().unwrap_or(0),
                (Some("done"), first) => p.done = first.into_iter().chain(words.by_ref()).filter_map(|w| w.parse().ok()).collect(),
                (Some("next"), Some(kind)) => {
                    let v = words.next().and_then(|w| w.parse().ok());
                    p.next = match (kind, v) {
                        ("mission", Some(v)) => Next::Mission(v),
                        ("choice", Some(v)) => Next::Choice(v),
                        _ => Next::End,
                    };
                }
                (Some("result"), Some(m)) => {
                    let v: Vec<i32> = words.by_ref().filter_map(|w| w.parse().ok()).collect();
                    if let (Ok(m), &[culture, prosperity, kingdom, population, funds, months, score, difficulty]) = (m.parse::<usize>(), v.as_slice()) {
                        let difficulty = difficulty.clamp(0, 4) as u8;
                        p.results.insert(m, MissionResult { culture, prosperity, kingdom, population, funds, months, score, difficulty });
                    }
                }
                _ => {}
            }
        }
        p
    }

    pub fn to_text(&self) -> String {
        let done: Vec<String> = self.done.iter().map(|d| d.to_string()).collect();
        let next = match self.next {
            Next::Mission(m) => format!("mission {m}"),
            Next::Choice(i) => format!("choice {i}"),
            Next::End => "end".into(),
        };
        let mut text = format!("path {}\ndone {}\nnext {next}\n", self.path, done.join(" "));
        for (m, r) in &self.results {
            text += &format!("result {m} {} {} {} {} {} {} {} {}\n", r.culture, r.prosperity, r.kingdom, r.population, r.funds, r.months, r.score, r.difficulty);
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAMPAIGN: &str = "[MISSION_NAMES]\nA\nB\nC\nD\nE\nF\n\n[ONE]\nmission=0,200,0,0\nmission=1,201,1,0\nchoicescreen= 0,19\nchoice= 1,10,10,20\nchoice= 2,20,20,21\nmission=2,202,2,1\nchoicescreen= 1,22\nchoice= 3,1,1,23\nchoice= 4,2,2,24\nmission=3,203,3,2\nchoicescreen= 1,22\nchoice= 3,1,1,23\nchoice= 4,2,2,24\n[TWO]\nmission=4,204,4,3\nmission=5,205,5,0\n";

    #[test]
    fn follows_the_chosen_path() {
        let c = Campaign::parse(CAMPAIGN).unwrap();
        let mut p = Progress::new(&c);
        assert_eq!(p.next, Next::Mission(0));
        p.won(&c, 0);
        p.won(&c, 1);
        assert!(matches!(p.next, Next::Choice(_)));
        p.choose(&c, 2);
        assert_eq!(p.next, Next::Mission(3));
        p.won(&c, 3);
        // The choice right after mission 3, not the one after mission 2.
        let Next::Choice(i) = p.next else { panic!("{:?}", p.next) };
        p.choose(&c, 3);
        assert!(i > 0);
        assert_eq!(p.next, Next::Mission(4));
        p.won(&c, 4);
        assert_eq!(p.next, Next::Mission(5));
        assert_eq!(p.path, 0);
        let back = Progress::parse(&c, &p.to_text());
        assert_eq!((back.done, back.path, back.next), (p.done.clone(), p.path, p.next.clone()));
        let r = MissionResult { culture: 40, prosperity: 20, kingdom: 55, population: 1200, funds: -300, months: 30, score: 5000, difficulty: 1 };
        p.record(1, r);
        p.record(1, MissionResult { score: 10, ..r });
        assert_eq!(Progress::parse(&c, &p.to_text()).results.get(&1), Some(&r));
        assert_eq!(p.playable(), vec![0, 1, 3, 4, 5]);
    }

    #[test]
    fn a_path_choice_skips_other_paths_choices() {
        let c = Campaign::parse(CAMPAIGN).unwrap();
        let mut p = Progress::new(&c);
        p.won(&c, 0);
        p.won(&c, 1);
        p.choose(&c, 1);
        assert_eq!(p.next, Next::Mission(2));
        p.won(&c, 2);
        assert!(matches!(p.next, Next::Choice(_)));
        p.choose(&c, 4);
        // Path 4 has no mission; the shared mission 5 follows.
        assert_eq!(p.next, Next::Mission(5));
    }
}
