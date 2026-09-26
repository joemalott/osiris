//! Hunting: game grazing, watching, running and falling, and hunters in their poses
//! (the original's FUN_0049bd40 and FUN_004a2430 choose these images).

use crate::city_view::Sprite;
use osiris_sim::World;
use osiris_sim::animals::{self, HUNTER_SPEAR, action, figure_kind};
use osiris_sim::defs::Anim;
use osiris_sim::figures::Figure;

fn anim<'a>(world: &'a World, kind: u16, key: &str) -> Option<&'a Anim> {
    world.defs.figure(kind).and_then(|d| d.anims.get(key))
}

/// The sprite for game, a hunter or his spear: `Some(None)` while a hunter waits
/// unseen in his lodge, `None` for everyone else.
pub fn figure_sprite(world: &World, f: &Figure) -> Option<Option<Sprite>> {
    let (a, step, directional) = if animals::is_animal(f.kind) {
        game(world, f)?
    } else if animals::is_hunter(f.kind) {
        if f.action == action::STARTING {
            return Some(None);
        }
        hunter(world, f)?
    } else if f.kind == HUNTER_SPEAR {
        // The spear in flight, in the 32 headings of its throw.
        (anim(world, HUNTER_SPEAR, "walk")?, f.look as u32 % 32, false)
    } else {
        return None;
    };
    let dir = if directional { f.direction as u32 } else { 0 };
    let step = if directional { 8 * step } else { step };
    Some(Some(Sprite { behind: false, x: f.x, y: f.y, offset: f.pixel_offset(), image: a.image + dir + step }))
}

/// Game walks and runs with its walk, lies as its death's last frame (one frame a
/// tick until then), grazes or stands (alternately by animal) while resting, and
/// stands still on its guard.
fn game<'a>(world: &'a World, f: &Figure) -> Option<(&'a Anim, u32, bool)> {
    let walk = anim(world, f.kind, "walk")?;
    Some(match f.action {
        osiris_sim::military::action::CORPSE => {
            let d = anim(world, f.kind, "death")?;
            (d, (f.counter.max(0) as u32).min(d.frames.max(1) - 1), false)
        }
        action::WANDERING | action::FLEEING => {
            let run = if f.action == action::FLEEING { anim(world, f.kind, "run").unwrap_or(walk) } else { walk };
            (run, if f.moving { f.frame(run.frames.max(1)) } else { 0 }, true)
        }
        action::ALERT => (anim(world, f.kind, "idle").unwrap_or(walk), 0, true),
        _ => {
            let key = if f.id % 2 == 0 { "eating" } else { "idle" };
            let a = anim(world, f.kind, key).unwrap_or(walk);
            (a, (world.time.total_ticks as u32 + f.id) % a.frames.max(1), true)
        }
    })
}

/// A hunter walks with his walk, throws with his throw (12 frames, then standing
/// on its first), butchers with his packing (birds are picked up where they fell)
/// and carries the meat home with his loaded walk.
fn hunter<'a>(world: &'a World, f: &Figure) -> Option<(&'a Anim, u32, bool)> {
    let walk = anim(world, f.kind, "walk")?;
    let birds = f.kind == figure_kind::BIRDS_HUNTER;
    Some(match f.action {
        action::THROWING => {
            let a = anim(world, f.kind, "hunt").unwrap_or(walk);
            (a, if f.counter < 12 { (f.counter.max(0) as u32).min(a.frames.max(1) - 1) } else { 0 }, true)
        }
        action::BUTCHERING if !birds => {
            let a = anim(world, f.kind, "pack").unwrap_or(walk);
            (a, (f.counter.max(0) as u32).min(a.frames.max(1) - 1), true)
        }
        action::CARRYING if !birds => {
            let a = anim(world, f.kind, "move_pack").unwrap_or(walk);
            (a, if f.moving { f.frame(a.frames.max(1)) } else { 0 }, true)
        }
        _ => (walk, if f.moving { f.frame(walk.frames.max(1)) } else { 0 }, true),
    })
}
