//! The city's sounds, as the original makes them (the tables are in
//! `osiris_audio::city`):
//!
//! - Now and then (one frame in fifty, FUN_004d6b60) the city picks a random point of
//!   the view and plays a sound of what is there: one of a building's sounds, or the
//!   land's (water, rock, meadow, floodplain, trees), panned to where the point is
//!   (FUN_0053acf0).
//! - Under them runs an ambience for the kind of place filling the middle of the view,
//!   counted over the tiles within two columns and two half-rows of its centre: people,
//!   industry, open land, meadow, floodplain, rock, water or trees; the people's hum
//!   grows with the city (FUN_0053b960).
//! - Opening a building's information window plays its sound (FUN_004f8e60), a walker's
//!   window his phrase and an animal's its cry (FUN_0053d0b0), and fighting men are
//!   heard striking and falling (FUN_0053b4a0).
//!
//! All of it follows Options > Sound: city sounds for the first two, effects for the
//! rest, speech for walkers.

use osiris_audio::{Audio, city};
use osiris_render::Renderer;
use osiris_sim::World;
use osiris_sim::figures::{Figure, FigureId};
use std::collections::HashMap;

/// The frame rate the original's one-in-fifty chances are counted at.
const FRAMES_PER_SECOND: f32 = 30.0;

#[derive(Default)]
pub struct CitySounds {
    seed: u64,
    /// Each walker's place in the phrases some types say in turn.
    turns: HashMap<FigureId, u8>,
}

/// The part of the screen the city shows: left, top, right, bottom.
pub type View = [f32; 4];

impl CitySounds {
    fn rand(&mut self) -> u32 {
        if self.seed == 0 {
            self.seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 33) as u32
    }

    /// Below `n`.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { self.rand() as usize % n }
    }

    /// A frame of the city on screen (`shown`), or of something covering it: the
    /// chance of a sound, and the ambience. `on` is the player's city-sounds switch.
    pub fn frame(&mut self, audio: &Audio, world: &World, r: &Renderer, view: View, dt: f32, shown: bool, on: bool) {
        if !shown || !on {
            audio.update_city_ambient(None, dt);
            return;
        }
        audio.update_city_ambient(ambience(world, r, view), dt);
        let chance = (dt * FRAMES_PER_SECOND / 50.0).min(1.0);
        if (self.rand() as f32 / u32::MAX as f32) >= chance {
            return;
        }
        let (w, h) = (view[2] - view[0], view[3] - view[1]);
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let px = self.rand() as f32 / u32::MAX as f32 * w;
        let py = self.rand() as f32 / u32::MAX as f32 * h;
        let Some((x, y)) = crate::city_view::world_to_tile(&world.map, r.screen_to_world([view[0] + px, view[1] + py])) else { return };
        let pan = px / w;
        let id = world.map.building.at_or(x, y, 0);
        let sounds: &[&str] = if id != 0 {
            match world.buildings.get(id).and_then(|b| city::building(b.kind)) {
                Some((_, sounds)) => sounds,
                None => return,
            }
        } else {
            match city::land_group(world.map.terrain.at_or(x, y, 0)) {
                Some(g) => city::TERRAIN[g],
                None => return,
            }
        };
        if !sounds.is_empty() {
            let pick = sounds[self.below(sounds.len())];
            audio.play_city_sound(pick, pan);
        }
    }

    /// A building's information window opened.
    pub fn building_opened(&self, audio: &Audio, kind: u16) {
        if let Some((click, _)) = city::building(kind)
            && !click.is_empty()
        {
            audio.play_effect_panned(click, 1.0, 0.5);
        }
    }

    /// A walker's information window opened on figure `fid`: an animal cries, a
    /// walker says his phrase.
    pub fn figure_opened(&mut self, audio: &Audio, world: &World, r: &Renderer, view: View, fid: FigureId) {
        let Some(f) = world.figures.get(fid) else { return };
        if let Some(row) = city::animal_row(f.kind) {
            self.figure_heard(audio, r, view, world, (f.x, f.y), f.kind, row, 0);
            return;
        }
        let Some(row) = speech_row(world, f) else { return };
        let Some(phrase) = self.phrase(world, f) else { return };
        if let Some(name) = city::voice(row, phrase) {
            audio.play_walker_voice(name);
        }
    }

    /// A figure of original type `kind` at tile `at` struck (`slot` 2) or fell
    /// (`slot` 3).
    pub fn figure_sound(&mut self, audio: &Audio, r: &Renderer, view: View, world: &World, at: (i32, i32), kind: u16, slot: u8) {
        let row = match slot {
            2 => match city::attack_row(kind) {
                Some(row) => row,
                None => return,
            },
            _ => city::death_row(kind, self.below(8)),
        };
        self.figure_heard(audio, r, view, world, at, kind, row, slot as usize);
    }

    /// Plays figure-sound `row`, `slot`, for a figure at tile `at`: panned to where it
    /// stands, and off the screen a soldier or ship at a quarter of the volume and
    /// anyone else not at all.
    #[allow(clippy::too_many_arguments)]
    fn figure_heard(&self, audio: &Audio, r: &Renderer, view: View, world: &World, at: (i32, i32), kind: u16, row: usize, slot: usize) {
        let Some(name) = city::FIGURE_SOUNDS.get(row).map(|s| s[slot]).filter(|n| !n.is_empty()) else { return };
        let t = crate::city_view::tile_to_world(&world.map, at.0, at.1);
        let p = [(t[0] + crate::city_view::TILE_W / 2.0 - r.camera.x) * r.camera.zoom, (t[1] + crate::city_view::TILE_H / 2.0 - r.camera.y) * r.camera.zoom];
        let on_screen = p[0] >= view[0] && p[1] >= view[1] && p[0] <= view[2] && p[1] <= view[3];
        let scale = match (on_screen, city::heard_off_screen(kind)) {
            (true, _) => 1.0,
            (false, true) => 0.25,
            (false, false) => return,
        };
        let pan = ((p[0] - view[0]) / (view[2] - view[0]).max(1.0)).clamp(0.0, 1.0);
        audio.play_effect_panned(name, scale, pan);
    }

    /// Which of his type's phrases walker `f` says (FUN_0053c100), by the original's
    /// rules where Osiris keeps what they look at; `None` for silence.
    fn phrase(&mut self, world: &World, f: &Figure) -> Option<usize> {
        let kind = original_type(f.kind);
        let far_from_home = || world.buildings.get(f.home).is_some_and(|b| (b.x - f.x).abs().max((b.y - f.y).abs()) > 24);
        let enemies = world.figures.iter().filter(|o| !o.dead && osiris_sim::invasions::is_invader_kind(o.kind)).count();
        let health = world.ratings.health;
        Some(match kind {
            // Immigrants, the homeless, thieves, robbers and market boys say theirs in turn.
            1 => self.turn(f.id, 3),
            3 => self.turn(f.id, 2),
            0x17 => self.turn(f.id, 4),
            0x18 => self.turn(f.id, 2),
            0x42 => self.turn(f.id, 3),
            // Emigrants tell why they go.
            2 => match world.sentiment_state.low_mood_cause {
                1 => 1,
                2 => 0,
                3 => 2,
                _ => 3,
            },
            // Cart pushers far from home say so, and are otherwise quiet.
            4 | 9 | 0x26 => {
                if far_from_home() {
                    2
                } else {
                    return None;
                }
            }
            // The engineer's and apothecary's tests of a byte against 599 and 800 always
            // come out the same way.
            8 => 10,
            0x21 => 1,
            // Doctors and embalmers worry about the city's health first.
            0x20 if health < 40 => 10,
            0x22 if health < 30 => 10,
            // Priests of Ra fret over the Kingdom's regard.
            0x1b if god_of(world, f.home) == Some(1) && world.ratings.kingdom < 20 => 11,
            // Guards and boats speak of the enemy.
            0x2a => match enemies {
                0 => 0,
                1..11 => 2,
                11..31 => 3,
                _ => return None,
            },
            0x4d => if enemies == 0 { 3 } else { 0 },
            0x4e => if enemies == 0 { 4 } else { 0 },
            0x13 | 0x15 => self.turn(f.id, 2) + 2,
            0x14 => 3,
            0x19 | 0x49 | 0x4b | 0x4f | 0x50 | 0x51 | 0x5a | 0x62 | 0x6c => 0,
            0x1a => 1,
            0x59 => 12,
            0x5b => 10,
            0x60 => 12,
            5 | 7 | 10 | 0xf..=0x12 | 0x1b | 0x1d..=0x20 | 0x22 | 0x23 | 0x27 | 0x28 | 0x57 | 0x58 | 0x69 => self.mood_phrase(world),
            _ => return None,
        })
    }

    /// The next of `n` phrases walker `id` says in turn.
    fn turn(&mut self, id: FigureId, n: u8) -> usize {
        let t = self.turns.entry(id).or_insert(0);
        *t = (*t + 1) % n;
        *t as usize
    }

    /// What most troubles the city, as the walkers who talk of it say (FUN_0053c060,
    /// FUN_0053d3a0): eight worries weighed 1 (none) to 8, one drawn by weight; 9 when
    /// nothing troubles it, 8 when the draw lands on a light one.
    fn mood_phrase(&mut self, world: &World) -> usize {
        let (w, total, fine) = worries(world);
        if fine {
            return 9;
        }
        let share = |x: u8| x as i32 * 100 / total.max(1);
        let sum: i32 = w.iter().map(|&x| share(x)).sum();
        // The original masks a random 0-127 with the sum, rather than taking a remainder.
        let roll = (self.rand() & 0x7f) as i32 & sum;
        let mut acc = 0;
        let mut pick = 7;
        for (i, &x) in w.iter().enumerate() {
            acc += share(x);
            if roll <= acc {
                pick = i;
                break;
            }
        }
        if w[pick] == 1 { 8 } else { pick }
    }
}

/// The eight worries' weights, their total, and whether none weighs anything:
/// 0 health, 1 food, 2 enemies, 3 too few workers, 4 angry gods, 5 the Kingdom's
/// regard, 6 too many idle, 7 entertainment.
fn worries(world: &World) -> ([u8; 8], i32, bool) {
    let mut w = [1u8; 8];
    let mut fine = true;
    let mut weigh = |i: usize, v: u8, w: &mut [u8; 8]| {
        w[i] = v;
        if v > 1 {
            fine = false;
        }
    };
    let health = world.ratings.health;
    weigh(0, match health { ..20 => 8, 20..30 => 6, 30..40 => 4, _ => 1 }, &mut w);
    let food = world.food_supply_months();
    weigh(1, match food { ..1 => 6, 1 => 4, _ => 1 }, &mut w);
    let enemies = world.figures.iter().any(|o| !o.dead && osiris_sim::invasions::is_invader_kind(o.kind));
    let soldiers = (0..world.military.companies.len()).any(|c| world.company_men(c) > 0);
    if enemies {
        if soldiers {
            // The original leaves the food weight in place here.
            w[2] = w[1];
        } else {
            weigh(2, 8, &mut w);
        }
    }
    let labor = &world.labor;
    if labor.needed > 0 {
        let short = 100 - labor.employed * 100 / labor.needed;
        weigh(3, match short { 21.. => 8, 11..=20 => 6, 6..=10 => 4, _ => 1 }, &mut w);
    }
    let gods = world.religion.gods.iter().map(|g| g.mood).min().unwrap_or(100).min(100);
    weigh(4, match gods { ..=20 => 6, 21..=30 => 4, _ => 1 }, &mut w);
    let kingdom = world.ratings.kingdom;
    weigh(5, match kingdom { ..=0 => 8, 1..20 => 6, 20..40 => 4, _ => 1 }, &mut w);
    if world.population > 0 {
        let idle = labor.unemployed * 100 / world.population;
        weigh(6, match idle { 18.. => 8, 11..=17 => 6, 6..=10 => 4, _ => 1 }, &mut w);
    }
    let fun = world.ratings.coverage.booth;
    weigh(7, match fun { ..=0 => 6, 1..20 => 4, _ => 1 }, &mut w);
    let mut total: i32 = w.iter().map(|&x| x as i32).sum();
    // Too few workers and too many idle don't both come up at their lightest.
    if w[3] == 1 && w[6] >= 1 {
        w[3] = 0;
        total -= 1;
    } else if w[3] > 1 && w[6] == 1 {
        w[6] = 0;
        total -= 1;
    }
    (w, total, fine)
}

/// The original's number for Osiris's figure type (infantry and charioteers swap).
fn original_type(kind: u16) -> u16 {
    match kind {
        osiris_sim::military::INFANTRY => 12,
        osiris_sim::military::CHARIOTEER => 13,
        k => k,
    }
}

/// Walker `f`'s speech row: by his type, a priest's by his temple's god and a
/// hunter's by his prey.
fn speech_row(world: &World, f: &Figure) -> Option<i16> {
    let mut kind = original_type(f.kind) as usize;
    // A festival walker speaks as the priest, performer, scribe or noble he looks.
    if *city::VOICE_ROWS.get(kind)? == -4 {
        kind = osiris_sim::festivals::festival_look(world, f).0 as usize;
    }
    match *city::VOICE_ROWS.get(kind)? {
        -3 => god_of(world, f.home).map(|g| 29 + g as i16),
        -2 => {
            let prey = world.figures.get(f.target).map(|p| p.kind).or_else(|| world.figures.iter().find(|p| matches!(p.kind, 68..=70)).map(|p| p.kind));
            match prey {
                Some(68) => Some(9),
                Some(69) => Some(8),
                _ => Some(7),
            }
        }
        row if row > 0 => Some(row),
        _ => None,
    }
}

/// The kind of place filling the middle of the view, and its ambience: the tiles
/// within two columns and two half-rows of the centre each count one for what they
/// are, a building once, at the tile it is drawn from.
fn ambience(world: &World, r: &Renderer, view: View) -> Option<&'static str> {
    ambience_at(world, r.screen_to_world([(view[0] + view[2]) / 2.0, (view[1] + view[3]) / 2.0]))
}

/// The ambience for a view whose middle is world pixel `centre`.
fn ambience_at(world: &World, centre: [f32; 2]) -> Option<&'static str> {
    use crate::city_view::{TILE_H, TILE_W, tile_to_world, world_to_tile};
    let (cx, cy) = world_to_tile(&world.map, centre).unwrap_or((-100, -100));
    if cx < 0 {
        return None;
    }
    let mut counts = [0i32; 12];
    for y in cy - 5..=cy + 5 {
        for x in cx - 5..=cx + 5 {
            if !world.map.contains(x, y) {
                continue;
            }
            let p = tile_to_world(&world.map, x, y);
            let (dx, dy) = (p[0] + TILE_W / 2.0 - centre[0], p[1] + TILE_H / 2.0 - centre[1]);
            if dx.abs() > 2.5 * TILE_W || dy.abs() > 2.5 * TILE_H / 2.0 {
                continue;
            }
            let id = world.map.building.at_or(x, y, 0);
            if id == 0 {
                counts[city::land_place(world.map.terrain.at_or(x, y, 0))] += 1;
                continue;
            }
            let Some(b) = world.buildings.get(id) else { continue };
            // A building counts once, at the tile it is drawn from.
            if (x, y) != (b.x, b.y + b.size - 1) {
                continue;
            }
            match city::ambient_kind(b.kind) {
                k @ (1 | 2 | 4) => counts[k as usize] += 1,
                _ => {}
            }
        }
    }
    let mut best = None;
    let mut most = 0;
    for (i, &n) in counts.iter().enumerate() {
        if n > most {
            most = n;
            best = Some(i);
        }
    }
    city::ambience(best?, world.population)
}

/// The god (0 Osiris, 1 Ra, 2 Ptah, 3 Seth, 4 Bast) of building `id`, a temple, temple
/// complex or shrine (FUN_004c5120).
fn god_of(world: &World, id: u32) -> Option<u16> {
    match world.buildings.get(id)?.kind {
        k @ 60..=69 => Some((k - 60) % 5),
        k @ 140..=144 => Some(k - 140),
        _ => None,
    }
}

/// For the script harness (`citysounds X,Y`): the ambience of a view centred on tile
/// `(x, y)`, the city's worries, and what each kind of walker in the city would say.
pub fn describe(world: &World, (x, y): (i32, i32)) -> String {
    let p = crate::city_view::tile_to_world(&world.map, x, y);
    let centre = [p[0] + crate::city_view::TILE_W / 2.0, p[1] + crate::city_view::TILE_H / 2.0];
    let (w, total, fine) = worries(world);
    let mut out = format!("ambience {:?} worries {w:?} total {total} fine {fine}", ambience_at(world, centre));
    let mut sounds = CitySounds::default();
    let mut seen = std::collections::BTreeSet::new();
    for f in world.figures.iter().filter(|f| !f.dead) {
        if !seen.insert(f.kind) {
            continue;
        }
        let voice = if let Some(row) = city::animal_row(f.kind) {
            Some(city::FIGURE_SOUNDS[row][0])
        } else {
            speech_row(world, f).zip(sounds.phrase(world, f)).and_then(|(row, phrase)| city::voice(row, phrase))
        };
        if let Some(v) = voice {
            out += &format!("\n  figure {} says {v}", f.kind);
        }
    }
    out
}
