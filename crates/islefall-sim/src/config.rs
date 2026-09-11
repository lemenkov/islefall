// SPDX-License-Identifier: Apache-2.0
//! Game rules and parameters, loaded from `rules.toml`. The code carries
//! no defaults: a world cannot exist without a loaded configuration.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::world::EventKind;
use thiserror::Error;

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
    pub animation: Animation,
    pub projectiles: Projectiles,
    pub effects: Effects,
    pub sidebar: Sidebar,
    /// Reading the original's campaign scenarios.
    #[serde(default)]
    pub fort: FortRules,
    /// Making maps up.
    #[serde(default)]
    pub generate: Generate,
}

/// How a `.fort` scenario's type codes and seats are read (see
/// `islefall_data::fort`).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FortRules {
    /// Structure type by two-digit hex code; an empty name skips the code.
    #[serde(default)]
    pub codes: BTreeMap<String, String>,
    /// Unit type by code.
    #[serde(default)]
    pub units: BTreeMap<String, String>,
    /// Island theme by a ground cell's byte.
    #[serde(default)]
    pub ground_themes: Vec<String>,
    /// A player's theme by the code of its altar, where the code tells.
    #[serde(default)]
    pub altar_themes: BTreeMap<String, String>,
    /// Temple type by theme.
    #[serde(default)]
    pub temples: BTreeMap<String, String>,
    #[serde(default = "default_altar")]
    pub altar: String,
    #[serde(default = "default_priest")]
    pub priest: String,
    #[serde(default = "default_geyser")]
    pub geyser: String,
    /// The player's theme when nothing in the file says.
    #[serde(default = "default_human_theme")]
    pub human_theme: String,
    /// Cells between the scenario's islands and the fortress ring.
    #[serde(default = "default_ring_margin")]
    pub ring_margin: i32,
    #[serde(default = "default_start_power")]
    pub start_power: i32,
    #[serde(default = "default_start_power")]
    pub ai_power: i32,
    /// Who owns the bridges the file lays.
    #[serde(default)]
    pub bridge_owner: u8,
}

impl Default for FortRules {
    fn default() -> Self {
        toml::from_str("").expect("all fields default")
    }
}

/// Making maps up: fortresses round a ring with geyser islands between.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Generate {
    /// Players when nothing else says.
    #[serde(default = "default_players")]
    pub players: usize,
    /// World size in cells.
    #[serde(default = "default_world")]
    pub world: [i32; 2],
    /// A fortress island's canvas in cells.
    #[serde(default = "default_fortress")]
    pub fortress: [i32; 2],
    /// Share of a canvas the island fills.
    #[serde(default = "default_fill")]
    pub fill: f32,
    /// Cells from the world's middle to the fortress seats.
    #[serde(default = "default_ring_radius")]
    pub ring_radius: i32,
    #[serde(default = "default_geysers_per_player")]
    pub geysers_per_player: usize,
    /// Cells kept clear round every island.
    #[serde(default = "default_geyser_spacing")]
    pub geyser_spacing: i32,
    #[serde(default = "default_geyser_theme")]
    pub geyser_theme: String,
    /// Bare islands, and their canvas.
    #[serde(default)]
    pub islets: usize,
    #[serde(default = "default_islet")]
    pub islet: [i32; 2],
    /// Player themes in seat order, cycling.
    #[serde(default = "default_themes")]
    pub themes: Vec<String>,
    #[serde(default = "default_start_power")]
    pub start_power: i32,
}

impl Default for Generate {
    fn default() -> Self {
        toml::from_str("").expect("all fields default")
    }
}

fn default_piece_cell() -> f32 {
    7.0
}
fn default_panel_font() -> f32 {
    12.0
}
fn default_altar() -> String {
    "dais".into()
}
fn default_priest() -> String {
    "priest".into()
}
fn default_geyser() -> String {
    "geyser".into()
}
fn default_human_theme() -> String {
    "wind".into()
}
fn default_ring_margin() -> i32 {
    28
}
fn default_start_power() -> i32 {
    2000
}
fn default_players() -> usize {
    2
}
fn default_world() -> [i32; 2] {
    [256, 256]
}
fn default_fortress() -> [i32; 2] {
    [32, 48]
}
fn default_fill() -> f32 {
    0.7
}
fn default_ring_radius() -> i32 {
    60
}
fn default_geysers_per_player() -> usize {
    4
}
fn default_geyser_spacing() -> i32 {
    6
}
fn default_geyser_theme() -> String {
    "sun".into()
}
fn default_islet() -> [i32; 2] {
    [12, 9]
}
fn default_themes() -> Vec<String> {
    ["sun", "wind", "rain", "thunder"].map(String::from).to_vec()
}

/// The panel down the left of the screen, built from the original's art.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sidebar {
    /// Width in logical pixels.
    pub width: f32,
    /// The type whose frames are the panel's art, and which frames.
    pub art: String,
    pub back_frame: usize,
    pub ruler_frame: usize,
    pub counter_frame: usize,
    /// The type drawn beside the Storm Power figure.
    pub crystal: String,
    pub icon_size: f32,
    pub minimap_height: f32,
    /// Pixels per cell of a bridge piece drawn in its slot.
    #[serde(default = "default_piece_cell")]
    pub piece_cell: f32,
    /// Text size in the build list.
    #[serde(default = "default_panel_font")]
    pub font_size: f32,
    /// Minimap colours: sky, the four island themes, bridge, mine, enemy, unit, view.
    pub colours: BTreeMap<String, [f32; 3]>,
}

/// An animation from the sprite cache: a type and the label of its frames.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EffectSprite {
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
}

/// Generated effects: the original drew fire and smoke as particles too.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Effects {
    /// Frames per second of effect animations (explosions, sparkles).
    pub fps: f32,
    /// What a missile shows where it lands, a structure when destroyed,
    /// and a structure now and then while a stream builds it.
    #[serde(default)]
    pub impact: Option<EffectSprite>,
    #[serde(default)]
    pub destroyed: Option<EffectSprite>,
    #[serde(default)]
    pub building: Option<EffectSprite>,
    /// Sparkles per second per footprint cell while building.
    pub sparkles_per_cell: f32,
    /// A structure burns once its health falls below this share, harder below the second.
    pub burning_below: f32,
    pub blazing_below: f32,
    /// Flames a burning and a blazing structure start per second per footprint cell.
    pub flames_per_cell: f32,
    pub blaze_per_cell: f32,
    /// How long a flame lives and how fast it rises, in source pixels per second.
    pub flame_seconds: f32,
    pub flame_rise_px: f32,
    /// Smoke puffs per flame, their life and rise.
    pub smoke_per_flame: f32,
    pub smoke_seconds: f32,
    pub smoke_rise_px: f32,
}

/// What flies from a shooter to its target.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Projectiles {
    /// Shooter type stem to missile type stem; nothing in the data links them.
    #[serde(default)]
    pub types: BTreeMap<String, String>,
    /// The missile of shooters not listed.
    pub default: String,
    /// Flight speed in source pixels per second, and how high the lob rises.
    pub speed_px: f32,
    pub arc_px: f32,
    /// A missile with at least this many frames turns to its bearing
    /// instead of spinning through them.
    pub bearing_min_frames: usize,
    /// Missiles whose frames are four runs, one per direction in
    /// `cardinal_order`, of this many animation frames each: the run of
    /// the flight's direction loops (the Thunder Cannon's bolt).
    #[serde(default)]
    pub cardinal_frames: BTreeMap<String, usize>,
    #[serde(default = "default_cardinal_order")]
    pub cardinal_order: Vec<String>,
    /// Seconds the flash where it lands stays.
    pub flash_seconds: f32,
}

/// How a structure's frames are read for idling, variants and turrets.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    /// Frames with this label loop while the structure stands.
    pub idle_label: String,
    /// Labels of a structure's looks by level, Level One first (a
    /// Workshop's `A`, `B`, `C`); a level with no frames falls back to `idle_label`.
    #[serde(default)]
    pub levels: Vec<String>,
    /// Frames of a group smaller than this share of its biggest frame's
    /// area are overlays drawn on top of it (window lights) rather than
    /// whole pictures of their own.
    pub overlay_share: f32,
    /// Frame flags that mark a frame's role rather than its look.
    pub tag_flags: Vec<String>,
    /// Frames with these flags are never drawn on the map (help pictures).
    pub hidden_flags: Vec<String>,
    /// Types with this flag show one frame of the default's group, picked per cell.
    pub variant_flag: String,
    /// Small frames of this label are a shooter's firing animation, drawn
    /// over its picture and played once per shot (a Disc Thrower's arm).
    pub fire_label: String,
    /// Shooters that aim only north, east, south and west play the label
    /// of that direction once when they fire, and rest on its first frame.
    pub cardinal: BTreeMap<String, String>,
    /// Seconds an all-round turret takes to swing a quarter turn: a type
    /// whose fire-label pictures lie between its direction groups has a
    /// ring of bearings, and turns along it to each new target.
    #[serde(default = "default_turn_seconds")]
    pub turn_seconds: f32,
    /// Firing sequences spelled out by type and direction as frame indices
    /// in the type's file order (the order `shpdump export` writes), for
    /// types whose label groups are not their play order. The first frame
    /// shows before the shot and again after it.
    #[serde(default)]
    pub fire_sequences: BTreeMap<String, BTreeMap<String, Vec<usize>>>,
    /// Types whose idle frames are so many runs, full to empty, chosen by
    /// the share of stock left (a geyser's three spouts).
    #[serde(default)]
    pub stages: BTreeMap<String, u32>,
    /// For variant types, the label whose frames repeat the variants on
    /// other grounds, in `ground_order` of island themes (a Temple's grey,
    /// autumn and snow); the default's label is the first theme's ground.
    #[serde(default)]
    pub ground_label: Option<String>,
    #[serde(default)]
    pub ground_order: Vec<String>,
}

fn default_cardinal_order() -> Vec<String> {
    ["north", "east", "south", "west"].map(String::from).to_vec()
}

fn default_turn_seconds() -> f32 {
    0.5
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
    /// The selected unit's mark: a ring on the ground round its feet and
    /// a frame round its picture, in this colour, the ring this wide and
    /// tall in source pixels, pulsing this many times a second.
    #[serde(default = "default_selection_colour")]
    pub selection_colour: [f32; 3],
    #[serde(default = "default_selection_ring")]
    pub selection_ring: [f32; 2],
    #[serde(default = "default_selection_pulse")]
    pub selection_pulse: f32,
}

fn default_selection_colour() -> [f32; 3] {
    [1.0, 0.9, 0.2]
}
fn default_selection_ring() -> [f32; 2] {
    [22.0, 11.0]
}
fn default_selection_pulse() -> f32 {
    1.5
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
    /// The colour behind everything, RGB 0 to 1.
    pub background: [f32; 3],
    pub extent_tiles: u32,
    #[serde(default)]
    pub layers: Vec<SkyLayer>,
}

/// One generated cloud layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SkyLayer {
    pub seed: u32,
    /// Tile edge in pixels.
    pub tile: u32,
    /// Cloud size across the tile: how many noise periods span it.
    pub scale: f64,
    pub octaves: u32,
    pub gain: f64,
    pub lacunarity: f64,
    /// Share of the tile that is cloud, 0 to 1.
    pub cover: f32,
    /// Width of the cloud edge, 0 to 1 of the noise range.
    pub softness: f32,
    pub tint: [f32; 3],
    pub opacity: f32,
    pub speed: [f32; 2],
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
    /// Steps heard per walk cycle.
    pub per_cycle: u32,
    /// The step sound of each type that has one, by type stem; numbered
    /// siblings of the file are cycled through. Types not listed walk silently.
    #[serde(default)]
    pub steps: BTreeMap<String, String>,
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
    /// Play one of the file's numbered siblings in turn (golemMove1 to 5)
    /// rather than the same file every time.
    #[serde(default)]
    pub variants: bool,
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
    /// Every structure's footprint blocks walking (and unit placement)
    /// unless its type carries a `walk_free` flag.
    pub structures_block: bool,
    /// A standing ground unit blocks its cell; a walker behind it waits
    /// `wait_seconds`, then paths round it.
    pub units_block: bool,
    pub wait_seconds: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Flags {
    pub walk_blocked: Vec<String>,
    pub walk_avoid: Vec<String>,
    pub walk_free: Vec<String>,
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
    /// Stars drifting round an Energy source's reach, and their orbit time.
    pub stars: u32,
    pub star_seconds: f32,
    /// The type whose frames draw the stars, by the source's theme; a
    /// generated star when missing.
    #[serde(default)]
    pub star_type: Option<String>,
    #[serde(default)]
    pub star_labels: BTreeMap<String, String>,
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

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Parse(toml::de::Error),
    /// A value is outside what the simulation can work with.
    #[error("invalid rules: {0}")]
    Invalid(String),
}

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
        if self.effects.fps <= 0.0 || self.effects.burning_below <= 0.0 || self.effects.flame_seconds <= 0.0 || self.effects.smoke_seconds <= 0.0 {
            return bad("effects need positive burning_below, flame_seconds and smoke_seconds");
        }
        if self.sky.extent_tiles == 0 || self.sky.layers.iter().any(|l| !(0.0..=1.0).contains(&l.opacity) || l.tile < 16 || l.tile > 2048 || l.octaves == 0 || l.scale <= 0.0) {
            return bad("sky layers need a tile of 16..2048, octaves, a positive scale and opacity 0.0..1.0");
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
