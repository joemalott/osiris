//! Scenario events: the 150 records of the `scenario_events` chunk (requests, gifts,
//! price and demand changes, city status changes, floods, disasters, messages), and the
//! phrases of `eventmsg.txt` their messages are written from.
//!
//! Each record is 124 bytes. The first record's leading field is the number of records
//! in use; the rest are padding.
//!
//! The chunk is the original's event list as it lies in memory (0x785d70: the count as
//! an i32, then the records), so a record's bytes 0-3 are the last four of the record
//! before it and take no part here. The Mission Editor writes the fields it sets
//! (`EventRecord::to_bytes`) over the bytes the record was read from, so an event it
//! doesn't change is written back exactly as it was.

use crate::Result;
use crate::bytes::Reader;
use crate::chunks::ChunkFile;

pub const MAX_EVENTS: usize = 150;
pub const RECORD: usize = 124;

/// A value the scenario lets the game pick: `value` is what applies, the other three
/// say how to pick it (see `EventValue::roll` in the simulation).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventValue {
    pub value: i16,
    pub fixed: i16,
    pub min: i16,
    pub max: i16,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventRecord {
    pub kind: u8,
    /// Month (0 = January) and years after the start the event happens; for events
    /// that only follow another, `year` holds the months to wait instead.
    pub month: i16,
    pub year: i16,
    /// The year's own pick: (fixed = month field, min, max) as stored.
    pub time: EventValue,
    /// The year as the scenario's author fixed it (bytes 26-27): the pick's own
    /// `fixed` in the original, which rolls `year` from this, `time.min` and
    /// `time.max` when the city starts (FUN_00448a90). -1 when a range is set.
    pub year_fixed: i16,
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
    /// The god a festival request honours (0 Osiris .. 4 Bast).
    pub god: i8,
    /// An invasion's primary target.
    pub attack_target: i8,
    /// 0 the city asks or gives, 1 Pharaoh.
    pub sender: i8,
    pub route: [i16; 4],
    pub subtype: i8,
    pub city: i8,
    /// Byte 100, unnamed: the original follows a request's `on_defeat` only when it is
    /// 1 or 2.
    pub defeat_link: u8,
    /// Phrase ids of the reasons the message may give; 0xffff is none.
    pub reasons: [u16; 4],
    /// The reason each outcome's follow-up event gives (bytes 110-113: comply, refuse,
    /// too late, lose battle), 0-6 from text group 299 (Direct result .. Automatic).
    pub link_reasons: [u8; 4],
    /// The record's bytes as read (empty for a new event), which `to_bytes` writes
    /// the fields over.
    pub raw: Vec<u8>,
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
    e.year_fixed = r.i16()?;
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
    r.skip(1 + 2)?;
    e.god = r.i8()?;
    r.skip(1)?;
    e.attack_target = r.i8()?;
    r.skip(1 + 20 + 4)?;
    e.on_too_late = r.i16()?;
    e.on_defeat = r.i16()?;
    e.sender = r.i8()?;
    r.skip(1)?;
    for v in &mut e.route {
        *v = r.i16()?;
    }
    e.subtype = r.i8()?;
    e.city = r.i8()?;
    e.defeat_link = r.u8()?;
    r.skip(1 + 6)?;
    for v in &mut e.link_reasons {
        *v = r.u8()?;
    }
    r.skip(2)?;
    for v in &mut e.reasons {
        *v = r.u16()?;
    }
    e.raw = data[..RECORD].to_vec();
    Ok((count, e))
}

impl EventRecord {
    /// The record's 124 bytes: those it was read from, with every field that differs
    /// from what they hold written over them (a new record starts from zeros). Bytes
    /// 0-3 are left alone; `write_events` sets the count in the first record's.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = if self.raw.len() == RECORD { self.raw.clone() } else { vec![0; RECORD] };
        let old = parse_event(&out).map(|(_, e)| e).unwrap_or_default();
        let i16_at = |out: &mut Vec<u8>, at: usize, new: i16, was: i16| {
            if new != was {
                out[at..at + 2].copy_from_slice(&new.to_le_bytes());
            }
        };
        let pick = |out: &mut Vec<u8>, at: usize, new: EventValue, was: EventValue| {
            for (k, (n, w)) in [(new.value, was.value), (new.fixed, was.fixed), (new.min, was.min), (new.max, was.max)].into_iter().enumerate() {
                i16_at(out, at + 2 * k, n, w);
            }
        };
        let byte = |out: &mut Vec<u8>, at: usize, new: u8, was: u8| {
            if new != was {
                out[at] = new;
            }
        };
        byte(&mut out, 6, self.kind, old.kind);
        byte(&mut out, 7, self.month as u8, old.month as u8);
        pick(&mut out, 8, self.item, old.item);
        pick(&mut out, 16, self.amount, old.amount);
        pick(&mut out, 24, EventValue { value: self.year, fixed: self.year_fixed, min: self.time.min, max: self.time.max }, EventValue { value: old.year, fixed: old.year_fixed, min: old.time.min, max: old.time.max });
        for k in 0..4 {
            i16_at(&mut out, 32 + 2 * k, self.location[k], old.location[k]);
            i16_at(&mut out, 92 + 2 * k, self.route[k], old.route[k]);
        }
        i16_at(&mut out, 40, self.on_completed, old.on_completed);
        i16_at(&mut out, 42, self.on_refusal, old.on_refusal);
        // The editor keeps the trigger as a 32-bit set of flags.
        if self.trigger != old.trigger {
            out[44..48].copy_from_slice(&(self.trigger as u32).to_le_bytes());
        }
        if self.months != old.months {
            out[48..50].copy_from_slice(&(self.months as u16).to_le_bytes());
        }
        byte(&mut out, 50, self.months_left, old.months_left);
        byte(&mut out, 52, self.state as u8, old.state as u8);
        byte(&mut out, 53, self.overdue as u8, old.overdue as u8);
        byte(&mut out, 54, self.active as u8, old.active as u8);
        byte(&mut out, 58, self.god as u8, old.god as u8);
        // The editor steps the target as a 16-bit value.
        if self.attack_target != old.attack_target {
            out[60..62].copy_from_slice(&(self.attack_target as i16).to_le_bytes());
        }
        i16_at(&mut out, 86, self.on_too_late, old.on_too_late);
        i16_at(&mut out, 88, self.on_defeat, old.on_defeat);
        byte(&mut out, 90, self.sender as u8, old.sender as u8);
        byte(&mut out, 100, self.subtype as u8, old.subtype as u8);
        byte(&mut out, 101, self.city as u8, old.city as u8);
        byte(&mut out, 102, self.defeat_link, old.defeat_link);
        for k in 0..4 {
            byte(&mut out, 110 + k, self.link_reasons[k], old.link_reasons[k]);
            if self.reasons[k] != old.reasons[k] {
                out[116 + 2 * k..118 + 2 * k].copy_from_slice(&self.reasons[k].to_le_bytes());
            }
        }
        out
    }
}

/// Writes `events` into `file`'s `scenario_events` chunk: the count in the first
/// record, each event over the record it holds, and the slots past the last event as
/// the file held them (the original leaves them as memory held them, too).
pub fn write_events(file: &mut ChunkFile, events: &[EventRecord]) -> Result<()> {
    let Some(data) = file.get("scenario_events") else { return Ok(()) };
    let mut data = data.to_vec();
    let slots = (data.len() / RECORD).min(MAX_EVENTS);
    if events.len() > slots {
        return Err(crate::Error::Invalid(format!("{} events, room for {slots}", events.len())));
    }
    for (i, e) in events.iter().enumerate() {
        let mut bytes = e.to_bytes();
        // Bytes 4-5 number the event by its place, as the original renumbers them
        // when one is deleted (FUN_00448e30).
        bytes[4..6].copy_from_slice(&(i as i16).to_le_bytes());
        data[i * RECORD + 4..(i + 1) * RECORD].copy_from_slice(&bytes[4..]);
    }
    // The count is the list's own i32 at the chunk's start.
    data[0..4].copy_from_slice(&(events.len() as i32).to_le_bytes());
    file.set("scenario_events", data)
}

/// The scenario's events, in file order (their index is how events refer to each other).
pub fn events_from_chunks(file: &ChunkFile) -> Result<Vec<EventRecord>> {
    let Some(data) = file.get("scenario_events") else { return Ok(Vec::new()) };
    let records = (data.len() / RECORD).min(MAX_EVENTS);
    if records == 0 {
        return Ok(Vec::new());
    }
    let (count, first) = parse_event(&data[..RECORD])?;
    // The original runs through `count` events, none when it is 0 (maps such as
    // Default.map keep a stale record behind a count of 0).
    if count <= 0 {
        return Ok(Vec::new());
    }
    let n = (count as usize).min(records);
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
