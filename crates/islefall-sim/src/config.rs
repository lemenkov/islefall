// SPDX-License-Identifier: Apache-2.0
//! Game rules and parameters, loaded from `rules.toml`. The code carries
//! no defaults: a world cannot exist without a loaded configuration.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::world::EventKind;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub air: Air,
    pub production: Production,
    pub construction: Construction,
    pub spells: Spells,
    pub ai: AiConfig,
    pub controls: Controls,
    pub sounds: Sounds,
    pub sky: Sky,
    pub sprites: Sprites,
    pub hud: Hud,
}

/// The on-screen text: templates with `{power}`, `{knowledge}`, `{techs}`,
/// `{tool}`, `{status}` and `{opponents}` filled in.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Hud {
    pub font_size: f32,
    pub lines: Vec<String>,
    pub victory: String,
    pub defeat: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sprites {
    /// Directory under the data directory holding a mod's sprite sheets.
    pub dir: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sky {
    pub extent_tiles: u32,
    #[serde(default)]
    pub layers: Vec<SkyLayer>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SkyLayer {
    pub image: String,
    pub speed: [f32; 2],
    pub opacity: f32,
    /// Colour the texture is multiplied by, RGB 0 to 1; white leaves it as authored.
    #[serde(default = "white")]
    pub tint: [f32; 3],
}

fn white() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sounds {
    pub dir: String,
    pub volume: f32,
    pub max_per_frame: usize,
    pub events: SoundEvents,
    pub attenuation: Attenuation,
    pub ambient: Ambient,
    pub footsteps: Footsteps,
    /// Shooter type stem to projectile type stem, consulted after the shooter.
    #[serde(default)]
    pub projectiles: BTreeMap<String, String>,
    /// Names the type files use that exist under another name on disk.
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// Names replaced by files of the mod's own, relative to the data directory.
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Attenuation {
    pub reference_zoom: f32,
    pub db_per_halving: f32,
    pub edge_db: f32,
    pub max_loops: usize,
    pub loop_property: String,
    #[serde(default)]
    pub building_loop: Option<String>,
}

/// Step sounds of walkers, tied to their walk animation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Footsteps {
    /// Type property naming the step (or, for flyers, the flight loop).
    pub property: String,
    /// Steps heard per walk cycle.
    pub per_cycle: u32,
    /// Cycle through sibling files that differ only by a number.
    pub variants: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ambient {
    #[serde(default)]
    pub bed: Option<String>,
    #[serde(default)]
    pub sky: Vec<String>,
    pub sky_seconds: [f32; 2],
}

/// What an event plays: the type's `property` when it names a file, else `file`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SoundCue {
    #[serde(default)]
    pub property: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct SoundEvents {
    pub fired: SoundCue,
    pub hit: SoundCue,
    pub placed: SoundCue,
    pub built: SoundCue,
    pub destroyed: SoundCue,
    pub salvaged: SoundCue,
    pub moved: SoundCue,
    pub pickup: SoundCue,
    pub sacrificed: SoundCue,
    pub harvested: SoundCue,
    pub piece_placed: SoundCue,
    pub bridge_cracked: SoundCue,
    pub bridge_fell: SoundCue,
    pub island_fell: SoundCue,
    pub unit_lost: SoundCue,
    pub launched: SoundCue,
    pub floating: SoundCue,
    pub eliminated: SoundCue,
    pub victory: SoundCue,
    pub learned: SoundCue,
    pub casting: SoundCue,
    pub cast: SoundCue,
}

impl SoundEvents {
    pub fn cue(&self, what: EventKind) -> &SoundCue {
        match what {
            EventKind::Fired => &self.fired,
            EventKind::Hit => &self.hit,
            EventKind::Placed => &self.placed,
            EventKind::Built => &self.built,
            EventKind::Destroyed => &self.destroyed,
            EventKind::Salvaged => &self.salvaged,
            EventKind::Moved => &self.moved,
            EventKind::Pickup => &self.pickup,
            EventKind::Sacrificed => &self.sacrificed,
            EventKind::Harvested => &self.harvested,
            EventKind::PiecePlaced => &self.piece_placed,
            EventKind::BridgeCracked => &self.bridge_cracked,
            EventKind::BridgeFell => &self.bridge_fell,
            EventKind::IslandFell => &self.island_fell,
            EventKind::UnitLost => &self.unit_lost,
            EventKind::Launched => &self.launched,
            EventKind::Floating => &self.floating,
            EventKind::Eliminated => &self.eliminated,
            EventKind::Victory => &self.victory,
            EventKind::Learned => &self.learned,
            EventKind::Casting => &self.casting,
            EventKind::Cast => &self.cast,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Air {
    pub base_classes: Vec<String>,
    pub launches: BTreeMap<String, String>,
    pub respawn_seconds: f64,
    pub strike_range: i32,
    pub attackers: BTreeMap<String, Attacker>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Attacker {
    pub life_seconds: f64,
    pub refuels: bool,
    pub hunts_transports: bool,
    pub cracks_bridges: bool,
    pub kill_extends_life: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Production {
    /// Slots per Workshop level, Level One first.
    pub workshop_slots: Vec<usize>,
    pub upgrade_cost_percent: i32,
    pub temple_types: Vec<String>,
    pub outpost_types: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Spells {
    pub default_cast_seconds: f64,
    pub prayer: String,
    pub seed: u64,
    #[serde(default)]
    pub pool: BTreeMap<String, u32>,
    #[serde(default)]
    pub effects: BTreeMap<String, SpellEffect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpellEffect {
    pub kind: EffectKind,
    #[serde(default)]
    pub amount: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectKind {
    Damage,
    Heal,
    Harden,
    Paralyse,
    Invisible,
    Treason,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Construction {
    pub power_per_rate: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sim {
    pub tick_hz: u32,
    pub max_players: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    pub cell_w: i32,
    pub cell_h: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Walking {
    pub avoid_cost: u32,
    pub subcell: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Flags {
    pub walk_blocked: Vec<String>,
    pub walk_avoid: Vec<String>,
    pub drop_blocking: Vec<String>,
    pub creates_island: Vec<String>,
    pub may_drop_on_rim: Vec<String>,
    pub unit: Vec<String>,
    pub flyer: Vec<String>,
    pub balloon: Vec<String>,
    pub priest: Vec<String>,
    pub geyser: Vec<String>,
    pub temple: Vec<String>,
    pub altar: Vec<String>,
    pub workshop: Vec<String>,
    pub obelisk: Vec<String>,
    pub spell: Vec<String>,
    pub energy_source_classes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PieceDef {
    pub name: String,
    pub cells: Vec<[i32; 2]>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bridges {
    pub crumble_seconds: f64,
    pub piece_slots: usize,
    pub queue_seed: u64,
    pub pieces: Vec<PieceDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Economy {
    pub nugget_power: i32,
    pub work_seconds: f64,
    pub salvage_refund_percent: i32,
    pub kill_reward_percent: i32,
    pub geyser_stock_from_cost: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Combat {
    pub explosion_radius: i32,
    pub explosion_damage: i32,
    pub default_delay_between_shots: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Priest {
    pub temple_heal_range: i32,
    pub heal_per_second: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Energy {
    pub range_px: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    pub move_seconds: f64,
    pub shooter_every: u32,
    pub shooter: String,
    /// What the opponent drops when a shooter's site lacks Energy.
    pub generator: String,
    pub shooter_reach: i32,
    pub queue_seed: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Controls {
    pub slot_keys: Vec<String>,
    pub build_tools: Vec<String>,
    pub unit_keys: Vec<String>,
    pub unit_tools: Vec<String>,
    pub pan_speed: f32,
    pub zoom: f32,
    pub animation_fps: f32,
    pub screenshot_seconds: f32,
    pub palette: String,
    /// Log the world's hash this often (0 for never), to compare runs.
    pub hash_every_seconds: f64,
    /// Where F5 saves a snapshot and F9 loads it from.
    pub save_file: String,
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
        if self.air.respawn_seconds <= 0.0 || self.air.strike_range < 0 {
            return bad("air timings must be positive");
        }
        if self.air.attackers.values().any(|a| a.life_seconds <= 0.0) {
            return bad("air.attackers life_seconds must be positive");
        }
        if self.controls.unit_keys.len() != self.controls.unit_tools.len() || self.controls.unit_tools.is_empty() {
            return bad("controls.unit_keys and unit_tools must pair up");
        }
        if !(0.0..=1.0).contains(&self.sounds.volume) {
            return bad("sounds.volume must be 0.0..1.0");
        }
        if self.spells.default_cast_seconds <= 0.0 {
            return bad("spells.default_cast_seconds must be positive");
        }
        if self.production.workshop_slots.is_empty() || self.construction.power_per_rate <= 0.0 {
            return bad("production.workshop_slots needs a level and construction.power_per_rate must be positive");
        }
        if self.sky.extent_tiles == 0 || self.sky.layers.iter().any(|l| !(0.0..=1.0).contains(&l.opacity)) {
            return bad("sky.extent_tiles must be positive and layer opacity 0.0..1.0");
        }
        if self.sounds.footsteps.per_cycle == 0 {
            return bad("sounds.footsteps.per_cycle must be positive");
        }
        let [least, most] = self.sounds.ambient.sky_seconds;
        if least <= 0.0 || most < least || self.sounds.attenuation.reference_zoom <= 0.0 {
            return bad("sounds.ambient.sky_seconds and attenuation.reference_zoom must be positive, least first");
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
        assert_eq!(c.sounds.events.cue(EventKind::Fired).property.as_deref(), Some("fireSound"));
        assert!(c.sounds.events.cue(EventKind::Destroyed).file.is_some());
    }

    #[test]
    fn rejects_unknown_and_invalid_values() {
        let text = include_str!("../../../data/rules.toml");
        assert!(Config::parse(&text.replace("tick_hz = 30", "tick_hz = 0")).is_err());
        assert!(Config::parse(&format!("{text}\n[extra]\nx = 1\n")).is_err());
    }
}
