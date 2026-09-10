// SPDX-License-Identifier: Apache-2.0
//! Drawing the simulation: a shape library that caches one atlas per type,
//! and a first scene with an island, an altar and the High Priest.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use islefall_data::isle::{self, Theme};
use islefall_data::{Installation, Palette, Picture, Sheet, bridge};
use islefall_sim::config::Grid;
use islefall_sim::{BridgeState, Cell, IslandMap, World};

/// Marks a terrain tile sprite; platform tiles are rebuilt when terrain changes.
#[derive(Component)]
pub struct TerrainTile {
    pub platform: bool,
}

/// Marks a structure sprite so the layer can be rebuilt; carries the
/// structure's index so its look can follow the build.
#[derive(Component)]
pub struct StructureSprite(pub usize);

use crate::sprites::{self, FrameInfo};

/// Draw order layers; units add a small y-sort offset on top.
/// The cloud layers, under everything.
pub const Z_SKY: f32 = -10.0;
pub const Z_TERRAIN: f32 = 0.0;
/// Bridges sit just above terrain; each row draws over the one above it.
pub const Z_BRIDGE: f32 = 1.0;
pub const Z_STRUCTURE: f32 = 10.0;
pub const Z_SHADOW: f32 = 5.0;
pub const Z_UNIT: f32 = 30.0;

/// A type's frames uploaded as atlases: the image layer and, if present, the shadow layer.
pub struct LoadedShape {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub frames: Vec<FrameInfo>,
    pub shadow: Option<(Handle<Image>, Handle<TextureAtlasLayout>, Vec<FrameInfo>)>,
    /// Animation label of each frame, from the type file or the mod's sheet.
    pub labels: Vec<String>,
    /// Each frame's flags (`default`, `unlit`, ...).
    pub flags: Vec<Vec<String>>,
}

impl LoadedShape {
    /// Distinct animation labels in frame order.
    pub fn animations(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for l in &self.labels {
            if !out.contains(&l.as_str()) {
                out.push(l);
            }
        }
        out
    }
}

/// Atlases built so far, keyed by type stem. A type whose sheet lies in
/// `sheets` (a mod's `<stem>.toml` plus picture) is loaded from there
/// instead of the sprite cache.
#[derive(Resource, Default)]
pub struct ShapeLibrary {
    shapes: HashMap<String, LoadedShape>,
    pub sheets: Option<PathBuf>,
}

impl ShapeLibrary {
    pub fn get_or_load(
        &mut self,
        install: &Installation,
        palette: &Palette,
        stem: &str,
        images: &mut Assets<Image>,
        layouts: &mut Assets<TextureAtlasLayout>,
    ) -> Option<&LoadedShape> {
        if !self.shapes.contains_key(stem) {
            let loaded = self.load_sheet(stem, images, layouts).or_else(|| Self::load_cache(install, palette, stem, images, layouts))?;
            self.shapes.insert(stem.to_string(), loaded);
        }
        self.shapes.get(stem)
    }

    /// Labels of a loaded type's frames.
    pub fn labels(&self, stem: &str) -> Option<&[String]> {
        self.shapes.get(stem).map(|s| s.labels.as_slice())
    }

    fn load_sheet(&self, stem: &str, images: &mut Assets<Image>, layouts: &mut Assets<TextureAtlasLayout>) -> Option<LoadedShape> {
        let dir = self.sheets.as_ref()?;
        let index = dir.join(format!("{stem}.toml"));
        if !index.is_file() {
            return None;
        }
        let sheet = match Sheet::load(&index) {
            Ok(s) => s,
            Err(e) => {
                warn!("{}: {e}", index.display());
                return None;
            }
        };
        let pic = match Picture::load(dir.join(&sheet.image)) {
            Ok(p) => p,
            Err(e) => {
                warn!("{}: {}: {e}", index.display(), sheet.image);
                return None;
            }
        };
        let (main, shadow) = match sprites::build_sheet_atlases(&pic, &sheet) {
            Ok(a) => a,
            Err(e) => {
                warn!("{}: {e}", index.display());
                return None;
            }
        };
        info!("{stem}: sprites from {}", index.display());
        Some(LoadedShape {
            image: images.add(main.image),
            layout: layouts.add(main.layout),
            frames: main.frames,
            shadow: shadow.map(|a| (images.add(a.image), layouts.add(a.layout), a.frames)),
            labels: sheet.labels(),
            flags: sheet.frames.iter().map(|f| f.flags.clone()).collect(),
        })
    }

    fn load_cache(install: &Installation, palette: &Palette, stem: &str, images: &mut Assets<Image>, layouts: &mut Assets<TextureAtlasLayout>) -> Option<LoadedShape> {
        let records = install.shape_records(stem)?;
        let main = sprites::build_atlas(&install.shapes, palette, &records.images).ok()?;
        let shadow = if records.shadows.is_empty() {
            None
        } else {
            sprites::build_atlas(&install.shapes, palette, &records.shadows).ok().map(|a| (images.add(a.image), layouts.add(a.layout), a.frames))
        };
        let mut labels: Vec<String> = install.type_def(stem).map(|d| d.frames.iter().map(|f| f.animation.clone()).collect()).unwrap_or_default();
        labels.resize(main.frames.len(), "A".to_string());
        let mut flags: Vec<Vec<String>> = install.type_def(stem).map(|d| d.frames.iter().map(|f| f.flags.clone()).collect()).unwrap_or_default();
        flags.resize(main.frames.len(), Vec::new());
        Some(LoadedShape { image: images.add(main.image), layout: layouts.add(main.layout), frames: main.frames, shadow, labels, flags })
    }
}

/// Bevy world position (y up) of a cell's hotspot pixel, in source pixels.
pub fn cell_to_world(cell: Cell, g: &Grid) -> Vec2 {
    let (x, y) = cell.hotspot_px(g);
    Vec2::new(x as f32, -(y as f32))
}

/// Spawn one static frame of a loaded shape with its hotspot at `pos`.
pub fn spawn_frame(commands: &mut Commands, shape: &LoadedShape, frame: usize, pos: Vec2, z: f32) -> Entity {
    commands
        .spawn((
            Sprite::from_atlas_image(shape.image.clone(), TextureAtlas { layout: shape.layout.clone(), index: frame }),
            shape.frames[frame].anchor(),
            Transform::from_translation(pos.extend(z)),
        ))
        .id()
}

/// Deterministic per-cell variation so terrain does not repeat visibly.
fn variation(cell: Cell, count: usize) -> usize {
    let h = (cell.x as u32).wrapping_mul(0x9E37_79B1) ^ (cell.y as u32).wrapping_mul(0x85EB_CA6B);
    (h.rotate_left(13) as usize) % count.max(1)
}

/// Draw every cell of an island with the terrain set of `theme`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_island(
    commands: &mut Commands,
    install: &Installation,
    lib: &mut ShapeLibrary,
    palette: &Palette,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
    island: &IslandMap,
    theme: Theme,
    platform: bool,
    grid: &Grid,
) {
    let world_grid = grid.clone();
    let Some(isle_def) = install.type_def("isle") else { return };
    let mut by_piece = HashMap::new();
    let Some(shape) = lib.get_or_load(install, palette, "isle", images, layouts) else { return };
    for cell in island.cells() {
        let Some(piece) = island.piece_at(cell) else { continue };
        let frames = by_piece.entry(piece).or_insert_with(|| isle::frames(isle_def, theme, piece));
        if frames.is_empty() {
            continue;
        }
        let frame = frames[variation(cell, frames.len())];
        let e = spawn_frame(commands, shape, frame, cell_to_world(cell, &world_grid), Z_TERRAIN + if platform { 0.5 } else { 0.0 });
        commands.entity(e).insert(TerrainTile { platform });
    }
}

/// Draw every structure of the world with its default frame.
pub fn spawn_structures(
    commands: &mut Commands,
    install: &Installation,
    lib: &mut ShapeLibrary,
    palette: &Palette,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
    world: &World,
) {
    for (i, st) in world.structures.iter().enumerate() {
        let Some(shape) = lib.get_or_load(install, palette, &st.kind, images, layouts) else { continue };
        let plan = structure_frames(shape, &world.cfg.animation, install.type_def(&st.kind).map(|d| d.flags.as_slice()).unwrap_or(&[]), st.cell, &st.kind, st.variant);
        let z = Z_STRUCTURE + st.cell.y as f32 * 0.01;
        let pos = cell_to_world(st.cell, &world.cfg.grid);
        if let Some(base) = plan.base {
            let b = spawn_frame(commands, shape, base, pos, z);
            commands.entity(b).insert(StructureSprite(i));
        }
        let e = spawn_frame(commands, shape, plan.sequence[0], pos, z + 0.001);
        commands.entity(e).insert((StructureSprite(i), plan));
    }
}

/// What a standing structure shows: a looping idle, a variant picked per
/// cell, or its default frame. Also marks what the turret can do.
#[derive(Component, Clone, Debug)]
pub struct StructureFrames {
    /// Frames to cycle; one frame means a still.
    pub sequence: Vec<usize>,
    /// A frame drawn underneath when the idle frames are an overlay (a
    /// Temple's window lights) rather than whole pictures.
    pub base: Option<usize>,
    /// Placement of every frame, for anchoring.
    pub frames: Vec<FrameInfo>,
    /// Frames of the all-round turret, in order of bearing; empty if none.
    pub turret: Vec<usize>,
    /// Frames of each cardinal firing animation, by direction name.
    pub cardinal: HashMap<String, Vec<usize>>,
    /// How many runs the idle splits into, full to empty; 1 for one loop.
    pub stages: u32,
}

/// A frame's flags without the role tags, for grouping frames that share a look.
fn look_flags(flags: &[String], tags: &[String]) -> Vec<String> {
    let mut v: Vec<String> = flags.iter().filter(|f| !tags.iter().any(|t| t.eq_ignore_ascii_case(f))).map(|f| f.to_lowercase()).collect();
    v.sort();
    v
}

pub fn structure_frames(shape: &LoadedShape, rules: &islefall_sim::config::Animation, type_flags: &[String], cell: Cell, kind: &str, pinned: Option<u32>) -> StructureFrames {
    let n = shape.labels.len();
    let default = (0..n).find(|&i| shape.flags[i].iter().any(|f| f.eq_ignore_ascii_case("default"))).unwrap_or(0);
    let default_look = look_flags(shape.flags.get(default).map(Vec::as_slice).unwrap_or(&[]), &rules.tag_flags);
    let untagged = |i: usize| !shape.flags[i].iter().any(|f| rules.tag_flags.iter().any(|t| t.eq_ignore_ascii_case(f)) && !f.eq_ignore_ascii_case("default"));
    let group = |label: &str, look: &Vec<String>| -> Vec<usize> {
        (0..n).filter(|&i| shape.labels[i].eq_ignore_ascii_case(label) && untagged(i) && look_flags(&shape.flags[i], &rules.tag_flags) == *look).collect()
    };
    let idle = {
        let same = group(&rules.idle_label, &default_look);
        if same.len() >= 2 { same } else { group(&rules.idle_label, &Vec::new()) }
    };
    let variants = if type_flags.iter().any(|f| f.eq_ignore_ascii_case(&rules.variant_flag)) {
        group(&shape.labels[default], &default_look)
    } else {
        Vec::new()
    };
    let (sequence, base) = if idle.len() >= 2 {
        let overlay = !idle.contains(&default);
        (idle, overlay.then_some(default))
    } else if variants.len() >= 2 {
        let pick = pinned.map(|f| f as usize % variants.len()).unwrap_or_else(|| variation(cell, variants.len()));
        (vec![variants[pick]], None)
    } else {
        (vec![default], None)
    };
    let turret = {
        let t = group(&rules.turret_label, &Vec::new());
        if t.len() >= rules.turret_min_frames { t } else { Vec::new() }
    };
    let cardinal = rules.cardinal.iter().map(|(dir, label)| (dir.clone(), group(label, &Vec::new()))).filter(|(_, f)| !f.is_empty()).collect();
    let stages = rules.stages.get(kind).copied().unwrap_or(1).max(1);
    StructureFrames { sequence, base, frames: shape.frames.clone(), turret, cardinal, stages }
}

/// Marks a bridge tile sprite so the layer can be rebuilt.
#[derive(Component)]
pub struct BridgeTile;

/// Draw every bridge cell of the world with the tile matching its connections.
#[allow(clippy::too_many_arguments)]
pub fn spawn_bridges(
    commands: &mut Commands,
    install: &Installation,
    lib: &mut ShapeLibrary,
    palette: &Palette,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
    world: &World,
) {
    let Some(def) = install.type_def("bridge") else { return };
    let Some(shape) = lib.get_or_load(install, palette, "bridge", images, layouts) else { return };
    let mut by_mask: HashMap<(u8, BridgeState), Vec<usize>> = HashMap::new();
    for (&cell, &state) in &world.bridges {
        let mask = world.bridge_connections(cell);
        let condition = match state {
            BridgeState::Normal => bridge::Condition::Normal,
            BridgeState::Cracked => bridge::Condition::Cracked,
            BridgeState::Hard => bridge::Condition::Hard,
        };
        let frames = by_mask.entry((mask, state)).or_insert_with(|| bridge::frames(def, mask, condition));
        if frames.is_empty() {
            continue;
        }
        let frame = frames[variation(cell, frames.len())];
        let z = Z_BRIDGE + cell.y as f32 * 0.001;
        let e = spawn_frame(commands, shape, frame, cell_to_world(cell, &world.cfg.grid), z);
        commands.entity(e).insert(BridgeTile);
    }
    spawn_bridge_connectors(commands, install, lib, palette, images, layouts, world);
}

/// Where a bridge meets island land, overlay the island's rim cell with the
/// connector ramp. `bridgeconnector` frames are ordered by where the island
/// lies from the bridge: north, east, south, west.
#[allow(clippy::too_many_arguments)]
fn spawn_bridge_connectors(
    commands: &mut Commands,
    install: &Installation,
    lib: &mut ShapeLibrary,
    palette: &Palette,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
    world: &World,
) {
    let Some(def) = install.type_def("bridgeconnector") else { return };
    let Some(shape) = lib.get_or_load(install, palette, "bridgeconnector", images, layouts) else { return };
    let mut done = std::collections::HashSet::new();
    for &cell in world.bridges.keys() {
        for (frame, dx, dy) in [(0usize, 0, -1), (1, 1, 0), (2, 0, 1), (3, -1, 0)] {
            let land = cell.offset(dx, dy);
            if frame >= def.frames.len() || !world.is_land(land) || !done.insert((land, frame)) {
                continue;
            }
            let z = Z_BRIDGE - 0.5 + land.y as f32 * 0.001;
            let e = spawn_frame(commands, shape, frame, cell_to_world(land, &world.cfg.grid), z);
            commands.entity(e).insert(BridgeTile);
        }
    }
}
