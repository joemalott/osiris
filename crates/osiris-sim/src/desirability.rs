//! Desirability: every building (and some terrain) spreads a value into the rings of
//! tiles around it, weakening by `step_size` every `step` rings, out to `range` (at
//! most 6). Houses evolve on the desirability of their footprint.

use crate::buildings::Buildings;
use crate::grid::Grid;
use crate::map::{Map, terrain};

#[derive(Debug, Clone, Copy, Default)]
pub struct Influence {
    pub value: i32,
    pub step: i32,
    pub step_size: i32,
    pub range: i32,
}

const PLAZA: Influence = Influence { value: 4, step: 1, step_size: -2, range: 2 };
const EARTHQUAKE: Influence = Influence { value: -2, step: 1, step_size: 1, range: 3 };
const GARDEN: Influence = Influence { value: 3, step: 1, step_size: -1, range: 3 };
const RUBBLE: Influence = Influence { value: -2, step: 1, step_size: 1, range: 2 };

fn add_ring(grid: &mut Grid<i8>, x: i32, y: i32, size: i32, distance: i32, value: i32) {
    let (x0, y0, x1, y1) = (x - distance, y - distance, x + size - 1 + distance, y + size - 1 + distance);
    let add = |grid: &mut Grid<i8>, xx: i32, yy: i32| grid.update(xx, yy, |d| (d as i32 + value).clamp(-100, 100) as i8);
    // The ring's tiles are all different, so the order they are visited in doesn't
    // matter: the top and bottom rows, then the two sides between them.
    for xx in x0..=x1 {
        add(grid, xx, y0);
        add(grid, xx, y1);
    }
    for yy in y0 + 1..y1 {
        add(grid, x0, yy);
        add(grid, x1, yy);
    }
}

/// Buildings wider than six tiles spread nothing.
pub fn spread(grid: &mut Grid<i8>, x: i32, y: i32, size: i32, inf: Influence) {
    if size <= 0 || size > 6 {
        return;
    }
    let mut value = inf.value;
    let mut within_step = 0;
    for distance in 1..=inf.range.min(6) {
        add_ring(grid, x, y, size, distance, value);
        within_step += 1;
        if within_step >= inf.step {
            value += inf.step_size;
            within_step = 0;
        }
    }
}

/// Recomputes the whole grid from buildings (via `influence_of`) and terrain.
pub fn recompute(
    grid: &mut Grid<i8>,
    map: &Map,
    buildings: &Buildings,
    influence_of: impl Fn(&crate::buildings::Building) -> Influence,
) {
    grid.fill(0);
    for b in buildings.iter() {
        spread(grid, b.x, b.y, b.size, influence_of(b));
    }
    for y in 0..map.height {
        for x in 0..map.width {
            let t = map.terrain.at_or(x, y, 0);
            // A plaza or quake mark rules out the garden and rubble checks.
            let plaza_or_quake = map.bitfields.at_or(x, y, 0) & 0x80 != 0;
            let inf = if plaza_or_quake {
                if t & terrain::ROAD != 0 {
                    Some(PLAZA)
                } else if t & terrain::ROCK != 0 {
                    Some(EARTHQUAKE)
                } else {
                    None
                }
            } else if t & terrain::GARDEN != 0 {
                Some(GARDEN)
            } else if t & terrain::RUBBLE != 0 {
                Some(RUBBLE)
            } else {
                None
            };
            if let Some(inf) = inf {
                spread(grid, x, y, 1, inf);
            }
        }
    }
}

/// The desirability a building sees: the best tile of its footprint, plus bonuses for
/// adjacent water and elevation.
pub fn at_building(grid: &Grid<i8>, map: &Map, x: i32, y: i32, size: i32) -> i32 {
    let mut best = i32::MIN;
    for dy in 0..size {
        for dx in 0..size {
            best = best.max(grid.at_or(x + dx, y + dy, 0) as i32);
        }
    }
    let adjacent_water = crate::buildings::ring(x, y, size).any(|(xx, yy)| map.terrain_is(xx, yy, terrain::WATER));
    if adjacent_water {
        best += 10;
    }
    best += match map.elevation.at_or(x, y, 0) {
        0 => 0,
        1 => 10,
        2 => 12,
        3 => 14,
        4 => 16,
        _ => 18,
    };
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_in_weakening_rings() {
        let mut g: Grid<i8> = Grid::new(20, 20);
        spread(&mut g, 10, 10, 1, Influence { value: 4, step: 1, step_size: -1, range: 3 });
        assert_eq!(g.get(10, 10), Some(0)); // own tile gets nothing
        assert_eq!(g.get(11, 10), Some(4));
        assert_eq!(g.get(12, 12), Some(3));
        assert_eq!(g.get(13, 7), Some(2));
        assert_eq!(g.get(14, 10), Some(0));
    }

    #[test]
    fn rubble_fades_toward_zero() {
        let mut g: Grid<i8> = Grid::new(20, 20);
        spread(&mut g, 10, 10, 1, RUBBLE);
        assert_eq!(g.get(11, 10), Some(-2));
        assert_eq!(g.get(12, 10), Some(-1));
        assert_eq!(g.get(13, 10), Some(0));
    }
}
