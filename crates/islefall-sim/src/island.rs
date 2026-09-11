// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Island shapes on the grid and the terrain piece each cell needs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use islefall_data::isle::Piece;

use crate::grid::Cell;

/// The set of cells that belong to one island.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IslandMap {
    cells: BTreeSet<Cell>,
}

impl IslandMap {
    pub fn new() -> IslandMap {
        IslandMap::default()
    }

    /// A solid rectangle with its top-left corner at `origin`.
    pub fn rect(origin: Cell, width: i32, height: i32) -> IslandMap {
        let mut m = IslandMap::new();
        for y in 0..height.max(0) {
            for x in 0..width.max(0) {
                m.insert(origin.offset(x, y));
            }
        }
        m
    }

    pub fn insert(&mut self, cell: Cell) -> bool {
        self.cells.insert(cell)
    }

    pub fn remove(&mut self, cell: Cell) -> bool {
        self.cells.remove(&cell)
    }

    pub fn contains(&self, cell: Cell) -> bool {
        self.cells.contains(&cell)
    }

    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.cells.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Terrain piece for a cell of this island, or `None` if the cell is not
    /// part of it. Outside corners take priority over edges, edges over
    /// inside corners; a cell missing opposite neighbours (a one-cell-wide
    /// strip) is drawn as the corner facing the top-left.
    pub fn piece_at(&self, cell: Cell) -> Option<Piece> {
        if !self.contains(cell) {
            return None;
        }
        let at = |dx, dy| self.contains(cell.offset(dx, dy));
        let (l, t, r, b) = (at(-1, 0), at(0, -1), at(1, 0), at(0, 1));
        let piece = match (l, t, r, b) {
            (false, false, _, _) => Piece::CornerTopLeft,
            (_, false, false, _) => Piece::CornerTopRight,
            (_, _, false, false) => Piece::CornerBottomRight,
            (false, _, _, false) => Piece::CornerBottomLeft,
            (true, false, true, false) | (false, true, false, true) => Piece::CornerTopLeft,
            (false, true, true, true) => Piece::EdgeLeft,
            (true, false, true, true) => Piece::EdgeTop,
            (true, true, false, true) => Piece::EdgeRight,
            (true, true, true, false) => Piece::EdgeBottom,
            (true, true, true, true) => {
                if !at(-1, -1) {
                    Piece::InsideTopLeft
                } else if !at(1, -1) {
                    Piece::InsideTopRight
                } else if !at(1, 1) {
                    Piece::InsideBottomRight
                } else if !at(-1, 1) {
                    Piece::InsideBottomLeft
                } else {
                    Piece::Filled
                }
            }
        };
        Some(piece)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_pieces() {
        let m = IslandMap::rect(Cell::new(0, 0), 4, 3);
        assert_eq!(m.len(), 12);
        assert_eq!(m.piece_at(Cell::new(0, 0)), Some(Piece::CornerTopLeft));
        assert_eq!(m.piece_at(Cell::new(3, 0)), Some(Piece::CornerTopRight));
        assert_eq!(m.piece_at(Cell::new(3, 2)), Some(Piece::CornerBottomRight));
        assert_eq!(m.piece_at(Cell::new(0, 2)), Some(Piece::CornerBottomLeft));
        assert_eq!(m.piece_at(Cell::new(1, 0)), Some(Piece::EdgeTop));
        assert_eq!(m.piece_at(Cell::new(0, 1)), Some(Piece::EdgeLeft));
        assert_eq!(m.piece_at(Cell::new(3, 1)), Some(Piece::EdgeRight));
        assert_eq!(m.piece_at(Cell::new(2, 2)), Some(Piece::EdgeBottom));
        assert_eq!(m.piece_at(Cell::new(1, 1)), Some(Piece::Filled));
        assert_eq!(m.piece_at(Cell::new(9, 9)), None);
    }

    #[test]
    fn inside_corner() {
        // An L shape: the cell at (1,1) has all four neighbours but no
        // diagonal at top-right after removing (2,0).
        let mut m = IslandMap::rect(Cell::new(0, 0), 3, 3);
        m.remove(Cell::new(2, 0));
        assert_eq!(m.piece_at(Cell::new(1, 1)), Some(Piece::InsideTopRight));
        assert_eq!(m.piece_at(Cell::new(1, 0)), Some(Piece::CornerTopRight));
    }
}
