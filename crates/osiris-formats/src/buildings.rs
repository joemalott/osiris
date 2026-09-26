//! The building table of a saved game (and of the campaign missions in `mission1.pak`,
//! which are saved games): 4000 records of 0x108 bytes in the `buildings` chunk, the
//! original's building array at 0x7999d8 as it stands in memory. Loading a mission
//! (FUN_004d58b0 -> FUN_004d41e0) reads it back whole, so a mission starts with the
//! buildings its designer left in it.
//!
//! Only the fields a city needs to be set up again are read. Offsets are from the
//! record's start; the functions named are Pharaoh.exe's.

/// Bytes per record.
pub const RECORD_SIZE: usize = 0x108;
/// Records in the table (slot 0 is never used).
pub const RECORDS: usize = 4000;

/// Record states (+0x00): 1 in use, 3 just created (FUN_0046a400 sets it; the next
/// update makes it 1).
pub mod state {
    pub const IN_USE: u8 = 1;
    pub const CREATED: u8 = 3;
}

/// One building record.
#[derive(Clone)]
pub struct BuildingRecord {
    /// Slot in the table, which the records' part links refer to.
    pub id: u16,
    bytes: [u8; RECORD_SIZE],
}

impl std::fmt::Debug for BuildingRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuildingRecord")
            .field("id", &self.id)
            .field("kind", &self.kind())
            .field("x", &self.x())
            .field("y", &self.y())
            .field("size", &self.size())
            .finish()
    }
}

impl BuildingRecord {
    fn u8(&self, at: usize) -> u8 {
        self.bytes[at]
    }

    fn i16(&self, at: usize) -> i16 {
        i16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    /// +0x00: 0 free, 1 in use, 3 just created; other values are ruins and buildings
    /// on their way out.
    pub fn state(&self) -> u8 {
        self.u8(0x00)
    }

    /// Whether the building stands: in use or just created.
    pub fn stands(&self) -> bool {
        matches!(self.state(), state::IN_USE | state::CREATED)
    }

    /// +0x10: building type.
    pub fn kind(&self) -> u16 {
        self.u16(0x10)
    }

    /// +0x03: footprint size in tiles (0 for a temple complex's altar and oracle).
    pub fn size(&self) -> i32 {
        self.u8(0x03) as i32
    }

    /// +0x04: a house of four lots merged into one (FUN_0046a400 clears it).
    pub fn house_merged(&self) -> bool {
        self.u8(0x04) != 0
    }

    /// +0x06, +0x08: the footprint's top corner, in map coordinates (0, 0 the first
    /// playable tile); +0x0c holds the same tile as a grid offset.
    pub fn x(&self) -> i32 {
        self.i16(0x06) as i32
    }

    pub fn y(&self) -> i32 {
        self.i16(0x08) as i32
    }

    /// +0x12: a house's level (type - 10), a part's index in a building of several
    /// records (a pyramid's block, a temple complex's part 3..5), a fort's soldier
    /// figure type (11 archer, 12 charioteer, 13 infantry), a storage room's resource.
    pub fn subtype(&self) -> i16 {
        self.i16(0x12)
    }

    /// +0x1c: a house's people (FUN_00469c60's loop over houses reads it).
    pub fn population(&self) -> i32 {
        self.i16(0x1c) as i32
    }

    /// +0x38, +0x3a: the previous and next record of a building of several parts
    /// (pyramid blocks, temple complex parts, storage rooms, a fort's parade ground).
    pub fn prev_part(&self) -> u16 {
        self.u16(0x38)
    }

    pub fn next_part(&self) -> u16 {
        self.u16(0x3a)
    }

    /// +0x3c: what a storage room holds (the yard's carts take 100 at a time from it).
    pub fn stored(&self) -> i32 {
        self.i16(0x3c) as i32
    }

    /// A house's goods and foods, by slot (+0x54 + 2 * slot): resources 1..8 in slots
    /// 0..7, beer 8, pottery 9, linen 10, luxury goods 11.
    pub fn house_slot(&self, slot: usize) -> i32 {
        if slot >= 12 {
            return 0;
        }
        self.i16(0x54 + 2 * slot) as i32
    }

    /// A granary's store of resource `r` (+0x56 + 2 * r; +0x56 itself is the room
    /// left, FUN_0045e360).
    pub fn granary_stock(&self, r: usize) -> i32 {
        if !(1..=35).contains(&r) {
            return 0;
        }
        self.i16(0x56 + 2 * r) as i32
    }

    /// A pyramid block's state (+0x55, 1 once cased) and course (+0x57), and its
    /// top course (+0x58).
    pub fn block_state(&self) -> u8 {
        self.u8(0x55)
    }

    pub fn block_level(&self) -> u8 {
        self.u8(0x57)
    }

    pub fn block_top(&self) -> u8 {
        self.u8(0x58)
    }

    /// +0xa9: a temple complex's altar (1) and oracle (2).
    pub fn upgrades(&self) -> u8 {
        self.u8(0xa9)
    }

    /// +0xaa: which way the building faces. A statue's look in the high nibble and
    /// facing in the low one; a gatehouse's run (0 along y, 1 along x); a temple
    /// complex's orientation (0, 2, 4, 6); a shore building's side of the water.
    pub fn orientation(&self) -> u8 {
        self.u8(0xaa)
    }

    /// +0xc2: a house's happiness with the governor.
    pub fn happiness(&self) -> i32 {
        self.u8(0xc2) as i32
    }
}

/// The standing buildings of a `buildings` chunk, in table order.
pub fn records(chunk: &[u8]) -> Vec<BuildingRecord> {
    chunk
        .as_chunks::<RECORD_SIZE>()
        .0
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, b)| b[0] != 0)
        .map(|(id, b)| BuildingRecord { id: id as u16, bytes: *b })
        .filter(BuildingRecord::stands)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_records_are_read_with_their_fields() {
        let mut chunk = vec![0u8; RECORD_SIZE * 4];
        let mut put = |id: usize, at: usize, bytes: &[u8]| chunk[id * RECORD_SIZE + at..][..bytes.len()].copy_from_slice(bytes);
        // A 2x2 merged rough cottage of 35 people at (145, 140).
        put(1, 0x00, &[1, 1, 0, 2, 1, 2]);
        put(1, 0x06, &145i16.to_le_bytes());
        put(1, 0x08, &140i16.to_le_bytes());
        put(1, 0x10, &14u16.to_le_bytes());
        put(1, 0x1c, &35i16.to_le_bytes());
        put(1, 0x54 + 2 * 7, &185i16.to_le_bytes());
        // A ruin on its way out, and a statue just created.
        put(2, 0x00, &[6]);
        put(3, 0x00, &[3, 1, 0, 1]);
        put(3, 0x10, &41u16.to_le_bytes());
        put(3, 0xaa, &[0x43]);
        let r = records(&chunk);
        assert_eq!(r.iter().map(|r| r.id).collect::<Vec<_>>(), [1, 3]);
        let house = &r[0];
        assert_eq!((house.kind(), house.x(), house.y(), house.size(), house.house_merged()), (14, 145, 140, 2, true));
        assert_eq!((house.population(), house.house_slot(7)), (35, 185));
        assert_eq!((r[1].kind(), r[1].state(), r[1].orientation()), (41, state::CREATED, 0x43));
    }
}
