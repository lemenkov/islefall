// SPDX-License-Identifier: Apache-2.0
//! The whole simulated state and its fixed-rate tick.

use crate::grid::Cell;
use crate::island::IslandMap;
use crate::unit::{Pos, Unit};

/// Simulation ticks per second. Rendering interpolates or samples; the
/// simulation never sees wall-clock time.
pub const TICK_HZ: u32 = 30;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct World {
    pub tick: u64,
    pub islands: Vec<IslandMap>,
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

    /// Order a unit to walk to the centre of `cell`. Refused off land.
    pub fn order_move(&mut self, unit: usize, cell: Cell) -> bool {
        if !self.is_land(cell) {
            return false;
        }
        if let Some(u) = self.units.get_mut(unit) {
            u.target = Some(Pos::cell_centre(cell));
            return true;
        }
        false
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

    #[test]
    fn orders_only_onto_land() {
        let mut w = World::new();
        w.islands.push(IslandMap::rect(Cell::new(0, 0), 4, 4));
        w.units.push(Unit::new("priest", Cell::new(1, 1), speed_per_tick(1.0)));
        assert!(!w.order_move(0, Cell::new(9, 9)));
        assert!(w.order_move(0, Cell::new(3, 3)));
        for _ in 0..200 {
            w.step();
        }
        assert_eq!(w.units[0].pos.cell(), Cell::new(3, 3));
        assert!(!w.units[0].is_moving());
        assert_eq!(w.tick, 200);
    }

    #[test]
    fn speed_conversion() {
        assert_eq!(speed_per_tick(1.0), 9);
        assert_eq!(speed_per_tick(0.0), 1);
    }
}
