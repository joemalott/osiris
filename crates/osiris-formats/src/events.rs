//! Scenario events: the 150 records of the `scenario_events` chunk (requests, gifts,
//! price and demand changes, city status changes, floods, disasters, messages), and the
//! phrases of `eventmsg.txt` their messages are written from.
//!
//! Each record is 124 bytes. The first record's leading field is the number of records
//! in use; the rest are padding.

use crate::Result;
use crate::bytes::Reader;
use crate::chunks::ChunkFile;

pub const MAX_EVENTS: usize = 150;
const RECORD: usize = 124;

/// A value the scenario lets the game pick: `value` is what applies, the other three
/// say how to pick it (see `EventValue::roll` in the simulation).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventValue {
    pub value: i16,
    pub fixed: i16,
    pub min: i16,
    pub max: i16,
}

#[derive(Debug, Clone, Default)]
pub struct EventRecord {
    pub kind: u8,
    /// Month (0 = January) and years after the start the event happens; for events
    /// that only follow another, `year` holds the months to wait instead.
    pub month: i16,
    pub year: i16,
    /// The year's own pick: (fixed = month field, min, max) as stored.
    pub time: EventValue,
    pub item: EventValue,
    pub amount: EventValue,
    pub location: [i16; 4],
    pub on_completed: i16,
    pub on_refusal: i16,
    pub on_too_late: i16,
    pub on_defeat: i16,
    pub trigger: u8,
    pub tag: u16,
    /// Months the player has to fulfil a request.
    pub months: u8,
    pub months_left: u8,
    pub state: i8,
    pub overdue: bool,
    pub active: bool,
    /// 0 the city asks or gives, 1 Pharaoh.
    pub sender: i8,
    pub route: [i16; 4],
    pub subtype: i8,
    pub city: i8,
    /// Phrase ids of the reasons the message may give; 0xffff is none.
    pub reasons: [u16; 4],
}

fn value(r: &mut Reader) -> Result<EventValue> {
    Ok(EventValue { value: r.i16()?, fixed: r.i16()?, min: r.i16()?, max: r.i16()? })
}

fn parse_event(data: &[u8]) -> Result<(i16, EventRecord)> {
    let mut r = Reader::new(data, "scenario event");
    let count = r.i16()?;
    r.skip(4)?;
    let mut e = EventRecord { kind: r.u8()?, month: r.i8()? as i16, ..Default::default() };
    e.item = value(&mut r)?;
    e.amount = value(&mut r)?;
    e.year = r.i16()?;
    r.skip(2)?;
    let (min, max) = (r.i16()?, r.i16()?);
    e.time = EventValue { value: e.year, fixed: e.month, min, max };
    for l in &mut e.location {
        *l = r.i16()?;
    }
    e.on_completed = r.i16()?;
    e.on_refusal = r.i16()?;
    e.trigger = r.u8()?;
    r.skip(1)?;
    e.tag = r.u16()?;
    e.months = r.u8()?;
    r.skip(1)?;
    e.months_left = r.u8()?;
    r.skip(1)?;
    e.state = r.i8()?;
    e.overdue = r.u8()? != 0;
    e.active = r.u8()? != 0;
    r.skip(1 + 2 + 1 + 1 + 1 + 1 + 20 + 4)?;
    e.on_too_late = r.i16()?;
    e.on_defeat = r.i16()?;
    e.sender = r.i8()?;
    r.skip(1)?;
    for v in &mut e.route {
        *v = r.i16()?;
    }
    e.subtype = r.i8()?;
    e.city = r.i8()?;
    r.skip(2 + 6 + 4 + 2)?;
    for v in &mut e.reasons {
        *v = r.u16()?;
    }
    Ok((count, e))
}

/// The scenario's events, in file order (their index is how events refer to each other).
pub fn events_from_chunks(file: &ChunkFile) -> Result<Vec<EventRecord>> {
    let Some(data) = file.get("scenario_events") else { return Ok(Vec::new()) };
    let records = (data.len() / RECORD).min(MAX_EVENTS);
    if records == 0 {
        return Ok(Vec::new());
    }
    let (count, first) = parse_event(&data[..RECORD])?;
    let n = (count.max(1) as usize).min(records);
    let mut out = vec![first];
    for i in 1..n {
        out.push(parse_event(&data[i * RECORD..(i + 1) * RECORD])?.1);
    }
    Ok(out)
}

/// The phrases of `eventmsg.txt`, in file order: `PHRASE_name "text"` lines, with `;`
/// comments.
#[derive(Debug, Clone, Default)]
pub struct Phrases {
    pub list: Vec<(String, String)>,
}

impl Phrases {
    pub fn parse(text: &str) -> Self {
        let mut list = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with(';') || !line.starts_with("PHRASE_") {
                continue;
            }
            let name_end = line.find(|c: char| c.is_whitespace()).unwrap_or(line.len());
            let name = &line[..name_end];
            let rest = line[name_end..].trim();
            let body = rest.strip_prefix('"').map(|s| s.rsplit_once('"').map_or(s, |(b, _)| b)).unwrap_or(rest);
            list.push((name.to_owned(), body.to_owned()));
        }
        Self { list }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.list.iter().find(|p| p.0 == name).map(|p| p.1.as_str())
    }
}
