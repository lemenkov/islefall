// SPDX-License-Identifier: Apache-2.0
//! Drawing the simulation: a shape library that caches one atlas per type,
//! and a first scene with an island, an altar and the High Priest.

use std::collections::HashMap;

use bevy::prelude::*;
use islefall_data::isle::{self, Theme};
use islefall_data::{Installation, Palette};
use islefall_sim::{Cell, IslandMap};

use crate::sprites::{self, FrameInfo};

/// Draw order layers; units add a small y-sort offset on top.
pub const Z_TERRAIN: f32 = 0.0;
pub const Z_STRUCTURE: f32 = 10.0;
pub const Z_SHADOW: f32 = 20.0;
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
pub fn cell_to_world(cell: Cell) -> Vec2 {
    let (x, y) = cell.hotspot_px();
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
pub fn spawn_island(
    commands: &mut Commands,
    install: &Installation,
    lib: &mut ShapeLibrary,
    palette: &Palette,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
    island: &IslandMap,
    theme: Theme,
) {
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
        spawn_frame(commands, shape, frame, cell_to_world(cell), Z_TERRAIN);
    }
}
