// SPDX-License-Identifier: Apache-2.0
//! The Islefall simulation.
//!
//! Everything here is deterministic and free of rendering concerns: integer
//! grid coordinates, island shapes, and later units and rules. The Bevy
//! crate reads this state and draws it; it never drives it.

pub mod grid;
pub mod island;
pub mod path;
pub mod structure;
pub mod unit;
pub mod world;

pub use grid::{CELL_H, CELL_W, Cell};
pub use island::IslandMap;
pub use path::find_path;
pub use structure::Structure;
pub use unit::{Dir8, Pos, SUBCELL, Unit};
pub use world::{TICK_HZ, World, speed_per_tick};
