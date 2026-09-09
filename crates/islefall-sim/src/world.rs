// SPDX-License-Identifier: Apache-2.0
//! The whole simulated state and its fixed-rate tick.

use std::collections::BTreeSet;

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
    /// Cells carrying a bridge tile. Bridges are one cell wide.
    pub bridges: BTreeSet<Cell>,
    /// Incremented whenever `bridges` changes, so renderers can refresh.
    pub bridge_version: u64,
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

    pub fn is_bridge(&self, cell: Cell) -> bool {
        self.bridges.contains(&cell)
    }

    /// Ground of any kind: island or bridge.
    pub fn is_ground(&self, cell: Cell) -> bool {
        self.is_land(cell) || self.is_bridge(cell)
    }

    /// Ground that no walk-blocking structure covers.
    pub fn is_walkable(&self, cell: Cell) -> bool {
        self.is_ground(cell) && !self.structures.iter().any(|s| s.blocks_walking && s.covers(cell))
    }

    /// A bridge cell may be placed on empty sky next to existing ground.
    /// Diagonal neighbours do not count: bridges connect orthogonally.
    pub fn can_place_bridge(&self, cell: Cell) -> bool {
        !self.is_ground(cell) && [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| self.is_ground(cell.offset(dx, dy)))
    }

    pub fn place_bridge(&mut self, cell: Cell) -> bool {
        if !self.can_place_bridge(cell) {
            return false;
        }
        self.bridges.insert(cell);
        self.bridge_version += 1;
        true
    }

    /// Connection mask of a bridge cell (north 1, east 2, south 4, west 8):
    /// a side connects when the neighbour is bridge or island.
    pub fn bridge_connections(&self, cell: Cell) -> u8 {
        let mut m = 0;
        for (bit, dx, dy) in [(1u8, 0, -1), (2, 1, 0), (4, 0, 1), (8, -1, 0)] {
            if self.is_ground(cell.offset(dx, dy)) {
                m |= bit;
            }
        }
        m
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
    fn bridges_attach_to_ground_and_connect() {
        let mut w = world();
        assert!(!w.place_bridge(Cell::new(3, 1)), "already land");
        assert!(!w.place_bridge(Cell::new(9, 1)), "not adjacent to ground");
        assert!(w.place_bridge(Cell::new(8, 1)), "east of the island edge");
        assert!(w.place_bridge(Cell::new(9, 1)));
        assert!(w.place_bridge(Cell::new(9, 0)));
        assert_eq!(w.bridge_version, 3);
        assert_eq!(w.bridge_connections(Cell::new(8, 1)), 2 | 8, "east to bridge, west to island");
        assert_eq!(w.bridge_connections(Cell::new(9, 1)), 1 | 8);
        assert_eq!(w.bridge_connections(Cell::new(9, 0)), 4);
        assert!(w.is_walkable(Cell::new(9, 0)));
        assert!(w.order_move(0, Cell::new(9, 0)), "units walk over bridges");
    }

    #[test]
    fn speed_conversion() {
        assert_eq!(speed_per_tick(1.0), 9);
        assert_eq!(speed_per_tick(0.0), 1);
    }
}
