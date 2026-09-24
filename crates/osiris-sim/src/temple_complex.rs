//! Temple complexes. A city may raise one, to a god the scenario allows: a 13x7
//! precinct of three linked buildings in a row (the sanctuary, a colonnade and a
//! pylon gateway) in a paved court with statues of the god and an avenue of
//! sphinxes. It employs fifty and sends out priests like a temple, but it counts
//! for its god as much as dozens of temples. Once it stands, an altar can be raised
//! on its pylon and an oracle in its colonnade, each blessing the city in its god's
//! way.

use crate::buildings::BuildingId;
use crate::world::World;

/// Complexes to Osiris, Ra, Ptah, Seth and Bast.
pub const OSIRIS_COMPLEX: u16 = 65;
pub const BAST_COMPLEX: u16 = 69;
pub const SIZE: (i32, i32) = (13, 7);
/// The three parts, 3x3 each, as (column, row) of the laid-out complex.
const PARTS: [(i32, i32); 3] = [(0, 2), (3, 2), (6, 2)];

/// The gods, by index.
pub const OSIRIS: usize = 0;
pub const RA: usize = 1;
pub const PTAH: usize = 2;
pub const SETH: usize = 3;
pub const BAST: usize = 4;

/// The upgrades, as bits of the complex's `upgrades`.
pub const ALTAR: u8 = 1;
pub const ORACLE: u8 = 2;

/// Upgrade tools of the build menu: (building type, god, altar or oracle). The
/// altars are Sebek (Osiris), Ma'at (Ra), Amon (Ptah), Anubis (Seth) and Isis (Bast);
/// the oracles Min, Horus, Thoth, Sekhmet and Hathor.
const UPGRADES: [(u16, usize, u8); 10] = [
    (305, 0, ALTAR),
    (306, 0, ORACLE),
    (307, 1, ALTAR),
    (308, 1, ORACLE),
    (299, 2, ALTAR),
    (300, 2, ORACLE),
    (303, 3, ALTAR),
    (304, 3, ORACLE),
    (309, 4, ALTAR),
    (310, 4, ORACLE),
];
/// The original's upgrade building types, whose model rows give their cost.
const ALTAR_TYPE: u16 = 211;
const ORACLE_TYPE: u16 = 212;

pub fn is_complex(k: u16) -> bool {
    (OSIRIS_COMPLEX..=BAST_COMPLEX).contains(&k)
}

/// The god (0 Osiris .. 4 Bast) a complex is to.
pub fn complex_god(k: u16) -> Option<usize> {
    is_complex(k).then(|| (k - OSIRIS_COMPLEX) as usize)
}

/// The god and upgrade bit of an altar or oracle tool.
pub fn upgrade_of(k: u16) -> Option<(usize, u8)> {
    UPGRADES.iter().find(|u| u.0 == k).map(|u| (u.1, u.2))
}

pub fn is_upgrade(k: u16) -> bool {
    UPGRADES.iter().any(|u| u.0 == k)
}

/// The tiles a complex covers, across and down, by facing. R turns the complex
/// between two facings: 0 runs its parts along x (the original's orientation 0),
/// 1 along y (its orientation 6).
pub fn footprint(facing: u8) -> (i32, i32) {
    if facing == 0 { SIZE } else { (SIZE.1, SIZE.0) }
}

/// The map tile of column `c`, row `r` of a complex whose corner is `(x0, y0)`: the
/// laid-out 13x7 grid as is, or turned so its columns run down.
fn tile((x0, y0): (i32, i32), facing: u8, (c, r): (i32, i32)) -> (i32, i32) {
    if facing == 0 { (x0 + c, y0 + r) } else { (x0 + r, y0 + c) }
}

/// A decorated tile of the court: a floor (four looks), a statue of the god, or a
/// sphinx half, with its image offset in the god's pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decor {
    Floor(u32),
    Statue(u32),
    Sphinx(u32),
}

/// The court's decoration at column `c`, row `r` (none on the parts). The statues
/// and sphinxes turned along y use other images of their groups (exe table 0x5df9b0).
fn decor(c: i32, r: i32, facing: u8) -> Option<Decor> {
    let d = decor_along_x(c, r)?;
    Some(match d {
        Decor::Statue(n) if facing != 0 => Decor::Statue(3 - n),
        Decor::Sphinx(n) if facing != 0 => Decor::Sphinx(7 - n),
        d => d,
    })
}

fn decor_along_x(c: i32, r: i32) -> Option<Decor> {
    use Decor::*;
    let edge = r == 0 || r == 6;
    Some(match (c, r) {
        (0..=8, 2..=4) => return None,
        (0..=1 | 3..=4 | 6..=7, _) if edge => Statue(if r == 0 { 0 } else { 2 }),
        (2 | 5, _) => Floor(1),
        (0..=8, _) => Floor(0),
        (9, _) if edge => Floor(2),
        (10 | 12, _) if edge => Floor(3),
        (11, _) if edge => Floor(2),
        (9, 3) | (10..=12, 3) => Floor(1),
        (9, _) => Floor(0),
        (_, 1) => Sphinx(5),
        (_, 2) => Sphinx(4),
        (_, 4) => Sphinx(1),
        (_, _) => Sphinx(0),
    })
}

impl World {
    /// The city's temple complex, if it has one.
    pub fn temple_complex(&self) -> Option<BuildingId> {
        self.buildings.iter().find(|b| is_complex(b.kind)).map(|b| b.id)
    }

    /// Whether building type `k` (a complex or an upgrade) may be chosen now: a
    /// complex to a god the scenario allows while the city has none; an upgrade for
    /// the city's complex that it lacks.
    pub(crate) fn complex_allowed(&self, k: u16) -> Option<bool> {
        if let Some(g) = complex_god(k) {
            return Some(self.complex_gods.get(g).copied().unwrap_or(false) && self.temple_complex().is_none());
        }
        let &(_, god, bit) = UPGRADES.iter().find(|u| u.0 == k)?;
        let complex = self.temple_complex().and_then(|id| self.buildings.get(id));
        Some(complex.is_some_and(|b| complex_god(b.kind) == Some(god) && b.upgrades & bit == 0))
    }

    /// Lays out a new complex: its three parts and the court about them.
    pub(crate) fn place_temple_complex(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (corner, k, upgrades, facing) = ((b.x, b.y), b.kind, b.upgrades, b.orientation);
        let Some(def) = self.defs.building(k) else { return };
        let img = |key: &str| def.anims.get(key).map(|a| a.image);
        // Turned along y, each part shows the other view of its art: the next
        // image for a built altar or oracle, three on for the rest.
        let (turn, built_turn) = if facing == 0 { (0, 0) } else { (3, 1) };
        let parts = [
            img("main_e").map(|i| i + turn),
            if upgrades & ORACLE != 0 { img("oracle_built").map(|i| i + built_turn) } else { img("oracle_n").map(|i| i + turn) },
            if upgrades & ALTAR != 0 { img("altar_built").map(|i| i + built_turn) } else { img("altar_n").map(|i| i + turn) },
        ];
        let (floor, statue, sphinx) = (img("tiles_0"), img("statue_1"), img("statue_2n"));
        let (w, h) = SIZE;
        let mut singles = Vec::new();
        for r in 0..h {
            for c in 0..w {
                let image = match decor(c, r, facing) {
                    Some(Decor::Floor(n)) => floor.map(|f| f + n),
                    Some(Decor::Statue(n)) => statue.map(|s| s + n),
                    Some(Decor::Sphinx(n)) => sphinx.map(|s| s + n),
                    None => continue,
                };
                if let Some(image) = image {
                    let (x, y) = tile(corner, facing, (c, r));
                    singles.push((x, y, image));
                }
            }
        }
        for (x, y, image) in singles {
            self.map.set_single_image(x, y, image);
        }
        for (&part, image) in PARTS.iter().zip(parts) {
            if let Some(image) = image {
                let (x, y) = tile(corner, facing, part);
                self.map.set_footprint(x, y, 3, image);
            }
        }
    }

    /// Raises an altar or oracle on the city's complex, clicked at `(x, y)`.
    pub(crate) fn build_upgrade(&mut self, k: u16, (x, y): (i32, i32), measure: bool) -> crate::world::Outcome {
        use crate::world::Outcome;
        let Some(&(_, _, bit)) = UPGRADES.iter().find(|u| u.0 == k) else { return Outcome::Invalid("Unknown building") };
        let Some(id) = self.temple_complex() else { return Outcome::Invalid("Build a temple complex first") };
        if self.complex_allowed(k) != Some(true) {
            return Outcome::Invalid("Not available");
        }
        if self.map.building.at_or(x, y, 0) != id {
            return Outcome::Invalid("Place it on your temple complex");
        }
        let cost = self.cost_of(if bit == ALTAR { ALTAR_TYPE } else { ORACLE_TYPE } as usize);
        if measure {
            return Outcome::Done { items: 1, cost };
        }
        if self.out_of_money() {
            return Outcome::NotEnoughMoney;
        }
        self.treasury -= cost;
        self.finance.this_year.construction += cost;
        if let Some(b) = self.buildings.get_mut(id) {
            b.upgrades |= bit;
        }
        self.place_temple_complex(id);
        Outcome::Done { items: 1, cost }
    }

    /// Whether houses need less food: Sebek's altar (Osiris) or a complex to Bast
    /// brings the city's need from a quarter of its people to a fifth.
    pub fn eats_less(&self) -> bool {
        self.complex_blessing(OSIRIS, ALTAR) || self.complex_blessing(BAST, 0)
    }

    /// Whether the city's complex is to god `g` and has upgrade `bit` (0: just the
    /// complex).
    pub fn complex_blessing(&self, g: usize, bit: u8) -> bool {
        self.temple_complex().and_then(|id| self.buildings.get(id)).is_some_and(|b| complex_god(b.kind) == Some(g) && b.upgrades & bit == bit)
    }
}
