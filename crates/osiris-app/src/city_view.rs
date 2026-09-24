//! Isometric city drawing and picking.
//!
//! Map tile `(x, y)` has its diamond's bounding box at world pixel
//! `((x - y) * 30 + origin, (x + y) * 15)`. A tile flagged as a draw tile holds the image
//! of a whole footprint of `n x n` tiles; the image's bottom edge sits at
//! `tile_y + 15 * (n + 1)`, which is the bottom corner of the footprint.

use osiris_formats::ImageKind;
use osiris_render::{Paint, Renderer, Space, WHITE};
use osiris_sim::map::{Map, edge};
use osiris_sim::placement::GhostImage;

pub const TILE_W: f32 = 60.0;
pub const TILE_H: f32 = 30.0;

/// The original's placement colours (5-6-5): a ghost, and ground where a building
/// may go, keep only green; ground where it may not keeps only red.
pub const PLACE_OK: u16 = 0x1fe3;
pub const PLACE_BAD: u16 = 0xf863;

#[derive(Default)]
pub struct CityView {
    pub last_sprites: usize,
    /// World-pixel bounds of the playable area, found on first use.
    bounds: Option<[f32; 4]>,
}

/// World-pixel bounding box of the tiles that are part of the map (tiles outside the
/// playable area have no image).
pub fn map_bounds(map: &Map) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for y in 0..map.height {
        for x in 0..map.width {
            if map.images.at_or(x, y, 0) == 0 {
                continue;
            }
            let p = tile_to_world(map, x, y);
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0] + TILE_W), b[3].max(p[1] + TILE_H)];
        }
    }
    if b[0] > b[2] { [0.0, 0.0, 1.0, 1.0] } else { b }
}

/// World pixel of the top-left corner of tile `(x, y)`'s bounding box.
pub fn tile_to_world(map: &Map, x: i32, y: i32) -> [f32; 2] {
    let origin = (map.height - 1) as f32 * TILE_W / 2.0;
    [(x - y) as f32 * TILE_W / 2.0 + origin, (x + y) as f32 * TILE_H / 2.0]
}

/// The tile whose diamond contains world pixel `p`.
pub fn world_to_tile(map: &Map, p: [f32; 2]) -> Option<(i32, i32)> {
    let origin = (map.height - 1) as f32 * TILE_W / 2.0;
    let a = (p[0] - origin - TILE_W / 2.0) / (TILE_W / 2.0); // x - y
    let b = (p[1] - TILE_H / 2.0) / (TILE_H / 2.0); // x + y
    let x = ((a + b) / 2.0).round() as i32;
    let y = ((b - a) / 2.0).round() as i32;
    map.contains(x, y).then_some((x, y))
}

pub fn zoom_at(r: &mut Renderer, screen: [f32; 2], factor: f32) {
    let before = r.screen_to_world(screen);
    r.camera.zoom = (r.camera.zoom * factor).clamp(0.25, 4.0);
    let after = r.screen_to_world(screen);
    r.camera.x += before[0] - after[0];
    r.camera.y += before[1] - after[1];
}

/// A sprite standing on a tile (walkers), drawn after that tile's diagonal.
#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    /// Drawn before the buildings of its diagonal rather than after them (a figure
    /// over a pyramid's far face, which the pyramid hides).
    pub behind: bool,
    pub x: i32,
    pub y: i32,
    /// Offset from the tile's box top-left to the figure's foot point.
    pub offset: (i32, i32),
    pub image: u32,
}

/// Draws a walker sprite with its foot at world pixel `foot`, honouring the image's
/// sprite offsets (taken from the source image for mirrored frames).
pub fn draw_sprite(r: &mut Renderer, image: u32, foot: [f32; 2]) {
    let Some(rec) = r.record(image).cloned() else { return };
    let (mut x, y);
    if rec.mirror_offset != 0 {
        let src = (image as i64 + rec.mirror_offset as i64) as u32;
        let Some(s) = r.record(src).cloned() else { return };
        x = foot[0] - (s.width as f32 - s.sprite_offset_x as f32);
        y = foot[1] - s.sprite_offset_y as f32;
    } else {
        x = foot[0] - rec.sprite_offset_x as f32;
        y = foot[1] - rec.sprite_offset_y as f32;
    }
    x = x.round();
    r.image(image, [x, y.round()], WHITE, Space::World);
}

/// An extra image drawn over a building (crops, working animations), placed at a
/// world pixel and sorted with tile `(x, y)`.
#[derive(Debug, Clone, Copy)]
pub struct Overlay {
    pub x: i32,
    pub y: i32,
    pub pos: [f32; 2],
    pub image: u32,
}

/// An overlay column over a house, at the house's draw tile.
#[derive(Debug, Clone, Copy)]
pub struct ColumnMark {
    pub x: i32,
    pub y: i32,
    /// First image of the column's colour (capital; shaft +1, base +2).
    pub image: u32,
    pub height: i32,
}

/// How the active overlay wants tiles drawn.
pub struct OverlayDraw<'a> {
    pub look: &'a dyn Fn(i32, i32) -> crate::overlay::TileLook,
    /// First flattened-footprint image.
    pub flat: u32,
    pub columns: Vec<ColumnMark>,
}

fn draw_column(r: &mut Renderer, c: &ColumnMark, p: [f32; 2]) {
    let cap_h = r.record(c.image).map_or(0.0, |rec| rec.height as f32);
    r.image(c.image + 2, [p[0] + 9.0, p[1] - 8.0], WHITE, Space::World);
    for i in 1..c.height {
        r.image(c.image + 1, [p[0] + 17.0, p[1] - 8.0 - 10.0 * i as f32 + 13.0], WHITE, Space::World);
    }
    if c.height > 0 {
        r.image(c.image, [p[0] + 5.0, p[1] - 8.0 - cap_h - 10.0 * (c.height - 1) as f32 + 13.0], WHITE, Space::World);
    }
}

/// A per-tile highlight drawn over the city.
#[derive(Debug, Clone, Copy)]
pub struct Highlight {
    pub x: i32,
    pub y: i32,
    pub color: [f32; 4],
    pub paint: Paint,
}

/// Where the map draws image `id` from draw tile `(x, y)`: the image's bottom edge
/// on the bottom corner of its footprint.
fn footprint_pos(r: &Renderer, map: &Map, x: i32, y: i32, id: u32) -> Option<[f32; 2]> {
    let rec = r.record(id)?;
    let n = if rec.kind == ImageKind::Isometric { rec.isometric_tiles().max(1) } else { 1 };
    let p = tile_to_world(map, x, y);
    Some([p[0], p[1] + TILE_H / 2.0 * (n + 1) as f32 - rec.height as f32])
}

impl CityView {
    /// Keeps the camera over the map. `visible_w` is the screen width left of the
    /// sidebar; `top` is the height of the bar above the view.
    pub fn clamp_camera(&mut self, r: &mut Renderer, map: &Map, visible_w: f32, top: f32) {
        let [x0, y0, x1, y1] = *self.bounds.get_or_insert_with(|| map_bounds(map));
        // Stay a tile inside the ragged edge of the map.
        let (x0, y0, x1, y1) = (x0 + TILE_W / 2.0, y0 + TILE_H * 1.5, x1 - TILE_W / 2.0, y1 - TILE_H);
        let z = r.camera.zoom;
        let (vw, vh) = (visible_w / z, (r.screen[1] - top) / z);
        let clamp = |v: f32, lo: f32, hi: f32, span: f32| {
            if hi - lo <= span { (lo + hi - span) / 2.0 } else { v.clamp(lo, hi - span) }
        };
        r.camera.x = clamp(r.camera.x, x0, x1, vw);
        // The view starts below the top bar.
        r.camera.y = clamp(r.camera.y + top / z, y0, y1, vh) - top / z;
    }

    pub fn center_on(&mut self, r: &mut Renderer, map: &Map, x: i32, y: i32) {
        let c = tile_to_world(map, x, y);
        r.camera.x = c[0] - r.screen[0] / 2.0 / r.camera.zoom;
        r.camera.y = c[1] - r.screen[1] / 2.0 / r.camera.zoom;
    }

    /// Draws tile `(x, y)`'s image if it belongs to this pass: flat ground (and the
    /// flattened overlay footprints) in the first, anything taller in the second.
    #[allow(clippy::too_many_arguments)]
    fn draw_tile(&self, r: &mut Renderer, map: &Map, x: i32, y: i32, overlay: Option<&OverlayDraw>, flat_pass: bool, [vx0, vy0, vx1, vy1]: [f32; 4]) {
        if map.edges.at_or(x, y, 0) & edge::DRAW_TILE == 0 {
            return;
        }
        let mut id = map.images.at_or(x, y, 0);
        if id == 0 {
            return;
        }
        if let Some(o) = overlay {
            match (o.look)(x, y) {
                crate::overlay::TileLook::Normal => {}
                crate::overlay::TileLook::Ground(image) => id = image,
                crate::overlay::TileLook::Flat => {
                    if !flat_pass {
                        return;
                    }
                    let n = r.record(id).filter(|rec| rec.kind == ImageKind::Isometric).map_or(1, |rec| rec.isometric_tiles().max(1));
                    let (ox, oy) = (x, y - (n - 1));
                    for dy in 0..n {
                        for dx in 0..n {
                            let shape = if dx == 0 {
                                if dy == 0 { 0 } else { 1 }
                            } else if dy == 0 {
                                2
                            } else {
                                3
                            };
                            let p = tile_to_world(map, ox + dx, oy + dy);
                            r.image(o.flat + shape, p, WHITE, Space::World);
                        }
                    }
                    return;
                }
            }
        }
        let Some(rec) = r.record(id) else { return };
        let n = if rec.kind == ImageKind::Isometric { rec.isometric_tiles().max(1) } else { 1 };
        let (iw, ih) = (rec.width as f32, rec.height as f32);
        let flat = ih <= TILE_H * n as f32;
        if flat != flat_pass {
            return;
        }
        let Some(pos) = footprint_pos(r, map, x, y, id) else { return };
        if pos[0] > vx1 || pos[1] > vy1 || pos[0] + iw < vx0 || pos[1] + ih < vy0 {
            return;
        }
        r.image(id, pos, WHITE, Space::World);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        r: &mut Renderer,
        map: &Map,
        highlights: &[Highlight],
        ghost: &[GhostImage],
        highlight_image: u32,
        sprites: &[Sprite],
        overlays: &[Overlay],
        overlay: Option<&OverlayDraw>,
    ) {
        let before = r.instance_count();
        let [vx0, vy0, vx1, vy1] = r.world_view();
        let (w, h) = (map.width, map.height);
        let (mut hidden, mut people): (Vec<Sprite>, Vec<Sprite>) = sprites.iter().partition(|s| s.behind);
        people.sort_by_key(|s| (s.x + s.y, s.x));
        hidden.sort_by_key(|s| (s.x + s.y, s.x));
        let mut next_person = 0;
        let mut next_hidden = 0;
        let mut extras = overlays.to_vec();
        extras.sort_by_key(|o| (o.x + o.y, o.x));
        let mut next_extra = 0;
        let mut columns = overlay.map(|o| o.columns.clone()).unwrap_or_default();
        columns.sort_by_key(|c| (c.x + c.y, c.x));
        let mut next_column = 0;
        // Flat ground first, then everything that stands up, back to front: a tall
        // building must not be painted over by the flat tiles behind it. Within a
        // diagonal, screen x runs the other way from map x, so the tile with the
        // smaller map x sits in front on screen and is drawn last: a tall sprite's
        // west edge (map x - 1, same diagonal) must be painted over it, not under it.
        for d in 0..(w + h - 1) {
            let x_min = (d - (h - 1)).max(0);
            let x_max = d.min(w - 1);
            for x in (x_min..=x_max).rev() {
                self.draw_tile(r, map, x, d - x, overlay, true, [vx0, vy0, vx1, vy1]);
            }
        }
        for d in 0..(w + h - 1) {
            let x_min = (d - (h - 1)).max(0);
            let x_max = d.min(w - 1);
            while next_hidden < hidden.len() && hidden[next_hidden].x + hidden[next_hidden].y <= d {
                let s = hidden[next_hidden];
                let p = tile_to_world(map, s.x, s.y);
                draw_sprite(r, s.image, [p[0] + s.offset.0 as f32 + 29.0, p[1] + s.offset.1 as f32 + 23.0]);
                next_hidden += 1;
            }
            for x in (x_min..=x_max).rev() {
                self.draw_tile(r, map, x, d - x, overlay, false, [vx0, vy0, vx1, vy1]);
            }
            while next_column < columns.len() && columns[next_column].x + columns[next_column].y <= d {
                let c = columns[next_column];
                let p = tile_to_world(map, c.x, c.y);
                draw_column(r, &c, p);
                next_column += 1;
            }
            while next_extra < extras.len() && extras[next_extra].x + extras[next_extra].y <= d {
                let o = extras[next_extra];
                r.image(o.image, o.pos, WHITE, Space::World);
                next_extra += 1;
            }
            while next_person < people.len() && people[next_person].x + people[next_person].y <= d {
                let s = people[next_person];
                let p = tile_to_world(map, s.x, s.y);
                let foot = [p[0] + s.offset.0 as f32 + 29.0, p[1] + s.offset.1 as f32 + 23.0];
                draw_sprite(r, s.image, foot);
                next_person += 1;
            }
        }
        // A placement ghost and the highlighted tiles go over the whole city, as the
        // original draws them after its buildings: nothing in front cuts into them.
        let mut ghost = ghost.to_vec();
        ghost.sort_by_key(|g| (g.x + g.y, -g.x));
        for g in ghost {
            if let Some(pos) = footprint_pos(r, map, g.x, g.y, g.image) {
                r.image_painted(g.image, pos, WHITE, Space::World, Paint::Masked(PLACE_OK));
            }
        }
        for m in highlights {
            r.image_painted(highlight_image, tile_to_world(map, m.x, m.y), m.color, Space::World, m.paint);
        }
        self.last_sprites = r.instance_count() - before;
    }
}
