// SPDX-License-Identifier: Apache-2.0
//! Islefall entry point.
//!
//! Two modes, chosen with `ISLEFALL_MODE`:
//! - `island` (default): a first scene with an island, an altar and the
//!   High Priest, drawn from the simulation's grid.
//! - `viewer`: animates one object type at a time. `[` and `]` step
//!   through types, `,` and `.` through animations.
//!
//! Environment:
//! - `NETSTORM_DIR`: directory containing the game's `d/` and `netstorm.tarc`.
//! - `ISLEFALL_PALETTE`: palette file stem from `d/` (default `suncannon`).
//! - `ISLEFALL_SCREENSHOT=file.png`: save a screenshot shortly after start-up.
//!
//! Keys in both modes: `Space` pauses animation, `P` saves a screenshot.

mod sprites;
mod world;

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::sprite::Anchor;
use islefall_data::isle::Theme;
use islefall_data::shapes::SHAPE_ORDER;
use islefall_data::{Installation, TypeDef};
use islefall_sim::{Cell, IslandMap};
use sprites::FrameInfo;
use world::{LoadedShape, ShapeLibrary, Z_SHADOW, Z_STRUCTURE, Z_UNIT};

const START_TYPE: &str = "priest";
const FRAME_SECONDS: f32 = 0.1;
const ZOOM: f32 = 3.0;
const AUTO_SCREENSHOT_SECONDS: f32 = 1.5;
const DEFAULT_PALETTE: &str = "suncannon";

#[derive(Resource, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Island,
    Viewer,
}

/// The user-supplied NetStorm data, loaded once at startup.
#[derive(Resource)]
struct GameData {
    install: Installation,
    palette: String,
}

#[derive(Resource)]
struct Viewer {
    /// Index into [`SHAPE_ORDER`].
    type_index: usize,
    /// Index into the type's animation list.
    animation: usize,
    timer: Timer,
    paused: bool,
}

/// Pending automatic screenshot requested through `ISLEFALL_SCREENSHOT`.
#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    delay: Timer,
}

/// An animated shape layer (image or shadow); the entity's transform marks the hotspot.
#[derive(Component)]
struct ShapeSprite {
    /// Frame indices (into `frames`) that make up the current animation.
    sequence: Vec<usize>,
    frames: Vec<FrameInfo>,
    step: usize,
}

/// Marks entities spawned by the viewer so they can be replaced.
#[derive(Component)]
struct ViewerEntity;

fn main() {
    let dir = match std::env::var_os("NETSTORM_DIR") {
        Some(d) => PathBuf::from(d),
        None => {
            eprintln!("NETSTORM_DIR is not set; point it at a NetStorm installation (the directory holding d/ and netstorm.tarc)");
            std::process::exit(2);
        }
    };
    let install = match Installation::load(&dir) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("failed to load NetStorm data from {}: {e}", dir.display());
            std::process::exit(1);
        }
    };
    let palette = std::env::var("ISLEFALL_PALETTE").unwrap_or_else(|_| DEFAULT_PALETTE.into());
    if install.palette(&palette).is_none() {
        eprintln!("palette {palette} not found in {}/d; available: {:?}", dir.display(), install.palettes.keys().collect::<Vec<_>>());
        std::process::exit(1);
    }
    let mode = match std::env::var("ISLEFALL_MODE").as_deref() {
        Ok("viewer") => Mode::Viewer,
        _ => Mode::Island,
    };

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window { title: "Islefall".into(), ..default() }),
                ..default()
            }),
    )
    .insert_resource(GameData { install, palette })
    .insert_resource(mode)
    .init_resource::<ShapeLibrary>()
    .insert_resource(Viewer {
        type_index: SHAPE_ORDER.iter().position(|s| *s == START_TYPE).unwrap_or(0),
        animation: 0,
        timer: Timer::from_seconds(FRAME_SECONDS, TimerMode::Repeating),
        paused: false,
    })
    .add_systems(Startup, setup)
    .add_systems(Update, (common_keys, animate, auto_screenshot))
    .add_systems(Update, viewer_keys.run_if(resource_equals(Mode::Viewer)));
    if let Ok(path) = std::env::var("ISLEFALL_SCREENSHOT") {
        app.insert_resource(AutoScreenshot {
            path,
            delay: Timer::from_seconds(AUTO_SCREENSHOT_SECONDS, TimerMode::Once),
        });
    }
    app.run();
}

fn auto_screenshot(mut commands: Commands, time: Res<Time>, auto: Option<ResMut<AutoScreenshot>>) {
    let Some(mut auto) = auto else { return };
    if auto.delay.tick(time.delta()).just_finished() {
        info!("saving screenshot to {}", auto.path);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(auto.path.clone()));
    }
}

fn setup(
    mut commands: Commands,
    mode: Res<Mode>,
    data: Res<GameData>,
    viewer: Res<Viewer>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    match *mode {
        Mode::Island => setup_island(&mut commands, &data, &mut lib, &mut images, &mut layouts),
        Mode::Viewer => {
            commands.spawn((Camera2d, zoomed_projection()));
            spawn_viewer_shape(&mut commands, &data, &viewer, &mut lib, &mut images, &mut layouts);
        }
    }
}

fn zoomed_projection() -> Projection {
    Projection::Orthographic(OrthographicProjection { scale: 1.0 / ZOOM, ..OrthographicProjection::default_2d() })
}

/// A first scene: a sun-themed island with a notch, the altar, and the priest.
fn setup_island(
    commands: &mut Commands,
    data: &GameData,
    lib: &mut ShapeLibrary,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let install = &data.install;
    let palette = install.palette(&data.palette).expect("palette checked at start-up");
    let (w, h) = (14, 10);
    let mut island = IslandMap::rect(Cell::new(0, 0), w, h);
    for c in [Cell::new(13, 0), Cell::new(12, 0), Cell::new(13, 1)] {
        island.remove(c);
    }
    world::spawn_island(commands, install, lib, palette, images, layouts, &island, Theme::Sun);

    // The altar has a 7x7 footprint; its hotspot is the bottom-right cell.
    if let Some(def) = install.type_def("dais") {
        let frame = def.frames.iter().position(|f| f.label.eq_ignore_ascii_case("P03")).unwrap_or(0);
        if let Some(shape) = lib.get_or_load(install, palette, "dais", images, layouts) {
            world::spawn_frame(commands, shape, frame, world::cell_to_world(Cell::new(9, 7)), Z_STRUCTURE);
        }
    }

    if let Some(def) = install.type_def("priest") {
        if let Some(shape) = lib.get_or_load(install, palette, "priest", images, layouts) {
            spawn_animated(commands, shape, def, "A", world::cell_to_world(Cell::new(2, 8)), Z_UNIT, false);
        }
    }

    let centre = (world::cell_to_world(Cell::new(0, 0)) + world::cell_to_world(Cell::new(w - 1, h - 1))) / 2.0;
    commands.spawn((Camera2d, zoomed_projection(), Transform::from_translation(centre.extend(0.0))));
    info!("island scene: {} cells, altar and priest", island.len());
}

/// Spawn image and shadow layers of one animation, looping.
fn spawn_animated(
    commands: &mut Commands,
    shape: &LoadedShape,
    def: &TypeDef,
    animation: &str,
    pos: Vec2,
    z: f32,
    viewer: bool,
) {
    let sequence: Vec<usize> = def
        .frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f.animation.eq_ignore_ascii_case(animation))
        .map(|(i, _)| i)
        .collect();
    if sequence.is_empty() {
        return;
    }
    let first = sequence[0];
    let mut layers = vec![(shape.image.clone(), shape.layout.clone(), shape.frames.clone(), z, Color::WHITE)];
    if let Some((img, lay, frames)) = &shape.shadow {
        // Shadow records only carry a silhouette; draw it as translucent black.
        layers.push((img.clone(), lay.clone(), frames.clone(), Z_SHADOW.min(z - 1.0), Color::srgba(0.0, 0.0, 0.0, 0.5)));
    }
    for (image, layout, frames, z, color) in layers {
        let mut sprite = Sprite::from_atlas_image(image, TextureAtlas { layout, index: first });
        sprite.color = color;
        let mut e = commands.spawn((
            sprite,
            frames[first].anchor(),
            Transform::from_translation(pos.extend(z)),
            ShapeSprite { sequence: sequence.clone(), frames, step: 0 },
        ));
        if viewer {
            e.insert(ViewerEntity);
        }
    }
}

fn spawn_viewer_shape(
    commands: &mut Commands,
    data: &GameData,
    viewer: &Viewer,
    lib: &mut ShapeLibrary,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let stem = SHAPE_ORDER[viewer.type_index];
    let install = &data.install;
    let palette = install.palette(&data.palette).expect("palette checked at start-up");
    let Some(def) = install.type_def(stem) else { return };
    let animations = def.animations();
    let Some(&animation) = animations.get(viewer.animation) else { return };
    info!(
        "{stem} ({}): {} frames, {} animations, showing {animation}",
        def.get_str("description").unwrap_or("-"),
        def.frames.len(),
        animations.len()
    );
    let Some(shape) = lib.get_or_load(install, palette, stem, images, layouts) else {
        warn!("{stem}: no atlas");
        return;
    };
    spawn_animated(commands, shape, def, animation, Vec2::ZERO, Z_UNIT, true);
}

fn animate(
    time: Res<Time>,
    mut viewer: ResMut<Viewer>,
    mut sprites: Query<(&mut Sprite, &mut Anchor, &mut ShapeSprite)>,
) {
    if viewer.paused || !viewer.timer.tick(time.delta()).just_finished() {
        return;
    }
    for (mut sprite, mut anchor, mut shape) in &mut sprites {
        let Some(atlas) = sprite.texture_atlas.as_mut() else { continue };
        shape.step = (shape.step + 1) % shape.sequence.len().max(1);
        let frame = shape.sequence[shape.step];
        atlas.index = frame;
        *anchor = shape.frames[frame].anchor();
    }
}

fn common_keys(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, mut viewer: ResMut<Viewer>) {
    if keys.just_pressed(KeyCode::Space) {
        viewer.paused = !viewer.paused;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk("islefall.png"));
    }
}

fn viewer_keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    data: Res<GameData>,
    mut viewer: ResMut<Viewer>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    existing: Query<Entity, With<ViewerEntity>>,
) {
    let n = SHAPE_ORDER.len();
    let mut changed = false;
    if keys.just_pressed(KeyCode::BracketRight) {
        viewer.type_index = (viewer.type_index + 1) % n;
        viewer.animation = 0;
        changed = true;
    } else if keys.just_pressed(KeyCode::BracketLeft) {
        viewer.type_index = (viewer.type_index + n - 1) % n;
        viewer.animation = 0;
        changed = true;
    } else if keys.just_pressed(KeyCode::Period) || keys.just_pressed(KeyCode::Comma) {
        let count = data.install.type_def(SHAPE_ORDER[viewer.type_index]).map(|d| d.animations().len()).unwrap_or(1).max(1);
        viewer.animation = if keys.just_pressed(KeyCode::Period) { (viewer.animation + 1) % count } else { (viewer.animation + count - 1) % count };
        changed = true;
    }
    if changed {
        for e in &existing {
            commands.entity(e).despawn();
        }
        spawn_viewer_shape(&mut commands, &data, &viewer, &mut lib, &mut images, &mut layouts);
    }
}
