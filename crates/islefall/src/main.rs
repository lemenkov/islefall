// SPDX-License-Identifier: Apache-2.0
//! Islefall entry point.
//!
//! Two modes, chosen with `ISLEFALL_MODE`:
//! - `island` (default): an island with the altar and the High Priest,
//!   driven by the simulation. Left-click on ground sends the priest there,
//!   right-click on sky next to ground places a bridge cell.
//!   Arrow keys or WASD pan the camera, `-` and `=` zoom.
//! - `viewer`: animates one object type at a time. `[` and `]` step
//!   through types, `,` and `.` through animations.
//!
//! Environment:
//! - `NETSTORM_DIR`: directory containing the game's `d/` and `netstorm.tarc`.
//! - `ISLEFALL_PALETTE`: palette file stem from `d/` (default `gifcloud`, the game palette).
//! - `ISLEFALL_SCREENSHOT=file.png`: save a screenshot shortly after start-up.
//! - `ISLEFALL_ISLAND=cross`: use a cross-shaped test island in the island scene.
//! - `ISLEFALL_CAMERA=x,y`: start the camera centred on that cell instead of the island.
//!
//! Keys in both modes: `Space` pauses animation, `P` saves a screenshot.

mod sprites;
mod world;

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::sprite::Anchor;
use bevy::window::PrimaryWindow;
use islefall_data::isle::Theme;
use islefall_data::shapes::SHAPE_ORDER;
use islefall_data::{Installation, TypeDef};
use islefall_sim::{CELL_H, CELL_W, Cell, Dir8, IslandMap, Structure, TICK_HZ, Unit, World, speed_per_tick};
use sprites::FrameInfo;
use world::{BridgeTile, LoadedShape, ShapeLibrary, Z_SHADOW, Z_STRUCTURE, Z_UNIT};

/// Type flags that keep units off a footprint, until the real walk rules are known.
const BLOCKING_FLAGS: [&str; 9] = ["walkBlocking", "yuckWalk", "dropBlocking", "emplacement", "factory", "tree", "dais", "fence", "vortex"];

/// Row-based draw order: things lower on screen draw over things above them.
fn depth_z(base: f32, row: f32) -> f32 {
    base + row * 0.01
}

const START_TYPE: &str = "priest";
const FRAME_SECONDS: f32 = 0.1;
const ZOOM: f32 = 4.0;
const AUTO_SCREENSHOT_SECONDS: f32 = 1.5;
const DEFAULT_PALETTE: &str = "gifcloud";
/// Camera pan speed in source pixels per second.
const PAN_SPEED: f32 = 120.0;

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

/// The simulation state; stepped on the fixed schedule only.
#[derive(Resource, Default)]
struct Sim {
    world: World,
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
    playing: bool,
}

/// Marks entities spawned by the viewer so they can be replaced.
#[derive(Component)]
struct ViewerEntity;

/// A sprite layer that follows a simulation unit.
#[derive(Component)]
struct UnitLayer {
    unit: usize,
    facing: Dir8,
    shadow: bool,
}

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
    .insert_resource(Time::<Fixed>::from_hz(TICK_HZ as f64))
    .init_resource::<ShapeLibrary>()
    .init_resource::<Sim>()
    .insert_resource(Viewer {
        type_index: SHAPE_ORDER.iter().position(|s| *s == START_TYPE).unwrap_or(0),
        animation: 0,
        timer: Timer::from_seconds(FRAME_SECONDS, TimerMode::Repeating),
        paused: false,
    })
    .add_systems(Startup, setup)
    .add_systems(FixedUpdate, sim_step.run_if(resource_equals(Mode::Island)))
    .add_systems(Update, (common_keys, animate, auto_screenshot))
    .add_systems(Update, (camera_keys, click_to_move, sync_units, sync_bridges).run_if(resource_equals(Mode::Island)))
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
    mut sim: ResMut<Sim>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    match *mode {
        Mode::Island => setup_island(&mut commands, &data, &mut sim, &mut lib, &mut images, &mut layouts),
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
    sim: &mut Sim,
    lib: &mut ShapeLibrary,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let install = &data.install;
    let palette = install.palette(&data.palette).expect("palette checked at start-up");
    let (w, h) = (14, 10);
    let mut island = IslandMap::rect(Cell::new(0, 0), w, h);
    if std::env::var("ISLEFALL_ISLAND").as_deref() == Ok("cross") {
        // A cross has all four inside corners; used to check the terrain set.
        for c in [(0, 0), (1, 0), (0, 1), (13, 0), (12, 0), (13, 1), (0, 9), (1, 9), (0, 8), (13, 9), (12, 9), (13, 8)] {
            island.remove(Cell::new(c.0, c.1));
        }
    } else {
        for c in [Cell::new(13, 0), Cell::new(12, 0), Cell::new(13, 1)] {
            island.remove(c);
        }
    }
    world::spawn_island(commands, install, lib, palette, images, layouts, &island, Theme::Sun);

    // The simulation owns the island, the structures and the priest; sprites follow it.
    sim.world.islands.push(island);
    for (stem, cell) in [("dais", Cell::new(9, 7)), ("treetwo", Cell::new(3, 3)), ("treetwo", Cell::new(12, 7))] {
        place_structure(commands, data, sim, lib, images, layouts, stem, cell);
    }
    // A bridge off the east edge that uses straights, a cross, a corner and ends.
    for (x, y) in [(14, 4), (15, 4), (16, 4), (17, 4), (18, 4), (18, 3), (18, 2), (16, 3), (16, 5), (16, 6), (19, 3), (17, 2), (15, 5), (17, 3)] {
        sim.world.place_bridge(Cell::new(x, y));
    }
    if let Some(def) = install.type_def("priest") {
        let speed = speed_per_tick(def.get_f64("speed").unwrap_or(1.0));
        sim.world.units.push(Unit::new("priest", Cell::new(2, 8), speed));
        sim.world.order_move(0, Cell::new(18, 2));
        if let Some(shape) = lib.get_or_load(install, palette, "priest", images, layouts) {
            let unit = &sim.world.units[0];
            let entities = spawn_animated(commands, shape, def, unit.facing.animation(), unit_to_world(unit), Z_UNIT, false);
            for (i, e) in entities.into_iter().enumerate() {
                commands.entity(e).insert(UnitLayer { unit: 0, facing: unit.facing, shadow: i == 1 });
            }
        }
    }

    let mut centre = (world::cell_to_world(Cell::new(0, 0)) + world::cell_to_world(Cell::new(w - 1, h - 1))) / 2.0;
    if let Ok(spec) = std::env::var("ISLEFALL_CAMERA") {
        if let Some((x, y)) = spec.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<i32>().ok()?, y.trim().parse::<i32>().ok()?))) {
            centre = world::cell_to_world(Cell::new(x, y));
        }
    }
    commands.spawn((Camera2d, zoomed_projection(), Transform::from_translation(centre.extend(0.0))));
    info!("island scene: {} cells, altar and priest", sim.world.islands[0].len());
}

/// Add a structure to the simulation from its type's footprint and flags, and draw its default frame.
#[allow(clippy::too_many_arguments)]
fn place_structure(
    commands: &mut Commands,
    data: &GameData,
    sim: &mut Sim,
    lib: &mut ShapeLibrary,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
    stem: &str,
    cell: Cell,
) {
    let install = &data.install;
    let palette = install.palette(&data.palette).expect("palette checked at start-up");
    let Some(def) = install.type_def(stem) else {
        warn!("{stem}: unknown type");
        return;
    };
    let foot_x = def.get_i64("foot_x").unwrap_or(1) as i32;
    let foot_y = def.get_i64("foot_y").unwrap_or(1) as i32;
    let blocks = BLOCKING_FLAGS.iter().any(|f| def.has_flag(f));
    sim.world.structures.push(Structure::new(stem, cell, foot_x, foot_y, blocks));
    let frame = def.frames.iter().position(|f| f.has_flag("default")).unwrap_or(0);
    if let Some(shape) = lib.get_or_load(install, palette, stem, images, layouts) {
        world::spawn_frame(commands, shape, frame, world::cell_to_world(cell), depth_z(Z_STRUCTURE, cell.y as f32));
    }
}

/// World position of a unit's feet: cells to source pixels, y flipped.
fn unit_to_world(unit: &Unit) -> Vec2 {
    let (x, y) = unit.pos.to_f32();
    Vec2::new(x * CELL_W as f32, -(y * CELL_H as f32))
}

/// Cell under a world position.
fn world_to_cell(p: Vec2) -> Cell {
    Cell::new((p.x / CELL_W as f32).floor() as i32, (-p.y / CELL_H as f32).floor() as i32)
}

/// Spawn image and shadow layers of one animation, looping. Returns the entities.
fn spawn_animated(
    commands: &mut Commands,
    shape: &LoadedShape,
    def: &TypeDef,
    animation: &str,
    pos: Vec2,
    z: f32,
    viewer: bool,
) -> Vec<Entity> {
    let sequence = animation_sequence(def, animation);
    let mut out = Vec::new();
    if sequence.is_empty() {
        return out;
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
            ShapeSprite { sequence: sequence.clone(), frames, step: 0, playing: true },
        ));
        if viewer {
            e.insert(ViewerEntity);
        }
        out.push(e.id());
    }
    out
}

/// Frame indices of one animation label, in file order.
fn animation_sequence(def: &TypeDef, animation: &str) -> Vec<usize> {
    def.frames.iter().enumerate().filter(|(_, f)| f.animation.eq_ignore_ascii_case(animation)).map(|(i, _)| i).collect()
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

fn sim_step(mut sim: ResMut<Sim>, viewer: Res<Viewer>) {
    if !viewer.paused {
        sim.world.step();
    }
}

/// Move unit sprites to their simulated position and pick the facing animation.
fn sync_units(
    sim: Res<Sim>,
    data: Res<GameData>,
    mut layers: Query<(&mut Transform, &mut Sprite, &mut Anchor, &mut ShapeSprite, &mut UnitLayer)>,
) {
    for (mut tf, mut sprite, mut anchor, mut shape, mut layer) in &mut layers {
        let Some(unit) = sim.world.units.get(layer.unit) else { continue };
        let pos = unit_to_world(unit);
        tf.translation.x = pos.x;
        tf.translation.y = pos.y;
        let (_, row) = unit.pos.to_f32();
        tf.translation.z = if layer.shadow { Z_SHADOW } else { depth_z(Z_STRUCTURE, row) + 0.005 };
        if unit.facing != layer.facing {
            if let Some(def) = data.install.type_def(&unit.kind) {
                let seq = animation_sequence(def, unit.facing.animation());
                if !seq.is_empty() {
                    shape.sequence = seq;
                    shape.step = 0;
                }
            }
            layer.facing = unit.facing;
        }
        shape.playing = unit.is_moving();
        if !shape.playing && shape.step != 0 {
            shape.step = 0;
            let frame = shape.sequence[0];
            if let Some(atlas) = sprite.texture_atlas.as_mut() {
                atlas.index = frame;
            }
            *anchor = shape.frames[frame].anchor();
        }
    }
}

fn click_to_move(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut sim: ResMut<Sim>,
) {
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cameras.single()) else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_tf, cursor) else { return };
    let cell = world_to_cell(world_pos);
    if left {
        if sim.world.order_move(0, cell) {
            info!("priest ordered to {cell:?}");
        } else {
            info!("{cell:?} is unreachable");
        }
    } else if sim.world.place_bridge(cell) {
        info!("bridge placed at {cell:?}");
    } else {
        info!("cannot place a bridge at {cell:?}");
    }
}

/// Rebuild the bridge layer whenever the simulation's bridge set changes.
fn sync_bridges(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    existing: Query<Entity, With<BridgeTile>>,
    mut seen: Local<Option<u64>>,
) {
    if *seen == Some(sim.world.bridge_version) {
        return;
    }
    *seen = Some(sim.world.bridge_version);
    for e in &existing {
        commands.entity(e).despawn();
    }
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    world::spawn_bridges(&mut commands, &data.install, &mut lib, palette, &mut images, &mut layouts, &sim.world);
}

fn camera_keys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut tf, mut proj)) = cameras.single_mut() else { return };
    let mut d = Vec2::ZERO;
    if keys.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
        d.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
        d.x += 1.0;
    }
    if keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        d.y += 1.0;
    }
    if keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        d.y -= 1.0;
    }
    if d != Vec2::ZERO {
        let step = d.normalize() * PAN_SPEED * time.delta_secs();
        tf.translation.x += step.x;
        tf.translation.y += step.y;
    }
    if let Projection::Orthographic(o) = &mut *proj {
        if keys.just_pressed(KeyCode::Equal) {
            o.scale = (o.scale / 1.25).max(1.0 / 16.0);
        }
        if keys.just_pressed(KeyCode::Minus) {
            o.scale = (o.scale * 1.25).min(2.0);
        }
    }
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
        if !shape.playing {
            continue;
        }
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
