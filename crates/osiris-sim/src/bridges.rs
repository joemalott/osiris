//! Bridges. A bridge is built from the water's edge straight across to the far bank:
//! a ramp up at each end and a deck between, carrying a road. People walk it like a
//! road; boats cannot pass under it.

use crate::buildings::BuildingId;
use crate::map::{NEIGHBOURS, terrain};
use crate::world::World;

pub const LOW_BRIDGE: u16 = 82;
/// Longest bridge, in tiles.
const MAX_LENGTH: i32 = 40;

impl World {
    fn water_at(&self, x: i32, y: i32) -> bool {
        self.map.contains(x, y) && self.map.terrain_is(x, y, terrain::WATER) && !self.map.terrain_is(x, y, terrain::BRIDGE | terrain::BUILDING)
    }

    fn land_at(&self, x: i32, y: i32) -> bool {
        self.map.contains(x, y) && !self.map.terrain_is(x, y, terrain::WATER)
    }

    /// The span of a bridge started at water tile `(x, y)`: its direction (0, 2, 4 or
    /// 6, pointing across the water) and the water tiles it covers, shore to shore.
    pub fn bridge_span(&self, x: i32, y: i32) -> Result<(u8, Vec<(i32, i32)>), &'static str> {
        if !self.water_at(x, y) {
            return Err("Bridges start at the water's edge");
        }
        for d in [0u8, 2, 4, 6] {
            let (dx, dy) = NEIGHBOURS[d as usize];
            // Land behind, water ahead.
            if !self.land_at(x - dx, y - dy) {
                continue;
            }
            let mut tiles = vec![(x, y)];
            let (mut cx, mut cy) = (x + dx, y + dy);
            while self.water_at(cx, cy) && tiles.len() < MAX_LENGTH as usize {
                tiles.push((cx, cy));
                cx += dx;
                cy += dy;
            }
            if tiles.len() >= 2 && self.land_at(cx, cy) {
                return Ok((d, tiles));
            }
        }
        Err("A bridge must reach straight across to the far bank")
    }

    /// Lays a bridge: a building on each tile of its span, drawn ramp, deck, ramp.
    pub(crate) fn place_bridge(&mut self, x: i32, y: i32) -> Option<Vec<BuildingId>> {
        let (d, tiles) = self.bridge_span(x, y).ok()?;
        let base = self.defs.building(LOW_BRIDGE).map(|b| b.image)?;
        let n = tiles.len();
        let along_x = d == 2 || d == 6;
        let mut ids = Vec::new();
        for (i, &(tx, ty)) in tiles.iter().enumerate() {
            // The water's own image is kept, for when the bridge comes down.
            let water = self.map.images.at_or(tx, ty, 0);
            let id = self.buildings.insert(crate::buildings::Building { kind: LOW_BRIDGE, x: tx, y: ty, size: 1, image: water, stock: vec![0; 40], ..Default::default() });
            self.map.building.set(tx, ty, id);
            self.map.terrain.update(tx, ty, |t| t | terrain::BRIDGE | terrain::ROAD);
            // Ramps face the way they climb; the deck runs along the span.
            let piece = if i == 0 || i == n - 1 {
                let up = if i == 0 { d } else { (d + 4) % 8 };
                1 + (up / 2) as u32
            } else if along_x {
                5
            } else {
                0
            };
            self.map.set_single_image(tx, ty, base + piece);
            ids.push(id);
        }
        Some(ids)
    }

    /// Demolishing any part of a bridge brings the whole of it down.
    pub(crate) fn remove_bridge(&mut self, id: BuildingId) {
        let mut todo = vec![id];
        while let Some(id) = todo.pop() {
            let Some(b) = self.buildings.remove(id) else { continue };
            let (x, y) = (b.x, b.y);
            self.map.building.set(x, y, 0);
            self.map.terrain.update(x, y, |t| t & !(terrain::BRIDGE | terrain::ROAD));
            self.map.set_single_image(x, y, b.image);
            for d in [0usize, 2, 4, 6] {
                let (dx, dy) = NEIGHBOURS[d];
                let n = self.map.building.at_or(x + dx, y + dy, 0);
                if self.buildings.get(n).is_some_and(|b| b.kind == LOW_BRIDGE) {
                    todo.push(n);
                }
            }
        }
    }
}
