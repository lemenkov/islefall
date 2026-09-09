// SPDX-License-Identifier: Apache-2.0
//! Bridge pieces: the multi-cell shapes a player drops as one unit.
//!
//! NetStorm's tutorial calls them "oddly shaped bridge pieces" that are
//! rotated with a right-click and "must attach to the edge of an island or
//! to the open end of another bridge". The exact shipped catalogue is not
//! in the data files, so this is a plausible set that can be adjusted.

use crate::config::PieceDef;
use crate::grid::Cell;

/// A piece shape as offsets from its origin cell, normalised so that the
/// smallest x and y offsets are zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub name: String,
    pub cells: Vec<(i32, i32)>,
}

impl Piece {
    pub fn new(name: &str, cells: &[(i32, i32)]) -> Piece {
        let mut p = Piece { name: name.to_string(), cells: cells.to_vec() };
        p.normalise();
        p
    }

    pub fn from_def(def: &PieceDef) -> Piece {
        Piece::new(&def.name, &def.cells.iter().map(|c| (c[0], c[1])).collect::<Vec<_>>())
    }

    fn normalise(&mut self) {
        let min_x = self.cells.iter().map(|c| c.0).min().unwrap_or(0);
        let min_y = self.cells.iter().map(|c| c.1).min().unwrap_or(0);
        for c in &mut self.cells {
            c.0 -= min_x;
            c.1 -= min_y;
        }
        self.cells.sort_unstable();
    }

    /// The piece turned a quarter turn clockwise on screen.
    pub fn rotated(&self) -> Piece {
        let mut p = Piece { name: self.name.clone(), cells: self.cells.iter().map(|&(x, y)| (-y, x)).collect() };
        p.normalise();
        p
    }

    /// Absolute cells when the origin sits at `at`.
    pub fn cells_at(&self, at: Cell) -> Vec<Cell> {
        self.cells.iter().map(|&(x, y)| at.offset(x, y)).collect()
    }

    /// Width and height of the bounding box in cells.
    pub fn size(&self) -> (i32, i32) {
        let w = self.cells.iter().map(|c| c.0).max().unwrap_or(0) + 1;
        let h = self.cells.iter().map(|c| c.1).max().unwrap_or(0) + 1;
        (w, h)
    }
}

/// The Production window: a few random pieces, each replaced by another
/// random piece when used. Deterministic given the seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceQueue {
    pub slots: Vec<Piece>,
    catalogue: Vec<Piece>,
    rng: u64,
}

impl PieceQueue {
    pub fn new(catalogue: Vec<Piece>, slots: usize, seed: u64) -> PieceQueue {
        let mut q = PieceQueue { slots: Vec::new(), catalogue, rng: seed | 1 };
        if q.catalogue.is_empty() {
            return q;
        }
        for _ in 0..slots {
            let p = q.draw();
            q.slots.push(p);
        }
        q
    }

    pub fn from_defs(defs: &[PieceDef], slots: usize, seed: u64) -> PieceQueue {
        PieceQueue::new(defs.iter().map(Piece::from_def).collect(), slots, seed)
    }

    /// Replace the piece in `slot` with a new random one; returns the old piece.
    pub fn refill(&mut self, slot: usize) -> Option<Piece> {
        if slot >= self.slots.len() {
            return None;
        }
        let next = self.draw();
        Some(std::mem::replace(&mut self.slots[slot], next))
    }

    fn draw(&mut self) -> Piece {
        // A small xorshift; only determinism matters here, not quality.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        let i = (self.rng >> 33) as usize % self.catalogue.len();
        self.catalogue[i].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_cycles_and_normalises() {
        let corner = Piece::new("corner", &[(0, 0), (1, 0), (1, 1)]);
        let r1 = corner.rotated();
        assert_eq!(r1.cells, vec![(0, 1), (1, 0), (1, 1)]);
        let r4 = r1.rotated().rotated().rotated();
        assert_eq!(r4, corner, "four quarter turns return the shape");
        assert_eq!(corner.size(), (2, 2));
        let three = Piece::new("three", &[(0, 0), (1, 0), (2, 0)]);
        assert_eq!(three.rotated().size(), (1, 3));
    }

    #[test]
    fn cells_at_offsets_origin() {
        let two = Piece::new("two", &[(0, 0), (1, 0)]);
        assert_eq!(two.cells_at(Cell::new(5, 7)), vec![Cell::new(5, 7), Cell::new(6, 7)]);
    }

    fn catalogue() -> Vec<Piece> {
        crate::config::test_config().bridges.pieces.iter().map(Piece::from_def).collect()
    }

    #[test]
    fn queue_is_deterministic_and_refills() {
        let mut a = PieceQueue::new(catalogue(), 4, 42);
        let mut b = PieceQueue::new(catalogue(), 4, 42);
        assert_eq!(a.slots, b.slots);
        assert_eq!(a.slots.len(), 4);
        let used = a.refill(2).unwrap();
        assert_eq!(used, b.slots[2]);
        b.refill(2);
        assert_eq!(a.slots, b.slots, "same draws in the same order");
        assert_eq!(a.refill(9), None);
    }

    #[test]
    fn catalogue_pieces_are_connected() {
        for p in catalogue() {
            let mut seen = vec![p.cells[0]];
            let mut frontier = vec![p.cells[0]];
            while let Some((x, y)) = frontier.pop() {
                for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                    if p.cells.contains(&n) && !seen.contains(&n) {
                        seen.push(n);
                        frontier.push(n);
                    }
                }
            }
            assert_eq!(seen.len(), p.cells.len(), "{} is not connected", p.name);
        }
    }
}
