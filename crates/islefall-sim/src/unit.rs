// SPDX-License-Identifier: Apache-2.0
//! Units: things that stand on cells and move between them.
//!
//! Positions are fixed point with [`SUBCELL`] steps per cell so that the
//! simulation stays integer-only and deterministic across machines.

use std::collections::VecDeque;

use crate::grid::Cell;

/// Fixed-point steps per cell.
pub const SUBCELL: i32 = 256;

/// The eight facing directions, in the order the walker animations are
/// listed in the type files (`A` = north, clockwise to `H` = north-west).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir8 {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Dir8 {
    pub const ALL: [Dir8; 8] = [Dir8::N, Dir8::NE, Dir8::E, Dir8::SE, Dir8::S, Dir8::SW, Dir8::W, Dir8::NW];

    /// Animation label of this facing in a walker type (`A`..`H`).
    pub fn animation(self) -> &'static str {
        ["A", "B", "C", "D", "E", "F", "G", "H"][self as usize]
    }

    /// Facing closest to a movement vector (y grows downwards, as on screen).
    pub fn from_vector(dx: i64, dy: i64) -> Option<Dir8> {
        if dx == 0 && dy == 0 {
            return None;
        }
        // Compare against tan(22.5 deg) = 0.4142 using integers: |minor| * 12 <= |major| * 5
        // is roughly minor/major <= 0.4167, close enough to split 45 degree sectors.
        let (ax, ay) = (dx.abs(), dy.abs());
        let diagonal = ax * 12 > ay * 5 && ay * 12 > ax * 5;
        Some(match (dx.signum(), dy.signum(), diagonal) {
            (_, -1, false) if ay >= ax => Dir8::N,
            (_, 1, false) if ay >= ax => Dir8::S,
            (1, _, false) => Dir8::E,
            (-1, _, false) => Dir8::W,
            (1, -1, true) => Dir8::NE,
            (1, 1, true) => Dir8::SE,
            (-1, 1, true) => Dir8::SW,
            (-1, -1, true) => Dir8::NW,
            // Unreachable combinations (zero component with diagonal=false handled above).
            _ => Dir8::N,
        })
    }
}

/// A fixed-point map position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    /// Centre of a cell.
    pub const fn cell_centre(cell: Cell) -> Pos {
        Pos { x: cell.x * SUBCELL + SUBCELL / 2, y: cell.y * SUBCELL + SUBCELL / 2 }
    }

    pub const fn cell(self) -> Cell {
        Cell::new(self.x.div_euclid(SUBCELL), self.y.div_euclid(SUBCELL))
    }

    /// Position in cell units as floats, for rendering only.
    pub fn to_f32(self) -> (f32, f32) {
        (self.x as f32 / SUBCELL as f32, self.y as f32 / SUBCELL as f32)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// Type file stem, e.g. `priest`.
    pub kind: String,
    pub pos: Pos,
    pub facing: Dir8,
    /// Waypoints still to visit, front first.
    pub path: VecDeque<Pos>,
    /// Movement per tick in fixed-point steps.
    pub speed: i32,
    /// Dead units stay in the list so indices remain stable.
    pub alive: bool,
}

impl Unit {
    pub fn new(kind: impl Into<String>, cell: Cell, speed: i32) -> Unit {
        Unit { kind: kind.into(), pos: Pos::cell_centre(cell), facing: Dir8::S, path: VecDeque::new(), speed, alive: true }
    }

    pub fn is_moving(&self) -> bool {
        !self.path.is_empty()
    }

    /// Advance one tick along the path, straight towards the next waypoint.
    /// Leftover movement after reaching a waypoint is not carried over, so a
    /// unit takes a whole tick per waypoint at most once per cell.
    pub fn step(&mut self) {
        if !self.alive {
            return;
        }
        let Some(&t) = self.path.front() else { return };
        let (dx, dy) = ((t.x - self.pos.x) as i64, (t.y - self.pos.y) as i64);
        let dist = ((dx * dx + dy * dy) as u64).isqrt() as i64;
        if let Some(f) = Dir8::from_vector(dx, dy) {
            self.facing = f;
        }
        if dist <= self.speed as i64 {
            self.pos = t;
            self.path.pop_front();
            return;
        }
        // Integer projection of the step onto the direction; rounding keeps it deterministic.
        self.pos.x += (dx * self.speed as i64 / dist) as i32;
        self.pos.y += (dy * self.speed as i64 / dist) as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_from_vectors() {
        assert_eq!(Dir8::from_vector(0, -10), Some(Dir8::N));
        assert_eq!(Dir8::from_vector(10, -10), Some(Dir8::NE));
        assert_eq!(Dir8::from_vector(10, 0), Some(Dir8::E));
        assert_eq!(Dir8::from_vector(10, 10), Some(Dir8::SE));
        assert_eq!(Dir8::from_vector(0, 10), Some(Dir8::S));
        assert_eq!(Dir8::from_vector(-10, 10), Some(Dir8::SW));
        assert_eq!(Dir8::from_vector(-10, 0), Some(Dir8::W));
        assert_eq!(Dir8::from_vector(-10, -10), Some(Dir8::NW));
        assert_eq!(Dir8::from_vector(10, -2), Some(Dir8::E));
        assert_eq!(Dir8::from_vector(2, -10), Some(Dir8::N));
        assert_eq!(Dir8::from_vector(0, 0), None);
        assert_eq!(Dir8::E.animation(), "C");
    }

    #[test]
    fn walks_to_target_and_stops() {
        let mut u = Unit::new("priest", Cell::new(0, 0), 64);
        u.path.push_back(Pos::cell_centre(Cell::new(3, 0)));
        let mut ticks = 0;
        while u.is_moving() {
            u.step();
            ticks += 1;
            assert!(ticks < 100, "did not arrive");
        }
        assert_eq!(u.pos, Pos::cell_centre(Cell::new(3, 0)));
        assert_eq!(u.facing, Dir8::E);
        assert_eq!(ticks, 12, "3 cells at a quarter cell per tick");
        assert_eq!(u.pos.cell(), Cell::new(3, 0));
    }

    #[test]
    fn negative_positions_map_to_cells() {
        assert_eq!(Pos { x: -1, y: -1 }.cell(), Cell::new(-1, -1));
        assert_eq!(Pos { x: 255, y: 256 }.cell(), Cell::new(0, 1));
    }
}
