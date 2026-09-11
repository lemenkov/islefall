// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! The map grid.
//!
//! NetStorm's world is a plain rectangular grid seen from a slight angle.
//! One cell is 16 x 11 screen pixels: the terrain tiles are 16 x 11, a
//! 3 x 3 island sprite is 46 x 33, and the sprite cache trailer measures
//! boxes in sixteenths and elevenths. Sprites are anchored on the
//! bottom-right pixel of the cell their hotspot occupies.

use crate::config::Grid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
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
    pub const fn top_left_px(self, g: &Grid) -> (i32, i32) {
        (self.x * g.cell_w, self.y * g.cell_h)
    }

    /// Screen-space position (x right, y down) of the cell's centre pixel.
    pub const fn centre_px(self, g: &Grid) -> (i32, i32) {
        (self.x * g.cell_w + g.cell_w / 2, self.y * g.cell_h + g.cell_h / 2)
    }

    /// Screen-space position (x right, y down) of the cell's bottom-right
    /// pixel, where sprites with a default hotspot are anchored.
    pub const fn hotspot_px(self, g: &Grid) -> (i32, i32) {
        (self.x * g.cell_w + g.cell_w - 1, self.y * g.cell_h + g.cell_h - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_positions() {
        let g = Grid { cell_w: 16, cell_h: 11 };
        assert_eq!(Cell::new(0, 0).top_left_px(&g), (0, 0));
        assert_eq!(Cell::new(0, 0).hotspot_px(&g), (15, 10));
        assert_eq!(Cell::new(2, 3).hotspot_px(&g), (47, 43));
        assert_eq!(Cell::new(1, 1).offset(-1, 2), Cell::new(0, 3));
    }
}
