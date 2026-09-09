// SPDX-License-Identifier: Apache-2.0
//! The whole simulated state and its fixed-rate tick.

use crate::grid::Cell;
use crate::island::IslandMap;
use crate::path::find_path;
use crate::structure::Structure;
use crate::unit::{Pos, Unit};

/// Simulation ticks per second. Rendering interpolates or samples; the
/// simulation never sees wall-clock time.
pub const TICK_HZ: u32 = 30;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct World {
    pub tick: u64,
    pub islands: Vec<IslandMap>,
    pub structures: Vec<Structure>,
    pub units: Vec<Unit>,
}

impl World {
    pub fn new() -> World {
        World::default()
    }

    /// Whether any island covers `cell`.
    pub fn is_land(&self, cell: Cell) -> bool {
        self.islands.iter().any(|i| i.contains(cell))
    }

    /// Land that no walk-blocking structure covers.
    pub fn is_walkable(&self, cell: Cell) -> bool {
        self.is_land(cell) && !self.structures.iter().any(|s| s.blocks_walking && s.covers(cell))
    }

    /// Order a unit to walk to `cell` along the shortest walkable path.
    /// Returns `false` if the cell is unreachable.
    pub fn order_move(&mut self, unit: usize, cell: Cell) -> bool {
        let Some(from) = self.units.get(unit).map(|u| u.pos.cell()) else { return false };
        let Some(path) = find_path(from, cell, |c| self.is_walkable(c)) else { return false };
        let u = &mut self.units[unit];
        u.path = path.into_iter().map(Pos::cell_centre).collect();
        if u.path.is_empty() {
            // Already in the cell: walk to its centre so the unit settles.
            u.path.push_back(Pos::cell_centre(cell));
        }
        true
    }

    /// Advance the simulation by one tick.
    pub fn step(&mut self) {
        for u in &mut self.units {
            u.step();
        }
        self.tick += 1;
    }
}

/// Convert a type's `speed` property (cells per second) to fixed-point steps per tick.
pub fn speed_per_tick(cells_per_second: f64) -> i32 {
    ((cells_per_second * crate::unit::SUBCELL as f64) / TICK_HZ as f64).round().max(1.0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new();
        w.islands.push(IslandMap::rect(Cell::new(0, 0), 8, 4));
        w.units.push(Unit::new("priest", Cell::new(0, 1), speed_per_tick(1.0)));
        w
    }

    #[test]
    fn orders_only_onto_land() {
        let mut w = world();
        assert!(!w.order_move(0, Cell::new(9, 9)));
        assert!(w.order_move(0, Cell::new(7, 3)));
        for _ in 0..400 {
            w.step();
        }
        assert_eq!(w.units[0].pos.cell(), Cell::new(7, 3));
        assert!(!w.units[0].is_moving());
        assert_eq!(w.tick, 400);
    }

    #[test]
    fn walks_around_a_blocking_structure() {
        let mut w = world();
        // A 1x4 wall at x=4 covering the whole island height.
        w.structures.push(Structure::new("wall", Cell::new(4, 3), 1, 4, true));
        assert!(!w.is_walkable(Cell::new(4, 2)));
        assert!(!w.order_move(0, Cell::new(7, 1)), "wall seals the island");
        w.structures[0].foot_y = 3; // open a gap at y=0
        assert!(w.order_move(0, Cell::new(7, 1)));
        let mut visited = Vec::new();
        for _ in 0..600 {
            w.step();
            visited.push(w.units[0].pos.cell());
            if !w.units[0].is_moving() {
                break;
            }
        }
        assert_eq!(w.units[0].pos.cell(), Cell::new(7, 1));
        assert!(visited.iter().all(|&c| w.is_walkable(c)), "never stood on the wall");
        assert!(visited.contains(&Cell::new(4, 0)), "went through the gap");
    }

    #[test]
    fn non_blocking_structure_is_walkable() {
        let mut w = world();
        w.structures.push(Structure::new("dais", Cell::new(4, 3), 2, 2, false));
        assert!(w.is_walkable(Cell::new(4, 3)));
    }

    #[test]
    fn speed_conversion() {
        assert_eq!(speed_per_tick(1.0), 9);
        assert_eq!(speed_per_tick(0.0), 1);
    }
}
