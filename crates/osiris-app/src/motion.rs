//! Smooth motion between simulation ticks (as OpenRCT2 draws its guests).
//!
//! The simulation moves figures in whole sub-steps (fifteen to a tile) once a tick,
//! and at normal speed a tick lasts several frames, so figures drawn where the last
//! tick left them step rather than glide. The city view instead draws each figure
//! one tick behind, part way from where it stood at the tick before to where it
//! stands now, by the fraction of the current tick that has gone by.
//!
//! Nothing here touches the simulation: [`Motion`] remembers, on the app's side,
//! where each figure's sprites stood at the last two ticks the view saw, keyed by
//! figure id. It only interpolates when the view saw both ticks one after the other,
//! so at speeds where a frame runs several ticks, or with no fraction (paused,
//! headless screenshots), figures are drawn where they are.
//!
//! A figure snaps rather than glides when it moved further in the tick than the
//! fastest walker can (a figure created, removed or given a new place: entering a
//! building, boarding, a perch change on a monument), or when its id now belongs to
//! another kind of figure or another home.
//!
//! Draw order: the view sorts figures by tile, and a figure's tile changes at the
//! midpoint of its step, when its foot crosses the edge between the tiles. An
//! interpolated figure is filed on the tick-before tile while less than half the
//! tick has gone by and on its current tile after that. As a tick moves a walker
//! one sub-step, that is the tile its drawn foot is over, so it goes behind a
//! building exactly when the original would have filed it there.

use std::collections::HashMap;

use osiris_sim::figures::FigureId;

/// The furthest a figure goes in one tick, in world pixels on either axis: three
/// sub-steps (the fastest stride of the model's speed table) of four pixels.
pub const MAX_STEP: i32 = 12;

/// Where a figure's sprites stood: the tile they are sorted on and the figure's foot
/// in world pixels (without the map's origin), and who it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub kind: u16,
    pub home: u32,
    pub tile: (i32, i32),
    pub foot: (i32, i32),
}

impl Anchor {
    pub fn new(kind: u16, home: u32, tile: (i32, i32), offset: (i32, i32)) -> Self {
        let base = tile_pixel(tile);
        Self { kind, home, tile, foot: (base.0 + offset.0, base.1 + offset.1) }
    }
}

/// Top-left of tile `t`'s box in world pixels, less the map's origin.
fn tile_pixel((x, y): (i32, i32)) -> (i32, i32) {
    ((x - y) * 30, (x + y) * 15)
}

/// How to move a figure's sprites to its interpolated place: add `tile` to their
/// tile and `offset` to their offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shift {
    pub tile: (i32, i32),
    pub offset: (i32, i32),
}

#[derive(Default)]
pub struct Motion {
    /// The tick `now` was taken at, if any.
    tick: Option<u64>,
    now: HashMap<FigureId, Anchor>,
    /// The figures at the tick before `tick`, when the view saw it.
    before: HashMap<FigureId, Anchor>,
}

impl Motion {
    /// Forgets both ticks (interpolation is off).
    pub fn clear(&mut self) {
        self.tick = None;
        self.now.clear();
        self.before.clear();
    }

    /// Takes the figures as they stand at `tick`. The first call at a new tick moves
    /// the last tick's figures back to "before", as long as it is the tick just
    /// before; later calls at the same tick change nothing.
    pub fn record(&mut self, tick: u64, anchors: impl Iterator<Item = (FigureId, Anchor)>) {
        if self.tick == Some(tick) {
            return;
        }
        if self.tick.is_some_and(|t| t + 1 == tick) {
            self.before = std::mem::take(&mut self.now);
        } else {
            self.before.clear();
            self.now.clear();
        }
        self.tick = Some(tick);
        self.now.extend(anchors);
    }

    /// Whether figure `id` glides this tick (for the headless check).
    pub fn moved(&self, id: FigureId) -> bool {
        self.now.get(&id).is_some_and(|at| self.shift(id, at, 0.0).is_some())
    }

    /// How far to move figure `id`, standing at `at`, back toward where it was a tick
    /// ago, with fraction `t` (0..=1) of the tick gone by; `None` to draw it where it is.
    pub fn shift(&self, id: FigureId, at: &Anchor, t: f32) -> Option<Shift> {
        let was = self.before.get(&id)?;
        if (was.kind, was.home) != (at.kind, at.home) || was.foot == at.foot {
            return None;
        }
        let d = (at.foot.0 - was.foot.0, at.foot.1 - was.foot.1);
        if d.0.abs() > MAX_STEP || d.1.abs() > MAX_STEP {
            return None;
        }
        let t = t.clamp(0.0, 1.0);
        let foot = (was.foot.0 + (d.0 as f32 * t).round() as i32, was.foot.1 + (d.1 as f32 * t).round() as i32);
        let tile = if t < 0.5 { was.tile } else { at.tile };
        let (base, own) = (tile_pixel(tile), tile_pixel(at.tile));
        // The sprites keep their offsets from the figure's foot; the tile they are
        // filed on moves the whole picture by the difference between the tiles.
        let offset = (foot.0 - at.foot.0 - (base.0 - own.0), foot.1 - at.foot.1 - (base.1 - own.1));
        let shift = Shift { tile: (tile.0 - at.tile.0, tile.1 - at.tile.1), offset };
        (shift != Shift { tile: (0, 0), offset: (0, 0) }).then_some(shift)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A walker's anchor `progress` sub-steps along a step east (+x) from tile (5, 5),
    /// filed as the simulation files it (the next tile from sub-step 8).
    fn walker(progress: i32) -> Anchor {
        let p = if progress >= 8 { progress - 15 } else { progress };
        let tile = if progress >= 8 { (6, 5) } else { (5, 5) };
        Anchor::new(10, 1, tile, (2 * p, p))
    }

    fn placed(a: &Anchor, s: Option<Shift>) -> ((i32, i32), (i32, i32)) {
        let s = s.unwrap_or(Shift { tile: (0, 0), offset: (0, 0) });
        let tile = (a.tile.0 + s.tile.0, a.tile.1 + s.tile.1);
        let off = (a.foot.0 - tile_pixel(a.tile).0 + s.offset.0, a.foot.1 - tile_pixel(a.tile).1 + s.offset.1);
        let base = tile_pixel(tile);
        (tile, (base.0 + off.0, base.1 + off.1))
    }

    #[test]
    fn a_walker_glides_between_ticks_and_changes_tile_halfway() {
        let mut m = Motion::default();
        m.record(100, [(1, walker(7))].into_iter());
        m.record(101, [(1, walker(8))].into_iter());
        let at = walker(8);
        // At the start of the tick he is where the last tick left him, on the old tile.
        assert_eq!(placed(&at, m.shift(1, &at, 0.0)), ((5, 5), walker(7).foot));
        // Half way through, half a sub-step on, filed on the new tile.
        assert_eq!(placed(&at, m.shift(1, &at, 0.5)), ((6, 5), (walker(7).foot.0 + 1, walker(7).foot.1 + 1)));
        // At its end, where he is now.
        assert_eq!(m.shift(1, &at, 1.0), None);
    }

    #[test]
    fn jumps_and_skipped_ticks_snap() {
        let mut m = Motion::default();
        m.record(100, [(1, walker(0)), (2, walker(0))].into_iter());
        let far = Anchor::new(10, 1, (9, 5), (0, 0));
        let other = Anchor { kind: 11, ..walker(1) };
        m.record(101, [(1, far), (2, other)].into_iter());
        assert_eq!(m.shift(1, &far, 0.3), None);
        assert_eq!(m.shift(2, &other, 0.3), None);
        // Another frame at the same tick keeps the tick before.
        m.record(101, std::iter::empty());
        assert!(m.shift(2, &Anchor { kind: 10, ..other }, 0.3).is_some());
        // Two ticks at once: nothing to interpolate from.
        m.record(103, [(2, walker(3))].into_iter());
        assert_eq!(m.shift(2, &walker(3), 0.3), None);
    }
}
