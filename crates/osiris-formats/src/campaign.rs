//! `campaign.txt`: the campaign structure linking missions, briefings and the
//! branching "choice" screens between them. Plain text, documented by its own
//! comments (lines starting with `;`).
//!
//! ```text
//! [MISSION_NAMES]                  -- one mission name per line, in mission-id order
//! Nubt (Naqada)
//! Thinis
//! ...
//!
//! [<campaign name>]                -- a named section of the campaign, in play order
//! mission=<mission>,<intro_MM>,<victory_text>[,<path_id>[,merge_path...]]
//! choicescreen=<graphic id>,<title text id>
//! choice=<path_id>,<x>,<y>,<text id>
//! ```
//!
//! `intro_MM` indexes `Pharaoh_MM.eng` ([`crate::messages`]); `victory_text` and the
//! choice screen/choice text ids index groups in `Pharaoh_Text.eng`
//! ([`crate::text`]) per the file's own comments (`TXT_VICTORY_SPEECH` *147,
//! `TXT_ASSIGNMENTS` *144). A `choicescreen`/`choice` block, when present, precedes
//! the mission it offers a path into.

use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Mission {
    pub id: u32,
    pub intro_mm: u32,
    pub victory_text: u32,
    pub path_id: u32,
    /// Extra path ids merged into this one (a mission with `path_id` 0 always
    /// plays and resets the current path to 0; `merge_paths` lists any further
    /// paths a mission also satisfies).
    pub merge_paths: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct ChoiceScreen {
    pub graphic_id: u32,
    pub title_text_id: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Choice {
    pub path_id: u32,
    pub x: i32,
    pub y: i32,
    pub text_id: u32,
}

/// One line of a campaign section, in file order.
#[derive(Debug, Clone)]
pub enum CampaignEntry {
    Mission(Mission),
    /// A `choicescreen=` line and the `choice=` lines that follow it.
    ChoiceScreen {
        screen: ChoiceScreen,
        choices: Vec<Choice>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct CampaignSection {
    pub name: String,
    pub entries: Vec<CampaignEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct Campaign {
    pub mission_names: Vec<String>,
    pub sections: Vec<CampaignSection>,
}

impl Campaign {
    pub fn parse(text: &str) -> Result<Self> {
        #[derive(PartialEq)]
        enum State {
            None,
            MissionNames,
            Section,
        }

        let mut state = State::None;
        let mut mission_names = Vec::new();
        let mut sections: Vec<CampaignSection> = Vec::new();

        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                if name == "MISSION_NAMES" {
                    state = State::MissionNames;
                } else {
                    state = State::Section;
                    sections.push(CampaignSection {
                        name: name.to_string(),
                        entries: Vec::new(),
                    });
                }
                continue;
            }
            match state {
                State::MissionNames => mission_names.push(line.to_string()),
                State::Section => {
                    let section = sections
                        .last_mut()
                        .expect("Section state implies a section was pushed");
                    if let Some(rest) = line.strip_prefix("mission=") {
                        section
                            .entries
                            .push(CampaignEntry::Mission(parse_mission(rest)?));
                    } else if let Some(rest) = line.strip_prefix("choicescreen=") {
                        section.entries.push(CampaignEntry::ChoiceScreen {
                            screen: parse_choicescreen(rest)?,
                            choices: Vec::new(),
                        });
                    } else if let Some(rest) = line.strip_prefix("choice=") {
                        let choice = parse_choice(rest)?;
                        match section.entries.last_mut() {
                            Some(CampaignEntry::ChoiceScreen { choices, .. }) => {
                                choices.push(choice);
                            }
                            _ => {
                                return Err(Error::Invalid(format!(
                                    "campaign.txt: choice= line with no preceding choicescreen=: {line:?}"
                                )));
                            }
                        }
                    }
                }
                State::None => {}
            }
        }
        Ok(Self {
            mission_names,
            sections,
        })
    }
}

fn field(context: &str, s: &str) -> Result<u32> {
    s.trim()
        .parse()
        .map_err(|_| Error::Invalid(format!("campaign.txt: bad number {s:?} in {context:?}")))
}

fn signed_field(context: &str, s: &str) -> Result<i32> {
    s.trim()
        .parse()
        .map_err(|_| Error::Invalid(format!("campaign.txt: bad number {s:?} in {context:?}")))
}

fn parse_mission(rest: &str) -> Result<Mission> {
    let parts: Vec<&str> = rest.split(',').collect();
    if parts.len() < 3 {
        return Err(Error::Invalid(format!(
            "campaign.txt: mission= needs at least 3 fields: {rest:?}"
        )));
    }
    Ok(Mission {
        id: field(rest, parts[0])?,
        intro_mm: field(rest, parts[1])?,
        victory_text: field(rest, parts[2])?,
        path_id: parts.get(3).map_or(Ok(0), |s| field(rest, s))?,
        merge_paths: parts[4.min(parts.len())..]
            .iter()
            .map(|s| field(rest, s))
            .collect::<Result<_>>()?,
    })
}

fn parse_choicescreen(rest: &str) -> Result<ChoiceScreen> {
    let parts: Vec<&str> = rest.split(',').collect();
    if parts.len() != 2 {
        return Err(Error::Invalid(format!(
            "campaign.txt: choicescreen= needs 2 fields: {rest:?}"
        )));
    }
    Ok(ChoiceScreen {
        graphic_id: field(rest, parts[0])?,
        title_text_id: field(rest, parts[1])?,
    })
}

fn parse_choice(rest: &str) -> Result<Choice> {
    let parts: Vec<&str> = rest.split(',').collect();
    if parts.len() != 4 {
        return Err(Error::Invalid(format!(
            "campaign.txt: choice= needs 4 fields: {rest:?}"
        )));
    }
    Ok(Choice {
        path_id: field(rest, parts[0])?,
        x: signed_field(rest, parts[1])?,
        y: signed_field(rest, parts[2])?,
        text_id: field(rest, parts[3])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; campaign definition file
[MISSION_NAMES]
Nubt (Naqada)
Thinis

[PREDYNASTIC_PERIOD]
mission=0,200,0,0
mission=1,201,1,0

[ARCHAIC_PERIOD]
mission=5,205,5,0
choicescreen= 0,19
choice= 1,436,328,20
choice= 2,363,245,21

mission=6,206,6,1
";

    #[test]
    fn parses_names_missions_and_choices() {
        let c = Campaign::parse(SAMPLE).unwrap();
        assert_eq!(c.mission_names, vec!["Nubt (Naqada)", "Thinis"]);
        assert_eq!(c.sections.len(), 2);

        let predynastic = &c.sections[0];
        assert_eq!(predynastic.name, "PREDYNASTIC_PERIOD");
        assert_eq!(predynastic.entries.len(), 2);
        let CampaignEntry::Mission(m) = &predynastic.entries[0] else {
            panic!("expected mission");
        };
        assert_eq!(
            (m.id, m.intro_mm, m.victory_text, m.path_id),
            (0, 200, 0, 0)
        );

        let archaic = &c.sections[1];
        assert_eq!(archaic.entries.len(), 3);
        let CampaignEntry::ChoiceScreen { screen, choices } = &archaic.entries[1] else {
            panic!("expected choice screen");
        };
        assert_eq!((screen.graphic_id, screen.title_text_id), (0, 19));
        assert_eq!(choices.len(), 2);
        assert_eq!(
            (
                choices[0].path_id,
                choices[0].x,
                choices[0].y,
                choices[0].text_id
            ),
            (1, 436, 328, 20)
        );
    }

    #[test]
    fn choice_without_screen_is_an_error() {
        assert!(Campaign::parse("[X]\nchoice=1,2,3,4\n").is_err());
    }
}
