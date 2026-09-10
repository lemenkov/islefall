// SPDX-License-Identifier: Apache-2.0
//! The Islefall simulation.
//!
//! Everything here is deterministic and free of rendering concerns: integer
//! grid coordinates, island shapes, and later units and rules. The Bevy
//! crate reads this state and draws it; it never drives it.

pub mod ai;
pub mod campaign;
pub mod command;
pub mod config;
pub mod generate;
pub mod grid;
pub mod map;
pub mod island;
pub mod path;
pub mod pieces;
pub mod rules;
pub mod script;
pub mod structure;
pub mod unit;
pub mod world;

pub use ai::{Ai, AiMove};
pub use config::{Config, ConfigError};
pub use map::{MapDef, MapError};
pub use script::{ScriptError, Scripts};
pub use grid::Cell;
pub use island::IslandMap;
pub use path::{find_path, find_path_costed};
pub use pieces::{Piece, PieceQueue};
pub use rules::{EnergyNeed, TypeRules, Walk};
pub use structure::{Structure, Weapon};
pub use unit::{Dir8, Pos, Task, Unit};
pub use command::{Applied, Command, Entry, Replay};
pub use world::{BridgeState, DropError, Event, EventKind, PieceError, ProductionError, World, speed_per_tick};
