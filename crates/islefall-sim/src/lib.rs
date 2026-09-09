// SPDX-License-Identifier: Apache-2.0
//! The Islefall simulation.
//!
//! Everything here is deterministic and free of rendering concerns: integer
//! grid coordinates, island shapes, and later units and rules. The Bevy
//! crate reads this state and draws it; it never drives it.

pub mod grid;
pub mod island;

pub use grid::{CELL_H, CELL_W, Cell};
pub use island::IslandMap;
