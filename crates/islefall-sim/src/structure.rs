// SPDX-License-Identifier: Apache-2.0
//! Things placed on the map that occupy a footprint of cells.

use crate::grid::Cell;
use crate::rules::Walk;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structure {
    /// Type file stem, e.g. `dais` or `treetwo`.
    pub kind: String,
    /// The cell holding the hotspot: the bottom-right cell of the footprint.
    pub cell: Cell,
    /// Footprint size in cells (`foot_x`, `foot_y` in the type file).
    pub foot_x: i32,
    pub foot_y: i32,
    /// How the footprint affects walking.
    pub walk: Walk,
    /// Whether nothing may be dropped onto the footprint.
    pub drop_blocking: bool,
    /// Storm crystals left, for geysers.
    pub stock: i32,
    /// Whether units deliver crystals here.
    pub is_temple: bool,
}

impl Structure {
    pub fn new(kind: impl Into<String>, cell: Cell, foot_x: i32, foot_y: i32, walk: Walk) -> Structure {
        Structure { kind: kind.into(), cell, foot_x: foot_x.max(1), foot_y: foot_y.max(1), walk, drop_blocking: false, stock: 0, is_temple: false }
    }

    /// Cells orthogonally adjacent to the footprint, where a unit can stand to work on it.
    pub fn adjacent_cells(&self) -> Vec<Cell> {
        let mut out = Vec::new();
        for c in self.cells() {
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let n = c.offset(dx, dy);
                if !self.covers(n) && !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out
    }

    pub fn is_adjacent(&self, cell: Cell) -> bool {
        !self.covers(cell) && [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| self.covers(cell.offset(dx, dy)))
    }

    pub fn blocks_walking(&self) -> bool {
        self.walk == Walk::Blocked
    }

    /// Every cell of the footprint. The hotspot cell is the bottom-right one.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.foot_y).flat_map(move |dy| (0..self.foot_x).map(move |dx| self.cell.offset(-dx, -dy)))
    }

    pub fn covers(&self, cell: Cell) -> bool {
        let dx = self.cell.x - cell.x;
        let dy = self.cell.y - cell.y;
        (0..self.foot_x).contains(&dx) && (0..self.foot_y).contains(&dy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_cells() {
        let s = Structure::new("dais", Cell::new(9, 7), 3, 2, Walk::Free);
        let cells: Vec<Cell> = s.cells().collect();
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&Cell::new(9, 7)));
        assert!(cells.contains(&Cell::new(7, 6)));
        assert!(!cells.contains(&Cell::new(6, 6)));
        assert!(s.covers(Cell::new(8, 7)));
        assert!(!s.covers(Cell::new(10, 7)));
        assert!(!s.covers(Cell::new(9, 5)));
        assert_eq!(s.adjacent_cells().len(), 10);
        assert!(s.is_adjacent(Cell::new(10, 7)));
        assert!(!s.is_adjacent(Cell::new(10, 8)), "diagonal does not count");
    }
}
