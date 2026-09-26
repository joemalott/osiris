//! Boats and the river: fishing boats, trade ships and ferry boats in their poses,
//! people crossing by ferry, and the fish marking the fishing grounds.

use crate::city_view::Sprite;
use osiris_sim::World;
use osiris_sim::defs::Anim;
use osiris_sim::docks::{TRADE_SHIP, ship_action};
use osiris_sim::figures::{Figure, Travel};
use osiris_sim::fishing::{self, FISHING_BOAT};
use osiris_sim::map::terrain;
use osiris_sim::water::FERRY_BOAT;

/// The fishing-ground marker figure, whose animations mark each ground.
const FISHING_POINT: u16 = 65;

fn anim<'a>(world: &'a World, kind: u16, key: &str) -> Option<&'a Anim> {
    world.defs.figure(kind).and_then(|d| d.anims.get(key))
}

/// Frame `ticks` of a looping animation, stepping every `duration` ticks.
fn frame(a: &Anim, ticks: u64) -> u32 {
    (ticks / a.duration.max(1) as u64 % a.frames.max(1) as u64) as u32
}

/// The sprite for a boat, or for someone on foot crossing the river by ferry; `None`
/// for everyone else.
pub fn figure_sprite(world: &World, f: &Figure) -> Option<Sprite> {
    let ticks = world.time.total_ticks;
    let (kind, key) = match f.kind {
        FISHING_BOAT => {
            let key = match f.action {
                fishing::action::FISHING => "work",
                fishing::action::AT_WHARF | fishing::action::CREATED => "idle",
                _ => "walk",
            };
            (FISHING_BOAT, key)
        }
        TRADE_SHIP => (TRADE_SHIP, if matches!(f.action, ship_action::MOORED | ship_action::QUEUED) { "idle" } else { "walk" }),
        FERRY_BOAT => (FERRY_BOAT, if f.moving { "walk" } else { "idle" }),
        k if f.travel != Travel::Water && world.map.terrain_is(f.x, f.y, terrain::WATER) => {
            // On foot across a ferry crossing: drawn in the ferry boat.
            if anim(world, k, "swim").is_some() { (k, "swim") } else { (FERRY_BOAT, "walk") }
        }
        _ => return None,
    };
    let a = anim(world, kind, key).or_else(|| anim(world, kind, "walk"))?;
    let step = match key {
        "walk" | "swim" if f.moving => frame(a, f.anim_tick as u64),
        "work" => frame(a, ticks + f.id as u64 * 7),
        _ => 0,
    };
    // A fishing boat casts its net turned two steps from its heading (FUN_004937a0).
    let dir = if key == "work" && f.kind == FISHING_BOAT { (f.direction as u32 + 6) % 8 } else { f.direction as u32 };
    Some(Sprite { behind: false, x: f.x, y: f.y, offset: f.pixel_offset(), image: a.image + dir + 8 * step })
}

/// Fish jumping at each fishing ground: bubbles, then a leap.
pub fn fishing_points(world: &World) -> Vec<Sprite> {
    let (Some(bubbles), Some(jump)) = (anim(world, FISHING_POINT, "bubbles"), anim(world, FISHING_POINT, "point")) else {
        return Vec::new();
    };
    let ticks = world.time.total_ticks;
    world
        .water
        .fishing_points
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let t = ticks + i as u64 * 37;
            let cycle = (bubbles.frames * bubbles.duration * 2 + jump.frames * jump.duration) as u64;
            let into = t % cycle.max(1);
            let bubbling = (bubbles.frames * bubbles.duration * 2) as u64;
            let image = if into < bubbling { bubbles.image + frame(bubbles, into) } else { jump.image + frame(jump, into - bubbling) };
            Sprite { behind: false, x, y, offset: (0, 0), image }
        })
        .collect()
}
