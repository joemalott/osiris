//! Placing and removing buildings.

use crate::buildings::{Building, BuildingId, kind};
use crate::map::{mask, terrain};
use crate::world::{Outcome, World};

impl World {
    pub fn size_of(&self, k: u16) -> i32 {
        self.defs.building(k).map_or(1, |d| d.size.max(1))
    }

    /// Checks the placement rules for one building of type `k` at `(x, y)`.
    pub fn can_place(&self, k: u16, x: i32, y: i32) -> Result<(), &'static str> {
        let def = self.defs.building(k).ok_or("Unknown building")?;
        if !self.is_allowed(k) {
            return Err("Not available yet");
        }
        let size = def.size.max(1);
        let (fw, fh) = self.monument_footprint(k).unwrap_or((size, size));
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
                if self.map.terrain_is(xx, yy, blocked) {
                    return Err("Can't build there");
                }
                if self.figures.iter().any(|f| (f.x, f.y) == (xx, yy)) {
                    return Err("People are in the way");
                }
            }
        }
        self.can_place_monument(k)?;
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
        let sites = self.build_sites(k, x, y, x1, y1);
        let ok: Vec<(i32, i32)> = sites.iter().copied().filter(|&(sx, sy)| self.can_place(k, sx, sy).is_ok()).collect();
        if ok.is_empty() {
            let reason = sites
                .first()
                .and_then(|&(sx, sy)| self.can_place(k, sx, sy).err())
                .unwrap_or("Can't build there");
            return Outcome::Invalid(reason);
        }
        let cost = self.cost_of(k as usize) * ok.len() as i32;
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
            self.create_building(k, sx, sy);
        }
        Outcome::Done { items, cost }
    }

    /// Adds a building without charging for it.
    pub fn create_building(&mut self, k: u16, x: i32, y: i32) -> BuildingId {
        let size = self.size_of(k);
        let dims = self.monument_footprint(k);
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
        } else if dims.is_some() {
            self.place_monument(id);
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
        } else {
            crate::buildings::road_access(&self.map, b.x, b.y, b.size)
        };
        if let Some(b) = self.buildings.get_mut(id) {
            b.road = road;
        }
    }

    /// Removes a building and frees its tiles.
    pub fn demolish(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.remove(id) else { return };
        for (xx, yy) in b.tiles().collect::<Vec<_>>() {
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
    }

    /// Changes the image of a building's whole footprint.
    pub fn set_building_image(&mut self, id: BuildingId, image: u32) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        b.image = image;
        let (x, y, s) = (b.x, b.y, b.size);
        self.map.set_footprint(x, y, s, image);
    }
}
