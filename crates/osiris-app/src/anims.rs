//! Animations drawn over buildings: staff at work, performers at venues, and the
//! grain heaps in granaries. Most buildings follow one of two general rules; the
//! granary and the venues have their own, as in the original.

use crate::city_view::{self, Overlay};
use osiris_render::Renderer;
use osiris_sim::World;
use osiris_sim::buildings::{Building, kind};
use osiris_sim::defs::Anim;
use osiris_sim::map::edge;

/// Where the granary's eight heaps of food sit, from its first heap.
const GRANARY_SPOTS: [(i32, i32); 8] = [(0, 0), (16, 9), (35, 18), (51, 26), (-16, 7), (1, 16), (20, 26), (37, 35)];
const GRANARY_FIRST_SPOT: (i32, i32) = (110, -74);
/// Where the granary's two scribes stand; the second works only when over half staffed.
const GRANARY_SCRIBES: [(i32, i32); 2] = [(114, 2), (96, -4)];
/// Stored units that fill one heap.
const GRANARY_SPOT_UNITS: i32 = 400;

pub struct AnimContext<'a> {
    pub world: &'a World,
    pub r: &'a Renderer,
    /// Game ticks since the start, for animations stepped by the game clock.
    pub ticks: u64,
    /// Unpaused real time in milliseconds, for animations with their own speed.
    pub millis: u64,
}

impl AnimContext<'_> {
    /// Pixel of tile `(x, y)` that animation offsets are measured from.
    fn point(&self, x: i32, y: i32) -> [f32; 2] {
        city_view::tile_to_world(&self.world.map, x, y)
    }

    /// Frame `image` of an animation drawn at `offset` from tile `(x, y)`. Sprites carry
    /// their own anchor, which is subtracted.
    fn sprite(&self, out: &mut Vec<Overlay>, x: i32, y: i32, offset: (i32, i32), image: u32) {
        let p = self.point(x, y);
        let (ax, ay) = self.r.record(image).map_or((0, 0), |rec| (rec.sprite_offset_x as i32, rec.sprite_offset_y as i32));
        out.push(Overlay { x, y, pos: [p[0] + (offset.0 - ax) as f32, p[1] + (offset.1 - ay) as f32], image });
    }

    /// The current frame of `a`, a looping animation (0-based), or `None` if it has
    /// no frames. `phase` keeps neighbouring buildings out of step.
    fn frame(&self, a: &Anim, frames: u32, phase: u64) -> Option<u32> {
        (frames > 0).then(|| ((self.ticks + phase) / a.duration.max(1) as u64 % frames as u64) as u32)
    }

    fn anim(&self, b: &Building, key: &str) -> Option<&Anim> {
        self.world.defs.building(b.kind)?.anims.get(key)
    }
}

/// Appends every building animation to `out`.
pub fn building_animations(cx: &AnimContext, out: &mut Vec<Overlay>) {
    for b in cx.world.buildings.iter() {
        if b.is_house() || cx.world.is_farm(b.kind) {
            continue;
        }
        if cx.world.is_road_venue(b.kind) {
            venue(cx, b, out);
            continue;
        }
        if b.kind == kind::GRANARY {
            granary(cx, b, out);
            continue;
        }
        if b.kind == kind::STORAGE_YARD {
            storage_yard(cx, b, out);
            continue;
        }
        if b.monument.is_some() {
            monument(cx, b, out);
            continue;
        }
        let active = if b.kind == kind::BURNING_RUIN { b.progress > 0 } else { b.kind == kind::WELL || b.workers > 0 };
        if active {
            working(cx, b, out);
        }
    }
}

/// A building at work: its "work" animation at a set offset, or else the frames
/// stored after its own image, drawn where that image says.
fn working(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    let (dx, dy) = (b.x, b.y + b.size - 1);
    let phase = b.id as u64 * 7;
    // An offset of (0, 0) or (-1, -1) means the image's own frames are used.
    let work = cx.anim(b, "work").filter(|a| a.frames > 1 && !matches!((a.x, a.y), (0, 0) | (-1, -1)));
    if let Some(a) = work {
        if let Some(f) = cx.frame(a, a.frames, phase) {
            cx.sprite(out, dx, dy, (a.x, a.y), a.image + f);
        }
        return;
    }
    let base = cx.world.map.images.at_or(dx, dy, 0);
    let Some(rec) = cx.r.record(base) else { return };
    let n = rec.num_animation_sprites as u64;
    if n == 0 {
        return;
    }
    let step = cx.millis / (20 * (rec.animation_speed_id as u64).max(1)) + phase;
    let frame = if rec.animation_can_reverse {
        let k = step % (2 * n);
        if k < n { k + 1 } else { 2 * n - k }
    } else {
        step % n + 1
    };
    let p = cx.point(dx, dy);
    let tiles = if rec.kind == osiris_formats::ImageKind::Isometric { rec.isometric_tiles().max(1) } else { 1 };
    let y = p[1] + rec.sprite_offset_y as f32 - rec.height as f32 + city_view::TILE_H / 2.0 * (tiles + 1) as f32;
    out.push(Overlay { x: dx, y: dy, pos: [p[0] + rec.sprite_offset_x as f32, y], image: base + frame as u32 });
}

/// A pyramid's upper rings: each block's courses above the ground, raised by the
/// rings beneath.
fn monument(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    for (bx, by, image, lift) in cx.world.monument_stacks(b.id) {
        if lift == 0 {
            continue;
        }
        let Some(rec) = cx.r.record(image) else { continue };
        let (dx, dy) = (bx, by + 1);
        let p = cx.point(dx, dy);
        let tiles = rec.isometric_tiles().max(1);
        let y = p[1] + rec.sprite_offset_y as f32 - rec.height as f32 + city_view::TILE_H / 2.0 * (tiles + 1) as f32 - lift as f32;
        out.push(Overlay { x: dx, y: dy, pos: [p[0] + rec.sprite_offset_x as f32, y], image });
    }
}

/// The granary: a heap of food per 400 units stored in its eight spots, and its
/// scribes at work. (Its image's own frames are the heaps, not an animation.)
fn granary(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    let (dx, dy) = (b.x, b.y + b.size - 1);
    if let Some(heaps) = cx.anim(b, "resources") {
        let mut spot = 0;
        for (r, &amount) in b.stock.iter().enumerate().skip(1) {
            if amount <= 0 {
                continue;
            }
            let filled = ((amount - 199) as f32 / GRANARY_SPOT_UNITS as f32).ceil().max(1.0) as usize;
            for _ in 0..filled {
                let Some(&(sx, sy)) = GRANARY_SPOTS.get(spot) else { break };
                let p = cx.point(dx, dy);
                let pos = [p[0] + (GRANARY_FIRST_SPOT.0 + sx) as f32, p[1] + (GRANARY_FIRST_SPOT.1 + sy) as f32];
                out.push(Overlay { x: dx, y: dy, pos, image: heaps.image + r as u32 });
                spot += 1;
            }
        }
    }
    if b.workers <= 0 {
        return;
    }
    let Some(a) = cx.anim(b, "work") else { return };
    let needed = cx.world.workers_needed(b.kind);
    let scribes = if b.workers * 2 > needed { 2 } else { 1 };
    for (i, &at) in GRANARY_SCRIBES.iter().take(scribes).enumerate() {
        if let Some(f) = cx.frame(a, a.frames, b.id as u64 * 7 + i as u64 * 5) {
            cx.sprite(out, dx, dy, at, a.image + 1 + f);
        }
    }
}

/// The storage yard's hut: its roof always, and its clerk at work when staffed. Both
/// are drawn from the hut's tile in the yard's north corner.
fn storage_yard(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    if let Some(cover) = cx.anim(b, "cover") {
        let p = cx.point(b.x, b.y);
        out.push(Overlay { x: b.x, y: b.y, pos: [p[0] + cover.x as f32, p[1] + cover.y as f32], image: cover.image });
    }
    if b.workers > 0
        && let Some(a) = cx.anim(b, "work")
        && let Some(f) = cx.frame(a, a.frames, b.id as u64 * 7)
    {
        cx.sprite(out, b.x, b.y, (a.x, a.y), a.image + f);
    }
}

/// A venue's performers, each on its own piece of the venue while it has shows:
/// jugglers at the stall, musicians on the stage, dancers on the pavilion floor.
fn venue(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    if b.workers <= 0 {
        return;
    }
    let defs = &cx.world.defs;
    let image = |k: u16, key: &str| defs.building(k).and_then(|d| if key.is_empty() { Some(d.image) } else { d.anims.get(key).map(|a| a.image) });
    let stall = image(kind::BOOTH, "");
    let floor = image(kind::PAVILION, "");
    let stage = image(kind::BANDSTAND, "stand_sn_s");
    let map = &cx.world.map;
    let phase = b.id as u64 * 7;
    for (x, y) in b.tiles() {
        if map.edges.at_or(x, y, 0) & edge::DRAW_TILE == 0 {
            continue;
        }
        let img = Some(map.images.at_or(x, y, 0));
        let key = if img == stall && b.shows[0] > 0 {
            "juggler"
        } else if img == floor && b.shows[2] > 0 {
            "dancer"
        } else if b.shows[1] > 0 && stage.is_some() {
            let off = img.zip(stage).map(|(i, s)| i.wrapping_sub(s));
            // The original plays its musicians on the stage halves facing the crowd.
            match (b.kind, off) {
                (kind::BANDSTAND, Some(1)) => "musician_we",
                (kind::BANDSTAND, Some(3)) => "musician_sn",
                (kind::PAVILION, Some(0)) => "musician_sn",
                (kind::PAVILION, Some(2)) => "musician_we",
                _ => continue,
            }
        } else {
            continue;
        };
        let Some(a) = cx.anim(b, key) else { continue };
        if a.frames > 0 {
            if let Some(f) = cx.frame(a, a.frames, phase) {
                cx.sprite(out, x, y, (a.x, a.y), a.image + f);
            }
        } else {
            // Booth jugglers take their frame count from the stall's image and
            // number their frames from one.
            let n = img.and_then(|i| cx.r.record(i)).map_or(0, |rec| rec.num_animation_sprites as u32);
            let slow = Anim { duration: 2, ..*a };
            if let Some(f) = cx.frame(&slow, n, phase) {
                cx.sprite(out, x, y, (a.x, a.y), a.image + 1 + f);
            }
        }
    }
}

/// Carts and sleds pushed by cart pushers, from SprMain.
#[derive(Clone, Copy)]
pub struct CartImages {
    /// The empty cart; other goods without their own cart follow it in blocks of 24.
    empty: u32,
    /// Resources with their own cart group: (resource, first image).
    carts: [(u16, u32); 9],
    /// Resources pulled on sleds: (resource, first image).
    sleds: [(u16, u32); 5],
}

/// Where the cart sits relative to its pusher, by direction.
const CART_OFFSETS: [(i32, i32); 8] = [(17, -7), (22, -1), (17, 7), (0, 11), (-17, 6), (-22, -1), (-17, -7), (0, -12)];
const SLED_OFFSETS: [(i32, i32); 8] = [(-17, 9), (22, -1), (-15, -5), (0, 11), (12, -2), (-22, -1), (17, 7), (0, -12)];

impl CartImages {
    pub fn load(lib: &osiris_formats::ImageLibrary) -> osiris_formats::Result<Self> {
        let g = |group: usize| lib.group_id("SprMain", group, 0);
        // Barley, copper, beer, papyrus, reeds, gold, gems, flax, timber.
        let carts = [(14, 91), (29, 107), (15, 92), (23, 100), (22, 99), (21, 98), (18, 95), (16, 93), (20, 97)];
        // Stone, limestone, granite, sandstone, bricks.
        let sleds = [(24, 101), (25, 104), (26, 103), (30, 101), (12, 89)];
        let mut out = Self { empty: g(77)?, carts: [(0, 0); 9], sleds: [(0, 0); 5] };
        for (i, (r, group)) in carts.into_iter().enumerate() {
            out.carts[i] = (r, g(group)?);
        }
        for (i, (r, group)) in sleds.into_iter().enumerate() {
            out.sleds[i] = (r, g(group)?);
        }
        Ok(out)
    }

    /// The cart image for `amount` of resource `r` facing `dir`, and its offset from
    /// the pusher's foot.
    pub fn cart(&self, r: u16, amount: i32, dir: u8) -> (u32, (i32, i32)) {
        let d = dir as usize % 8;
        let tier = (amount > 100) as u32 + (amount > 200) as u32;
        if let Some(&(_, base)) = self.sleds.iter().find(|s| s.0 == r) {
            let image = if amount > 0 { base } else { self.empty };
            return (image + d as u32, SLED_OFFSETS[d]);
        }
        let image = match self.carts.iter().find(|c| c.0 == r) {
            _ if amount <= 0 || r == 0 => self.empty,
            Some(&(_, base)) => base + 8 * tier,
            None => self.empty + 8 + 24 * (r as u32 - 1) + 8 * tier,
        };
        (image + d as u32, CART_OFFSETS[d])
    }
}
