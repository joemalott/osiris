//! Placing and removing buildings.

use crate::buildings::{Building, BuildingId, kind};
use crate::map::{mask, terrain};
use crate::world::{Outcome, World};

impl World {
    pub fn size_of(&self, k: u16) -> i32 {
        self.defs.building(k).map_or(1, |d| d.size.max(1))
    }

    /// The tiles building type `k` covers, across and down: a monument's or temple
    /// complex's own shape, otherwise its square.
    pub fn footprint_of(&self, k: u16) -> (i32, i32) {
        if crate::temple_complex::is_complex(k) {
            return crate::temple_complex::SIZE;
        }
        self.monument_footprint(k).unwrap_or_else(|| {
            let s = self.size_of(k);
            (s, s)
        })
    }

    /// Checks the placement rules for one building of type `k` at `(x, y)`.
    pub fn can_place(&self, k: u16, x: i32, y: i32) -> Result<(), &'static str> {
        let def = self.defs.building(k).ok_or("Unknown building")?;
        if !self.is_allowed(k) {
            return Err("Not available yet");
        }
        let size = def.size.max(1);
        if let Some(rule) = self.can_place_defense(k, x, y) {
            return rule;
        }
        if k == crate::bridges::LOW_BRIDGE {
            return self.bridge_span(x, y).map(|_| ());
        }
        let (fw, fh) = self.footprint_of(k);
        if def.needs("shoreline") {
            return self.can_place_on_shore(k, x, y);
        }
        if self.is_road_venue(k) {
            if self.venue_orientation(k, x, y).is_none() {
                return Err("Must be built where roads meet");
            }
            let walker = |xx: i32, yy: i32| {
                !self.map.terrain_is(xx, yy, terrain::ROAD) && self.figures.iter().any(|f| (f.x, f.y) == (xx, yy))
            };
            if (y..y + size).any(|yy| (x..x + size).any(|xx| walker(xx, yy))) {
                return Err("People are in the way");
            }
            return Ok(());
        }
        let floodplain_ok =
            def.needs("floodplain") || (kind::FARM_FIRST..=kind::FARM_LAST).contains(&k) || def.has_flag("is_farm");
        let mut blocked = mask::NOT_CLEAR;
        if floodplain_ok {
            blocked &= !terrain::FLOODPLAIN;
        }
        for yy in y..y + fh {
            for xx in x..x + fw {
                if !self.map.contains(xx, yy) {
                    return Err("Outside the map");
                }
                if self.map.terrain_is(xx, yy, blocked) || self.map.building.at_or(xx, yy, 0) != 0 {
                    return Err("Can't build there");
                }
                if self.figures.iter().any(|f| (f.x, f.y) == (xx, yy)) {
                    return Err("People are in the way");
                }
            }
        }
        self.can_place_monument(k, (x, y))?;
        if crate::military::fort_soldier(k).is_some() && !self.fort_ground_clear(x, y) {
            return Err("No room for the parade ground");
        }
        if def.needs("groundwater") && !self.map.terrain_is(x, y, terrain::GROUNDWATER) {
            return Err("Needs groundwater");
        }
        if def.needs("floodplain") {
            let all = (y..y + size).all(|yy| (x..x + size).all(|xx| self.map.terrain_is(xx, yy, terrain::FLOODPLAIN)));
            if !all {
                return Err("Must be built on the floodplain");
            }
        }
        let near = |mask: u32, radius: i32| self.map.terrain_in_radius(x, y, size, radius, mask);
        if def.needs("ore") && !near(terrain::ORE, 1) {
            return Err("Must be built next to ore-bearing rock");
        }
        if def.needs("rock") && !def.needs("ore") && !near(terrain::ROCK, 1) {
            return Err("Must be built next to rock");
        }
        if def.needs("nearby_water") && !near(terrain::WATER, 3) {
            return Err("Must be built near water");
        }
        let is_farm = (kind::FARM_FIRST..=kind::FARM_LAST).contains(&k) || def.has_flag("is_farm");
        let on_floodplain =
            (y..y + size).all(|yy| (x..x + size).all(|xx| self.map.terrain_is(xx, yy, terrain::FLOODPLAIN)));
        if def.needs("meadow") && !(is_farm && on_floodplain) {
            let any = (y..y + size).any(|yy| (x..x + size).any(|xx| self.map.terrain_is(xx, yy, terrain::MEADOW)));
            if !any {
                return Err("Must be built on meadow");
            }
        }
        Ok(())
    }

    /// Tiles a build command covers, one entry per building placed.
    pub fn build_sites(&self, k: u16, x: i32, y: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
        if crate::defenses::is_wall(k) {
            return self.wall_sites(x, y, x1, y1);
        }
        if k == kind::VACANT_LOT {
            let mut v = Vec::new();
            for yy in y.min(y1)..=y.max(y1) {
                for xx in x.min(x1)..=x.max(x1) {
                    v.push((xx, yy));
                }
            }
            v
        } else {
            vec![(x1, y1)]
        }
    }

    pub(crate) fn build(&mut self, k: u16, x: i32, y: i32, x1: i32, y1: i32, measure: bool) -> Outcome {
        if crate::temple_complex::is_upgrade(k) {
            return self.build_upgrade(k, (x1, y1), measure);
        }
        let sites = self.build_sites(k, x, y, x1, y1);
        let ok: Vec<(i32, i32)> = sites.iter().copied().filter(|&(sx, sy)| self.can_place(k, sx, sy).is_ok()).collect();
        if ok.is_empty() {
            let reason = sites
                .first()
                .and_then(|&(sx, sy)| self.can_place(k, sx, sy).err())
                .unwrap_or("Can't build there");
            return Outcome::Invalid(reason);
        }
        // A bridge costs by its length.
        let tiles = if k == crate::bridges::LOW_BRIDGE { ok.iter().filter_map(|&(sx, sy)| self.bridge_span(sx, sy).ok()).map(|(_, t)| t.len() as i32).sum() } else { ok.len() as i32 };
        let cost = self.cost_of(k as usize) * tiles;
        let items = ok.len() as i32;
        if measure {
            return Outcome::Done { items, cost };
        }
        if cost > self.treasury {
            return Outcome::NotEnoughMoney;
        }
        self.treasury -= cost;
        self.finance.this_year.construction += cost;
        for (sx, sy) in ok {
            if k == crate::bridges::LOW_BRIDGE {
                self.place_bridge(sx, sy);
                continue;
            }
            if crate::defenses::is_tower(k) {
                self.clear_walls_for_tower(sx, sy);
            }
            self.create_building(k, sx, sy);
        }
        Outcome::Done { items, cost }
    }

    /// Adds a building without charging for it.
    pub fn create_building(&mut self, k: u16, x: i32, y: i32) -> BuildingId {
        let size = self.size_of(k);
        let dims = self.monument_footprint(k).or_else(|| crate::temple_complex::is_complex(k).then_some(crate::temple_complex::SIZE));
        let (fw, fh) = dims.unwrap_or((size, size));
        let image = self.defs.building(k).map_or(0, |d| d.image);
        let mut b = Building {
            dims,
            kind: k,
            x,
            y,
            size,
            image,
            fire_risk: 0,
            damage_risk: 0,
            stock: vec![0; 40],
            ..Default::default()
        };
        if kind::is_house(k) {
            b.house = Some(crate::houses::House { level: (k - kind::HOUSE_FIRST) as u8, common_health: 50, ..Default::default() });
        }
        let id = self.buildings.insert(b);
        for yy in y..y + fh {
            for xx in x..x + fw {
                self.map.terrain.update(xx, yy, |t| (t & !(terrain::MEADOW | terrain::SHRUB)) | terrain::BUILDING);
                self.map.building.set(xx, yy, id);
            }
        }
        if crate::water::is_shore_building(k) {
            self.place_on_shore(id);
        } else if self.is_road_venue(k) {
            self.place_venue(id);
        } else if k == kind::STORAGE_YARD {
            self.place_storage_yard(id);
        } else if crate::temple_complex::is_complex(k) {
            self.place_temple_complex(id);
        } else if dims.is_some() {
            self.place_monument(id);
        } else if crate::military::fort_soldier(k).is_some() {
            self.map.set_footprint(x, y, size, image);
            self.place_fort(id);
        } else if crate::defenses::is_wall(k) || crate::defenses::is_gatehouse(k) || k == crate::defenses::ROADBLOCK {
            self.map.set_footprint(x, y, size, image);
            self.place_defense(id);
        } else {
            self.map.set_footprint(x, y, size, image);
        }
        if let Some(farm) = self.farm_image(id) {
            self.set_building_image(id, farm);
        }
        self.refresh_road_access(id);
        id
    }

    pub(crate) fn refresh_road_access(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let road = if b.is_house() {
            crate::buildings::road_within(&self.map, b.x, b.y, b.size, 2)
        } else if b.dims.is_some() {
            crate::buildings::road_access_rect(&self.map, b.x, b.y, b.footprint())
        } else {
            crate::buildings::road_access(&self.map, b.x, b.y, b.size)
        };
        if let Some(b) = self.buildings.get_mut(id) {
            b.road = road;
        }
    }

    /// Removes a building and frees its tiles.
    pub fn demolish(&mut self, id: BuildingId) {
        if self.buildings.get(id).is_some_and(|b| b.kind == crate::bridges::LOW_BRIDGE) {
            self.remove_bridge(id);
            return;
        }
        if self.buildings.get(id).is_some_and(|b| crate::military::fort_soldier(b.kind).is_some()) {
            self.remove_fort(id);
        }
        let Some(b) = self.buildings.remove(id) else { return };
        let defense = (crate::defenses::is_wall(b.kind) || crate::defenses::is_gatehouse(b.kind) || b.kind == crate::defenses::ROADBLOCK).then_some((b.kind, b.x, b.y));
        let parts = b.monument.as_ref().map(|m| Self::part_tiles(&m.parts)).unwrap_or_default();
        let part_tiles = parts.into_iter().map(|(px, py)| (b.x + px, b.y + py));
        for (xx, yy) in b.tiles().chain(part_tiles).collect::<Vec<_>>() {
            self.map.terrain.update(xx, yy, |t| t & !terrain::BUILDING);
            self.map.building.set(xx, yy, 0);
            self.map.set_single_image(xx, yy, 0);
        }
        if crate::water::is_shore_building(b.kind) {
            self.remove_from_shore(&b);
        }
        for f in b.walkers {
            if f != 0 {
                self.figures.remove(f);
            }
        }
        if let Some(h) = &b.house {
            self.population -= h.population;
            self.census.remove(&self.rng, h.population);
        }
        if let Some((k, x, y)) = defense {
            self.remove_defense(k, x, y);
        }
    }

    /// Changes the image of a building's whole footprint.
    pub fn set_building_image(&mut self, id: BuildingId, image: u32) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        b.image = image;
        let (x, y, s) = (b.x, b.y, b.size);
        self.map.set_footprint(x, y, s, image);
    }
}
