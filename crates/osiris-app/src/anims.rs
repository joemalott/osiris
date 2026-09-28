//! Animations drawn over buildings: staff at work, farmers among their crops,
//! performers at venues, and the grain heaps in granaries.
//!
//! The original's building pass (FUN_00436b60) animates a building only when the
//! image on its draw tile has frames of its own (`num_animation_sprites`), and runs
//! that many frames. Most buildings show the frames stored after that image, placed
//! by the image's record: `sprite_offset` from the image's top-left corner. A few
//! types show frames from SprAmbient instead, at a spot measured from the draw
//! tile less each frame's own anchor; those spots are the original's, kept in the
//! tables below or in the building's anims.

use crate::city_view::{self, Overlay};
use osiris_formats::ImageRecord;
use osiris_render::Renderer;
use osiris_sim::World;
use osiris_sim::buildings::{Building, kind};
use osiris_sim::defs::Anim;
use osiris_sim::map::edge;

/// Where the granary's eight heaps of food sit, from its draw tile (FUN_00442280).
const GRANARY_SPOTS: [(i32, i32); 8] = [(110, -73), (126, -64), (145, -55), (161, -47), (94, -66), (111, -57), (130, -47), (147, -38)];
/// Stored units that fill one heap.
const GRANARY_SPOT_UNITS: i32 = 400;
/// Where a fishing wharf's staff stand, by the wharf's facing (FUN_00438350).
const WHARF_SPOTS: [(i32, i32); 4] = [(75, 10), (78, 7), (62, 18), (48, 8)];
/// Where a dock's dockers stand, by facing: waiting, then unloading a ship.
const DOCK_SPOTS: [[(i32, i32); 4]; 2] = [[(132, -7), (115, 25), (60, 20), (49, -4)], [(152, -13), (123, 16), (65, 14), (38, -12)]];
/// Unloading dockers have 20 frames, however many the dock's image has.
const DOCK_UNLOAD_FRAMES: u32 = 20;
/// Where the boat on a shipwright's stocks sits, by facing (FUN_00438210).
const SHIPWRIGHT_SPOTS: [(i32, i32); 4] = [(110, 20), (85, 20), (85, 20), (100, 10)];
/// Where a water lift's shaduf stands, by facing.
const LIFT_SPOTS: [(i32, i32); 4] = [(54, 15), (53, 13), (62, 15), (65, 21)];

/// The anims of buildings the original draws with frames from another group at a
/// set spot rather than their own image's frames: quarries (two workers), mines,
/// and the conservatory's and dance school's pupils.
fn ambient_keys(k: u16) -> &'static [&'static str] {
    match k {
        // Plain stone, limestone, granite and sandstone quarries.
        106 | 107 | 216 | 221 => &["work", "work_2"],
        // Gold, gemstone and copper mines.
        161 | 162 | 217 => &["work"],
        kind::CONSERVATORY | kind::DANCE_SCHOOL => &["work"],
        _ => &[],
    }
}

pub struct AnimContext<'a> {
    pub world: &'a World,
    pub r: &'a Renderer,
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

    /// The frame (from 1) of `rec`'s own animation showing now, at its own speed and
    /// back and forth if it reverses, or `None` if the image has no frames. `phase`
    /// keeps neighbouring buildings out of step.
    fn native_frame(&self, rec: &ImageRecord, phase: u64) -> Option<u32> {
        let n = rec.num_animation_sprites as u64;
        if n == 0 {
            return None;
        }
        let step = self.millis / (20 * (rec.animation_speed_id as u64).max(1)) + phase;
        let frame = if rec.animation_can_reverse {
            let k = step % (2 * n);
            if k < n { k + 1 } else { 2 * n - k }
        } else {
            step % n + 1
        };
        Some(frame as u32)
    }

    /// The current frame of the image on tile `(x, y)`, as `native_frame`.
    fn tile_frame(&self, x: i32, y: i32, phase: u64) -> Option<u32> {
        self.native_frame(self.r.record(self.world.map.images.at_or(x, y, 0))?, phase)
    }

    fn anim(&self, b: &Building, key: &str) -> Option<&Anim> {
        self.world.defs.building(b.kind)?.anims.get(key)
    }

    /// Which of its four facings `b`'s image on tile `(x, y)` is.
    fn facing(&self, b: &Building, x: i32, y: i32) -> usize {
        let first = self.world.defs.building(b.kind).map_or(0, |d| d.image);
        (self.world.map.images.at_or(x, y, 0).wrapping_sub(first) & 3) as usize
    }
}

/// Appends every building animation to `out`.
pub fn building_animations(cx: &AnimContext, out: &mut Vec<Overlay>) {
    for b in cx.world.buildings.iter() {
        if b.is_house() {
            continue;
        }
        if cx.world.is_farm(b.kind) {
            farm(cx, b, out);
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
        if osiris_sim::military::fort_soldier(b.kind).is_some() {
            // The fort's emblem: which kind of company it holds. A patch of the
            // south-east wall carrying the relief, placed from the fort's leftmost
            // (draw) tile as the original places it.
            if let Some(a) = cx.anim(b, "picture") {
                let p = cx.point(b.x, b.y + b.size - 1);
                out.push(Overlay { x: b.x, y: b.y + b.size - 1, pos: [p[0] + a.x as f32, p[1] + a.y as f32], image: a.image });
            }
            continue;
        }
        if osiris_sim::water::is_shore_building(b.kind) {
            stock_piles(cx, b, out);
            shore(cx, b, out);
            continue;
        }
        if b.kind == osiris_sim::irrigation::WATER_LIFT {
            water_lift(cx, b, out);
            continue;
        }
        stock_piles(cx, b, out);
        let active = if b.kind == kind::BURNING_RUIN { b.progress > 0 } else { b.kind == kind::WELL || (b.workers > 0 && has_materials(cx.world, b)) };
        if active {
            working(cx, b, out);
        }
    }
}

/// A farm's crops, and a floodplain farm's farmer among them. Floodplain farms crop
/// every tile, meadow farms only the front edge. The farmer's frame is the farm's own
/// animation (its farmland image has eleven frames, which the original steps on the
/// draw tile), his image that frame's row of eight facings in his work's group, and
/// he stands over the crop of his tile, drawn just after it, at the tile's middle
/// less his anchor, 40 pixels down from the top of that crop (FUN_00488840).
fn farm(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    let crops = cx.world.crop_overlays(b.id);
    let n = crops.len();
    let worker = cx.world.farm_worker(b.id).and_then(|w| {
        let frames = cx.world.farm_image(b.id).and_then(|i| cx.r.record(i)).and_then(|rec| cx.native_frame(rec, b.id as u64 * 7))?;
        let first = cx.anim(b, w.work.anim_key())?.image;
        Some((w.tile, first + w.direction + 8 * (frames - 1)))
    });
    let top = city_view::tile_to_world(&cx.world.map, b.x, b.y);
    let point = [top[0] - (b.size - 1) as f32 * city_view::TILE_W / 2.0, top[1]];
    for (i, image) in crops {
        let (dx, dy) = if n == 9 { ((i % 3) as i32, (i / 3) as i32) } else { [(0, 2), (1, 2), (2, 2), (2, 1), (2, 0)][i] };
        let (ox, oy) = (((dx - dy) * 30 + (b.size - 1) * 30) as f32, ((dx + dy) * 15) as f32);
        let h = cx.r.record(image).map_or(30.0, |rec| rec.height as f32);
        let pos = [point[0] + ox, point[1] + oy + city_view::TILE_H - h];
        out.push(Overlay { x: b.x + dx, y: b.y + dy, pos, image });
        if let Some((tile, man)) = worker
            && tile == i
            && n == 9
        {
            let (ax, ay) = cx.r.record(man).map_or((0, 0), |rec| (rec.sprite_offset_x as i32, rec.sprite_offset_y as i32));
            out.push(Overlay { x: b.x + dx, y: b.y + dy, pos: [pos[0] + (30 - ax) as f32, pos[1] + (40 - ay) as f32], image: man });
        }
    }
}

/// Buildings the original animates only while they hold their raw material, as well
/// as staff (FUN_0040f2c0): the workshops, the guilds of carpenters, bricklayers and
/// artisans, the shipwright, the cattle ranch, the zoo, the senet house, the
/// mortuary, the scribal school and the library.
const NEEDS_MATERIAL: [u16; 20] = [32, 47, 51, 53, 74, 110, 111, 112, 113, 114, 177, 178, 194, 203, 204, 205, 226, 231, 232, 233];
/// Of those, the ones that need their second material on hand too: the brickworks,
/// the zoo and the lamp maker.
const NEEDS_BOTH: [u16; 3] = [204, 226, 232];

/// Whether `b` holds the materials its working animation needs: some of its first
/// input, and of its second for the few in `NEEDS_BOTH`. A shipwright laying down a
/// fishing boat needs no timber. A building with no input of its own (the
/// bricklayers, whose bricks go to the site by sled) never has any, as in the
/// original, where their stock stays empty.
fn has_materials(world: &World, b: &Building) -> bool {
    if !NEEDS_MATERIAL.contains(&b.kind) || (b.kind == kind::SHIPWRIGHT && b.boat_kind == osiris_sim::fishing::FISHING_BOAT) {
        return true;
    }
    let Some(def) = world.defs.building(b.kind) else { return true };
    let held = |i: usize| def.inputs.get(i).and_then(|r| world.resource_id(r)).is_some_and(|r| b.stock.get(r as usize).is_some_and(|&n| n > 0));
    held(0) && (!NEEDS_BOTH.contains(&b.kind) || held(1))
}

/// Which of a building's stores a pile shows.
#[derive(Clone, Copy)]
enum Store {
    /// Its first input.
    First,
    /// Its second input.
    Second,
    /// What it gathers (wood cutters, reed gatherers, the hunting lodge).
    Output,
}

/// A pile of goods drawn on a building (FUN_00436310): the store it shows, the
/// images (pack and group), the last image, and the spot from the draw tile.
struct Pile {
    store: Store,
    pack: &'static str,
    group: usize,
    most: i32,
    at: (i32, i32),
}

const fn pile(store: Store, pack: &'static str, group: usize, most: i32, x: i32, y: i32) -> Pile {
    Pile { store, pack, group, most, at: (x, y) }
}

const GENERAL: &str = "Pharaoh_General";
const EXPANSION: &str = "Expansion";
/// Where a shipwright's timber lies, by facing.
const SHIPWRIGHT_TIMBER: [(i32, i32); 4] = [(70, 25), (120, -25), (90, 10), (40, -5)];

/// The piles each building type shows. The shipwright's spot follows its facing
/// (`SHIPWRIGHT_TIMBER`); its entry's spot is unused.
const PILES: [(u16, Pile); 28] = {
    use Store::*;
    [
        (32, pile(First, GENERAL, 198, 2, 107, 5)),
        (47, pile(First, GENERAL, 199, 4, 49, 16)),
        (51, pile(First, GENERAL, 60, 2, 28, 15)),
        (53, pile(First, GENERAL, 60, 2, 61, -13)),
        (74, pile(First, GENERAL, 202, 4, 0, 0)),
        (95, pile(First, GENERAL, 201, 1, 131, 2)),
        (95, pile(Second, GENERAL, 209, 1, 74, 10)),
        (108, pile(Output, GENERAL, 202, 4, 65, 3)),
        (110, pile(First, GENERAL, 208, 2, 43, -1)),
        (111, pile(First, GENERAL, 210, 1, 41, 1)),
        (112, pile(First, GENERAL, 203, 1, 69, 19)),
        (113, pile(First, GENERAL, 200, 1, 63, -1)),
        (114, pile(First, GENERAL, 207, 1, 65, 20)),
        (115, pile(Output, GENERAL, 205, 4, 61, 13)),
        (177, pile(First, GENERAL, 202, 4, 41, 8)),
        (194, pile(First, GENERAL, 204, 1, 38, 7)),
        (195, pile(Output, GENERAL, 206, 4, 34, 17)),
        (203, pile(First, GENERAL, 206, 4, 35, 4)),
        (204, pile(Second, GENERAL, 204, 1, 91, 2)),
        (204, pile(First, GENERAL, 207, 1, 52, -17)),
        (205, pile(First, GENERAL, 202, 4, 166, -23)),
        (226, pile(First, GENERAL, 205, 2, 200, -10)),
        (226, pile(Second, GENERAL, 204, 2, 220, 10)),
        (231, pile(Second, GENERAL, 207, 4, 1, 7)),
        (231, pile(First, EXPANSION, 34, 4, 46, 12)),
        (232, pile(Second, EXPANSION, 28, 1, 50, 20)),
        (232, pile(First, EXPANSION, 29, 1, 15, 8)),
        (233, pile(First, EXPANSION, 30, 1, 45, 25)),
    ]
};

/// The goods a building holds, piled beside it: one image more per hundred units past
/// the first hundred (any at all shows the first), up to each pile's last. As in the
/// original the count runs on from the group's first image, so where a pile's last
/// goes past its group (the artisans' clay and paint) the next group's images show.
fn stock_piles(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    let mut list = PILES.iter().filter(|(k, _)| *k == b.kind).map(|(_, p)| p).peekable();
    if list.peek().is_none() {
        return;
    }
    let Some(def) = cx.world.defs.building(b.kind) else { return };
    let (dx, dy) = (b.x, b.y + b.size - 1);
    for p in list {
        let key = match p.store {
            Store::First => def.inputs.first(),
            Store::Second => def.inputs.get(1),
            Store::Output => def.outputs.first(),
        };
        let Some(r) = key.and_then(|k| cx.world.resource_id(k)) else { continue };
        let held = b.stock.get(r as usize).copied().unwrap_or(0);
        // The original's (held - 100) / 100 rounds toward zero, so nothing shows only
        // for an empty store.
        if held <= 0 {
            continue;
        }
        let n = ((held - 100) / 100).min(p.most);
        let Ok(first) = cx.r.library.group_id(p.pack, p.group, 0) else { continue };
        let at = if b.kind == kind::SHIPWRIGHT { SHIPWRIGHT_TIMBER[(b.orientation & 3) as usize] } else { p.at };
        cx.sprite(out, dx, dy, at, first + n as u32);
    }
}

/// A building at work: the frames stored after the image on its draw tile, placed
/// where that image's record says, or for the few types in `ambient_keys` their
/// SprAmbient frames at the original's spots.
fn working(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    let (dx, dy) = (b.x, b.y + b.size - 1);
    let base = cx.world.map.images.at_or(dx, dy, 0);
    let Some(rec) = cx.r.record(base) else { return };
    let Some(frame) = cx.native_frame(rec, b.id as u64 * 7) else { return };
    let keys = ambient_keys(b.kind);
    if !keys.is_empty() {
        for a in keys.iter().filter_map(|k| cx.anim(b, k)) {
            cx.sprite(out, dx, dy, (a.x, a.y), a.image + frame - 1);
        }
        return;
    }
    let p = cx.point(dx, dy);
    let tiles = if rec.kind == osiris_formats::ImageKind::Isometric { rec.isometric_tiles().max(1) } else { 1 };
    let y = p[1] + rec.sprite_offset_y as f32 - rec.height as f32 + city_view::TILE_H / 2.0 * (tiles + 1) as f32;
    out.push(Overlay { x: dx, y: dy, pos: [p[0] + rec.sprite_offset_x as f32, y], image: base + frame });
}

/// Buildings on the river. A fishing wharf shows its staff waiting, or unloading
/// while its boat is in; a dock shows its dockers, unloading while a ship is moored;
/// the shipwright shows the boat on its stocks. Wharf and dock frames come in fours,
/// one per facing, and each facing has its own spot. Transport and warship wharves
/// and the ferry have no frames in their images, so nothing moves on them.
fn shore(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    use osiris_sim::water::{DOCK, FISHING_WHARF, SHIPWRIGHT};
    if b.workers <= 0 {
        return;
    }
    let (dx, dy) = (b.x, b.y + b.size - 1);
    let Some(frame) = cx.tile_frame(dx, dy, b.id as u64 * 7) else { return };
    let facing = cx.facing(b, dx, dy);
    // The original's frame for facing i is the (i - 1)th of each four.
    let variant = (facing as u32 + 3) % 4;
    match b.kind {
        FISHING_WHARF => {
            let boat_in = cx.world.wharf_boat(b.id).and_then(|f| cx.world.figures.get(f)).is_some_and(|f| f.action == osiris_sim::fishing::action::AT_WHARF);
            if let Some(a) = cx.anim(b, if boat_in { "work" } else { "wait" }) {
                cx.sprite(out, dx, dy, WHARF_SPOTS[facing], a.image + 4 * (frame - 1) + variant);
            }
        }
        DOCK => {
            let unloading = cx.world.moored_ship(b.id).is_some();
            let (key, frame) = if unloading { ("unload", frame.min(DOCK_UNLOAD_FRAMES)) } else { ("work", frame) };
            if let Some(a) = cx.anim(b, key) {
                cx.sprite(out, dx, dy, DOCK_SPOTS[unloading as usize][facing], a.image + 4 * (frame - 1) + variant);
            }
        }
        SHIPWRIGHT if b.boat_kind != 0 && has_materials(cx.world, b) => {
            let key = match b.boat_kind {
                osiris_sim::fishing::FISHING_BOAT => "work_fishing_boat",
                osiris_sim::navy::WARSHIP => "work_warship",
                _ => "work_transport",
            };
            if let Some(a) = cx.anim(b, key) {
                cx.sprite(out, dx, dy, SHIPWRIGHT_SPOTS[facing], a.image + frame - 1);
            }
        }
        _ => {}
    }
}

/// A water lift's shaduf, swinging while the lift is staffed and has water: one
/// animation per facing.
fn water_lift(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    if !cx.world.lift_pumping(b) {
        return;
    }
    let (dx, dy) = (b.x, b.y + b.size - 1);
    let Some(frame) = cx.tile_frame(dx, dy, b.id as u64 * 7) else { return };
    let facing = cx.facing(b, dx, dy);
    if let Some(a) = cx.anim(b, ["work_n", "work_e", "work_s", "work_w"][facing]) {
        cx.sprite(out, dx, dy, LIFT_SPOTS[facing], a.image + frame - 1);
    }
}

/// A pyramid's upper rings: each block's courses above the ground, raised by the
/// rings beneath.
fn monument(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    // An obelisk under carving stands in its scaffolding.
    if let (Some(def), Some(m)) = (osiris_sim::monuments::monument_def(b.kind), &b.monument)
        && let osiris_sim::monuments::Style::Obelisk { size, .. } = def.style
    {
        if !m.finished && def.crew(m.phase).contains(&osiris_sim::monuments::STONEMASON)
            && let Some(ladder) = cx.anim(b, "ladder")
        {
            cx.sprite(out, b.x, b.y + size - 1, (20, -40), ladder.image);
        }
        return;
    }
    // Scaffolding about a sun temple's obelisk while it is shaped, placed from the
    // obelisk's left edge and the top of its footprint, in front of it.
    let (pieces, (ox, oy)) = cx.world.sun_temple_scaffold(b.id);
    if !pieces.is_empty() {
        let left = cx.point(ox, oy + 4)[0];
        let top = cx.point(ox, oy)[1];
        for (image, (dx, dy)) in pieces {
            out.push(Overlay { x: ox + 4, y: oy + 4, pos: [left + dx as f32, top + dy as f32], image });
        }
    }
    // The surveyor's stakes at the corners of a site not yet worked.
    for (x, y, image) in cx.world.monument_stakes(b.id) {
        let Some(rec) = cx.r.record(image) else { continue };
        let p = cx.point(x, y);
        out.push(Overlay { x, y, pos: [p[0], p[1] + city_view::TILE_H - rec.height as f32], image });
    }
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
    // The carpenters' ramps, over the blocks they stand on.
    for ((x, y), image, (dx, dy)) in cx.world.tomb_ramps(b.id) {
        let p = cx.point(x, y);
        out.push(Overlay { x, y, pos: [p[0] + dx as f32, p[1] + dy as f32], image });
    }
}

/// The granary: a heap of food in its eight spots per 400 units stored (the first
/// up to 600), and its scribe at work. (Its image's own frames are the heaps, and
/// their count is the scribe's.)
fn granary(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    let (dx, dy) = (b.x, b.y + b.size - 1);
    let p = cx.point(dx, dy);
    if let Some(heaps) = cx.anim(b, "resources") {
        let mut spot = 0;
        for (r, &amount) in b.stock.iter().enumerate().skip(1) {
            if amount <= 0 {
                continue;
            }
            let filled = ((amount - 200) as f32 / GRANARY_SPOT_UNITS as f32).ceil().max(1.0) as usize;
            for _ in 0..filled {
                let Some(&(sx, sy)) = GRANARY_SPOTS.get(spot) else { break };
                out.push(Overlay { x: dx, y: dy, pos: [p[0] + sx as f32, p[1] + sy as f32], image: heaps.image + r as u32 });
                spot += 1;
            }
        }
    }
    if b.workers <= 0 {
        return;
    }
    if let Some(a) = cx.anim(b, "work")
        && let Some(frame) = cx.tile_frame(dx, dy, b.id as u64 * 7)
    {
        cx.sprite(out, dx, dy, (a.x, a.y), a.image + frame - 1);
    }
}

/// The storage yard's hut: its clerk at work when staffed, then its roof over him.
/// Both are drawn from the hut's tile in the yard's north corner; the roof by its
/// top-left corner.
fn storage_yard(cx: &AnimContext, b: &Building, out: &mut Vec<Overlay>) {
    if b.workers > 0
        && let Some(a) = cx.anim(b, "work")
        && let Some(frame) = cx.tile_frame(b.x, b.y, b.id as u64 * 7)
    {
        cx.sprite(out, b.x, b.y, (a.x, a.y), a.image + frame - 1);
    }
    if let Some(cover) = cx.anim(b, "cover") {
        let p = cx.point(b.x, b.y);
        out.push(Overlay { x: b.x, y: b.y, pos: [p[0] + cover.x as f32, p[1] + cover.y as f32], image: cover.image });
    }
}

/// A venue's performers, each on its own piece of the venue while it has shows:
/// jugglers at the stall, dancers on the pavilion floor, and musicians on the stage
/// pieces that have frames (one kind on the south half of a north-south stage,
/// the other on the east half of a west-east one).
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
        let Some(frame) = cx.tile_frame(x, y, phase) else { continue };
        let img = Some(map.images.at_or(x, y, 0));
        let key = if img == stall {
            "juggler"
        } else if img == floor {
            "dancer"
        } else if img == stage {
            "musician_sn"
        } else if img.zip(stage).is_some_and(|(i, s)| i.wrapping_sub(s) < 4) {
            "musician_we"
        } else {
            continue;
        };
        let shows = match key {
            "juggler" => b.shows[0],
            "dancer" => b.shows[2],
            _ => b.shows[1],
        };
        if shows <= 0 {
            continue;
        }
        if let Some(a) = cx.anim(b, key) {
            cx.sprite(out, x, y, (a.x, a.y), a.image + frame - 1);
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
