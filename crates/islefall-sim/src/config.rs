// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
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
    /// Placed types that are only the floor of something: the original
    /// stands a second type on them, which has the hit points (the `dais`
    /// you place carries the `altar` that can be destroyed). Floor to body.
    #[serde(default)]
    pub bodies: BTreeMap<String, String>,
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
    #[serde(default)]
    pub knowledge: Knowledge,
    #[serde(default)]
    pub fences: Fences,
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
    /// What hangs under islands.
    #[serde(default)]
    pub fringe: FringeRules,
}

/// The original's island undersides: `fringe` pieces hanging under the
/// rim cells of big islands, and `islandstalag`, one rock under each
/// three-by-three island.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FringeRules {
    /// Draw the undersides from the rock generator (`islefall-art`)
    /// instead of the original's wall pieces: one strip per run of an
    /// island's bottom edge, following its real outline. On unless the
    /// rules say otherwise.
    #[serde(default = "yes")]
    pub generated: bool,
    #[serde(default)]
    pub rock: RockRules,
    /// Draw the filled ground of an island from the ground generator, one
    /// picture per island in the colours of that theme's own tiles; the
    /// rim tiles stay the original's. `ground` holds the shaping per
    /// theme (`sun`, `thunder`, `wind`, `rain`), `default` for the rest.
    /// On unless the rules say otherwise.
    #[serde(default = "yes")]
    pub generated_ground: bool,
    #[serde(default)]
    pub ground: BTreeMap<String, GroundRules>,
    /// Filled cells at least this many cells from the island's edge are
    /// drawn with the terrain scrambler's core pieces; 0 never uses them.
    #[serde(default = "default_core_depth")]
    pub core_depth: i32,
    /// The core pieces are one large picture of ground cut into a grid
    /// this many tiles wide; a cell takes the tile of its own place in
    /// it, so the picture repeats whole instead of being shuffled. 0
    /// picks a tile at random per cell.
    #[serde(default = "default_core_columns")]
    pub core_columns: i32,
    #[serde(default = "default_fringe_kind")]
    pub kind: String,
    /// Fringe label by island piece label (the `isle` type's).
    #[serde(default)]
    pub pieces: BTreeMap<String, String>,
    /// Cells below a rim cell where a fringe piece's hotspot goes. The
    /// pieces' hotspots lie 43 pixels, four cell rows, below their tops, so
    /// at 4 the picture starts right under the rim tile and its windows
    /// and doors show beneath the lip.
    #[serde(default = "default_hang")]
    pub hang: i32,
    /// An islet that is gone falls out of the picture: for so many
    /// seconds, speeding up by so many source pixels per second squared,
    /// fading over the last share of the fall.
    #[serde(default = "default_fall_seconds")]
    pub fall_seconds: f32,
    #[serde(default = "default_fall_gravity")]
    pub fall_gravity_px: f32,
    #[serde(default = "default_fall_fade")]
    pub fall_fade_share: f32,
    /// The picture breaks into so many chunks, flung outward at so many
    /// source pixels per second, lifted by so many, tumbling up to so many
    /// radians per second, in so many puffs of dust.
    /// A unit's islet from the ground and rock generators, with a rim
    /// and a band in the owner's colour, in place of the original's
    /// emblem and stalag.
    #[serde(default)]
    pub generated_islets: bool,
    /// The islet's rock, shallowest and deepest, in source pixels; how
    /// far its outline wanders in from a full ellipse; rows of the
    /// owner's band at the top of the rock.
    #[serde(default = "default_islet_depth")]
    pub islet_depth: [u32; 2],
    #[serde(default = "default_islet_wobble")]
    pub islet_wobble: f64,
    #[serde(default = "default_islet_band")]
    pub islet_band: u32,
    #[serde(default = "default_shatter_pieces")]
    pub shatter_pieces: u32,
    #[serde(default = "default_shatter_burst")]
    pub shatter_burst_px: f32,
    #[serde(default = "default_shatter_lift")]
    pub shatter_lift_px: f32,
    #[serde(default = "default_shatter_spin")]
    pub shatter_spin: f32,
    #[serde(default = "default_shatter_dust")]
    pub shatter_dust: u32,
    #[serde(default = "default_stalag_kind")]
    pub stalag: String,
    /// Which stalag frame: the last has no player-colour apron.
    #[serde(default = "default_stalag_frame")]
    pub stalag_frame: usize,
    /// The islet a unit makes for itself: one picture of this type with
    /// an emblem in its owner's colour (and the stalag underneath with the
    /// matching apron), frame `player_frames[owner]`; `stalag_frame` is
    /// the plain one, for what belongs to nobody.
    #[serde(default = "default_platform_kind")]
    pub platform: String,
    #[serde(default = "default_player_frames")]
    pub player_frames: Vec<usize>,
    /// Cliff dwellings: this share of an island's wall pieces are the
    /// fringe frames with windows, grilles and doors, the `lit_flag` ones
    /// on an island somebody owns and the `unlit_flag` ones on a neutral one.
    #[serde(default = "default_dwelling_share")]
    pub dwelling_share: f32,
    #[serde(default = "default_lit_flag")]
    pub lit_flag: String,
    #[serde(default = "default_unlit_flag")]
    pub unlit_flag: String,
}

fn default_platform_kind() -> String {
    "island".into()
}
fn default_player_frames() -> Vec<usize> {
    (0..8).collect()
}
fn default_dwelling_share() -> f32 {
    0.3
}
fn default_lit_flag() -> String {
    "lit".into()
}
fn default_unlit_flag() -> String {
    "unlit".into()
}

/// The ground generator's shaping; lengths are source pixels. See
/// `islefall_art::ground::Ground`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct GroundRules {
    pub drift: f64,
    pub patch: f64,
    pub drift_strength: f64,
    pub patch_strength: f64,
    pub grain: f64,
    /// Tufts and clumps: one to a square of `clump_spacing` pixels with
    /// probability `clump_density`, moving the brightness by up to
    /// `clump_strength`.
    pub clump_spacing: f64,
    pub clump_density: f64,
    pub clump_strength: f64,
    /// Regions of other materials (dry grass, moss, bare earth, snow):
    /// a colour ramp, the size of the regions and their share of the area.
    pub accents: Vec<GroundAccent>,
    /// Single pixels scattered about: flowers, stones, glints, embers.
    pub specks: Vec<GroundSpeck>,
    /// Cracks between cells about this many pixels across; 0 for none.
    pub crack_spacing: f64,
    pub crack_colour: [u8; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroundAccent {
    pub ramp: Vec<[u8; 3]>,
    pub scale: f64,
    pub coverage: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroundSpeck {
    pub colour: [u8; 3],
    pub per_1000px: f64,
}

impl Default for GroundRules {
    fn default() -> Self {
        GroundRules {
            drift: 70.0,
            patch: 17.0,
            drift_strength: 0.2,
            patch_strength: 0.1,
            grain: 0.75,
            clump_spacing: 9.0,
            clump_density: 0.4,
            clump_strength: 0.3,
            accents: Vec::new(),
            specks: Vec::new(),
            crack_spacing: 0.0,
            crack_colour: [20, 20, 20],
        }
    }
}

/// The rock generator's settings; lengths are source pixels. See
/// `islefall_art::rock::Rock`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct RockRules {
    /// Colours from the darkest to the lightest, snapped to the game's palette.
    pub ramp: Vec<[u8; 3]>,
    pub depth_min: u32,
    pub depth_max: u32,
    pub end_depth: u32,
    pub taper: u32,
    pub lobe: f64,
    pub tooth: f64,
    pub windows_per_100px: f64,
    pub pane: [u8; 3],
}

impl Default for RockRules {
    fn default() -> Self {
        RockRules {
            ramp: vec![[20, 10, 4], [44, 22, 8], [70, 36, 12], [98, 52, 18], [126, 70, 24], [152, 90, 34], [178, 112, 46], [200, 136, 64]],
            depth_min: 22,
            depth_max: 42,
            end_depth: 7,
            taper: 14,
            lobe: 46.0,
            tooth: 9.0,
            windows_per_100px: 1.2,
            pane: [255, 222, 96],
        }
    }
}

fn default_core_columns() -> i32 {
    6
}
fn default_core_depth() -> i32 {
    0
}

impl Default for FringeRules {
    fn default() -> Self {
        toml::from_str("").expect("all fields default")
    }
}

fn default_fringe_kind() -> String {
    "fringe".into()
}
fn default_hang() -> i32 {
    4
}
fn default_fall_seconds() -> f32 {
    1.4
}
fn default_fall_gravity() -> f32 {
    260.0
}
fn default_fall_fade() -> f32 {
    0.5
}
fn default_islet_depth() -> [u32; 2] {
    [20, 38]
}
fn default_islet_wobble() -> f64 {
    0.2
}
fn default_islet_band() -> u32 {
    3
}
fn default_shatter_pieces() -> u32 {
    9
}
fn default_shatter_burst() -> f32 {
    60.0
}
fn default_shatter_lift() -> f32 {
    50.0
}
fn default_shatter_spin() -> f32 {
    3.0
}
fn default_shatter_dust() -> u32 {
    6
}
fn default_stalag_kind() -> String {
    "islandstalag".into()
}
fn default_stalag_frame() -> usize {
    8
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
    /// Sparkles per second per footprint cell while building, and per
    /// second left behind by a stream on its way.
    pub sparkles_per_cell: f32,
    #[serde(default = "default_stream_trail")]
    pub stream_trail: f32,
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
    /// Drawn flames and smoke from `islefall-art` on a burning structure,
    /// standing on its picture; the rising specks above when `false`.
    #[serde(default = "yes")]
    pub generated_fire: bool,
    /// A generated explosion where a structure dies, in place of `destroyed`.
    #[serde(default)]
    pub generated_blast: bool,
    #[serde(default)]
    pub fire: FireRules,
}

/// The fire generator's settings; lengths are source pixels. See
/// `islefall_art::fire`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct FireRules {
    /// Colours of fire from the coolest to the hottest, and of smoke from
    /// the darkest, snapped to the game's palette.
    pub ramp: Vec<[u8; 3]>,
    pub smoke_ramp: Vec<[u8; 3]>,
    /// Width and height of each size of flame, smallest first; a blazing
    /// structure uses them all, a burning one all but the largest.
    pub flame_sizes: Vec<[u32; 2]>,
    /// Different flames made of each size.
    pub variants: u32,
    pub flame_frames: u32,
    pub flame_fps: f32,
    pub flicker: f64,
    pub lick: f64,
    /// Flames standing on a burning and on a blazing structure, per footprint cell.
    pub sites_per_cell: f32,
    pub blaze_sites_per_cell: f32,
    /// The most flames on one structure, however large.
    pub max_sites: [usize; 2],
    /// The shortest and the longest a flame stays where it is.
    pub site_seconds: [f32; 2],
    /// The part of a structure's picture flames may stand on, as shares
    /// of its width and height: left, top, right, bottom.
    pub stand_on: [f32; 4],
    pub smoke_size: u32,
    pub smoke_frames: u32,
    /// Seconds between the puffs of one flame.
    pub smoke_every: f32,
    /// Explosion sizes, smallest first: for one cell, for up to
    /// `blast_medium_cells` along the longer side, and for larger.
    pub blast_sizes: [u32; 3],
    pub blast_medium_cells: i32,
    pub blast_frames: u32,
    pub blast_fps: f32,
    pub sparks: u32,
    /// A missile's landing: a small, short explosion; 0 for none.
    pub impact_size: u32,
    pub impact_frames: u32,
}

impl Default for FireRules {
    fn default() -> Self {
        FireRules {
            ramp: vec![[92, 20, 12], [168, 40, 12], [228, 92, 16], [248, 156, 28], [252, 208, 60], [255, 240, 150]],
            smoke_ramp: vec![[44, 42, 44], [76, 74, 76], [112, 110, 112], [152, 150, 152]],
            flame_sizes: vec![[8, 12], [10, 16], [12, 20]],
            variants: 3,
            flame_frames: 8,
            flame_fps: 12.0,
            flicker: 0.9,
            lick: 0.35,
            sites_per_cell: 0.15,
            blaze_sites_per_cell: 0.3,
            max_sites: [4, 8],
            site_seconds: [1.2, 3.0],
            stand_on: [0.15, 0.3, 0.85, 0.85],
            smoke_size: 16,
            smoke_frames: 10,
            smoke_every: 0.7,
            blast_sizes: [40, 64, 96],
            blast_medium_cells: 3,
            blast_frames: 16,
            blast_fps: 18.0,
            sparks: 14,
            impact_size: 20,
            impact_frames: 8,
        }
    }
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
    /// A structure under construction is drawn as its own outline filled
    /// with this type's first frame (the original's grey cloud), the
    /// picture coming through in this many steps as the stream builds it.
    #[serde(default = "default_construction_cloud")]
    pub construction_cloud: String,
    #[serde(default = "default_construction_stages")]
    pub construction_stages: u32,
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

fn default_construction_cloud() -> String {
    "mcloud".into()
}
fn default_construction_stages() -> u32 {
    16
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
    /// The selected unit's mark: a ring on the ground round its feet, in
    /// this colour, this wide and tall in source pixels, pulsing this
    /// many times a second.
    #[serde(default = "default_selection_colour")]
    pub selection_colour: [f32; 3],
    #[serde(default = "default_selection_ring")]
    pub selection_ring: [f32; 2],
    #[serde(default = "default_selection_pulse")]
    pub selection_pulse: f32,
    /// The crystal a harvesting Transport is seen carrying: this type's
    /// frames, cycled, drawn this many source pixels above its feet at
    /// this scale (the crystal lying on the ground is drawn full size).
    #[serde(default = "default_carried_crystal")]
    pub carried_crystal: Option<String>,
    #[serde(default = "default_carried_lift")]
    pub carried_lift: f32,
    #[serde(default = "default_carried_scale")]
    pub carried_scale: f32,
    /// The manual's island label, name, Level and Rank, printed over each
    /// player's Altar: whether, how far above it, and how large.
    #[serde(default = "yes")]
    pub island_labels: bool,
    #[serde(default = "default_island_label_lift")]
    pub island_label_lift: f32,
    #[serde(default = "default_island_label_size")]
    pub island_label_size: f32,
}

fn yes() -> bool {
    true
}
fn default_island_label_lift() -> f32 {
    36.0
}
fn default_island_label_size() -> f32 {
    11.0
}

fn default_carried_crystal() -> Option<String> {
    Some("nugget".into())
}
fn default_carried_lift() -> f32 {
    12.0
}
fn default_carried_scale() -> f32 {
    0.5
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
    /// How far more noise bends the sample points, in noise periods;
    /// without it the clouds line up on the noise lattice.
    #[serde(default = "default_warp")]
    pub warp: f64,
}

fn default_warp() -> f64 {
    0.8
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
    /// The same file is not played again within so many seconds: a voice
    /// with one line answers now and then, not every order.
    #[serde(default)]
    pub gap_seconds: f32,
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
    pub dropped: SoundCue,
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
            EventKind::Dropped => &self.dropped,
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
    /// Seconds of flight for an attacker whose type names no `lifeSpan`.
    #[serde(default = "default_life_seconds")]
    pub default_life_seconds: f64,
    pub attackers: BTreeMap<String, Attacker>,
}

fn default_stream_trail() -> f32 {
    6.0
}

fn default_life_seconds() -> f64 {
    60.0
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Attacker {
    /// Seconds of flight, overriding the type's own `lifeSpan`; without
    /// either, `air.default_life_seconds`.
    #[serde(default)]
    pub life_seconds: Option<f64>,
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
    /// How quickly a placed unit comes back on offer (the manual's Unit
    /// Rate: slow, medium or fast): the chosen entry of `refresh_rates`,
    /// seconds per hundred Storm Power of the unit's price, and the least
    /// any unit waits, both fed to the `refresh_seconds` hook.
    #[serde(default = "default_unit_rate")]
    pub unit_rate: String,
    #[serde(default = "default_refresh_rates")]
    pub refresh_rates: BTreeMap<String, f64>,
    #[serde(default = "default_refresh_min_seconds")]
    pub refresh_min_seconds: f64,
}

fn default_unit_rate() -> String {
    "medium".into()
}
fn default_refresh_rates() -> BTreeMap<String, f64> {
    [("slow".to_string(), 3.0), ("medium".to_string(), 2.0), ("fast".to_string(), 1.0)].into_iter().collect()
}
fn default_refresh_min_seconds() -> f64 {
    2.0
}

impl Production {
    /// Seconds per hundred Storm Power at the configured Unit Rate.
    pub fn refresh_rate(&self) -> f64 {
        self.refresh_rates.get(&self.unit_rate).copied().unwrap_or(0.0)
    }
}

/// What a sacrifice teaches (the manual's multiplayer rules): the owner
/// chooses a unit of a level the Altar allows, or raises the Altar so
/// the next sacrifice offers the next level.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Knowledge {
    /// How far an Altar can be raised (the manual: Levels One to Three).
    pub altar_levels: u8,
    /// Let the `knowledge_choice` hook choose for the player too, as the
    /// campaign did, instead of asking.
    pub auto_choose: bool,
    /// The manual: knowing every unit, the next sacrifice returns the
    /// island to Level One and raises the Rank, and each Rank adds 25% to
    /// the hits and damage of Battle units.
    pub rank_bonus_percent: i32,
}

impl Default for Knowledge {
    fn default() -> Self {
        Knowledge { altar_levels: 3, auto_choose: false, rank_bonus_percent: 25 }
    }
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
    /// Damage only what flies (Graviton).
    #[serde(default)]
    pub air_only: bool,
    /// A Summons: the unit type conjured, this many of them (0: the
    /// Spell's own `spawns` property, else one).
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub count: u32,
    /// What the game draws where the Spell lands.
    #[serde(default)]
    pub effect: Option<EffectSprite>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectKind {
    Damage,
    /// Conjure `unit`s beside the caster; they fight like a base's
    /// attackers and, having no base, fall when their time is up.
    Summon,
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
    /// The `constructionRate` of a type that costs something and names
    /// none (the buildings: a Workshop is seen rising in the original).
    #[serde(default = "default_construction_rate")]
    pub default_rate: f64,
    /// The share of its full health a shell has when it is placed; the
    /// stream adds the rest as it builds.
    #[serde(default = "default_start_health_percent")]
    pub start_health_percent: i32,
    /// Seconds a shell may wait for a stream before the game says that
    /// none can reach it.
    #[serde(default = "default_stall_seconds")]
    pub stall_seconds: f64,
    /// How fast the stream of Storm Power travels from its source to a
    /// shell, in cells per second; the build begins when it arrives. 0
    /// for at once.
    #[serde(default = "default_stream_speed")]
    pub stream_cells_per_second: f64,
}

fn default_stream_speed() -> f64 {
    12.0
}

fn default_stall_seconds() -> f64 {
    3.0
}

fn default_construction_rate() -> f64 {
    10.0
}

fn default_start_health_percent() -> i32 {
    25
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
    /// Path cost of an island's rim cells: walkers keep to the inside of
    /// an island and step onto its edge only to reach a bridge or a place
    /// on the rim itself. 1 treats the rim like any ground.
    #[serde(default = "default_rim_cost")]
    pub rim_cost: u32,
    pub subcell: i32,
    /// Every structure's footprint blocks walking (and unit placement)
    /// unless its type carries a `walk_free` flag.
    pub structures_block: bool,
    /// A standing ground unit blocks its cell; a walker behind it waits
    /// `wait_seconds`, then paths round it.
    pub units_block: bool,
    pub wait_seconds: f64,
}

fn default_rim_cost() -> u32 {
    6
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
    /// Edge Farms: no bridge may attach to the island edge they grow on.
    #[serde(default)]
    pub edge_farm: Vec<String>,
    /// Barricade posts: two of a kind in line raise a barrier between them.
    #[serde(default)]
    pub fence: Vec<String>,
}

/// What a barricade does between its posts, by type.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FenceEffect {
    /// Enemy shots do not cross it (the Sun Barricade).
    Shield,
    /// Enemy ground units between the posts die (the Acid Barricade).
    Dissolve,
    /// Enemy units between the posts take the type's `hpPerSec` (the Arc Spire).
    Arc,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Fences {
    /// The effect of each barricade type's barrier.
    pub effects: BTreeMap<String, FenceEffect>,
    /// The barrier's colour on screen, by type.
    pub colours: BTreeMap<String, [f32; 4]>,
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
    /// A piece new in the Production window is cracked and hardens there
    /// in this long (the manual: "initially, bridges in the Production
    /// window appear cracked, but if not immediately used, they will
    /// quickly harden"); laid before then, it is laid cracked.
    #[serde(default = "default_harden_seconds")]
    pub harden_seconds: f64,
    /// An unattached bridge holds sound this long, then cracks (the
    /// manual: it "will eventually crack, then crumble and fall").
    #[serde(default = "default_hold_seconds")]
    pub hold_seconds: f64,
    /// Cracked, it holds this long before the first section falls.
    pub crumble_seconds: f64,
    /// Then it falls section by section from the break outwards: this
    /// many cells at a time, this long apart.
    #[serde(default = "default_section_cells")]
    pub section_cells: u32,
    #[serde(default = "default_section_seconds")]
    pub section_seconds: f64,
    pub piece_slots: usize,
    pub queue_seed: u64,
    pub pieces: Vec<PieceDef>,
}

fn default_harden_seconds() -> f64 {
    8.0
}
fn default_hold_seconds() -> f64 {
    3.0
}
fn default_section_cells() -> u32 {
    3
}
fn default_section_seconds() -> f64 {
    0.6
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
    /// The share of shots that connect with a unit of each type; every
    /// shot at a type not listed connects (the manual's Cloud Floater,
    /// "only one shot in twenty").
    #[serde(default)]
    pub hit_chance: BTreeMap<String, f64>,
    /// Seed of the dice those rolls come from; the same seed and commands
    /// give the same misses on every machine.
    #[serde(default)]
    pub dice_seed: u64,
    /// Shooters whose shot splinters on impact: every other enemy within
    /// `range` cells of the hit takes `percent` of the shot (the manual's
    /// Ice Cannon; the range is its missile's).
    #[serde(default)]
    pub shrapnel: BTreeMap<String, Shrapnel>,
    /// Shooters that fire in bursts: so many shots so many seconds apart,
    /// then the type's delay; each shot carries its share of the damage.
    #[serde(default)]
    pub bursts: BTreeMap<String, Burst>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Burst {
    pub shots: u32,
    pub interval_seconds: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Shrapnel {
    pub range: i32,
    pub percent: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Priest {
    pub temple_heal_range: i32,
    pub heal_per_second: u32,
    /// The manual: a stunned priest throws a shield round himself that
    /// keeps him from being destroyed. `false` lets fire finish him.
    #[serde(default = "yes")]
    pub stun_shield: bool,
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
    /// Ctrl+Z, offline only: how many of the player's commands can be
    /// taken back, one per press; 0 turns it off.
    #[serde(default = "default_undo_depth")]
    pub undo_depth: usize,
    /// Log the world's hash this often (0 for never), to compare runs.
    pub hash_every_seconds: f64,
    /// Where F5 saves a snapshot and F9 loads it from.
    pub save_file: String,
}

fn default_undo_depth() -> usize {
    20
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
        if self.air.attackers.values().any(|a| a.life_seconds.is_some_and(|s| s <= 0.0)) || self.air.default_life_seconds <= 0.0 {
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
        if !self.production.refresh_rates.contains_key(&self.production.unit_rate) || self.production.refresh_min_seconds < 0.0 {
            return bad("production.unit_rate must name an entry of production.refresh_rates");
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
