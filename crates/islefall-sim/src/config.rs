// SPDX-License-Identifier: Apache-2.0
//! Game rules and parameters, loaded from `rules.toml`. The code carries
//! no defaults: a world cannot exist without a loaded configuration.

use std::path::Path;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub sim: Sim,
    pub grid: Grid,
    pub walking: Walking,
    pub flags: Flags,
    pub bridges: Bridges,
    pub economy: Economy,
    pub combat: Combat,
    pub priest: Priest,
    pub energy: Energy,
    pub production: Production,
    pub ai: AiConfig,
    pub controls: Controls,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Production {
    pub workshop_slots: usize,
    pub temple_types: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Sim {
    pub tick_hz: u32,
    pub max_players: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    pub cell_w: i32,
    pub cell_h: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Walking {
    pub avoid_cost: u32,
    pub subcell: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Flags {
    pub walk_blocked: Vec<String>,
    pub walk_avoid: Vec<String>,
    pub drop_blocking: Vec<String>,
    pub creates_island: Vec<String>,
    pub may_drop_on_rim: Vec<String>,
    pub unit: Vec<String>,
    pub priest: Vec<String>,
    pub geyser: Vec<String>,
    pub temple: Vec<String>,
    pub altar: Vec<String>,
    pub workshop: Vec<String>,
    pub energy_source_classes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PieceDef {
    pub name: String,
    pub cells: Vec<[i32; 2]>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bridges {
    pub crumble_seconds: f64,
    pub piece_slots: usize,
    pub queue_seed: u64,
    pub pieces: Vec<PieceDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Economy {
    pub nugget_power: i32,
    pub work_seconds: f64,
    pub salvage_refund_percent: i32,
    pub geyser_stock_from_cost: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Combat {
    pub explosion_radius: i32,
    pub explosion_damage: i32,
    pub default_delay_between_shots: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Priest {
    pub temple_heal_range: i32,
    pub heal_per_second: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Energy {
    pub range_px: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    pub move_seconds: f64,
    pub shooter_every: u32,
    pub shooter: String,
    pub shooter_reach: i32,
    pub queue_seed: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Controls {
    pub slot_keys: Vec<String>,
    pub build_tools: Vec<String>,
    pub unit_tool: String,
    pub pan_speed: f32,
    pub zoom: f32,
    pub animation_fps: f32,
    pub screenshot_seconds: f32,
    pub palette: String,
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
    /// A value is outside what the simulation can work with.
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "{e}"),
            ConfigError::Parse(e) => write!(f, "{e}"),
            ConfigError::Invalid(s) => write!(f, "invalid rules: {s}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        let cfg: Config = toml::from_str(text).map_err(ConfigError::Parse)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Config, ConfigError> {
        Config::parse(&std::fs::read_to_string(path).map_err(ConfigError::Io)?)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let bad = |s: &str| Err(ConfigError::Invalid(s.to_string()));
        if self.sim.tick_hz == 0 || self.sim.tick_hz > 1000 {
            return bad("sim.tick_hz must be 1..1000");
        }
        if self.sim.max_players == 0 || self.sim.max_players > 255 {
            return bad("sim.max_players must be 1..255");
        }
        if self.grid.cell_w <= 0 || self.grid.cell_h <= 0 {
            return bad("grid cells must be positive");
        }
        if self.walking.subcell < 16 {
            return bad("walking.subcell must be at least 16");
        }
        if self.bridges.piece_slots == 0 || self.bridges.pieces.is_empty() {
            return bad("bridges need at least one slot and one piece");
        }
        if self.bridges.pieces.iter().any(|p| p.cells.is_empty()) {
            return bad("every bridge piece needs at least one cell");
        }
        if self.combat.explosion_radius < 0 || self.combat.default_delay_between_shots <= 0.0 {
            return bad("combat values must be positive");
        }
        if self.energy.range_px <= 0 {
            return bad("energy.range_px must be positive");
        }
        if self.ai.move_seconds <= 0.0 || self.ai.shooter_every == 0 {
            return bad("ai.move_seconds and ai.shooter_every must be positive");
        }
        Ok(())
    }

    /// Ticks in `seconds`, at least one.
    pub fn ticks(&self, seconds: f64) -> u32 {
        ((seconds * self.sim.tick_hz as f64).round() as u32).max(1)
    }

    /// Whether a type flag list contains any of the configured names.
    pub fn has_any(flags: &[String], type_flags: &[String]) -> bool {
        type_flags.iter().any(|f| flags.iter().any(|n| n.eq_ignore_ascii_case(f)))
    }
}

/// The repository's rules file, for tests.
#[cfg(test)]
pub fn test_config() -> Config {
    Config::parse(include_str!("../../../data/rules.toml")).expect("data/rules.toml parses")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_rules_parse_and_validate() {
        let c = test_config();
        assert_eq!(c.sim.tick_hz, 30);
        assert_eq!(c.ticks(4.0), 120);
        assert_eq!(c.bridges.pieces.len(), 8);
        assert!(Config::has_any(&c.flags.walk_avoid, &["yuckwalk".to_string()]));
    }

    #[test]
    fn rejects_unknown_and_invalid_values() {
        let text = include_str!("../../../data/rules.toml");
        assert!(Config::parse(&text.replace("tick_hz = 30", "tick_hz = 0")).is_err());
        assert!(Config::parse(&format!("{text}\n[extra]\nx = 1\n")).is_err());
    }
}
