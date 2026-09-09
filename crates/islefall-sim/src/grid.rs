// SPDX-License-Identifier: Apache-2.0
//! The map grid.
//!
//! NetStorm's world is a plain rectangular grid seen from a slight angle.
//! One cell is 16 x 11 screen pixels: the terrain tiles are 16 x 11, a
//! 3 x 3 island sprite is 46 x 33, and the sprite cache trailer measures
//! boxes in sixteenths and elevenths. Sprites are anchored on the
//! bottom-right pixel of the cell their hotspot occupies.

/// Cell width in screen pixels.
pub const CELL_W: i32 = 16;
/// Cell height in screen pixels.
pub const CELL_H: i32 = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

impl Cell {
    pub const fn new(x: i32, y: i32) -> Cell {
        Cell { x, y }
    }

    pub const fn offset(self, dx: i32, dy: i32) -> Cell {
        Cell { x: self.x + dx, y: self.y + dy }
    }

    /// Screen-space position (x right, y down) of the cell's top-left pixel.
    pub const fn top_left_px(self) -> (i32, i32) {
        (self.x * CELL_W, self.y * CELL_H)
    }

    /// Screen-space position (x right, y down) of the cell's bottom-right
    /// pixel, where sprites with a default hotspot are anchored.
    pub const fn hotspot_px(self) -> (i32, i32) {
        (self.x * CELL_W + CELL_W - 1, self.y * CELL_H + CELL_H - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_positions() {
        assert_eq!(Cell::new(0, 0).top_left_px(), (0, 0));
        assert_eq!(Cell::new(0, 0).hotspot_px(), (15, 10));
        assert_eq!(Cell::new(2, 3).hotspot_px(), (47, 43));
        assert_eq!(Cell::new(1, 1).offset(-1, 2), Cell::new(0, 3));
    }
}
