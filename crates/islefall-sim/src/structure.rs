// SPDX-License-Identifier: Apache-2.0
//! Things placed on the map that occupy a footprint of cells.

use crate::grid::Cell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structure {
    /// Type file stem, e.g. `dais` or `treetwo`.
    pub kind: String,
    /// The cell holding the hotspot: the bottom-right cell of the footprint.
    pub cell: Cell,
    /// Footprint size in cells (`foot_x`, `foot_y` in the type file).
    pub foot_x: i32,
    pub foot_y: i32,
    /// Whether units may not walk through the footprint.
    pub blocks_walking: bool,
}

impl Structure {
    pub fn new(kind: impl Into<String>, cell: Cell, foot_x: i32, foot_y: i32, blocks_walking: bool) -> Structure {
        Structure { kind: kind.into(), cell, foot_x: foot_x.max(1), foot_y: foot_y.max(1), blocks_walking }
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
        let s = Structure::new("dais", Cell::new(9, 7), 3, 2, false);
        let cells: Vec<Cell> = s.cells().collect();
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&Cell::new(9, 7)));
        assert!(cells.contains(&Cell::new(7, 6)));
        assert!(!cells.contains(&Cell::new(6, 6)));
        assert!(s.covers(Cell::new(8, 7)));
        assert!(!s.covers(Cell::new(10, 7)));
        assert!(!s.covers(Cell::new(9, 5)));
    }
}
