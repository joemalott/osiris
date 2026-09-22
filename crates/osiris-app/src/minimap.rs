//! City minimap for the sidebar's minimap window.
//!
//! This module is intentionally *not* wired into the app (see the task constraints on
//! `main.rs`/`game.rs`/`sidebar.rs`/`city_view.rs`). To integrate it:
//!
//! 1. Add `mod minimap;` to `osiris-app/src/main.rs`, and give `Game` a
//!    `minimap: minimap::Minimap` field, built once with `Minimap::new(&gfx.renderer)`
//!    (needs `ImageLibrary` access, via `Renderer::library`).
//! 2. Whenever the map changes shape — after a successful `World::apply` command, and
//!    periodically to catch simulation-driven terrain changes such as flooding or tree
//!    growth — call `game.minimap.mark_dirty()`. Rebuilding is skipped on frames where
//!    nothing changed.
//! 3. From `Game::draw_overlay` (or wherever `self.sidebar.draw(...)` is called), after
//!    drawing the sidebar, call:
//!    ```ignore
//!    let panel_left = r.screen[0] - 162.0; // sidebar::PANEL_W; not exported, keep in sync
//!    let is_house = |id| self.world.buildings.get(id).is_some_and(|b| b.house.is_some());
//!    self.minimap.draw(r, &self.world.map, is_house, panel_left);
//!    ```
//! 4. From the mouse-press handler, before falling through to the normal tile click:
//!    ```ignore
//!    let panel_left = gfx.renderer.screen[0] - 162.0;
//!    if game.minimap.contains(panel_left, game.cursor) {
//!        if let Some((x, y)) = game.minimap.pixel_to_tile(&game.world.map, panel_left, game.cursor) {
//!            game.view.center_on(&mut gfx.renderer, &game.world.map, x, y);
//!        }
//!        return; // absorbed by the minimap, not a normal map/sidebar click
//!    }
//!    ```
//!    `Sidebar::contains`/`Sidebar::click` would need a similar early-out for the
//!    minimap's rectangle so drags don't fall through to road/build tools; that's a
//!    one-line change in `sidebar.rs` for the lead dev to make alongside the `mod`
//!    line above.

use osiris_formats::ImageLibrary;
use osiris_render::{DynamicHandle, Renderer, Space};
use osiris_sim::map::{Map, terrain};

/// Pixel rectangle of the minimap window inside the sidebar panel art (`Pharaoh_General`
/// group 121 offset 0, a 162x450 image), measured from its transparent hole: x `8..154`,
/// y `30+15..30+125` (30 is the sidebar's top margin, `sidebar::TOP`) relative to the
/// panel's own left edge.
pub const X: f32 = 8.0;
pub const Y: f32 = 45.0;
pub const W: f32 = 146.0;
pub const H: f32 = 110.0;

/// Key passed to `Renderer::upload_dynamic`; arbitrary but stable so re-uploads reuse
/// the same texture instead of leaking a new one.
const TEXTURE_KEY: u32 = u32::from_be_bytes(*b"mmap");

const BUF_W: u32 = W as u32;
const BUF_H: u32 = H as u32;

/// Global image id of offset 0 of each `GROUP_MINIMAP_*` group in `Pharaoh_General`
/// (facts taken from the original's `image_groups.h`, sample colours only — no code
/// copied): 141 empty land, 142 water, 143 trees, 144 reeds/shrub, 145 rock, 146
/// meadow/floodplain, 147 road, 148 house, 149 other building, 150 wall, 151 aqueduct
/// (canal), 152 off-map black, 211 dunes.
mod group {
    pub const EMPTY_LAND: usize = 141;
    pub const WATER: usize = 142;
    pub const TREE: usize = 143;
    pub const SHRUB: usize = 144;
    pub const ROCK: usize = 145;
    pub const MEADOW: usize = 146;
    pub const ROAD: usize = 147;
    pub const HOUSE: usize = 148;
    pub const BUILDING: usize = 149;
    pub const WALL: usize = 150;
    pub const CANAL: usize = 151;
    pub const BLACK: usize = 152;
    pub const DUNE: usize = 211;
}

struct Colors {
    empty: [f32; 4],
    water: [f32; 4],
    tree: [f32; 4],
    shrub: [f32; 4],
    rock: [f32; 4],
    meadow: [f32; 4],
    road: [f32; 4],
    house: [f32; 4],
    building: [f32; 4],
    wall: [f32; 4],
    canal: [f32; 4],
    dune: [f32; 4],
    black: [f32; 4],
}

/// Average colour of `Pharaoh_General` group `group`'s first image (its pixels are
/// small, roughly solid-colour minimap tiles, so sampling once and reusing it is
/// enough), or `fallback` if the group/image can't be found or is fully transparent.
fn group_color(lib: &ImageLibrary, group: usize, fallback: [f32; 4]) -> [f32; 4] {
    let Ok(id) = lib.group_id("Pharaoh_General", group, 0) else { return fallback };
    let Some(pack_img) = lib.resolve(id) else { return fallback };
    let Ok(sprite) = lib.pack(pack_img.pack).sg3.decode(pack_img.index as usize) else {
        return fallback;
    };
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for px in &sprite.pixels {
        if px[3] > 0 {
            r += px[0] as u64;
            g += px[1] as u64;
            b += px[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        return fallback;
    }
    [(r / n) as f32 / 255.0, (g / n) as f32 / 255.0, (b / n) as f32 / 255.0, 1.0]
}

impl Colors {
    fn sample(lib: &ImageLibrary) -> Self {
        let black = [0.0, 0.0, 0.0, 1.0];
        Self {
            empty: group_color(lib, group::EMPTY_LAND, [0.55, 0.47, 0.33, 1.0]),
            water: group_color(lib, group::WATER, [0.15, 0.35, 0.55, 1.0]),
            tree: group_color(lib, group::TREE, [0.13, 0.35, 0.13, 1.0]),
            shrub: group_color(lib, group::SHRUB, [0.2, 0.45, 0.2, 1.0]),
            rock: group_color(lib, group::ROCK, [0.5, 0.5, 0.5, 1.0]),
            meadow: group_color(lib, group::MEADOW, [0.4, 0.6, 0.25, 1.0]),
            road: group_color(lib, group::ROAD, [0.6, 0.55, 0.45, 1.0]),
            house: group_color(lib, group::HOUSE, [0.75, 0.65, 0.4, 1.0]),
            building: group_color(lib, group::BUILDING, [0.7, 0.3, 0.3, 1.0]),
            wall: group_color(lib, group::WALL, [0.45, 0.45, 0.45, 1.0]),
            canal: group_color(lib, group::CANAL, [0.25, 0.45, 0.6, 1.0]),
            dune: group_color(lib, group::DUNE, [0.75, 0.65, 0.35, 1.0]),
            black: group_color(lib, group::BLACK, black),
        }
    }
}

/// A CPU-rendered top-down view of the city, uploaded as a dynamic texture and drawn
/// into the sidebar's minimap window.
pub struct Minimap {
    colors: Colors,
    buffer: Vec<u8>,
    handle: Option<DynamicHandle>,
    dirty: bool,
}

impl Minimap {
    pub fn new(r: &Renderer) -> Self {
        Self {
            colors: Colors::sample(&r.library),
            buffer: vec![0; (BUF_W * BUF_H * 4) as usize],
            handle: None,
            dirty: true,
        }
    }

    /// Marks the minimap for a rebuild on the next `draw` (cheap; the actual redraw of
    /// the CPU buffer only happens then, once).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// World-pixel bounding box of the whole map's tile grid, matching the anchor
    /// points `city_view::tile_to_world` would produce for tiles `(0, 0)..(width,
    /// height)`, padded by one tile so the last row/column's footprint fits.
    fn world_bounds(map: &Map) -> (f32, f32, f32, f32) {
        let tw = crate::city_view::TILE_W;
        let th = crate::city_view::TILE_H;
        let x1 = (map.width + map.height - 2) as f32 * tw / 2.0 + tw;
        let y1 = (map.width + map.height - 2) as f32 * th / 2.0 + th;
        (0.0, 0.0, x1, y1)
    }

    /// Scale (world pixels -> minimap pixels) and the margin that centres the map's
    /// bounding box inside the `W x H` window.
    fn transform(map: &Map) -> (f32, f32, f32) {
        let (x0, y0, x1, y1) = Self::world_bounds(map);
        let (bw, bh) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let scale = (W / bw).min(H / bh);
        let margin_x = (W - bw * scale) / 2.0;
        let margin_y = (H - bh * scale) / 2.0;
        (scale, margin_x, margin_y)
    }

    /// World pixel `w` to a local (window-relative) minimap pixel.
    fn to_local(map: &Map, w: [f32; 2]) -> [f32; 2] {
        let (scale, mx, my) = Self::transform(map);
        [w[0] * scale + mx, w[1] * scale + my]
    }

    /// A local (window-relative) minimap pixel back to world pixels.
    fn from_local(map: &Map, p: [f32; 2]) -> [f32; 2] {
        let (scale, mx, my) = Self::transform(map);
        [(p[0] - mx) / scale, (p[1] - my) / scale]
    }

    /// True if screen point `p` lands inside the minimap window, given `panel_left`
    /// (the sidebar panel's left edge in screen space, `screen_w - 162.0`).
    pub fn contains(&self, panel_left: f32, p: [f32; 2]) -> bool {
        let x = p[0] - (panel_left + X);
        let y = p[1] - Y;
        (0.0..W).contains(&x) && (0.0..H).contains(&y)
    }

    /// The map tile under screen point `p`, or `None` if it's outside the minimap or
    /// off the map.
    pub fn pixel_to_tile(&self, map: &Map, panel_left: f32, p: [f32; 2]) -> Option<(i32, i32)> {
        if !self.contains(panel_left, p) {
            return None;
        }
        let local = [p[0] - (panel_left + X), p[1] - Y];
        let world = Self::from_local(map, local);
        crate::city_view::world_to_tile(map, world)
    }

    fn tile_color(&self, map: &Map, is_house: &dyn Fn(u32) -> bool, x: i32, y: i32) -> [f32; 4] {
        let t = map.terrain.at_or(x, y, 0);
        let c = &self.colors;
        if t & terrain::BUILDING != 0 {
            let id = map.building.at_or(x, y, 0);
            return if id != 0 && is_house(id) { c.house } else { c.building };
        }
        if t & terrain::WATER != 0 || t & terrain::DEEPWATER != 0 {
            c.water
        } else if t & terrain::SHRUB != 0 {
            c.shrub
        } else if t & terrain::TREE != 0 {
            c.tree
        } else if t & terrain::ROCK != 0 {
            c.rock
        } else if t & (terrain::ROAD | terrain::SUBMERGED_ROAD) != 0 {
            c.road
        } else if t & terrain::CANAL != 0 {
            c.canal
        } else if t & terrain::WALL != 0 {
            c.wall
        } else if t & terrain::DUNE != 0 {
            c.dune
        } else if t & (terrain::FLOODPLAIN | terrain::MEADOW) != 0 {
            c.meadow
        } else {
            c.empty
        }
    }

    fn put(&mut self, x: i32, y: i32, color: [f32; 4]) {
        if x < 0 || y < 0 || x >= BUF_W as i32 || y >= BUF_H as i32 {
            return;
        }
        let i = ((y as u32 * BUF_W + x as u32) * 4) as usize;
        self.buffer[i] = (color[0] * 255.0) as u8;
        self.buffer[i + 1] = (color[1] * 255.0) as u8;
        self.buffer[i + 2] = (color[2] * 255.0) as u8;
        self.buffer[i + 3] = (color[3] * 255.0) as u8;
    }

    fn rebuild(&mut self, map: &Map, is_house: &dyn Fn(u32) -> bool) {
        let black = self.colors.black;
        let bytes = [(black[0] * 255.0) as u8, (black[1] * 255.0) as u8, (black[2] * 255.0) as u8, 255];
        for px in self.buffer.as_chunks_mut::<4>().0.iter_mut() {
            *px = bytes;
        }
        let (scale, ..) = Self::transform(map);
        let dot = ((scale * crate::city_view::TILE_H / 2.0).round() as i32).clamp(1, 2);
        for y in 0..map.height {
            for x in 0..map.width {
                let color = self.tile_color(map, is_house, x, y);
                let tw = crate::city_view::TILE_W;
                let th = crate::city_view::TILE_H;
                let center = [(x - y) as f32 * tw / 2.0 + tw / 2.0, (x + y) as f32 * th / 2.0 + th / 2.0];
                let p = Self::to_local(map, center);
                let (cx, cy) = (p[0].round() as i32, p[1].round() as i32);
                for dy in 0..dot {
                    for dx in 0..dot {
                        self.put(cx + dx, cy + dy, color);
                    }
                }
            }
        }
    }

    /// Draws the minimap (rebuilding the CPU buffer first if [`Minimap::mark_dirty`]
    /// was called) into the sidebar's minimap window, plus the camera's viewport
    /// rectangle. `is_house` classifies a `map.building` id (see `Map::building`, and
    /// `Buildings::get`/`Building::house` in `osiris-sim`) as a house versus any other
    /// building. `panel_left` is the sidebar panel's left edge (`screen_w - 162.0`).
    pub fn draw(&mut self, r: &mut Renderer, map: &Map, is_house: impl Fn(u32) -> bool, panel_left: f32) {
        if self.dirty {
            self.rebuild(map, &is_house);
            self.dirty = false;
            self.handle = None;
        }
        let handle = *self
            .handle
            .get_or_insert_with(|| r.upload_dynamic(TEXTURE_KEY, BUF_W, BUF_H, &self.buffer));
        let origin = [panel_left + X, Y];
        r.dynamic_image(handle, origin, [W, H], Space::Screen);

        let [vx0, vy0, vx1, vy1] = r.world_view();
        let a = Self::to_local(map, [vx0, vy0]);
        let b = Self::to_local(map, [vx1, vy1]);
        let (x0, y0) = (a[0].max(0.0), a[1].max(0.0));
        let (x1, y1) = (b[0].min(W), b[1].min(H));
        if x1 > x0 && y1 > y0 {
            let thickness = 1.0;
            let color = [1.0, 1.0, 1.0, 0.9];
            let (ox, oy) = (origin[0] + x0, origin[1] + y0);
            let (w, h) = (x1 - x0, y1 - y0);
            for rect in [
                ([ox, oy], [w, thickness]),
                ([ox, oy + h - thickness], [w, thickness]),
                ([ox, oy], [thickness, h]),
                ([ox + w - thickness, oy], [thickness, h]),
            ] {
                r.rect(rect.0, rect.1, color, Space::Screen);
            }
        }
    }
}
