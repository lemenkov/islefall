// SPDX-License-Identifier: Apache-2.0
//! The Islefall simulation.
//!
//! Everything here is deterministic and free of rendering concerns: integer
//! grid coordinates, island shapes, and later units and rules. The Bevy
//! crate reads this state and draws it; it never drives it.

pub mod grid;
pub mod island;
pub mod path;
pub mod pieces;
pub mod rules;
pub mod structure;
pub mod unit;
pub mod world;

pub use grid::{CELL_H, CELL_W, Cell};
pub use island::IslandMap;
pub use path::{find_path, find_path_costed};
pub use pieces::{Piece, PieceQueue};
pub use rules::{TypeRules, Walk};
pub use structure::Structure;
pub use unit::{Dir8, Pos, SUBCELL, Task, Unit};
pub use world::{BridgeState, DropError, NUGGET_POWER, PieceError, TICK_HZ, World, speed_per_tick};
