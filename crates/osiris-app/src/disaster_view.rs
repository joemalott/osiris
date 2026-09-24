//! How disasters look: the dust of falling buildings, frogs, locusts, the wrecks Seth
//! leaves of boats, people struck down by hail, and the hailstorm over the city.
//!
//! Dust is the original's explosion figures (type 6): sixteen clouds to a fallen
//! building, each drifting up the screen from its middle toward a point six tiles
//! out, at one to four steps (a fifteenth of a tile) a tick. The slow ones are the
//! big smoke (20 frames, one or two ticks each), the fast ones the small (15 frames,
//! three ticks each). They change nothing in the city, so they live here.

use crate::city_view::Sprite;
use osiris_render::{Renderer, Space};
use osiris_sim::World;
use osiris_sim::effects::Fx;
use osiris_sim::plagues::{self, FALL_FRAMES, LOCUST, SHIPWRECK};

/// By a building's size (1-5): how far into it, in tiles and in steps, the clouds start.
const CLOUD_TILE: [i32; 6] = [0, 0, 0, 1, 1, 2];
const CLOUD_STEP: [i32; 6] = [0, 7, 14, 7, 14, 7];
/// Where each cloud heads, in tiles from where it starts, and its speed in steps a tick.
const CLOUD_HEADING: [(i32, i32); 16] =
    [(0, -6), (-2, -5), (-4, -4), (-5, -2), (-6, 0), (-5, -2), (-4, -4), (-2, -5), (0, -6), (-2, -5), (-4, -4), (-5, -2), (-6, 0), (-5, -2), (-4, -4), (-2, -5)];
const CLOUD_SPEED: [i32; 16] = [1, 2, 1, 3, 2, 4, 3, 2, 1, 4, 2, 3, 4, 1, 3, 1];
/// Steps to a tile.
const STEPS: i32 = 15;
/// Hail lightning comes every this many ticks of the storm, for this long.
const LIGHTNING_TICKS: i32 = 136;
const LIGHTNING_SECONDS: f32 = 0.15;
/// Dynamic texture key of the hail streaks.
const HAIL_KEY: u32 = 0x4841_494c;

struct Cloud {
    born: u64,
    from: (i32, i32),
    to: (i32, i32),
    speed: i32,
    /// Ticks each frame shows, and the frames: big smoke or small.
    frame_ticks: u64,
    big: bool,
}

impl Cloud {
    fn frames(&self) -> u64 {
        if self.big { 20 } else { 15 }
    }

    /// Where the cloud is after `steps` steps along its line (it stops at the end).
    fn at(&self, steps: i32) -> (i32, i32) {
        let (dx, dy) = ((self.to.0 - self.from.0).abs(), (self.to.1 - self.from.1).abs());
        let (sx, sy) = ((self.to.0 - self.from.0).signum(), (self.to.1 - self.from.1).signum());
        let n = steps.min(dx.max(dy));
        // Along the longer axis a step at a time, the shorter in proportion.
        if dx >= dy {
            (self.from.0 + sx * n, self.from.1 + sy * (n * dy + dx / 2) / dx.max(1))
        } else {
            (self.from.0 + sx * (n * dx + dy / 2) / dy, self.from.1 + sy * n)
        }
    }
}

#[derive(Default)]
pub struct Disasters {
    clouds: Vec<Cloud>,
    /// The storm tick the last lightning came at, and the clock it began at.
    lightning: Option<(i32, f32)>,
    /// Varies the hail streaks from frame to frame.
    seed: u32,
}

impl Disasters {
    /// Takes the sim's queued effects: clouds are kept, sounds returned to be played.
    pub fn take(&mut self, world: &mut World) -> Vec<&'static str> {
        let mut sounds = Vec::new();
        for (tick, fx) in world.fx.drain(..) {
            match fx {
                Fx::Dust { x, y, size } => {
                    let s = size.clamp(1, 5) as usize;
                    let tile = (x + CLOUD_TILE[s], y + CLOUD_TILE[s]);
                    let at = |t: (i32, i32)| (t.0 * STEPS + CLOUD_STEP[s], t.1 * STEPS + CLOUD_STEP[s]);
                    for i in 0..16 {
                        let speed = CLOUD_SPEED[i];
                        let big = speed < 3;
                        let (hx, hy) = CLOUD_HEADING[i];
                        self.clouds.push(Cloud {
                            born: tick,
                            from: at(tile),
                            to: at((tile.0 + hx, tile.1 + hy)),
                            speed,
                            // Big smoke shows each frame one or two ticks, by the figure's slot.
                            frame_ticks: if big { (i as u64 & 1) + 1 } else { 3 },
                            big,
                        });
                    }
                }
                Fx::Sound(s) => sounds.push(s),
            }
        }
        sounds
    }

    /// The dust clouds still in the air at tick `now`.
    pub fn cloud_sprites(&mut self, world: &World, now: u64) -> Vec<Sprite> {
        let defs = &world.defs;
        let explosion = defs.figure(6);
        let (big, small) = (explosion.and_then(|d| d.anims.get("poof")).map(|a| a.image), explosion.and_then(|d| d.anims.get("small")).map(|a| a.image));
        self.clouds.retain(|c| now.saturating_sub(c.born) < c.frame_ticks * c.frames());
        let mut out = Vec::new();
        for c in &self.clouds {
            let age = now.saturating_sub(c.born);
            if age == 0 {
                continue;
            }
            let Some(base) = (if c.big { big } else { small }) else { continue };
            let (cx, cy) = c.at(age as i32 * c.speed);
            let (cx, cy) = (cx.max(0), cy.max(0));
            let (fx, fy) = (cx % STEPS - 7, cy % STEPS - 7);
            out.push(Sprite { x: cx / STEPS, y: cy / STEPS, offset: (2 * (fx - fy), fx + fy), image: base + (age / c.frame_ticks) as u32 });
        }
        out
    }

    /// Draws the hailstorm over the city view: the city dimmed by half and streaked
    /// with hail, and every 136 ticks of the storm a flash of lightning.
    pub fn draw_hail(&mut self, r: &mut Renderer, world: &World, clock: f32, left: f32, top: f32) {
        let hail = world.plagues.hail;
        if hail <= 0 {
            self.lightning = None;
            return;
        }
        let (w, h) = (left.max(1.0) as u32, (r.screen[1] - top).max(1.0) as u32);
        if hail % LIGHTNING_TICKS == 0 && self.lightning.is_none_or(|(t, _)| t != hail) {
            self.lightning = Some((hail, clock));
        }
        if let Some((_, start)) = self.lightning
            && clock - start < LIGHTNING_SECONDS
        {
            // The original greys the picture with noise; a pale wash stands in for it.
            r.rect([0.0, top], [w as f32, h as f32], [0.75, 0.75, 0.8, 0.55], Space::Screen);
            return;
        }
        r.rect([0.0, top], [w as f32, h as f32], [0.0, 0.0, 0.0, 0.5], Space::Screen);
        // One streak of hail in every 128 pixels of a row, each row shifted at random.
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let mut s = self.seed;
        for y in 0..h {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let shift = (s >> 16) & 127;
            let mut x = (128 - shift) % 128;
            while x < w {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                x += 128;
            }
        }
        let handle = r.upload_dynamic(HAIL_KEY, w, h, &rgba);
        r.dynamic_image(handle, [0.0, top], [w as f32, h as f32], Space::Screen);
    }
}

/// How a figure of a plague looks: a frog hopping, a locust flying or eating, a wreck's
/// flotsam, or anyone struck down by hail dying. `None` for other figures.
pub fn figure_sprite(world: &World, f: &osiris_sim::figures::Figure) -> Option<Sprite> {
    let def = world.defs.figure(f.kind)?;
    let anim = |key: &str| def.anims.get(key);
    let at = |image: u32| Some(Sprite { x: f.x, y: f.y, offset: f.pixel_offset(), image });
    if f.action == osiris_sim::military::action::CORPSE && !plagues::keeps_own_corpse(f.kind) {
        let death = anim("death")?;
        let frame = (FALL_FRAMES[(f.counter / 2).clamp(0, 63) as usize] as u32).min(death.frames.max(1) - 1);
        return at(death.image + frame);
    }
    match f.kind {
        plagues::FROG => {
            let walk = anim("walk")?;
            let frame = if f.action == plagues::action::WAITING { 0 } else { f.anim_tick % walk.frames.max(1) };
            at(walk.image + 8 * frame + f.direction as u32)
        }
        // Locusts are a swarm without facings.
        LOCUST => {
            let a = if f.action == plagues::action::ARRIVING { anim("eat")? } else { anim("walk")? };
            let frame = if f.action == plagues::action::WAITING { 0 } else { f.anim_tick % a.frames.max(1) };
            at(a.image + frame)
        }
        SHIPWRECK => {
            let wreck = anim("wreck")?;
            at(wreck.image + (f.counter as u32).min(wreck.frames.max(1) - 1))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clouds_stop_where_they_head() {
        let c = Cloud { born: 0, from: (100, 100), to: (100 - 30, 100 - 75), speed: 1, frame_ticks: 1, big: true };
        assert_eq!(c.at(0), (100, 100));
        assert_eq!(c.at(75), (70, 25));
        assert_eq!(c.at(500), (70, 25));
    }
}
