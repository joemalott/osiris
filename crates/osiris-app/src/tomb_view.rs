//! Laborers and craftsmen at work on pyramids and mastabas: laborers on the tile of
//! the site they work, craftsmen up on the blocks, in the poses the original gives
//! them (its figure AIs at 0x4a7603, 0x4a7ea4, 0x4a87fc and 0x4a98b0).

use crate::city_view::Sprite;
use osiris_sim::World;
use osiris_sim::farms::PEASANT;
use osiris_sim::figures::Figure;
use osiris_sim::monuments::{BRICKLAYER, CARPENTER, STONEMASON, TombPose};

/// Ticks a mason spends setting a block before he works it (the original's 0x5e4ee8).
const SETTING_TICKS: i32 = 41;

fn image(world: &World, kind: u16, key: &str) -> Option<u32> {
    world.defs.figure(kind).and_then(|d| d.anims.get(key)).map(|a| a.image)
}

/// A waiting mason's fidgeting, `frames` long, changing every `turn` ticks (0x4a95b4,
/// 0x4a868c): he stands, rocks between two frames, or runs through the whole of it.
fn idle_frame(ticks: u64, fid: u32, turn: u64, frames: u64) -> u32 {
    let e = ticks / 2 + fid as u64 * 13;
    match (e / turn) & 3 {
        0 => ((e >> 2) & 1) as u32,
        1 => (e % frames) as u32,
        _ => 0,
    }
}

/// The sprite for a laborer working a tomb's site, or a craftsman at a tomb; `None`
/// for everyone else.
pub fn figure_sprite(world: &World, f: &Figure) -> Option<Sprite> {
    let ticks = world.time.total_ticks;
    let dir = f.direction as u32;
    if f.kind == PEASANT {
        // Working a tile of the site: the work frames at half speed.
        if f.action != 4 || world.tomb_entry(f.target).is_none() {
            return None;
        }
        let work = image(world, PEASANT, "work")?;
        return Some(Sprite { x: f.x, y: f.y, offset: (0, 0), image: work + dir + 8 * (ticks % 12 / 2) as u32 });
    }
    let pose = world.tomb_pose(f)?;
    let walk = image(world, f.kind, "walk")?;
    let (base, frame) = match (f.kind, pose) {
        (_, TombPose::Walk) => (walk, f.frame(12)),
        (STONEMASON, TombPose::Idle) => (image(world, f.kind, "idle")?, idle_frame(ticks, f.id, 6, 12)),
        (STONEMASON, TombPose::Work { ticks: t, laying }) if laying && t < SETTING_TICKS => (image(world, f.kind, "idle")?, (ticks % 12) as u32),
        (STONEMASON, TombPose::Work { .. }) => (image(world, f.kind, "work_ground")?, (ticks % 7) as u32),
        (BRICKLAYER, TombPose::Idle) => (image(world, f.kind, "idle")?, idle_frame(ticks, f.id, 9, 9)),
        (BRICKLAYER, TombPose::Work { ticks: t, laying }) if laying && t < SETTING_TICKS => (image(world, f.kind, "idle")?, (ticks % 9) as u32),
        (BRICKLAYER, TombPose::Work { ticks: t, .. }) => (image(world, f.kind, "work")?, (t % 23) as u32),
        (CARPENTER, TombPose::Work { .. }) => (image(world, f.kind, "work_ground")?, (ticks % 12 / 2) as u32),
        _ => (walk, 0),
    };
    let image = base + dir + 8 * frame;
    let (tile, offset) = match f.perch {
        Some(p) => world.perch_sprite(f.target, &p, world.tomb_entry(f.target)?)?,
        None => ((f.x, f.y), f.pixel_offset()),
    };
    Some(Sprite { x: tile.0, y: tile.1, offset, image })
}
