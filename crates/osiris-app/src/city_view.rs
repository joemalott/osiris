//! Isometric city drawing and picking.
//!
//! Map tile `(x, y)` has its diamond's bounding box at world pixel
//! `((x - y) * 30 + origin, (x + y) * 15)`. A tile flagged as a draw tile holds the image
//! of a whole footprint of `n x n` tiles; the image's bottom edge sits at
//! `tile_y + 15 * (n + 1)`, which is the bottom corner of the footprint.

use osiris_formats::ImageKind;
use osiris_render::{Paint, Renderer, Space, WHITE};
use osiris_sim::map::{Map, edge};

pub const TILE_W: f32 = 60.0;
pub const TILE_H: f32 = 30.0;

#[derive(Default)]
pub struct CityView {
    pub last_sprites: usize,
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

/// A per-tile highlight drawn over the terrain.
#[derive(Debug, Clone, Copy)]
pub struct Highlight {
    pub x: i32,
    pub y: i32,
    pub color: [f32; 4],
}

impl CityView {
    pub fn center_on(&mut self, r: &mut Renderer, map: &Map, x: i32, y: i32) {
        let c = tile_to_world(map, x, y);
        r.camera.x = c[0] - r.screen[0] / 2.0 / r.camera.zoom;
        r.camera.y = c[1] - r.screen[1] / 2.0 / r.camera.zoom;
    }

    pub fn draw(
        &mut self,
        r: &mut Renderer,
        map: &Map,
        highlights: &[Highlight],
        highlight_image: u32,
        sprites: &[Sprite],
        overlays: &[Overlay],
    ) {
        let before = r.instance_count();
        let [vx0, vy0, vx1, vy1] = r.world_view();
        let (w, h) = (map.width, map.height);
        let mut marks = highlights.to_vec();
        marks.sort_by_key(|m| (m.x + m.y, m.x));
        let mut next_mark = 0;
        let mut people = sprites.to_vec();
        people.sort_by_key(|s| (s.x + s.y, s.x));
        let mut next_person = 0;
        let mut extras = overlays.to_vec();
        extras.sort_by_key(|o| (o.x + o.y, o.x));
        let mut next_extra = 0;
        // Back to front: by diagonal, then left to right within it.
        for d in 0..(w + h - 1) {
            let x_min = (d - (h - 1)).max(0);
            let x_max = d.min(w - 1);
            for x in x_min..=x_max {
                let y = d - x;
                if map.edges.at_or(x, y, 0) & edge::DRAW_TILE == 0 {
                    continue;
                }
                let id = map.images.at_or(x, y, 0);
                if id == 0 {
                    continue;
                }
                let Some(rec) = r.record(id) else { continue };
                let n = if rec.kind == ImageKind::Isometric {
                    rec.isometric_tiles().max(1)
                } else {
                    1
                };
                let (iw, ih) = (rec.width as f32, rec.height as f32);
                let p = tile_to_world(map, x, y);
                let pos = [p[0], p[1] + TILE_H / 2.0 * (n + 1) as f32 - ih];
                if pos[0] > vx1 || pos[1] > vy1 || pos[0] + iw < vx0 || pos[1] + ih < vy0 {
                    continue;
                }
                r.image(id, pos, WHITE, Space::World);
            }
            while next_extra < extras.len() && extras[next_extra].x + extras[next_extra].y <= d {
                let o = extras[next_extra];
                r.image(o.image, o.pos, WHITE, Space::World);
                next_extra += 1;
            }
            // Highlights on this diagonal go over the terrain drawn so far.
            while next_mark < marks.len() && marks[next_mark].x + marks[next_mark].y <= d {
                let m = marks[next_mark];
                let p = tile_to_world(map, m.x, m.y);
                r.image_painted(highlight_image, p, m.color, Space::World, Paint::Silhouette);
                next_mark += 1;
            }
            while next_person < people.len() && people[next_person].x + people[next_person].y <= d {
                let s = people[next_person];
                let p = tile_to_world(map, s.x, s.y);
                let foot = [p[0] + s.offset.0 as f32 + 29.0, p[1] + s.offset.1 as f32 + 23.0];
                draw_sprite(r, s.image, foot);
                next_person += 1;
            }
        }
        self.last_sprites = r.instance_count() - before;
    }
}
