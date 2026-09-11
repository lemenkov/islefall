// SPDX-License-Identifier: Apache-2.0
//! Map files: the islands, bridges, structures, units and opponents a
//! scene starts with, loaded from TOML.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::grid::Cell;
use thiserror::Error;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub name: String,
    pub start_power: i32,
    pub camera: [i32; 2],
    /// Type stems whose Knowledge the player starts with.
    #[serde(default)]
    pub player_tech: Vec<String>,
    #[serde(default)]
    pub islands: Vec<IslandDef>,
    #[serde(default)]
    pub bridges: Vec<BridgeDef>,
    #[serde(default)]
    pub structures: Vec<PlacementDef>,
    #[serde(default)]
    pub units: Vec<UnitDef>,
    #[serde(default)]
    pub opponents: Vec<OpponentDef>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IslandDef {
    /// Omitted for a neutral island.
    #[serde(default)]
    pub owner: Option<u8>,
    pub theme: String,
    pub origin: [i32; 2],
    pub size: [i32; 2],
    #[serde(default)]
    pub remove: Vec<[i32; 2]>,
    /// The island's cells outright, when it is no rectangle; `origin`,
    /// `size` and `remove` are ignored then.
    #[serde(default)]
    pub cells: Vec<[i32; 2]>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeDef {
    pub owner: u8,
    pub cells: Vec<[i32; 2]>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementDef {
    pub owner: u8,
    pub kind: String,
    pub at: [i32; 2],
    /// The Spell an Obelisk holds; drawn from the rules' pool when omitted.
    #[serde(default)]
    pub spell: Option<String>,
    /// Which of a type's variant frames to show (a tree's shape), else one is chosen per cell.
    #[serde(default)]
    pub frame: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDef {
    pub owner: u8,
    pub kind: String,
    pub at: [i32; 2],
    pub move_to: Option<[i32; 2]>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpponentDef {
    pub owner: u8,
    pub target: Option<[i32; 2]>,
    /// What this opponent drops, when not the rules' `ai.shooter` and `ai.generator`.
    #[serde(default)]
    pub shooter: Option<String>,
    #[serde(default)]
    pub generator: Option<String>,
    /// Knowledge bits this opponent starts with (its faction's weapons).
    #[serde(default)]
    pub knowledge: Vec<u8>,
    /// Type stems whose Knowledge it starts with, by name.
    #[serde(default)]
    pub tech: Vec<String>,
    /// Storm Power it starts with instead of the map's.
    #[serde(default)]
    pub power: Option<i32>,
}

#[derive(Debug, Error)]
pub enum MapError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Parse(toml::de::Error),
}

impl MapDef {
    pub fn parse(text: &str) -> Result<MapDef, MapError> {
        toml::from_str(text).map_err(MapError::Parse)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<MapDef, MapError> {
        MapDef::parse(&std::fs::read_to_string(path).map_err(MapError::Io)?)
    }

    /// The map as a TOML file that `load` reads back.
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn camera_cell(&self) -> Cell {
        Cell::new(self.camera[0], self.camera[1])
    }
}

pub fn cell(xy: [i32; 2]) -> Cell {
    Cell::new(xy[0], xy[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_map_parses() {
        let m = MapDef::parse(include_str!("../../../data/maps/demo.toml")).unwrap();
        assert_eq!(m.islands.len(), 5, "Sun, Wind, neutral, Rain, Thunder");
        assert_eq!(m.structures.len(), 33);
        assert_eq!(m.units.len(), 8);
        assert_eq!(m.opponents.len(), 3, "Wind, Rain and Thunder are computer players");
        assert_eq!(m.camera_cell(), Cell::new(11, 5));
    }

    #[test]
    fn maps_survive_a_round_trip_through_toml() {
        let m = MapDef::parse(include_str!("../../../data/maps/demo.toml")).unwrap();
        let again = MapDef::parse(&m.to_toml()).unwrap();
        assert_eq!(again, m);
        let odd = MapDef { name: "odd".into(), islands: vec![IslandDef { owner: Some(1), theme: "rain".into(), cells: vec![[3, 4], [4, 4]], ..IslandDef::default() }], ..MapDef::default() };
        assert_eq!(MapDef::parse(&odd.to_toml()).unwrap(), odd);
    }
}
