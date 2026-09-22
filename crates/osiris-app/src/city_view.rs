//! Isometric city drawing.
//!
//! Map tile `(x, y)` has its diamond's bounding box at world pixel
//! `((x - y) * 30 + origin, (x + y) * 15)`. A tile flagged as a draw tile holds the image
//! of a whole footprint of `n x n` tiles; the image's bottom edge sits at
//! `tile_y + 15 * (n + 1)`, which is the bottom corner of the footprint.

use osiris_formats::{ImageKind, Scenario};
use osiris_render::{Renderer, Space, WHITE};

pub const TILE_W: f32 = 60.0;
pub const TILE_H: f32 = 30.0;
const DRAW_TILE: u8 = 0x40;

#[derive(Default)]
pub struct CityView {
    pub last_sprites: usize,
}

/// World pixel of the top-left corner of tile `(x, y)`'s bounding box.
pub fn tile_to_world(s: &Scenario, x: i32, y: i32) -> [f32; 2] {
    let origin = (s.info.height - 1) as f32 * TILE_W / 2.0;
    [
        (x - y) as f32 * TILE_W / 2.0 + origin,
        (x + y) as f32 * TILE_H / 2.0,
    ]
}

pub fn zoom_at(r: &mut Renderer, screen: [f32; 2], factor: f32) {
    let before = r.screen_to_world(screen);
    r.camera.zoom = (r.camera.zoom * factor).clamp(0.25, 4.0);
    let after = r.screen_to_world(screen);
    r.camera.x += before[0] - after[0];
    r.camera.y += before[1] - after[1];
}

impl CityView {
    pub fn center_camera(&mut self, r: &mut Renderer, s: &Scenario) {
        let c = tile_to_world(s, s.info.width / 2, s.info.height / 2);
        r.camera.x = c[0] - r.screen[0] / 2.0 / r.camera.zoom;
        r.camera.y = c[1] - r.screen[1] / 2.0 / r.camera.zoom;
    }

    pub fn draw(&mut self, r: &mut Renderer, s: &Scenario) {
        let before = r.instance_count();
        let [vx0, vy0, vx1, vy1] = r.world_view();
        let (w, h) = (s.info.width, s.info.height);
        // Back to front: by diagonal, then left to right within it.
        for d in 0..(w + h - 1) {
            let x_min = (d - (h - 1)).max(0);
            let x_max = d.min(w - 1);
            for x in x_min..=x_max {
                let y = d - x;
                let Some(off) = s.offset(x, y) else { continue };
                if s.edges[off] & DRAW_TILE == 0 {
                    continue;
                }
                let id = s.images[off];
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
                let p = tile_to_world(s, x, y);
                let pos = [p[0], p[1] + TILE_H / 2.0 * (n + 1) as f32 - ih];
                if pos[0] > vx1 || pos[1] > vy1 || pos[0] + iw < vx0 || pos[1] + ih < vy0 {
                    continue;
                }
                r.image(id, pos, WHITE, Space::World);
            }
        }
        self.last_sprites = r.instance_count() - before;
    }
}
