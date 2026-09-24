//! Placing and removing buildings.

use crate::buildings::{Building, BuildingId, kind};
use crate::map::{mask, terrain};
use crate::world::{Outcome, World};

/// The original's warning 19:74, for a building placed with no groundwater under it.
pub const NEEDS_GROUNDWATER: &str = "This structure needs groundwater. Build on a grassy area.";

impl World {
    pub fn size_of(&self, k: u16) -> i32 {
        self.defs.building(k).map_or(1, |d| d.size.max(1))
    }

    /// The tiles building type `k` covers, across and down: a monument's, temple
    /// complex's or gatehouse's own shape, otherwise its square.
    pub fn footprint_of(&self, k: u16) -> (i32, i32) {
        if crate::temple_complex::is_complex(k) {
            return crate::temple_complex::footprint(self.complex_facing);
        }
        if k == crate::defenses::GATEHOUSE {
            return crate::defenses::gatehouse_footprint(self.gatehouse_facing);
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
        if let Some(rule) = self.can_place_royal_tomb(k, x, y) {
            return rule;
        }
        let (fw, fh) = self.footprint_of(k);
        if k == crate::irrigation::WATER_LIFT {
            return self.can_place_lift(x, y);
        }
        if def.needs("shoreline") {
            return self.can_place_on_shore(k, x, y);
        }
        debug_assert_eq!(self.uses_common_rules(k), !self.is_road_venue(k));
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
        for yy in y..y + fh {
            for xx in x..x + fw {
                if let Some(why) = self.footprint_tile_problem(k, xx, yy) {
                    return Err(why);
                }
            }
        }
        self.can_place_monument(k, (x, y))?;
        if crate::military::fort_soldier(k).is_some() && crate::military::fort_ground(x, y).any(|(xx, yy)| !self.fort_ground_tile_clear(xx, yy)) {
            return Err("No room for the parade ground");
        }
        // The original wants groundwater under at least one tile of the footprint
        // (wells, water supplies, mansions and palaces), warning 19:74 if none.
        if def.needs("groundwater") && !(y..y + size).any(|yy| (x..x + size).any(|xx| self.map.terrain_is(xx, yy, terrain::GROUNDWATER))) {
            return Err(NEEDS_GROUNDWATER);
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

    /// Whether building type `k` is placed by the common rules, tile by tile (with any
    /// monument or fort rules after), rather than by rules of its own: defences,
    /// bridges, royal tombs, shore buildings and venues where roads meet.
    pub(crate) fn uses_common_rules(&self, k: u16) -> bool {
        let Some(def) = self.defs.building(k) else { return false };
        let own = crate::defenses::is_gatehouse(k)
            || k == crate::defenses::ROADBLOCK
            || crate::defenses::is_tower(k)
            || k == crate::bridges::LOW_BRIDGE
            || crate::royal_tombs::layout(k).is_some()
            || def.needs("shoreline")
            || self.is_road_venue(k);
        !own
    }

    /// Why tile `(xx, yy)` of a building of type `k` (placed through the common rules)
    /// can't be built on, if it can't: off the map, the ground not clear, or someone
    /// standing there.
    pub(crate) fn footprint_tile_problem(&self, k: u16, xx: i32, yy: i32) -> Option<&'static str> {
        let def = self.defs.building(k)?;
        if !self.map.contains(xx, yy) {
            return Some("Outside the map");
        }
        use crate::monuments::Style;
        let style = crate::monuments::monument_def(k).map(|d| d.style);
        let floodplain_ok =
            def.needs("floodplain") || (kind::FARM_FIRST..=kind::FARM_LAST).contains(&k) || def.has_flag("is_farm");
        // The original lets pyramids, mastabas, sun temples and mausoleums take ground
        // with trees and shrubs (its mask 0xfeffd76e); the laborers clear it.
        let big = matches!(style, Some(Style::Pyramid(_) | Style::Mastaba | Style::SunTemple | Style::Mausoleum));
        let mut blocked = if big { crate::pyramids::TOMB_BLOCKED } else { mask::NOT_CLEAR };
        if floodplain_ok {
            blocked &= !terrain::FLOODPLAIN;
        }
        if self.map.terrain_is(xx, yy, blocked) || self.map.building.at_or(xx, yy, 0) != 0 {
            // The original's warnings: 19:211 for tombs, 19:0 for other monuments.
            return Some(match style {
                Some(Style::Pyramid(_) | Style::Mastaba) => crate::pyramids::FREE_OF_OBSTRUCTIONS,
                Some(_) => "Must build on cleared land",
                None => "Can't build there",
            });
        }
        if self.figures.iter().any(|f| (f.x, f.y) == (xx, yy)) {
            return Some("People are in the way");
        }
        None
    }

    /// Tiles a build command covers, one entry per building placed. `x1`/`y1` come
    /// straight from the player's drag (or a script/save command): reined in to a
    /// generous distance from `(x, y)` so an absurd value can't build a tile list with
    /// billions of entries. Any span that could occur on a real map (however far
    /// off-map) is left untouched, so `can_place`'s own bounds check still decides
    /// what's valid exactly as before.
    pub fn build_sites(&self, k: u16, x: i32, y: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
        const MAX_SPAN: i32 = 1024;
        let bound = |a: i32, b: i32| {
            let too_far = b.checked_sub(a).is_none_or(|d| d.unsigned_abs() > MAX_SPAN as u32);
            if too_far { a.saturating_add(if b >= a { MAX_SPAN } else { -MAX_SPAN }) } else { b }
        };
        let x1 = bound(x, x1);
        let y1 = bound(y, y1);
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
        if k == crate::irrigation::DITCH {
            return self.build_ditch((x, y), (x1, y1), measure);
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
        if self.out_of_money() {
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
        let gatehouse = k == crate::defenses::GATEHOUSE;
        let dims = self
            .monument_footprint(k)
            .or_else(|| crate::temple_complex::is_complex(k).then(|| crate::temple_complex::footprint(self.complex_facing)))
            .or_else(|| gatehouse.then(|| crate::defenses::gatehouse_footprint(self.gatehouse_facing)));
        let (fw, fh) = dims.unwrap_or((size, size));
        let image = self.statue_image(k).unwrap_or_else(|| self.defs.building(k).map_or(0, |d| d.image));
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
            orientation: if gatehouse {
                self.gatehouse_facing
            } else if crate::temple_complex::is_complex(k) {
                self.complex_facing
            } else {
                0
            },
            ..Default::default()
        };
        if kind::is_house(k) {
            // A new house starts at its level's base crime risk.
            let level = (k - kind::HOUSE_FIRST) as u8;
            b.house = Some(crate::houses::House { level, crime: self.balance.house(level).crime_base, ..Default::default() });
        }
        let id = self.buildings.insert(b);
        for yy in y..y + fh {
            for xx in x..x + fw {
                self.map.terrain.update(xx, yy, |t| (t & !(terrain::MEADOW | terrain::SHRUB | terrain::TREE)) | terrain::BUILDING);
                self.map.building.set(xx, yy, id);
            }
        }
        if crate::water::is_shore_building(k) {
            self.place_on_shore(id);
        } else if k == crate::irrigation::WATER_LIFT {
            self.place_lift(id);
        } else if self.is_road_venue(k) {
            self.place_venue(id);
        } else if k == kind::STORAGE_YARD {
            self.place_storage_yard(id);
        } else if crate::temple_complex::is_complex(k) {
            self.place_temple_complex(id);
        } else if gatehouse {
            self.place_defense(id);
        } else if dims.is_some() {
            self.place_monument(id);
        } else if crate::military::fort_soldier(k).is_some() {
            self.map.set_footprint(x, y, size, image);
            self.place_fort(id);
        } else if crate::defenses::is_defense(k) || k == crate::defenses::ROADBLOCK {
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
        let defense = (crate::defenses::is_defense(b.kind) || b.kind == crate::defenses::ROADBLOCK).then_some((b.kind, b.x, b.y, b.footprint()));
        let parts = b.monument.as_ref().map(|m| Self::part_tiles(&m.parts)).unwrap_or_default();
        let part_tiles = parts.into_iter().map(|(px, py)| (b.x + px, b.y + py));
        for (xx, yy) in b.tiles().chain(part_tiles).collect::<Vec<_>>() {
            self.map.terrain.update(xx, yy, |t| t & !terrain::BUILDING);
            self.map.building.set(xx, yy, 0);
            self.map.set_single_image(xx, yy, 0);
        }
        if crate::water::is_shore_building(b.kind) || b.kind == crate::irrigation::WATER_LIFT {
            self.remove_from_shore(&b);
        }
        if b.kind == crate::irrigation::WATER_LIFT {
            self.ditch_images_in(b.x - 1, b.y - 1, b.x + b.size, b.y + b.size);
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
        if let Some((k, x, y, dims)) = defense {
            self.remove_defense(k, x, y, dims);
        }
        if crate::royal_tombs::is_royal_tomb(b.kind) {
            self.remove_royal_tomb(b.kind, b.x, b.y);
        }
    }

    /// A statue's image: its groups hold looks of four facings each; the build tool
    /// picks the look and the facing (the original's default facing is 1, face-on).
    pub fn statue_image(&self, k: u16) -> Option<u32> {
        let d = self.defs.building(k).filter(|d| d.has_flag("is_statue"))?;
        let looks = &d.variants;
        (!looks.is_empty()).then(|| looks[self.statue_variant as usize % looks.len()] + (self.statue_facing % 4) as u32)
    }

    /// Games saved before statues had images kept them imageless (drawn black); give
    /// those the default look and facing.
    pub(crate) fn upgrade_statues(&mut self) {
        let bare: Vec<(BuildingId, u16)> = self.buildings.ids().into_iter().filter_map(|id| self.buildings.get(id).filter(|b| b.image == 0).map(|b| (id, b.kind))).collect();
        for (id, k) in bare {
            if let Some(image) = self.statue_image(k) {
                self.set_building_image(id, image);
            }
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
