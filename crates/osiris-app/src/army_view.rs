//! Drawing the army and its enemies: soldiers and invaders marching, striking and
//! fallen, and missiles in flight.

use crate::city_view::Sprite;
use osiris_sim::World;
use osiris_sim::defs::Anim;
use osiris_sim::figures::Figure;
use osiris_sim::invasions::{self, ENEMY_ARCHER, ENEMY_CHARIOT};
use osiris_sim::military::{self, action};

/// Ticks each frame of a death lasts; it plays once.
const DEATH_FRAME_TICKS: i32 = 4;

/// A fighter's marching, striking and falling animations: an invader's from his
/// army's art, anyone else's from the figure list.
fn fighter_anims(world: &World, f: &Figure) -> Option<(Anim, Option<Anim>, Option<Anim>)> {
    if invasions::is_invader_kind(f.kind) {
        let arm = match f.kind {
            ENEMY_ARCHER => 1,
            ENEMY_CHARIOT => 2,
            _ => 0,
        };
        let army = world.defs.armies.get(f.cargo as usize)?;
        let s = army.arms[arm].or(army.arms[0])?;
        return Some((s.walk, Some(s.attack), Some(s.death)));
    }
    let anims = &world.defs.figure(f.kind)?.anims;
    Some((*anims.get("walk")?, anims.get("attack").copied(), anims.get("death").copied()))
}

/// The sprite for a fighter or missile, or `None` for other figures.
pub fn fighter_sprite(world: &World, f: &Figure) -> Option<Sprite> {
    let fighter = military::is_soldier(f.kind) || f.kind == military::STANDARD_BEARER || invasions::is_invader_kind(f.kind);
    let missile = matches!(f.kind, military::ARROW | military::JAVELIN);
    if !fighter && !missile {
        return None;
    }
    let offset = f.pixel_offset();
    if missile {
        let a = world.defs.figure(f.kind)?.anims.get("walk")?;
        return Some(Sprite { x: f.x, y: f.y, offset, image: a.image + f.direction as u32 % a.frames.max(1) });
    }
    let (walk, attack, death) = fighter_anims(world, f)?;
    let image = match (f.action, attack, death) {
        (action::CORPSE, _, Some(d)) => d.image + (f.counter / DEATH_FRAME_TICKS).clamp(0, d.frames as i32 - 1) as u32,
        (action::ATTACK, Some(a), _) => {
            let frame = (f.attack_tick as u32 * a.frames.max(1) / 24).min(a.frames.max(1) - 1);
            a.image + f.direction as u32 + 8 * frame
        }
        _ => {
            let frame = if f.moving { f.frame(walk.frames.max(1)) } else { 0 };
            walk.image + f.direction as u32 + 8 * frame
        }
    };
    Some(Sprite { x: f.x, y: f.y, offset, image })
}
