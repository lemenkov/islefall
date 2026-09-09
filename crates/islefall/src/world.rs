// SPDX-License-Identifier: Apache-2.0
//! Drawing the simulation: a shape library that caches one atlas per type,
//! and a first scene with an island, an altar and the High Priest.

use std::collections::HashMap;

use bevy::prelude::*;
use islefall_data::isle::{self, Theme};
use islefall_data::{Installation, Palette, bridge};
use islefall_sim::config::Grid;
use islefall_sim::{BridgeState, Cell, IslandMap, World};

/// Marks a terrain tile sprite; platform tiles are rebuilt when terrain changes.
#[derive(Component)]
pub struct TerrainTile {
    pub platform: bool,
}

/// Marks a structure sprite so the layer can be rebuilt.
#[derive(Component)]
pub struct StructureSprite;

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
}

/// Atlases built so far, keyed by type stem.
#[derive(Resource, Default)]
pub struct ShapeLibrary {
    shapes: HashMap<String, LoadedShape>,
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
            let records = install.shape_records(stem)?;
            let main = sprites::build_atlas(&install.shapes, palette, &records.images).ok()?;
            let shadow = if records.shadows.is_empty() {
                None
            } else {
                sprites::build_atlas(&install.shapes, palette, &records.shadows)
                    .ok()
                    .map(|a| (images.add(a.image), layouts.add(a.layout), a.frames))
            };
            self.shapes.insert(
                stem.to_string(),
                LoadedShape { image: images.add(main.image), layout: layouts.add(main.layout), frames: main.frames, shadow },
            );
        }
        self.shapes.get(stem)
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
    for st in &world.structures {
        let Some(def) = install.type_def(&st.kind) else { continue };
        let frame = def.frames.iter().position(|f| f.has_flag("default")).unwrap_or(0);
        let Some(shape) = lib.get_or_load(install, palette, &st.kind, images, layouts) else { continue };
        let z = Z_STRUCTURE + st.cell.y as f32 * 0.01;
        let e = spawn_frame(commands, shape, frame, cell_to_world(st.cell, &world.cfg.grid), z);
        commands.entity(e).insert(StructureSprite);
    }
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
