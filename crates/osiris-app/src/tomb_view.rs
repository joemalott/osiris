//! Laborers and craftsmen at work on pyramids and mastabas: laborers on the tile of
//! the site they work, craftsmen up on the blocks, sleds dragged up to them, in the
//! poses the original gives them (its figure AIs at 0x4a7603, 0x4a7ea4, 0x4a87fc and
//! 0x4a98b0, the sled puller's at 0x4af2c0 and the sled's at 0x4ab980).

use crate::city_view::Sprite;
use osiris_sim::World;
use osiris_sim::farms::PEASANT;
use osiris_sim::figures::Figure;
use osiris_sim::monuments::{BRICKLAYER, CARPENTER, SLED, SLED_CLIMB, SLED_CROSS, SLED_DONE, SLED_PULLER, SLED_TO_TOMB, STONEMASON, TombPose};

/// Ticks a mason spends setting a block before he works it (the original's 0x5e4ee8).
const SETTING_TICKS: i32 = 41;

fn image(world: &World, kind: u16, key: &str) -> Option<u32> {
    world.defs.figure(kind).and_then(|d| d.anims.get(key)).map(|a| a.image)
}

/// A waiting mason's fidgeting, `frames` long, changing every `turn` ticks of his wait
/// (0x4a95b4, 0x4a868c): he stands, rocks between two frames, or runs through the
/// whole of it. (His turning about is done by the simulation.)
fn idle_frame(waited: i32, fid: u32, turn: i32, frames: i32) -> u32 {
    let e = waited / 2 + fid as i32 * 13;
    match (e / turn) & 3 {
        0 => ((e >> 2) & 1) as u32,
        1 => (e % frames) as u32,
        _ => 0,
    }
}

/// The sled's image group for its load (the original's switches at 0x4abc13 and
/// 0x4abacf): stone, limestone, granite, marble and sandstone have their own; anything
/// else is drawn as bricks. Going up a tomb's way up it is drawn tilted.
fn sled_key(r: u16, up: bool) -> &'static str {
    let key = match r {
        24 => "stone",
        25 => "limestone",
        26 => "granite",
        35 => "marble",
        30 => "sandstone",
        _ => "bricks",
    };
    if !up {
        return key;
    }
    match key {
        "stone" => "up_stone",
        "limestone" => "up_limestone",
        "granite" => "up_granite",
        "marble" => "up_marble",
        "sandstone" => "up_sandstone",
        _ => "up_bricks",
    }
}

/// Where a figure is drawn: up on a tomb by its perch, else where it walks. The flag
/// says it goes before its tile's building.
fn place(world: &World, f: &Figure) -> Option<((i32, i32), (i32, i32), bool)> {
    match f.perch {
        Some(p) => world.perch_sprite(f.target, &p, world.tomb_entry(f.target)?, f.direction),
        None => Some(((f.x, f.y), f.pixel_offset(), false)),
    }
}

fn sprite(((x, y), offset, behind): ((i32, i32), (i32, i32), bool), image: u32) -> Sprite {
    Sprite { behind, x, y, offset, image }
}

/// The sprite for a laborer working a tomb's site or dragging a sled, a sled or its
/// puller, or a craftsman at a tomb; `None` for everyone else.
pub fn figure_sprite(world: &World, f: &Figure) -> Option<Sprite> {
    let ticks = world.time.total_ticks;
    let dir = f.direction as u32;
    match f.kind {
        PEASANT => {
            let walk = image(world, PEASANT, "walk")?;
            let pull = image(world, SLED_PULLER, "walk")?;
            let frame = 8 * f.frame(12);
            // With the sled he walks as its puller over the ground and across the top,
            // as himself up the way up and while he stands after (0x4ab2af).
            let image = match f.action {
                4 if world.tomb_entry(f.target).is_some() => image(world, PEASANT, "work")? + dir + 8 * (ticks % 12 / 2) as u32,
                SLED_TO_TOMB | SLED_CROSS => pull + dir + frame,
                SLED_CLIMB | SLED_DONE => walk + dir + frame,
                _ => return None,
            };
            return Some(sprite(place(world, f)?, image));
        }
        SLED_PULLER => return Some(sprite(place(world, f)?, image(world, SLED_PULLER, "walk")? + dir + 8 * f.frame(12))),
        SLED => {
            let up = f.perch.is_some_and(|p| p.from.is_some() && p.from_height < p.height);
            return Some(sprite(place(world, f)?, image(world, SLED, sled_key(f.cargo, up))? + dir));
        }
        _ => {}
    }
    let pose = world.tomb_pose(f)?;
    let walk = image(world, f.kind, "walk")?;
    let waited = f.counter;
    let (base, frame) = match (f.kind, pose) {
        (_, TombPose::Walk) => (walk, f.frame(12)),
        (STONEMASON, TombPose::Idle) => (image(world, f.kind, "idle")?, idle_frame(waited, f.id, 6, 12)),
        (STONEMASON, TombPose::Work { ticks: t, laying }) if laying && t < SETTING_TICKS => (image(world, f.kind, "idle")?, (ticks % 12) as u32),
        (STONEMASON, TombPose::Work { .. }) => (image(world, f.kind, "work_ground")?, (ticks % 7) as u32),
        (BRICKLAYER, TombPose::Idle) => (image(world, f.kind, "idle")?, idle_frame(waited, f.id, 9, 9)),
        (BRICKLAYER, TombPose::Work { ticks: t, laying }) if laying && t < SETTING_TICKS => (image(world, f.kind, "idle")?, (ticks % 9) as u32),
        (BRICKLAYER, TombPose::Work { ticks: t, .. }) => (image(world, f.kind, "work")?, (t % 23) as u32),
        (CARPENTER, TombPose::Work { .. }) => (image(world, f.kind, "work_ground")?, (ticks % 12 / 2) as u32),
        _ => (walk, 0),
    };
    Some(sprite(place(world, f)?, base + dir + 8 * frame))
}
