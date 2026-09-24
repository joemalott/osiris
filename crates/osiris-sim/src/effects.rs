//! Things the city shows and plays that change nothing in it: dust clouds and sound
//! effects. The sim queues them with the tick they happened on; the screen takes
//! them from `World::fx` and may drop them.

use crate::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fx {
    /// A building of `size` came down with its top corner at `(x, y)`: the original's
    /// sixteen dust clouds (and a crash, queued with them).
    Dust { x: i32, y: i32, size: i32 },
    /// A sound from AUDIO/Wavs.
    Sound(&'static str),
}

/// The crash of a building falling (the original's sound 17).
pub const CRASH: &str = "CRASH.WAV";
/// Fire catching (sound 18).
pub const FIRE: &str = "FIRE.WAV";

/// Effects kept for a screen that isn't looking (headless runs): the oldest go.
const KEEP: usize = 512;

impl World {
    pub(crate) fn fx(&mut self, fx: Fx) {
        if self.fx.len() >= KEEP {
            self.fx.drain(..KEEP / 2);
        }
        self.fx.push((self.time.total_ticks, fx));
    }

    /// Dust over a fallen building, with its crash.
    pub(crate) fn dust(&mut self, x: i32, y: i32, size: i32) {
        self.fx(Fx::Dust { x, y, size });
        self.fx(Fx::Sound(CRASH));
    }
}
