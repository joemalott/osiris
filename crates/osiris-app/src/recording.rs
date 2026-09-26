//! The app's side of recording and replaying games (`osiris_sim::replay`): every
//! city played is recorded from the moment it starts or is loaded, so a crash can
//! leave the game so far next to the log, and `--record` keeps it; `--replay`
//! plays a recording back and says whether it still comes out the same.

use anyhow::{Result, bail};
use osiris_sim::replay::{Player, Recording, Replay};
use osiris_sim::{Balance, Defs, World};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The recording of the city being played, for the crash handler, and where
/// `--record` wants it written.
static CURRENT: Mutex<Option<Recording>> = Mutex::new(None);
static RECORD_TO: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The file name of a crash dump, in the folder the log is in.
pub const CRASH_FILE: &str = "crash.osiris-replay";

/// `--record FILE`: the last city played is written there when the next one
/// starts and when the game closes.
pub fn record_to(path: PathBuf) {
    *RECORD_TO.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
}

/// Starts recording a city about to be played, keeping the last one if asked to.
pub fn start(world: &mut World) {
    flush();
    match world.start_recording() {
        Ok(()) => *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = world.recording(),
        Err(e) => log::warn!("not recording this game: {e}"),
    }
}

/// Writes the recording so far to the `--record` file, if there is one.
pub fn flush() {
    let Some(path) = RECORD_TO.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    let Some(replay) = CURRENT.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(Recording::so_far) else { return };
    match replay.write(&path) {
        Ok(()) => log::info!("recorded {} commands to {}", replay.events.len(), path.display()),
        Err(e) => log::warn!("could not write the recording: {e}"),
    }
}

/// From the panic handler: writes the game so far to `dir`, returning where.
/// Never blocks (the crash may have come while the recording was held).
pub fn write_crash_dump(dir: &Path) -> Option<PathBuf> {
    let replay = CURRENT.try_lock().ok()?.as_ref()?.crash_dump()?;
    let path = dir.join(CRASH_FILE);
    replay.write(&path).ok()?;
    Some(path)
}

/// Plays `path` back from its start into a new city, checking it month by month.
/// Returns the city as the replay left it and the verdict: `Ok` with a summary
/// when it came out the same, `Err` with where it first differed.
pub fn play(path: &Path, defs: Arc<Defs>, balances: Arc<[Arc<Balance>; 5]>) -> Result<(World, std::result::Result<String, String>)> {
    let (mut world, mut player) = open(path, defs, balances)?;
    let verdict = match player.run(&mut world) {
        Ok((commands, months)) => Ok(format!("identical: {commands} commands, {months} months checked, {} ticks", world.time.total_ticks)),
        Err(d) => Err(d.to_string()),
    };
    Ok((world, verdict))
}

/// The replay at `path` ready to run: its starting city and the player.
pub fn open(path: &Path, defs: Arc<Defs>, balances: Arc<[Arc<Balance>; 5]>) -> Result<(World, Player)> {
    let replay = Replay::read(path).map_err(anyhow::Error::msg)?;
    if replay.osiris != env!("CARGO_PKG_VERSION") {
        log::warn!("{} was recorded by Osiris {}", path.display(), replay.osiris);
    }
    if replay.crashed {
        eprintln!("{} is a crash dump: it runs to the tick the game crashed in", path.display());
    }
    Player::start(replay, defs, balances).map_err(anyhow::Error::msg)
}

/// A replay's verdict as an error for the command line, after it was printed.
pub fn check(verdict: std::result::Result<String, String>) -> Result<()> {
    match verdict {
        Ok(s) => {
            println!("replay {s}");
            Ok(())
        }
        Err(d) => bail!("replay {d}"),
    }
}
