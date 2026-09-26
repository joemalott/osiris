//! The buildings a campaign mission starts with. The missions in `mission1.pak` are
//! saved games, and the original loads their building table whole (FUN_004d58b0 ->
//! FUN_004d41e0), so a mission such as Thinis (Civil War) opens with the city its
//! designer left: houses, a granary, statues, a temple complex, a mansion, walls. Here
//! each standing record becomes an Osiris building of the same kind on the same tiles,
//! set up the way building it would (its images, terrain, road access, a fort's
//! company, a complex's court), with the state that matters at the start: a house's
//! level, size and people with their food and goods, a statue's look and facing, a
//! gatehouse's run, a complex's altar and oracle, what granaries and storage yards
//! hold, a finished pyramid. The original's walls are terrain alone; they become
//! Osiris's wall pieces. Walkers, soldiers and animals on the map are not carried
//! over: the buildings send out their own.
//!
//! The terrain pass has already run (`World::new`) with the file's building tiles
//! left alone, as the original's leaves them; each building then draws itself over
//! its footprint, replacing the stored images (which older missions hold in an
//! earlier sprite layout).

use crate::buildings::{BuildingId, kind};
use crate::map::terrain;
use crate::world::World;
use osiris_formats::buildings::BuildingRecord;

/// The original's types that are parts of another record's building, or markers
/// on it: a storage yard's rooms, a fort's parade ground, a temple complex's altar
/// and oracle.
const STORAGE_ROOM: u16 = 73;
const FORT: u16 = 57;
const PYRAMID_BLOCK: u16 = 183;
const COMPLEX_ALTAR: u16 = 211;
const COMPLEX_ORACLE: u16 = 212;

/// Terrain the file's buildings leave on their tiles, cleared before a building is
/// set up there again.
const BUILDING_BITS: u32 = terrain::BUILDING | terrain::GATEHOUSE | terrain::WALKABLE_BUILDING;

/// What a record becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub kind: u16,
    pub x: i32,
    pub y: i32,
    /// A gatehouse's or temple complex's facing, as Osiris's build tool keeps it.
    pub facing: u8,
}

impl World {
    /// Sets up the buildings of a saved game's building table (see the module docs).
    /// `version` is the file's: statues saved before version 160 keep their look
    /// and facing another way.
    pub(crate) fn import_buildings(&mut self, records: &[BuildingRecord], version: i32) {
        if records.is_empty() && !(0..self.map.height).any(|y| (0..self.map.width).any(|x| self.map.terrain_is(x, y, terrain::WALL))) {
            return;
        }
        let saved = (self.complex_facing, self.gatehouse_facing, self.statue_variant, self.statue_facing);
        // Walls first, so the towers and gatehouses beside them redraw them.
        self.import_walls();
        for r in records {
            let Some(p) = self.placement(r, records) else { continue };
            self.import_building(r, records, p, version);
        }
        (self.complex_facing, self.gatehouse_facing, self.statue_variant, self.statue_facing) = saved;
        self.clear_orphan_building_tiles();
    }

    /// Where and as what record `r` is set up, or `None` for a part of another
    /// record's building and anything Osiris has no building for.
    pub fn placement(&self, r: &BuildingRecord, records: &[BuildingRecord]) -> Option<Placement> {
        let at = |kind: u16, facing: u8| Some(Placement { kind, x: r.x(), y: r.y(), facing });
        match r.kind() {
            STORAGE_ROOM | crate::military::FORT_GROUND | COMPLEX_ALTAR | COMPLEX_ORACLE => None,
            // A fort's soldiers are its figure type (FUN_00470990 sets them by the
            // fort chosen).
            FORT => match r.subtype() {
                11 => at(crate::military::FORT_ARCHERS, 0),
                12 => at(crate::military::FORT_CHARIOTEERS, 0),
                13 => at(crate::military::FORT_INFANTRY, 0),
                _ => None,
            },
            PYRAMID_BLOCK => {
                if r.prev_part() != 0 {
                    return None;
                }
                let blocks = chain(r, records);
                let x = blocks.iter().map(|b| b.x()).min()?;
                let y = blocks.iter().map(|b| b.y()).min()?;
                let side = blocks.iter().map(|b| b.x() + b.size()).max()? - x;
                // The table's pyramid type is every family's: the scenario's
                // monument of that size says which it is.
                let def = self.scenario_monuments.iter().filter_map(|&t| crate::monuments::monument_for_title(t as usize)).find(|d| {
                    matches!(d.style, crate::monuments::Style::Pyramid(_)) && d.cols * 2 == side && d.rows * 2 == side
                })?;
                Some(Placement { kind: def.kind, x, y, facing: 0 })
            }
            k if crate::temple_complex::is_complex(k) => {
                if r.prev_part() != 0 {
                    return None;
                }
                // The parts run from the main one by the orientation (exe table
                // 0x5df970), and the court's corner lies off it (0x5e00cc). Osiris
                // lays a complex along x or along y with the sanctuary at the low
                // end; the original's 2 and 4 are those turned end for end.
                let (x, y, facing) = match r.orientation() {
                    0 => (r.x(), r.y() - 2, 0),
                    2 => (r.x() - 2, r.y() - 10, 1),
                    4 => (r.x() - 10, r.y() - 2, 0),
                    _ => (r.x() - 2, r.y(), 1),
                };
                Some(Placement { kind: k, x, y, facing })
            }
            crate::defenses::GATEHOUSE => at(crate::defenses::GATEHOUSE, u8::from(r.orientation() & 1 != 0)),
            // Akhenaten's one-tile gatehouse: not the original's, never in its files.
            crate::defenses::OLD_GATEHOUSE => None,
            crate::defenses::OLD_WALL => at(crate::defenses::WALL, 0),
            crate::defenses::OLD_TOWER => at(crate::defenses::TOWER, 0),
            k if crate::missions::is_obsolete_kind(k) => at(crate::missions::upgraded_palace(k), 0),
            k if self.defs.building(k).is_some() => at(k, 0),
            _ => None,
        }
    }

    fn import_building(&mut self, r: &BuildingRecord, records: &[BuildingRecord], p: Placement, version: i32) {
        let k = p.kind;
        self.complex_facing = p.facing;
        self.gatehouse_facing = p.facing;
        (self.statue_variant, self.statue_facing) = statue_look(r.orientation(), version);
        let (w, h) = self.footprint_of(k);
        let mut area: Vec<(i32, i32)> = rect(p.x, p.y, w, h).collect();
        if crate::military::fort_soldier(k).is_some() {
            area.extend(crate::military::fort_ground(p.x, p.y));
        }
        if kind::is_house(k) {
            area.extend(rect(p.x, p.y, r.size(), r.size()));
        }
        // A farm the flood covers is water in the file, its record kept (the original
        // gives the tiles back when the water goes); Osiris's farms stand through the
        // flood on dry tiles.
        for &(x, y) in &area {
            self.map.terrain.update(x, y, |t| {
                let t = t & !BUILDING_BITS;
                if t & terrain::FLOODPLAIN != 0 { t & !(terrain::WATER | terrain::DEEPWATER) } else { t }
            });
        }
        // Water under a dock or wharf is drawn afresh, for when the building goes.
        if crate::water::is_shore_building(k) {
            crate::terrain_images::refresh_water(&mut self.map, &self.defs, p.x, p.y, p.x + w - 1, p.y + h - 1);
        }
        let id = self.create_building(k, p.x, p.y);
        if kind::is_house(k) {
            self.import_house(id, r);
        } else if crate::temple_complex::is_complex(k) && r.upgrades() != 0 {
            if let Some(b) = self.buildings.get_mut(id) {
                b.upgrades = r.upgrades() & (crate::temple_complex::ALTAR | crate::temple_complex::ORACLE);
            }
            self.place_temple_complex(id);
        } else if k == kind::GRANARY {
            if let Some(b) = self.buildings.get_mut(id) {
                for (res, n) in b.stock.iter_mut().enumerate() {
                    *n = r.granary_stock(res).max(0);
                }
            }
        } else if k == kind::STORAGE_YARD {
            self.import_storage_rooms(id, r, records);
        } else if crate::monuments::monument_def(k).is_some() {
            let blocks = chain(r, records);
            if blocks.iter().all(|b| b.block_state() == 1 && b.block_level() >= b.block_top()) {
                self.set_tomb_stage(id, crate::pyramids::POLISH + 1);
            }
        }
    }

    /// A house as the file has it: its level (from its type), size, people, happiness,
    /// foods and goods.
    fn import_house(&mut self, id: BuildingId, r: &BuildingRecord) {
        let size = r.size().max(1);
        let Some(b) = self.buildings.get_mut(id) else { return };
        let (x, y) = (b.x, b.y);
        b.size = size;
        let Some(h) = b.house.as_mut() else { return };
        let level = h.level;
        h.merged = r.house_merged();
        h.population = r.population().max(0);
        h.happiness = r.happiness().min(100);
        for (slot, food) in h.foods.iter_mut().enumerate() {
            *food = r.house_slot(slot).max(0);
        }
        // Osiris's goods are pottery, luxury goods, linen and beer; the original's
        // slots 9, 11, 10 and 8.
        h.goods = [9, 11, 10, 8].map(|slot| r.house_slot(slot).max(0));
        let people = h.population;
        for (xx, yy) in rect(x, y, size, size) {
            self.map.terrain.update(xx, yy, |t| (t & !(terrain::SHRUB | terrain::TREE | terrain::GARDEN)) | terrain::BUILDING);
            self.map.building.set(xx, yy, id);
        }
        // An empty lot keeps its sign; a lived-in house takes its level's look.
        if people > 0 || level > 0 {
            self.set_house_level(id, level);
        }
        self.refresh_road_access(id);
        self.population += people;
        self.census.add(&self.rng, people);
    }

    /// A storage yard's rooms (the office's chain of records) fill the spaces on the
    /// same tiles.
    fn import_storage_rooms(&mut self, id: BuildingId, office: &BuildingRecord, records: &[BuildingRecord]) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        let (x, y) = (b.x, b.y);
        for room in chain(office, records).into_iter().skip(1) {
            let at = (room.x() - x, room.y() - y);
            let Some(i) = crate::storage::YARD_TILES.iter().skip(1).position(|&t| t == at) else { continue };
            let (res, n) = (room.subtype(), room.stored());
            if let Some(space) = b.spaces.get_mut(i)
                && res > 0
                && n > 0
            {
                *space = (res as u16, n);
            }
        }
        self.refresh_yard_images(id);
    }

    /// The original's walls stand in its terrain alone (no record, FUN_0047b1a0 draws
    /// them); each piece becomes one of Osiris's.
    fn import_walls(&mut self) {
        let walls: Vec<(i32, i32)> = (0..self.map.height)
            .flat_map(|y| (0..self.map.width).map(move |x| (x, y)))
            .filter(|&(x, y)| self.map.terrain_is(x, y, terrain::WALL) && self.map.building.at_or(x, y, 0) == 0)
            .collect();
        for (x, y) in walls {
            self.map.terrain.update(x, y, |t| t & !BUILDING_BITS);
            self.create_building(crate::defenses::WALL, x, y);
        }
    }

    /// Tiles the file marks as built on that no building took: their marks go and
    /// their land is drawn.
    fn clear_orphan_building_tiles(&mut self) {
        let orphans: Vec<(i32, i32)> = (0..self.map.height)
            .flat_map(|y| (0..self.map.width).map(move |x| (x, y)))
            .filter(|&(x, y)| self.map.terrain_is(x, y, BUILDING_BITS | terrain::WALL) && self.map.building.at_or(x, y, 0) == 0)
            .collect();
        for &(x, y) in &orphans {
            self.map.terrain.update(x, y, |t| t & !(BUILDING_BITS | terrain::WALL));
            self.map.set_single_image(x, y, 0);
        }
        for (x, y) in orphans {
            self.refresh_land(x, y, x, y);
        }
    }
}

/// Record `first` and the records linked after it, in order.
fn chain<'a>(first: &'a BuildingRecord, records: &'a [BuildingRecord]) -> Vec<&'a BuildingRecord> {
    let mut v = vec![first];
    let mut next = first.next_part();
    while next != 0 && v.len() < records.len() {
        let Some(r) = records.iter().find(|r| r.id == next) else { break };
        v.push(r);
        next = r.next_part();
    }
    v
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> impl Iterator<Item = (i32, i32)> {
    (y..y + h).flat_map(move |yy| (x..x + w).map(move |xx| (xx, yy)))
}

/// A statue's look (0-3) and the offset of its facing within the look's four images,
/// from its orientation byte. Pharaoh.exe keeps the look in the high nibble and the
/// facing, one past the image, in the low one (FUN_0046a400 stores it; FUN_00470990
/// draws look * 4 + facing - 1 at the default view). The campaign missions saved
/// before version 160 keep the look in the top two bits and the image's own offset
/// in the low ones: their stored images show it.
fn statue_look(orientation: u8, version: i32) -> (u8, u8) {
    if version < 160 { (orientation >> 6, orientation & 3) } else { ((orientation >> 4) & 3, (orientation & 0xf).wrapping_sub(1) & 3) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The looks and facings the statues of the campaign missions show in their
    /// stored images: Pharaoh_General 61 +7 for 0x43 at version 149, Pharaoh_General 7
    /// +1 for 0x02 and Expansion 35 +4 +2 (look 3) for 0x33 at version 160.
    #[test]
    fn statue_looks_follow_the_file_version() {
        assert_eq!(statue_look(0x43, 149), (1, 3));
        assert_eq!(statue_look(0x02, 147), (0, 2));
        assert_eq!(statue_look(0x02, 160), (0, 1));
        assert_eq!(statue_look(0x33, 160), (3, 2));
        assert_eq!(statue_look(0x40, 160), (0, 3));
    }
}
