//! Things the city shows and plays that change nothing in it: dust clouds and sound
//! effects. The sim queues them with the tick they happened on; the screen takes
//! them from `World::fx` and may drop them.

use crate::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fx {
    /// A building of `size` came down with its top corner at `(x, y)`: the original's
    /// sixteen dust clouds (and a crash, queued with them). A ship rammed raises the
    /// first four of them, from its own tile (size 0; FUN_0048c0e0).
    Dust { x: i32, y: i32, size: i32, pieces: u8 },
    /// A sound from AUDIO/Wavs.
    Sound(&'static str),
    /// A figure of the original's type `kind` at `(x, y)` struck a blow or loosed a
    /// missile (`slot` 2) or fell (`slot` 3): the screen picks its sound
    /// (`osiris_audio::city`).
    FigureSound { x: i32, y: i32, kind: u16, slot: u8 },
}

/// The crash of a building falling (the original's sound 17).
pub const CRASH: &str = "CRASH.WAV";
/// Fire catching (sound 18).
pub const FIRE: &str = "FIRE.WAV";
/// A building the player pulls down (sound 2).
pub const DIG: &str = "DIG.WAV";
/// A ram striking a ship, by the angle of the blow 1 to 5 (sounds 11 to 15,
/// FUN_004a5880): on the beam, the fore quarter, the aft quarter, the bow, the stern.
pub const SHIP_COLLISION: [&str; 5] = ["ship_collision_beam.wav", "ship_collision_fore_quarter.wav", "ship_collision_aft_quarter.wav", "ship_collision_bow.wav", "ship_collision_stern.wav"];

/// Effects kept for a screen that isn't looking (headless runs): the oldest go.
const KEEP: usize = 512;

impl World {
    pub(crate) fn fx(&mut self, fx: Fx) {
        if self.fx.len() >= KEEP {
            self.fx.drain(..KEEP / 2);
        }
        self.fx.push((self.time.total_ticks, fx));
    }

    /// The sound of figure `fid` striking (`slot` 2) or falling (`slot` 3). Osiris
    /// numbers infantry and charioteers the other way round from the original.
    pub(crate) fn figure_sound(&mut self, fid: crate::figures::FigureId, slot: u8) {
        let Some(f) = self.figures.get(fid) else { return };
        let kind = match f.kind {
            crate::military::INFANTRY => 12,
            crate::military::CHARIOTEER => 13,
            k => k,
        };
        let (x, y) = (f.x, f.y);
        self.fx(Fx::FigureSound { x, y, kind, slot });
    }

    /// Dust over a fallen building, with its crash.
    pub(crate) fn dust(&mut self, x: i32, y: i32, size: i32) {
        self.fx(Fx::Dust { x, y, size, pieces: 16 });
        self.fx(Fx::Sound(CRASH));
    }
}
