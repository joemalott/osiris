//! Houses: population, the services and goods they have access to, and evolution.
//!
//! Each of the 20 house levels has a row in the model file listing the desirability at
//! which it devolves or may evolve, and what the *next* level needs. A house evolves
//! when its desirability reaches the evolve threshold and it already satisfies the
//! next level's needs; it devolves when desirability drops to the devolve threshold or
//! it no longer satisfies its own level's needs.
//!
//! Four 1x1 houses of the same small level in a square merge into one 2x2 house that
//! holds four times the people. A spacious apartment, an elegant residence and a
//! stately manor need a bigger footprint (2x2, 3x3, 4x4) to evolve, taking in the
//! houses, clear land and gardens around them; devolving gives the land back.

use crate::buildings::{BuildingId, kind};
use crate::map::terrain;
use crate::rules::Rules;
use crate::world::World;

/// One row of the model file's house table.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct HouseModel {
    pub devolve_desirability: i32,
    pub evolve_desirability: i32,
    pub entertainment: i32,
    pub water: i32,
    pub religion: i32,
    pub education: i32,
    pub bazaar: i32,
    pub dentist: i32,
    /// Column i, labelled "physician" in the model file, is the magistrate need: the
    /// original checks it against a courthouse magistrate's visits.
    pub magistrate: i32,
    pub health: i32,
    pub food_types: i32,
    pub pottery: i32,
    pub linen: i32,
    pub jewelry: i32,
    pub beer: i32,
    pub crime_increment: i32,
    pub crime_base: i32,
    pub prosperity: i32,
    pub max_people: i32,
    pub tax_multiplier: i32,
    pub malaria_increment: i32,
    pub disease_increment: i32,
}

/// Service coverage counters: set to 100 (or similar) when a walker passes, decaying
/// daily. A non-zero value means "has access".
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Coverage {
    pub water_supply: i32,
    pub juggler: i32,
    pub musician: i32,
    pub dancer: i32,
    pub senet: i32,
    pub zoo: i32,
    pub school: i32,
    pub library: i32,
    pub academy: i32,
    pub apothecary: i32,
    pub dentist: i32,
    pub mortuary: i32,
    pub physician: i32,
    pub magistrate: i32,
    pub temples: [i32; 5],
    pub shrine: i32,
    pub bazaar: i32,
    pub tax: i32,
}

/// A house keeps each of the eight foods apart, in resource order.
pub const FOOD_TYPES: usize = 8;

/// Reads a house's foods, including saves from when they were four groups (grain,
/// meat, fish, fruit and vegetables), which go to grain, meat, fish and lettuce.
fn foods_compat<'de, D: serde::Deserializer<'de>>(d: D) -> Result<[i32; FOOD_TYPES], D::Error> {
    let v: Vec<i32> = serde::Deserialize::deserialize(d)?;
    let mut out = [0; FOOD_TYPES];
    if v.len() == 4 {
        for (slot, n) in [0, 1, 6, 2].into_iter().zip(v) {
            out[slot] = n;
        }
    } else {
        for (o, n) in out.iter_mut().zip(v) {
            *o = n;
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct House {
    /// 0 = crude hut (or vacant lot when empty) .. 19 = palatial estate.
    pub level: u8,
    pub population: i32,
    /// People on their way here as immigrants.
    pub incoming: i32,
    pub coverage: Coverage,
    /// Tiles within reach of a well.
    pub well_access: bool,
    /// Derived each day from coverage.
    pub entertainment: i32,
    pub education: i32,
    pub health: i32,
    pub gods: i32,
    /// Food stored, per food (see `resource::food_slot`).
    #[serde(deserialize_with = "foods_compat")]
    pub foods: [i32; FOOD_TYPES],
    /// Pottery, jewelry, linen, beer.
    pub goods: [i32; 4],
    pub devolve_delay: i32,
    /// Why the house can't evolve (or is decaying), for the info panel.
    pub blocked_by: Option<Need>,
    /// The house fails its own level's needs and will devolve soon.
    #[serde(default)]
    pub decaying: bool,
    /// How happy the household is with the governor, 0-100.
    #[serde(default)]
    pub happiness: i32,
    /// Sentiment updates in a row without food (up to 3).
    #[serde(default)]
    pub days_without_food: i32,
    /// Disease and malaria risks, 0-1000; at 1000 the household is wiped out.
    #[serde(default)]
    pub disease_risk: i32,
    #[serde(default)]
    pub malaria_risk: i32,
    /// Months until malaria spreading from a nearby house reaches this one (0 none).
    #[serde(default)]
    pub malaria_countdown: i32,
    /// Months of quarantine left after plague or disease: no one moves in.
    #[serde(default)]
    pub quarantine: i32,
    /// Crime risk, 0-1000; at 1000 the house sends out a thief.
    #[serde(default)]
    pub crime: i32,
    /// Four small lots joined into one 2x2 house, which holds four times the people.
    #[serde(default)]
    pub merged: bool,
    /// Foods the household ate at its last monthly meal.
    #[serde(default)]
    pub foods_eaten: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Need {
    Desirability,
    Water,
    WaterSupply,
    Entertainment,
    Education,
    Religion,
    Dentist,
    Magistrate,
    Health,
    Food,
    Pottery,
    Linen,
    Jewelry,
    /// Wants a second kind of luxury good (see `World::luxury_sources`).
    SecondLuxury,
    Beer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Evolve,
    Stay,
    Decay,
}

impl House {
    pub fn kind(&self) -> u16 {
        crate::buildings::kind::HOUSE_FIRST + self.level as u16
    }

    pub fn is_vacant(&self) -> bool {
        self.population <= 0
    }

    /// Recomputes entertainment/education/health/religion from coverage. Entertainment
    /// is the city's venue coverage (`city_entertainment_base`) plus 10 for a juggler,
    /// 20 a musician, 30 a dancer and 40 a senet player, and a zoo adds 40 (or tops it
    /// up to 100 from 61). Education is 1 for a school or library and 2 for both;
    /// religion counts the gods whose priests visit; health is 1 each for physician
    /// and mortuary.
    pub fn derive_culture(&mut self, city_entertainment_base: i32) {
        let c = &self.coverage;
        let mut ent = city_entertainment_base;
        for (visits, points) in [(c.juggler, 10), (c.musician, 20), (c.dancer, 30), (c.senet, 40)] {
            if visits > 0 {
                ent += points;
            }
        }
        if c.zoo > 0 {
            ent = if ent < 61 { ent + 40 } else { 100 };
        }
        self.entertainment = ent.min(100);
        self.education = (c.school > 0) as i32 + (c.library > 0) as i32;
        self.gods = c.temples.iter().filter(|&&t| t > 0).count() as i32;
        self.health = (c.physician > 0) as i32 + (c.mortuary > 0) as i32;
    }

    /// Whether this house has what `model` asks for; the first unmet need otherwise.
    /// A level wanting more than one jewelry also needs two luxury sources in the city.
    pub fn meets(&self, model: &HouseModel, rules: &Rules, luxury_sources: i32) -> Result<(), Need> {
        let has_water_supply = self.coverage.water_supply > 0;
        if !has_water_supply {
            if model.water >= 2 {
                return Err(Need::WaterSupply);
            }
            if model.water == 1 && !self.well_access {
                return Err(Need::Water);
            }
        }
        if self.entertainment < model.entertainment {
            return Err(Need::Entertainment);
        }
        if self.education < model.education {
            return Err(Need::Education);
        }
        if rules.gods_enabled && self.gods < model.religion {
            return Err(Need::Religion);
        }
        if (self.coverage.dentist <= 0) & (model.dentist > 0) {
            return Err(Need::Dentist);
        }
        if (self.coverage.magistrate <= 0) & (model.magistrate > 0) {
            return Err(Need::Magistrate);
        }
        if self.health < model.health {
            return Err(Need::Health);
        }
        let food_types = self.foods.iter().filter(|&&f| f > 0).count() as i32;
        if food_types < model.food_types {
            return Err(Need::Food);
        }
        if self.goods[0] < model.pottery {
            return Err(Need::Pottery);
        }
        if model.beer > 0 && self.goods[3] <= 0 {
            return Err(Need::Beer);
        }
        if self.goods[2] < model.linen {
            return Err(Need::Linen);
        }
        if self.goods[1] < model.jewelry {
            return Err(Need::Jewelry);
        }
        if model.jewelry > 1 && luxury_sources < 2 {
            return Err(Need::SecondLuxury);
        }
        Ok(())
    }

    /// Decides whether the house evolves, stays or devolves.
    pub fn progress(&mut self, models: &[HouseModel], desirability: i32, rules: &Rules, luxury_sources: i32) -> Progress {
        let level = self.level as usize;
        let model = &models[level];
        let evolve_des = if level + 1 >= models.len() { 1000 } else { model.evolve_desirability };
        self.decaying = false;
        let mut status = if desirability <= model.devolve_desirability {
            self.blocked_by = Some(Need::Desirability);
            Progress::Decay
        } else if desirability >= evolve_des {
            Progress::Evolve
        } else {
            self.blocked_by = Some(Need::Desirability);
            Progress::Stay
        };
        if let Err(need) = self.meets(model, rules, luxury_sources) {
            self.blocked_by = Some(need);
            status = Progress::Decay;
        }
        if status == Progress::Decay {
            self.decaying = true;
        } else if status == Progress::Evolve {
            match models.get(level + 1).map(|next| self.meets(next, rules, luxury_sources)) {
                Some(Ok(())) => self.blocked_by = None,
                Some(Err(need)) => {
                    self.blocked_by = Some(need);
                    status = Progress::Stay;
                }
                None => status = Progress::Stay,
            }
        }
        status
    }
}

/// Daily decay of coverage counters.
pub fn decay(c: &mut Coverage) {
    let dec = |v: &mut i32| *v = (*v - 1).max(0);
    for v in [
        &mut c.water_supply,
        &mut c.juggler,
        &mut c.musician,
        &mut c.dancer,
        &mut c.senet,
        &mut c.zoo,
        &mut c.school,
        &mut c.library,
        &mut c.academy,
        &mut c.apothecary,
        &mut c.dentist,
        &mut c.mortuary,
        &mut c.physician,
        &mut c.magistrate,
        &mut c.shrine,
        &mut c.bazaar,
    ] {
        dec(v);
    }
    for t in &mut c.temples {
        dec(t);
    }
}

/// The level a house must grow to leave, and the size it grows to: a spacious
/// apartment becomes a 2x2 residence, an elegant residence a 3x3 manor, a stately
/// manor a 4x4 estate.
fn expands_to(level: u8) -> Option<i32> {
    match level {
        9 => Some(2),
        13 => Some(3),
        17 => Some(4),
        _ => None,
    }
}

/// Where a growing house may put its new top-left corner, tried in this order: in
/// place, up and left, left, up.
const EXPAND_CORNERS: [(i32, i32); 4] = [(0, 0), (-1, -1), (-1, 0), (0, -1)];

/// Terrain that stops a house growing onto a tile: all but groundwater, meadow,
/// fountain and irrigation range and floodplain (and Osiris's own bridge flag).
const EXPAND_BLOCKING: u32 = 0xeefe_d77f | terrain::BRIDGE;

const SENET_HOUSE: u16 = 32;
const ZOO: u16 = 226;

/// The tiles of the `n` x `n` block at `(x, y)`.
fn block(x: i32, y: i32, n: i32) -> impl Iterator<Item = (i32, i32)> {
    (0..n).flat_map(move |dy| (0..n).map(move |dx| (x + dx, y + dy)))
}

/// A household's people, foods and goods, as handed on when houses merge or split.
#[derive(Debug, Clone, Copy, Default)]
struct Share {
    population: i32,
    foods: [i32; FOOD_TYPES],
    goods: [i32; 4],
}

impl Share {
    fn of(h: &House) -> Self {
        Self { population: h.population, foods: h.foods, goods: h.goods }
    }

    fn add(&mut self, o: Share) {
        self.population += o.population;
        for i in 0..FOOD_TYPES {
            self.foods[i] += o.foods[i];
        }
        for i in 0..4 {
            self.goods[i] += o.goods[i];
        }
    }

    /// Splits it `parts` ways: the first share keeps the remainders.
    fn split(self, parts: i32) -> (Share, Share) {
        let each = Share { population: self.population / parts, foods: self.foods.map(|f| f / parts), goods: self.goods.map(|g| g / parts) };
        let first = Share {
            population: each.population + self.population % parts,
            foods: std::array::from_fn(|i| each.foods[i] + self.foods[i] % parts),
            goods: std::array::from_fn(|i| each.goods[i] + self.goods[i] % parts),
        };
        (first, each)
    }

    fn set(self, h: &mut House) {
        h.population = self.population;
        h.foods = self.foods;
        h.goods = self.goods;
    }
}

impl World {
    /// How many people a house holds: its level's capacity, four times over when merged.
    pub fn house_capacity(&self, id: BuildingId) -> i32 {
        let Some(h) = self.buildings.get(id).and_then(|b| b.house.as_ref()) else { return 0 };
        self.balance.house(h.level).max_people * if h.merged { 4 } else { 1 }
    }

    /// Tick 39: every occupied house evolves, stays or devolves. Small houses first try
    /// to merge with their neighbours; houses outgrowing their footprint expand.
    pub(crate) fn evolve_houses(&mut self) {
        let ids: Vec<BuildingId> = self.buildings.iter().filter(|b| b.is_house()).map(|b| b.id).collect();
        let luxury_sources = self.luxury_sources();
        for id in ids {
            let occupied = self.buildings.get(id).and_then(|b| b.house.as_ref()).filter(|h| h.population > 0);
            let Some(level) = occupied.map(|h| h.level) else { continue };
            if level <= 9 {
                self.merge_house(id);
            }
            let Some(b) = self.buildings.get_mut(id) else { continue };
            let des = b.desirability;
            let Some(h) = b.house.as_mut() else { continue };
            match h.progress(&self.balance.houses, des, &self.rules, luxury_sources) {
                Progress::Evolve if (level as usize) + 1 < self.balance.houses.len() => {
                    h.devolve_delay = 0;
                    match expands_to(level) {
                        Some(n) => self.expand_house(id, n),
                        None => self.set_house_level(id, level + 1),
                    }
                }
                Progress::Decay if level > 0 => {
                    // Two checks' grace: it devolves on the third failed check in a row.
                    h.devolve_delay += 1;
                    if h.devolve_delay > 2 {
                        h.devolve_delay = 0;
                        self.devolve_house(id);
                    }
                }
                _ => h.devolve_delay = 0,
            }
        }
    }

    /// Four 1x1 houses of the same level in a square, this one at its top left, merge
    /// into a 2x2 house (on five days in eight, by the tile's random byte).
    fn merge_house(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(h) = &b.house else { return };
        if h.merged || b.size != 1 || self.map.random.at_or(b.x, b.y, 0) & 7 >= 5 {
            return;
        }
        let level = h.level;
        let fits = block(b.x, b.y, 2).all(|(x, y)| {
            let other = self.map.building.at_or(x, y, 0);
            self.map.terrain_is(x, y, terrain::BUILDING)
                && (other == id
                    || self.buildings.get(other).filter(|o| o.size == 1).and_then(|o| o.house.as_ref()).is_some_and(|o| o.level == level && !o.merged))
        });
        if fits {
            let (x, y) = (b.x, b.y);
            self.absorb_block(id, x, y, 2);
            if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                h.merged = true;
            }
            self.set_house_level(id, level);
        }
    }

    /// The corner of an `n` x `n` block the house can grow onto, if any: first over
    /// houses no grander than itself, then also over clear land, then also over gardens.
    fn expansion_corner(&self, id: BuildingId, n: i32) -> Option<(i32, i32)> {
        let b = self.buildings.get(id)?;
        let level = b.house.as_ref()?.level;
        let absorbable = |x: i32, y: i32| {
            let other = self.map.building.at_or(x, y, 0);
            other == id || self.buildings.get(other).and_then(|o| o.house.as_ref()).is_some_and(|o| o.level <= level)
        };
        for pass in 0..3 {
            for (dx, dy) in EXPAND_CORNERS {
                let (cx, cy) = (b.x + dx, b.y + dy);
                let fits = block(cx, cy, n).all(|(x, y)| {
                    let t = self.map.terrain.at_or(x, y, 0);
                    if !self.map.contains(x, y) {
                        false
                    } else if pass > 0 && t & EXPAND_BLOCKING == 0 {
                        true
                    } else if t & terrain::BUILDING != 0 {
                        absorbable(x, y)
                    } else {
                        pass == 2 && t & terrain::GARDEN != 0
                    }
                });
                if fits {
                    return Some((cx, cy));
                }
            }
        }
        None
    }

    /// The house grows onto an `n` x `n` block and becomes the next level; with no room
    /// to grow it stays as it is.
    pub fn expand_house(&mut self, id: BuildingId, n: i32) {
        let Some((x, y)) = self.expansion_corner(id, n) else { return };
        let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) else { return };
        h.merged = false;
        let level = h.level + 1;
        self.absorb_block(id, x, y, n);
        self.set_house_level(id, level);
    }

    /// Makes house `id` the `n` x `n` block at `(x, y)`. Other houses on it that span
    /// several tiles break up first; then everyone on the block moves in with their
    /// foods and goods, and the land is built over.
    fn absorb_block(&mut self, id: BuildingId, x: i32, y: i32, n: i32) {
        let others = |w: &World| -> Vec<BuildingId> {
            let mut found: Vec<BuildingId> = Vec::new();
            for (xx, yy) in block(x, y, n) {
                let other = w.map.building.at_or(xx, yy, 0);
                if other != id && w.map.terrain_is(xx, yy, terrain::BUILDING) && w.buildings.get(other).is_some_and(|o| o.is_house()) && !found.contains(&other) {
                    found.push(other);
                }
            }
            found
        };
        for other in others(self) {
            if self.buildings.get(other).is_some_and(|o| o.size > 1) {
                self.break_up_house(other);
            }
        }
        let mut moved = Share::default();
        for other in others(self) {
            let Some(o) = self.buildings.remove(other) else { continue };
            for (xx, yy) in o.tiles() {
                self.map.building.set(xx, yy, 0);
            }
            if let Some(h) = &o.house {
                moved.add(Share::of(h));
            }
        }
        let Some(b) = self.buildings.get_mut(id) else { return };
        for (xx, yy) in b.tiles().collect::<Vec<_>>() {
            self.map.building.set(xx, yy, 0);
        }
        b.x = x;
        b.y = y;
        b.size = n;
        if let Some(h) = b.house.as_mut() {
            let mut all = Share::of(h);
            all.add(moved);
            all.set(h);
        }
        for (xx, yy) in block(x, y, n) {
            self.map.terrain.update(xx, yy, |t| (t & !(terrain::MEADOW | terrain::SHRUB | terrain::TREE | terrain::GARDEN)) | terrain::BUILDING);
            self.map.building.set(xx, yy, id);
        }
        self.refresh_road_access(id);
    }

    /// Shrinks house `id` to `size` at its corner; returns the tiles it no longer covers.
    fn shrink_house(&mut self, id: BuildingId, size: i32) -> Vec<(i32, i32)> {
        let Some(b) = self.buildings.get_mut(id) else { return Vec::new() };
        let (x, y) = (b.x, b.y);
        let freed: Vec<(i32, i32)> = b.tiles().filter(|&(xx, yy)| xx >= x + size || yy >= y + size).collect();
        b.size = size;
        for &(xx, yy) in &freed {
            self.map.building.set(xx, yy, 0);
            self.map.terrain.update(xx, yy, |t| t & !terrain::BUILDING);
        }
        freed
    }

    /// A new 1x1 house of `level` at `(x, y)` holding `share`.
    fn new_house_lot(&mut self, level: u8, x: i32, y: i32, share: Share) {
        let id = self.create_building(kind::HOUSE_FIRST + level as u16, x, y);
        if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
            share.set(h);
            h.happiness = 50;
        }
        self.set_house_level(id, level);
    }

    /// A house spanning several tiles breaks up into 1x1 houses sharing out its people
    /// and goods: a merged house into four of its level, a residence into four
    /// spacious apartments, a manor into nine.
    fn break_up_house(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(h) = &b.house else { return };
        let (level, parts) = match (h.merged, b.size) {
            (true, _) => (h.level, 4),
            (false, 2) => (9, 4),
            (false, 3) => (9, 9),
            _ => return,
        };
        let (first, each) = Share::of(h).split(parts);
        let freed = self.shrink_house(id, 1);
        if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
            h.merged = false;
            first.set(h);
        }
        self.set_house_level(id, level);
        for (x, y) in freed {
            self.new_house_lot(level, x, y, each);
        }
    }

    /// One level down. A residence breaks up into four spacious apartments; a manor
    /// shrinks to a 2x2 residence and an estate to a 3x3 manor, keeping a sixth (an
    /// eighth) of their people and goods and leaving gardens on the rest of the land.
    fn devolve_house(&mut self, id: BuildingId) {
        let Some(level) = self.buildings.get(id).and_then(|b| b.house.as_ref()).map(|h| h.level) else { return };
        match level {
            10 => self.break_up_house(id),
            14 | 18 => {
                let (size, parts) = if level == 14 { (2, 6) } else { (3, 8) };
                let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) else { return };
                let (kept, _) = Share::of(h).split(parts);
                let lost = h.population - kept.population;
                kept.set(h);
                self.population -= lost;
                self.census.remove(&self.rng, lost);
                let garden = self.defs.terrain.garden;
                for (x, y) in self.shrink_house(id, size) {
                    self.map.terrain.update(x, y, |t| t | terrain::GARDEN);
                    let pattern = if y & 1 == 0 { [0, 1, 0, 1] } else { [2, 3, 2, 3] };
                    self.map.set_single_image(x, y, garden + pattern[(x & 3) as usize]);
                }
                self.set_house_level(id, level - 1);
                self.refresh_road_access(id);
            }
            _ => self.set_house_level(id, level - 1),
        }
        self.evict_overflow(id);
    }

    /// An emptied house becomes a vacant lot; one spanning several tiles breaks up into
    /// 1x1 lots.
    pub(crate) fn make_vacant_lot(&mut self, id: BuildingId) {
        let freed = self.shrink_house(id, 1);
        if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
            h.merged = false;
            h.population = 0;
        }
        self.set_house_level(id, 0);
        for (x, y) in freed {
            self.new_house_lot(0, x, y, Share::default());
        }
    }

    /// People above a house's capacity leave as homeless.
    fn evict_overflow(&mut self, id: BuildingId) {
        let cap = self.house_capacity(id);
        let Some(b) = self.buildings.get(id) else { return };
        let Some(h) = &b.house else { return };
        let extra = h.population - cap;
        if extra <= 0 {
            return;
        }
        let (x, y) = b.road.unwrap_or((b.x, b.y));
        if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
            h.population = cap;
        }
        self.population -= extra;
        self.census.remove(&self.rng, extra);
        let fid = self.figures.spawn(crate::people::figure_kind::HOMELESS, x, y, crate::figures::Travel::PreferRoads);
        if let Some(f) = self.figures.get_mut(fid) {
            f.amount = extra;
        }
    }

    /// The part of every house's entertainment that comes from the city as a whole: a
    /// fifth of the average share of the people its booths (400 each), bandstands
    /// (700), pavilions (1200), senet houses (5000) and zoos (7500) could entertain. A
    /// pavilion also counts as a bandstand and a booth, a bandstand as a booth.
    pub(crate) fn entertainment_base(&self) -> i32 {
        let staffed = |k: u16| self.buildings.iter().filter(|b| b.kind == k && b.workers > 0).count() as i32;
        let (booths, bandstands, pavilions) = (staffed(kind::BOOTH), staffed(kind::BANDSTAND), staffed(kind::PAVILION));
        let pop = self.population;
        let pct = |served: i32| if pop > 0 { (served * 100 / pop).min(100) } else { 0 };
        let sum = pct(400 * (booths + bandstands + pavilions))
            + pct(700 * (bandstands + pavilions))
            + pct(1200 * pavilions)
            + pct(5000 * staffed(SENET_HOUSE))
            + pct(7500 * staffed(ZOO));
        sum / 5 / 5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::mask;

    /// The Sandbox map, and the top-left corner of a clear 8x8 patch of it.
    fn sandbox() -> Option<(World, i32, i32)> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).expect("load map");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        let (w, h) = (world.map.width, world.map.height);
        let (x, y) = (0..h - 8)
            .flat_map(|y| (0..w - 8).map(move |x| (x, y)))
            .find(|&(x, y)| world.map.area_clear_of(x, y, 8, mask::NOT_CLEAR))
            .expect("a clear patch");
        Some((world, x, y))
    }

    fn house(world: &mut World, level: u8, x: i32, y: i32, population: i32) -> BuildingId {
        let id = world.create_building(kind::HOUSE_FIRST + level as u16, x, y);
        world.buildings.get_mut(id).unwrap().house.as_mut().unwrap().population = population;
        world.population += population;
        world.census.add(&world.rng, population);
        id
    }

    fn get(world: &World, id: BuildingId) -> (&crate::buildings::Building, &House) {
        let b = world.buildings.get(id).expect("building");
        (b, b.house.as_ref().expect("house"))
    }

    #[test]
    fn estates_want_two_kinds_of_luxury_goods() {
        use crate::trade::{RESOURCES, TradeCity, TradeRoute, status};
        let Some((mut world, x, y)) = sandbox() else { return };
        let lux = crate::economy::resource::LUXURY_GOODS as usize;
        world.trade.cities.clear();
        assert_eq!(world.luxury_sources(), 0);
        world.create_building(kind::JEWELER, x, y);
        assert_eq!(world.luxury_sources(), 1);
        let route = world.trade.routes.len() as u8;
        let mut limit = vec![0; RESOURCES];
        limit[lux] = 1500;
        world.trade.routes.push(TradeRoute { limit, traded: vec![0; RESOURCES], ..Default::default() });
        let mut sells = vec![false; RESOURCES];
        sells[lux] = true;
        // Name 0 sells jewelry (same kind as our jewelers); names 2 and 27 both sell wine.
        for name_id in [0, 2, 27] {
            world.trade.cities.push(TradeCity { name_id, city_type: 1, route, open: true, sells: sells.clone(), buys: vec![false; RESOURCES], ..Default::default() });
        }
        assert_eq!(world.luxury_sources(), 1, "only while luxury goods are imported");
        world.trade.status[lux] = status::IMPORT_AS_NEEDED;
        assert_eq!(world.luxury_sources(), 2);
        let model = HouseModel { jewelry: 2, ..Default::default() };
        let h = House { goods: [0, 2, 0, 0], coverage: Coverage { water_supply: 1, ..Default::default() }, ..Default::default() };
        let rules = Rules::default();
        assert_eq!(h.meets(&model, &rules, 1), Err(Need::SecondLuxury));
        assert_eq!(h.meets(&model, &rules, 2), Ok(()));
    }

    #[test]
    fn old_saves_with_four_food_groups_load() {
        #[derive(serde::Serialize)]
        struct Old {
            foods: [i32; 4],
        }
        #[derive(serde::Deserialize)]
        struct New {
            #[serde(deserialize_with = "foods_compat")]
            foods: [i32; FOOD_TYPES],
        }
        let bytes = rmp_serde::to_vec_named(&Old { foods: [10, 20, 30, 40] }).unwrap();
        let new: New = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(new.foods, [10, 20, 40, 0, 0, 0, 30, 0]);
        let bytes = rmp_serde::to_vec_named(&House { foods: [1, 2, 3, 4, 5, 6, 7, 8], ..Default::default() }).unwrap();
        let h: House = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(h.foods, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn each_food_counts_as_its_own_kind() {
        let rules = Rules::default();
        let water = Coverage { water_supply: 1, ..Default::default() };
        let mut h = House { coverage: water, ..Default::default() };
        let two = HouseModel { food_types: 2, ..Default::default() };
        let three = HouseModel { food_types: 3, ..Default::default() };
        // Lettuce and figs used to share one slot; now they are two kinds.
        h.foods[2] = 10;
        assert_eq!(h.meets(&two, &rules, 0), Err(Need::Food));
        h.foods[5] = 10;
        assert_eq!(h.meets(&two, &rules, 0), Ok(()));
        assert_eq!(h.meets(&three, &rules, 0), Err(Need::Food));
        h.foods[3] = 10;
        assert_eq!(h.meets(&three, &rules, 0), Ok(()));
    }

    #[test]
    fn bazaar_trader_fills_food_and_goods_for_the_next_level() {
        use crate::economy::resource::*;
        let Some((mut world, x, y)) = sandbox() else { return };
        let bazaar = world.create_building(kind::BAZAAR, x + 4, y + 4);
        let stock = &mut world.buildings.get_mut(bazaar).unwrap().stock;
        stock[GRAIN as usize] = 1000;
        stock[LETTUCE as usize] = 1000;
        stock[FIGS as usize] = 1000;
        stock[POTTERY as usize] = 100;
        stock[BEER as usize] = 100;
        // A small homestead growing into a large one: one food (so two kinds taken),
        // pottery, no beer.
        let id = house(&mut world, 6, x, y, 10);
        world.deliver_to_house(bazaar, id);
        let (_, h) = get(&world, id);
        assert_eq!(h.foods[food_slot(GRAIN).unwrap()], 20, "two meals a head of grain");
        assert_eq!(h.foods[food_slot(LETTUCE).unwrap()], 10, "half as much of other foods");
        assert_eq!(h.foods[food_slot(FIGS).unwrap()], 0, "only two kinds");
        assert_eq!(h.goods, [2, 0, 0, 0], "twice the pottery need, no beer");
        let m = world.buildings.get(bazaar).unwrap();
        assert_eq!(m.goods_demand, [10, 0, 0, 0]);
        // Grain tops up to six meals a head, and a full food doesn't count as taken.
        for _ in 0..4 {
            world.deliver_to_house(bazaar, id);
        }
        let (_, h) = get(&world, id);
        assert_eq!(h.foods[food_slot(GRAIN).unwrap()], 60);
        assert_eq!(h.goods[0], 2);
        // With grain full, the trader moves on to figs.
        assert!(h.foods[food_slot(FIGS).unwrap()] > 0);
        assert_eq!(h.foods.iter().filter(|&&f| f > 0).count(), 3);
    }

    #[test]
    fn bazaars_buy_only_goods_nearby_houses_want() {
        let Some((mut world, x, y)) = sandbox() else { return };
        let bazaar = world.create_building(kind::BAZAAR, x + 4, y + 4);
        assert_eq!(world.unwanted_goods(bazaar), [true; 4], "a new bazaar wants nothing");
        house(&mut world, 6, x, y, 10);
        world.refresh_goods_demand(bazaar);
        assert_eq!(world.buildings.get(bazaar).unwrap().goods_demand, [10, 0, 0, 0]);
        for _ in 0..10 {
            assert_eq!(world.unwanted_goods(bazaar), [false, true, true, true]);
        }
        assert_eq!(world.unwanted_goods(bazaar), [true; 4], "demand runs out");
    }

    #[test]
    fn four_lots_of_a_level_merge() {
        let Some((mut world, x, y)) = sandbox() else { return };
        world.map.random.set(x, y, 0);
        let ids: Vec<BuildingId> = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|&(dx, dy)| house(&mut world, 2, x + dx, y + dy, 5)).collect();
        house(&mut world, 3, x + 2, y, 5);
        world.merge_house(ids[0]);
        let (b, h) = get(&world, ids[0]);
        assert_eq!((b.size, h.merged, h.level, h.population), (2, true, 2, 20));
        assert!(ids[1..].iter().all(|&id| world.buildings.get(id).is_none()));
        assert_eq!(world.house_capacity(ids[0]), 4 * world.balance.house(2).max_people);
        assert_eq!(world.population, 25);
        // A different level next door keeps a 2x2 of level 3 lots from forming.
        world.map.random.set(x + 2, y, 0);
        let lone = world.map.building.at_or(x + 2, y, 0);
        world.merge_house(lone);
        assert_eq!(world.buildings.get(lone).unwrap().size, 1);
    }

    #[test]
    fn apartments_expand_into_residences_and_break_up_again() {
        let Some((mut world, x, y)) = sandbox() else { return };
        let (hx, hy) = (x + 3, y + 3);
        let id = house(&mut world, 9, hx, hy, 18);
        // A humbler neighbour on the block moves in; clear land is built over.
        house(&mut world, 4, hx + 1, hy + 1, 10);
        world.expand_house(id, 2);
        let (b, h) = get(&world, id);
        assert_eq!((b.x, b.y, b.size, h.level, h.merged, h.population), (hx, hy, 2, 10, false, 28));
        assert!(block(hx, hy, 2).all(|(tx, ty)| world.map.building.at_or(tx, ty, 0) == id));
        assert_eq!(world.house_capacity(id), world.balance.house(10).max_people);
        // Devolving gives four spacious apartments sharing the people.
        world.buildings.get_mut(id).unwrap().house.as_mut().unwrap().population = 30;
        world.devolve_house(id);
        let parts: Vec<(u8, i32, i32)> = block(hx, hy, 2)
            .map(|(tx, ty)| {
                let (b, h) = get(&world, world.map.building.at_or(tx, ty, 0));
                (h.level, b.size, h.population)
            })
            .collect();
        assert_eq!(parts, vec![(9, 1, 9), (9, 1, 7), (9, 1, 7), (9, 1, 7)]);
    }

    #[test]
    fn a_house_boxed_in_stays_put() {
        let Some((mut world, x, y)) = sandbox() else { return };
        let (hx, hy) = (x + 3, y + 3);
        let id = house(&mut world, 9, hx, hy, 18);
        // Every 2x2 block holding the house also holds a residence tile or rock.
        for (dx, dy) in [(1, 0), (0, 1), (1, 1), (-1, -1), (-1, 0), (0, -1)] {
            world.map.terrain.update(hx + dx, hy + dy, |t| t | terrain::ROCK);
        }
        assert_eq!(world.expansion_corner(id, 2), None);
        world.expand_house(id, 2);
        let (b, h) = get(&world, id);
        assert_eq!((b.size, h.level), (1, 9));
    }

    #[test]
    fn manors_shrink_to_residences_leaving_gardens() {
        let Some((mut world, x, y)) = sandbox() else { return };
        let id = house(&mut world, 13, x, y, 60);
        world.expand_house(id, 3);
        assert_eq!(get(&world, id).0.size, 3);
        assert_eq!(get(&world, id).1.level, 14);
        world.devolve_house(id);
        let (b, h) = get(&world, id);
        assert_eq!((b.size, h.level, h.population), (2, 13, 10));
        assert_eq!(world.population, 10);
        let gardens = block(x, y, 3).filter(|&(tx, ty)| world.map.terrain_is(tx, ty, terrain::GARDEN)).count();
        assert_eq!(gardens, 5);
    }

    #[test]
    fn any_visit_gives_the_full_entertainment() {
        let mut h = House::default();
        h.coverage.juggler = 1;
        h.derive_culture(0);
        assert_eq!(h.entertainment, 10);
        h.coverage.musician = 96;
        h.coverage.dancer = 5;
        h.coverage.senet = 96;
        h.derive_culture(3);
        assert_eq!(h.entertainment, 100);
        let mut h = House::default();
        h.coverage.juggler = 50;
        h.coverage.musician = 50;
        h.coverage.senet = 50;
        h.coverage.zoo = 50;
        h.derive_culture(0);
        assert_eq!(h.entertainment, 100);
    }

    #[test]
    fn leavers_come_from_the_humblest_houses() {
        let Some((mut world, x, y)) = sandbox() else { return };
        let high = house(&mut world, 5, x, y, 13);
        let low = house(&mut world, 1, x + 2, y, 3);
        let mid = house(&mut world, 2, x + 4, y, 9);
        world.create_emigrants(8);
        // All three from the hut (which empties to a vacant lot), then four, then one.
        assert_eq!(get(&world, low).1.population, 0);
        assert_eq!(get(&world, low).1.level, 0);
        assert_eq!(get(&world, mid).1.population, 5);
        assert_eq!(get(&world, high).1.population, 12);
        assert_eq!(world.population, 17);
    }
}
