//! Recording a game and playing it back, for regression tests, bug reports and
//! crash dumps (an idea from OpenRCT2). The simulation is deterministic and the
//! player changes it only through `Command`s, so a game is its starting save plus
//! the commands and the ticks they came at. A hash of the city at the end of every
//! month goes along, so a replay that no longer matches says in which month it
//! first went its own way.
//!
//! A replay file is:
//!
//! - `OSIRISRP`, then the format version as a little-endian u32 (1);
//! - the starting save (`World::save`): its length as a u32, the length packed as
//!   a u32, then the save packed with PKWare DCL (`osiris_formats::pkware`);
//! - the rest, MessagePack (named fields) of `Body`: the Osiris version that
//!   recorded it, the commands with their ticks, the monthly checkpoints, the tick
//!   it ends at and the city's hash there.

use crate::world::{Command, World};
use crate::{Balance, Defs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MAGIC: &[u8; 8] = b"OSIRISRP";
const VERSION: u32 = 1;

/// Something the player did at a tick.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Entry {
    Command(Command),
    /// The Undo button: the city goes back to before the last clear, road or build
    /// that changed anything (see `World::restore_snapshot`).
    Undo,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Event {
    /// `time.total_ticks` when it was done, before that tick ran.
    pub tick: u64,
    pub entry: Entry,
}

/// The city's hash as a month ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    pub tick: u64,
    pub year: i32,
    /// The month just begun (0-11).
    pub month: u32,
    pub hash: u64,
}

/// A recorded game.
#[derive(Debug, Clone, PartialEq)]
pub struct Replay {
    /// The Osiris version that recorded it.
    pub osiris: String,
    /// The city as the recording began (`World::save`).
    pub start: Vec<u8>,
    /// Every building got all the workers it wanted (a scripted test's setting,
    /// which saves don't keep).
    pub full_staff: bool,
    pub events: Vec<Event>,
    pub checkpoints: Vec<Checkpoint>,
    /// The tick the recording stops at.
    pub end_tick: u64,
    /// The city's hash there, when it was known (a crash dump has none).
    pub end_hash: Option<u64>,
    /// Written when the game crashed: the crash came in the tick up to `end_tick`
    /// or in the last command.
    pub crashed: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Body {
    osiris: String,
    full_staff: bool,
    events: Vec<Event>,
    checkpoints: Vec<Checkpoint>,
    end_tick: u64,
    end_hash: Option<u64>,
    #[serde(default)]
    crashed: bool,
}

impl Replay {
    pub fn to_bytes(&self) -> Vec<u8> {
        let packed = osiris_formats::pkware::implode(&self.start);
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(self.start.len() as u32).to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&packed);
        let body = Body {
            osiris: self.osiris.clone(),
            full_staff: self.full_staff,
            events: self.events.clone(),
            checkpoints: self.checkpoints.clone(),
            end_tick: self.end_tick,
            end_hash: self.end_hash,
            crashed: self.crashed,
        };
        out.extend_from_slice(&rmp_serde::to_vec_named(&body).expect("replay body serialises"));
        out
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).ok_or("replay file is cut short");
        if data.len() < 20 || &data[..8] != MAGIC {
            return Err("not an Osiris replay".into());
        }
        let version = word(8)? as u32;
        if version != VERSION {
            return Err(format!("replay version {version} is not supported"));
        }
        let (len, packed) = (word(12)?, word(16)?);
        let packed_end = 20 + packed;
        let start = osiris_formats::pkware::explode(data.get(20..packed_end).ok_or("replay file is cut short")?, len).map_err(|e| e.to_string())?;
        let body: Body = rmp_serde::from_slice(&data[packed_end..]).map_err(|e| e.to_string())?;
        Ok(Self {
            osiris: body.osiris,
            start,
            full_staff: body.full_staff,
            events: body.events,
            checkpoints: body.checkpoints,
            end_tick: body.end_tick,
            end_hash: body.end_hash,
            crashed: body.crashed,
        })
    }

    pub fn read(path: &std::path::Path) -> Result<Self, String> {
        Self::from_bytes(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
    }

    pub fn write(&self, path: &std::path::Path) -> Result<(), String> {
        std::fs::write(path, self.to_bytes()).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// What the recording holds so far.
struct Log {
    start: Vec<u8>,
    full_staff: bool,
    events: Vec<Event>,
    checkpoints: Vec<Checkpoint>,
}

struct Shared {
    log: Mutex<Log>,
    /// The last tick run, for a crash dump taken from outside the world.
    tick: AtomicU64,
}

/// The recording of a game in progress, kept by the world (`World::journal`).
/// Commands are pushed as they are applied and a checkpoint once a month, so
/// recording costs nothing per tick but a comparison and a store.
///
/// A clone shares the recording: the one clone the simulation makes, the trial
/// city of a placement ghost, never applies commands.
#[derive(Clone)]
pub struct Journal {
    shared: Arc<Shared>,
    /// The planner settings last recorded; `None` until the first build.
    planner: Option<[u8; 4]>,
    /// The month the last checkpoint was taken in.
    month: (i32, u32),
}

/// A handle on a recording that outlives the world's borrow, for the crash
/// handler: it can take what was recorded without the world.
#[derive(Clone)]
pub struct Recording(Arc<Shared>);

impl Recording {
    /// The recording so far, up to the last tick run. There is no final hash (that
    /// needs the world); the monthly checkpoints are all there. `None` if the
    /// recording is busy, as it is when a crash came while a command was being
    /// written down.
    pub fn so_far(&self) -> Option<Replay> {
        let log = self.0.log.try_lock().ok()?;
        Some(Replay {
            osiris: env!("CARGO_PKG_VERSION").to_owned(),
            start: log.start.clone(),
            full_staff: log.full_staff,
            events: log.events.clone(),
            checkpoints: log.checkpoints.clone(),
            end_tick: self.0.tick.load(Ordering::Relaxed),
            end_hash: None,
            crashed: false,
        })
    }

    /// The recording as a crash dump: it runs on through the tick after the last
    /// one finished, where a crash in the simulation came.
    pub fn crash_dump(&self) -> Option<Replay> {
        self.so_far().map(|r| Replay { end_tick: r.end_tick + 1, crashed: true, ..r })
    }
}

impl Journal {
    fn push(&mut self, tick: u64, entry: Entry) {
        self.shared.log.lock().unwrap_or_else(|e| e.into_inner()).events.push(Event { tick, entry });
    }

    fn checkpoints(&self) -> usize {
        self.shared.log.lock().unwrap_or_else(|e| e.into_inner()).checkpoints.len()
    }

    fn checkpoint(&self, i: usize) -> Option<Checkpoint> {
        self.shared.log.lock().unwrap_or_else(|e| e.into_inner()).checkpoints.get(i).copied()
    }
}

/// FNV-1a over everything written to it.
struct Fnv(u64);

impl std::io::Write for Fnv {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &b in buf {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl World {
    /// Starts recording the game from here: the city as it is now, then every
    /// command applied and a hash at the end of each month.
    pub fn start_recording(&mut self) -> Result<(), String> {
        let start = self.save()?;
        let log = Log { start, full_staff: self.test_full_staff, events: Vec::new(), checkpoints: Vec::new() };
        self.journal = Some(Journal {
            shared: Arc::new(Shared { log: Mutex::new(log), tick: AtomicU64::new(self.time.total_ticks) }),
            planner: None,
            month: (self.time.year, self.time.month),
        });
        Ok(())
    }

    /// A handle on the recording under way, for a crash dump.
    pub fn recording(&self) -> Option<Recording> {
        self.journal.as_ref().map(|j| Recording(j.shared.clone()))
    }

    /// The recording so far, ending now, with the city's hash.
    pub fn replay(&mut self) -> Option<Replay> {
        let end_hash = self.state_hash();
        let j = self.journal.as_ref()?;
        let log = j.shared.log.lock().unwrap_or_else(|e| e.into_inner());
        Some(Replay {
            osiris: env!("CARGO_PKG_VERSION").to_owned(),
            start: log.start.clone(),
            full_staff: log.full_staff,
            events: log.events.clone(),
            checkpoints: log.checkpoints.clone(),
            end_tick: self.time.total_ticks,
            end_hash: Some(end_hash),
            crashed: false,
        })
    }

    /// Writes down a command about to be applied; before a build, the planner
    /// settings it goes by, when they changed.
    pub(crate) fn record(&mut self, cmd: &Command) {
        let planner = [self.statue_variant, self.statue_facing, self.gatehouse_facing, self.complex_facing];
        let tick = self.time.total_ticks;
        let Some(j) = self.journal.as_mut() else { return };
        match *cmd {
            Command::Build { .. } if j.planner != Some(planner) => {
                let [statue_variant, statue_facing, gatehouse_facing, complex_facing] = planner;
                j.push(tick, Entry::Command(Command::Planner { statue_variant, statue_facing, gatehouse_facing, complex_facing }));
                j.planner = Some(planner);
            }
            Command::Planner { statue_variant, statue_facing, gatehouse_facing, complex_facing } => j.planner = Some([statue_variant, statue_facing, gatehouse_facing, complex_facing]),
            _ => {}
        }
        j.push(tick, Entry::Command(cmd.clone()));
    }

    /// After each tick while recording: note the tick, and hash the city when a
    /// new month has begun.
    pub(crate) fn journal_tick(&mut self) {
        let now = (self.time.year, self.time.month);
        let Some(j) = self.journal.as_mut() else { return };
        j.shared.tick.store(self.time.total_ticks, Ordering::Relaxed);
        if j.month == now {
            return;
        }
        j.month = now;
        let hash = self.state_hash();
        let cp = Checkpoint { tick: self.time.total_ticks, year: now.0, month: now.1, hash };
        if let Some(j) = &self.journal {
            j.shared.log.lock().unwrap_or_else(|e| e.into_inner()).checkpoints.push(cp);
        }
    }

    /// A hash of the simulation: the saved game but for what the screen takes
    /// away as it shows it (queued messages and warnings, the message log's read
    /// marks). Streamed, so it allocates nothing.
    pub fn state_hash(&mut self) -> u64 {
        let messages = std::mem::take(&mut self.messages);
        let texts = std::mem::take(&mut self.message_texts);
        let warnings = std::mem::take(&mut self.warnings);
        let notices = std::mem::take(&mut self.notices);
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        rmp_serde::encode::write_named(&mut h, self).expect("world serialises");
        self.messages = messages;
        self.message_texts = texts;
        self.warnings = warnings;
        self.notices = notices;
        h.0
    }

    /// Undo: puts back the city saved in `data` (by the game, before the last
    /// build), keeping the rules, the difficulty tables and the recording, which
    /// notes the undo.
    pub fn restore_snapshot(&mut self, data: &[u8]) -> Result<(), String> {
        let mut w = World::load(data, self.defs.clone(), self.balance.clone())?;
        w.rules = self.rules.clone();
        if let Some(b) = self.balances.clone() {
            w.attach_balances(b);
        }
        w.test_full_staff = self.test_full_staff;
        w.journal = self.journal.take();
        if let Some(j) = &mut w.journal {
            j.push(self.time.total_ticks, Entry::Undo);
            j.planner = None;
            j.month = (w.time.year, w.time.month);
        }
        *self = w;
        Ok(())
    }
}

/// Where a replay first stopped matching its recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// The tick it was found at: for a city that differs, the end of the month.
    pub tick: u64,
    /// The month it went wrong in (0-11) and its year.
    pub year: i32,
    pub month: u32,
    pub what: String,
}

impl std::fmt::Display for Divergence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        let year = if self.year < 0 { format!("{} BC", -self.year) } else { format!("{} AD", self.year) };
        write!(f, "diverged in {} {year}, found at tick {}: {}", MONTHS[self.month as usize % 12], self.tick, self.what)
    }
}

/// Plays a replay back into a world, checking it against the recording.
pub struct Player {
    replay: Replay,
    next: usize,
    /// The city before the last clear, road or build that changed it, for an
    /// undo; kept only when the replay has one.
    undo: Option<Vec<u8>>,
    has_undo: bool,
    month: (i32, u32),
    checked: usize,
    pub divergence: Option<Divergence>,
    pub done: bool,
}

impl Player {
    /// The replay's starting city (recording itself, for the checkpoints) and a
    /// player to run it. `balances` are the five difficulties' tables, as the game
    /// loads them (`Balance::load_all`).
    pub fn start(replay: Replay, defs: Arc<Defs>, balances: Arc<[Arc<Balance>; 5]>) -> Result<(World, Player), String> {
        let mut world = World::load(&replay.start, defs, balances[crate::difficulty::NORMAL as usize].clone())?;
        world.attach_balances(balances);
        world.test_full_staff = replay.full_staff;
        world.journal = Some(Journal {
            shared: Arc::new(Shared {
                log: Mutex::new(Log { start: Vec::new(), full_staff: replay.full_staff, events: Vec::new(), checkpoints: Vec::new() }),
                tick: AtomicU64::new(world.time.total_ticks),
            }),
            planner: None,
            month: (world.time.year, world.time.month),
        });
        let has_undo = replay.events.iter().any(|e| e.entry == Entry::Undo);
        let month = (world.time.year, world.time.month);
        Ok((world, Player { replay, next: 0, undo: None, has_undo, month, checked: 0, divergence: None, done: false }))
    }

    pub fn replay(&self) -> &Replay {
        &self.replay
    }

    fn diverge(&mut self, world: &World, what: String) {
        self.divergence = Some(Divergence { tick: world.time.total_ticks, year: world.time.year, month: world.time.month, what });
    }

    /// Applies the events due now.
    fn apply_due(&mut self, world: &mut World) {
        while self.divergence.is_none() && let Some(e) = self.replay.events.get(self.next).cloned() {
            let now = world.time.total_ticks;
            if e.tick > now {
                return;
            }
            if e.tick < now {
                let what = format!("a command recorded at tick {} came after the city reached tick {now}", e.tick);
                self.diverge(world, what);
                return;
            }
            self.next += 1;
            match e.entry {
                Entry::Command(cmd) => {
                    let map = matches!(cmd, Command::Clear { .. } | Command::Road { .. } | Command::Build { .. });
                    let before = if self.has_undo && map { world.save().ok() } else { None };
                    let out = world.apply(&cmd);
                    if map && matches!(out, crate::world::Outcome::Done { items: 1.., .. }) && before.is_some() {
                        self.undo = before;
                    }
                }
                Entry::Undo => match self.undo.take().map(|s| world.restore_snapshot(&s)) {
                    Some(Ok(())) => self.month = (world.time.year, world.time.month),
                    Some(Err(e)) => self.diverge(world, format!("undo failed: {e}")),
                    None => self.diverge(world, "an undo came with nothing to undo".into()),
                },
            }
        }
    }

    /// Compares the checkpoints taken since the last look with the recording's.
    fn check(&mut self, world: &World) {
        let Some(j) = &world.journal else { return };
        let n = j.checkpoints();
        while self.checked < n && self.divergence.is_none() {
            let got = j.checkpoint(self.checked).expect("taken");
            match self.replay.checkpoints.get(self.checked) {
                Some(want) if *want == got => {}
                Some(want) if (want.tick, want.year, want.month) == (got.tick, got.year, got.month) => {
                    let prev = self.checked.checked_sub(1).and_then(|i| self.replay.checkpoints.get(i)).map_or("the start".to_owned(), |p| format!("tick {}", p.tick));
                    // The month that just ended.
                    let (year, month) = if got.month == 0 { (got.year - 1, 11) } else { (got.year, got.month - 1) };
                    self.divergence = Some(Divergence { tick: got.tick, year, month, what: format!("the city differs at the end of the month (it matched at {prev})") });
                }
                Some(want) => self.divergence = Some(Divergence { tick: got.tick, year: got.year, month: got.month, what: format!("month ended at tick {}, recorded at tick {}", got.tick, want.tick) }),
                None => self.divergence = Some(Divergence { tick: got.tick, year: got.year, month: got.month, what: "a month ended the recording doesn't have".into() }),
            }
            self.checked += 1;
        }
    }

    /// Runs the replay on for up to `max_ticks` ticks (applying the commands due
    /// on the way), stopping at a divergence or the end. True while there is more.
    pub fn advance(&mut self, world: &mut World, max_ticks: u64) -> bool {
        let mut ran = 0;
        loop {
            self.apply_due(world);
            if self.divergence.is_some() || self.done {
                return false;
            }
            let events_left = self.next < self.replay.events.len();
            if !events_left && world.time.total_ticks >= self.replay.end_tick {
                self.finish(world);
                return false;
            }
            if ran >= max_ticks {
                return true;
            }
            world.tick();
            ran += 1;
            if (world.time.year, world.time.month) != self.month {
                self.month = (world.time.year, world.time.month);
                self.check(world);
            }
        }
    }

    fn finish(&mut self, world: &mut World) {
        self.done = true;
        self.check(world);
        if self.divergence.is_some() {
            return;
        }
        if self.checked < self.replay.checkpoints.len() {
            let what = format!("the recording has {} months, the replay {}", self.replay.checkpoints.len(), self.checked);
            self.diverge(world, what);
            return;
        }
        if let Some(want) = self.replay.end_hash
            && world.state_hash() != want
        {
            let what = "the city differs at the end of the recording".to_owned();
            self.diverge(world, what);
        }
    }

    /// Runs the whole replay. `Ok` with the number of commands and months checked
    /// when it matches, the divergence otherwise.
    pub fn run(&mut self, world: &mut World) -> Result<(usize, usize), Divergence> {
        while self.advance(world, u64::MAX) {}
        match self.divergence.clone() {
            Some(d) => Err(d),
            None => Ok((self.next, self.checked)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Outcome;

    /// Mission 12's city recorded from its start, in the game's own terms (no test
    /// settings), with the five difficulties' tables.
    fn recorded_world() -> Option<(World, Arc<[Arc<Balance>; 5]>)> {
        let data = std::env::var_os("OSIRIS_TEST_DATA").map_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData"), std::path::PathBuf::from);
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("open mission1.pak").scenario(12).expect("scenario");
        let defs = Arc::new(Defs::load(&library).expect("load defs"));
        let balances = Balance::load_all(&data).expect("balances");
        let mut world = World::new(&scenario, defs, balances[2].clone());
        world.attach_balances(balances.clone());
        world.begin_at(2);
        world.start(&scenario);
        world.load_mission(12);
        world.start_recording().unwrap();
        Some((world, balances))
    }

    /// A small town, some settings changed and an undo (the way the game does it:
    /// back to before the last clear, road or build that changed anything), played
    /// over a few months.
    fn play(w: &mut World) {
        let cmds = [
            Command::Road { start: (76, 122), end: (96, 122) },
            Command::Build { kind: crate::buildings::kind::VACANT_LOT, x: 80, y: 123, x1: 85, y1: 124 },
            Command::TaxRate(9),
            Command::Wages(40),
            Command::Road { start: (86, 112), end: (86, 121) },
        ];
        let mut snapshot = None;
        for cmd in &cmds {
            for _ in 0..300 {
                w.tick();
            }
            let before = w.save().unwrap();
            let map = matches!(cmd, Command::Road { .. } | Command::Build { .. });
            let out = w.apply(cmd);
            if matches!(out, Outcome::Done { items: 1.., .. }) && map {
                snapshot = Some(before);
            }
        }
        for _ in 0..40 {
            w.tick();
        }
        w.restore_snapshot(&snapshot.expect("the last road was laid")).unwrap();
        for _ in 0..3000 {
            w.tick();
        }
    }

    #[test]
    fn a_recorded_game_replays_identically() {
        let Some((mut w, balances)) = recorded_world() else { return };
        play(&mut w);
        let replay = Replay::from_bytes(&w.replay().unwrap().to_bytes()).unwrap();
        assert!(replay.checkpoints.len() >= 2, "{} months", replay.checkpoints.len());
        assert!(replay.events.contains(&Event { tick: 1540, entry: Entry::Undo }), "{:?}", replay.events);
        let (mut r, mut player) = Player::start(replay, w.defs.clone(), balances).unwrap();
        assert_eq!(player.run(&mut r), Ok((player.replay().events.len(), player.replay().checkpoints.len())));
        assert_eq!(r.state_hash(), w.state_hash());
    }

    #[test]
    fn a_changed_command_is_caught_in_its_month() {
        let Some((mut w, balances)) = recorded_world() else { return };
        play(&mut w);
        let mut replay = w.replay().unwrap();
        // The tax rate set at tick 900, month 1: the city differs from the end of that month.
        let e = replay.events.iter_mut().find(|e| matches!(e.entry, Entry::Command(Command::TaxRate(_)))).unwrap();
        assert_eq!(e.tick, 900);
        e.entry = Entry::Command(Command::TaxRate(12));
        let (mut r, mut player) = Player::start(replay, w.defs.clone(), balances).unwrap();
        let d = player.run(&mut r).unwrap_err();
        assert_eq!((d.tick, d.month), (2 * 816, 1), "{d}");
    }

    /// A crash dump, taken from outside the world, replays through the tick after
    /// the last one run.
    #[test]
    fn a_crash_dump_replays_to_the_crash() {
        let Some((mut w, balances)) = recorded_world() else { return };
        let handle = w.recording().unwrap();
        play(&mut w);
        let dump = Replay::from_bytes(&handle.crash_dump().unwrap().to_bytes()).unwrap();
        assert!(dump.crashed && dump.end_hash.is_none());
        assert_eq!(dump.end_tick, w.time.total_ticks + 1);
        let (mut r, mut player) = Player::start(dump, w.defs.clone(), balances).unwrap();
        assert!(player.run(&mut r).is_ok());
        w.tick();
        assert_eq!(r.state_hash(), w.state_hash());
    }

    /// Working out what a build would cost changes nothing, since the game does it
    /// every frame while the player drags and a replay never does.
    #[test]
    fn estimates_leave_the_city_alone() {
        let Some((mut w, _)) = recorded_world() else { return };
        let (ex, ey) = (86, 122);
        let before = w.state_hash();
        for k in [crate::buildings::kind::VACANT_LOT, crate::buildings::kind::BAZAAR, crate::buildings::kind::ROAD] {
            w.estimate(&Command::Build { kind: k, x: ex + 1, y: ey + 1, x1: ex + 4, y1: ey + 4 });
        }
        w.estimate(&Command::Road { start: (ex, ey), end: (ex + 10, ey + 10) });
        w.estimate(&Command::Clear { x0: ex - 5, y0: ey - 5, x1: ex + 5, y1: ey + 5 });
        assert_eq!(w.state_hash(), before);
    }
}
