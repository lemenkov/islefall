// SPDX-License-Identifier: Apache-2.0
//! Islefall entry point.
//!
//! Two modes, chosen with `ISLEFALL_MODE`:
//! - `island` (default): an island with the altar and the High Priest,
//!   driven by the simulation. Left-click selects a unit, sends the selected
//!   unit somewhere, or, on a Storm Geyser, sets it harvesting; right-click
//!   acts with the current tool. The window title shows Storm Power.
//!   Tools: `Q`, `W`, `A`, `S` pick a bridge piece from the four slots on
//!   offer (`R` rotates it), `1`..`5` drop a building, `U` spawn a golem.
//!   `Delete` destroys, `C` cracks and `H` hardens the bridge cell under the
//!   cursor; `V` salvages the player's structure under the cursor. Arrow keys or WASD pan the camera, `-` and `=` zoom.
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
use islefall_sim::{CELL_H, CELL_W, Cell, Dir8, IslandMap, Piece, TICK_HZ, TypeRules, Unit, World};
use sprites::FrameInfo;
use world::{BridgeTile, LoadedShape, ShapeLibrary, StructureSprite, TerrainTile, Z_SHADOW, Z_STRUCTURE, Z_UNIT};

const START_TYPE: &str = "priest";
const FRAME_SECONDS: f32 = 0.1;
const ZOOM: f32 = 4.0;
const AUTO_SCREENSHOT_SECONDS: f32 = 1.5;
const DEFAULT_PALETTE: &str = "gifcloud";
/// Camera pan speed in source pixels per second.
const PAN_SPEED: f32 = 120.0;
/// Buildings on the number keys.
const BUILD_TOOLS: [(KeyCode, &str); 5] = [
    (KeyCode::Digit1, "suncannon"),
    (KeyCode::Digit2, "sunarcher"),
    (KeyCode::Digit3, "sunbattery"),
    (KeyCode::Digit4, "sunfactory"),
    (KeyCode::Digit5, "treetwo"),
];
const UNIT_TOOL: &str = "sunwalker";
/// Storm Power to start the demo with.
const START_POWER: i32 = 1500;

/// Row-based draw order: things lower on screen draw over things above them.
fn depth_z(base: f32, row: f32) -> f32 {
    base + row * 0.01
}

#[derive(Resource, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Island,
    Viewer,
}

/// What a right-click does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tool {
    /// A bridge piece taken from queue slot `.0`, rotated as `.1`.
    Bridge(usize, Piece),
    Drop(&'static str),
    Spawn(&'static str),
}

/// Keys for the piece slots, as in the original Production window.
const SLOT_KEYS: [KeyCode; 6] = [KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyZ, KeyCode::KeyX];

/// The user-supplied NetStorm data, loaded once at startup.
#[derive(Resource)]
struct GameData {
    install: Installation,
    palette: String,
}

impl GameData {
    fn rules(&self, stem: &str) -> TypeRules {
        self.install.type_def(stem).map(TypeRules::from_type).unwrap_or_else(TypeRules::plain)
    }
}

/// The simulation state; stepped on the fixed schedule only.
#[derive(Resource, Default)]
struct Sim {
    world: World,
}

/// Player-side interaction state.
#[derive(Resource)]
struct Player {
    tool: Tool,
    selected: usize,
}

/// Marks the placement preview sprites.
#[derive(Component)]
struct Ghost;

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
    .insert_resource(Player { tool: Tool::Spawn(UNIT_TOOL), selected: 0 })
    .insert_resource(Viewer {
        type_index: SHAPE_ORDER.iter().position(|s| *s == START_TYPE).unwrap_or(0),
        animation: 0,
        timer: Timer::from_seconds(FRAME_SECONDS, TimerMode::Repeating),
        paused: false,
    })
    .add_systems(Startup, setup)
    .add_systems(FixedUpdate, sim_step.run_if(resource_equals(Mode::Island)))
    .add_systems(Update, (common_keys, animate, auto_screenshot))
    .add_systems(
        Update,
        (camera_keys, tool_keys, mouse_actions, bridge_keys, ghost, title, overlays, sync_units, sync_bridges, sync_structures, sync_platforms)
            .run_if(resource_equals(Mode::Island)),
    )
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

/// A first scene: a sun-themed island with a notch, the altar, trees, a bridge and the priest.
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
    let (w, h) = (16, 10);
    let mut island = IslandMap::rect(Cell::new(0, 0), w, h);
    if std::env::var("ISLEFALL_ISLAND").as_deref() == Ok("cross") {
        // A cross has all four inside corners; used to check the terrain set.
        for c in [(0, 0), (1, 0), (0, 1), (15, 0), (14, 0), (15, 1), (0, 9), (1, 9), (0, 8), (15, 9), (14, 9), (15, 8)] {
            island.remove(Cell::new(c.0, c.1));
        }
    } else {
        for c in [Cell::new(15, 0), Cell::new(14, 0), Cell::new(15, 1)] {
            island.remove(c);
        }
    }
    world::spawn_island(commands, install, lib, palette, images, layouts, &island, Theme::Sun, false);
    sim.world.islands.push(island);
    // A small neutral island to the east with a Storm Geyser on it.
    let geyser_isle = IslandMap::rect(Cell::new(28, 4), 9, 9);
    world::spawn_island(commands, install, lib, palette, images, layouts, &geyser_isle, Theme::Sun, false);
    sim.world.islands.push(geyser_isle);

    // The simulation owns everything else; sprites follow it through the sync systems.
    // The starting layout is free; the reserve is set once it stands.
    sim.world.storm_power = i32::MAX / 2;
    for (stem, cell) in [("dais", Cell::new(9, 7)), ("residence", Cell::new(13, 6)), ("treetwo", Cell::new(2, 3)), ("treetwo", Cell::new(12, 8)), ("geyser", Cell::new(34, 9))] {
        if let Err(e) = sim.world.drop_structure(stem, &data.rules(stem), cell) {
            warn!("{stem} at {cell:?}: {e}");
        }
    }
    // A bridge off the east edge that uses straights, a cross, T pieces, corners and ends.
    for (x, y) in [(16, 4), (17, 4), (18, 4), (19, 4), (20, 4), (20, 3), (20, 2), (18, 3), (18, 5), (18, 6), (21, 3), (19, 2), (17, 5), (19, 3)] {
        sim.world.place_bridge(Cell::new(x, y));
    }
    // A battery dropped in the sky off the bridge end at (21,3) makes its own island.
    if let Err(e) = sim.world.drop_structure("sunbattery", &data.rules("sunbattery"), Cell::new(24, 4)) {
        warn!("sunbattery: {e}");
    }
    // An enemy Disc Thrower guards the geyser island; its range reaches the battery islet.
    if let Err(e) = sim.world.drop_structure_for(1, "sunarcher", &data.rules("sunarcher"), Cell::new(31, 7)) {
        warn!("enemy sunarcher: {e}");
    }
    sim.world.storm_power = START_POWER;
    if let Some(i) = sim.world.spawn_unit("priest", &data.rules("priest"), Cell::new(2, 8)) {
        sim.world.order_move(i, Cell::new(23, 3));
    }
    if let Some(i) = sim.world.spawn_unit(UNIT_TOOL, &data.rules(UNIT_TOOL), Cell::new(5, 8)) {
        sim.world.order_move(i, Cell::new(11, 6));
    }

    let mut centre = (world::cell_to_world(Cell::new(0, 0)) + world::cell_to_world(Cell::new(w - 1, h - 1))) / 2.0;
    if let Ok(spec) = std::env::var("ISLEFALL_CAMERA") {
        if let Some((x, y)) = spec.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<i32>().ok()?, y.trim().parse::<i32>().ok()?))) {
            centre = world::cell_to_world(Cell::new(x, y));
        }
    }
    commands.spawn((Camera2d, zoomed_projection(), Transform::from_translation(centre.extend(0.0))));
    info!("island scene: {} islands, {} structures, {} bridge cells, {} Storm Power", sim.world.islands.len(), sim.world.structures.len(), sim.world.bridges.len(), sim.world.storm_power);
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

/// Keep the window title showing the Storm Power reserve.
fn title(sim: Res<Sim>, mut windows: Query<&mut Window, With<PrimaryWindow>>, mut last: Local<Option<i32>>) {
    if *last == Some(sim.world.storm_power) {
        return;
    }
    *last = Some(sim.world.storm_power);
    if let Ok(mut w) = windows.single_mut() {
        w.title = format!("Islefall - Storm Power {}", sim.world.storm_power);
    }
}

/// Give every simulation unit its sprites, then keep them at the simulated
/// position with the facing animation; frozen when idle.
#[allow(clippy::too_many_arguments)]
fn sync_units(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut layers: Query<(Entity, &mut Transform, &mut Sprite, &mut Anchor, &mut ShapeSprite, &mut UnitLayer)>,
    mut known: Local<usize>,
) {
    // New units since last frame get their layers.
    while *known < sim.world.units.len() {
        let i = *known;
        *known += 1;
        let unit = &sim.world.units[i];
        let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
        let (Some(def), Some(shape)) = (data.install.type_def(&unit.kind), lib.get_or_load(&data.install, palette, &unit.kind, &mut images, &mut layouts)) else {
            warn!("unit {i}: no sprites for {}", unit.kind);
            continue;
        };
        let entities = spawn_animated(&mut commands, shape, def, unit.facing.animation(), unit_to_world(unit), Z_UNIT, false);
        for (k, e) in entities.into_iter().enumerate() {
            commands.entity(e).insert(UnitLayer { unit: i, facing: unit.facing, shadow: k == 1 });
        }
    }
    for (entity, mut tf, mut sprite, mut anchor, mut shape, mut layer) in &mut layers {
        let Some(unit) = sim.world.units.get(layer.unit) else { continue };
        if !unit.alive {
            commands.entity(entity).despawn();
            continue;
        }
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

/// Cell under the mouse cursor, if it is over the window.
fn cursor_cell(windows: &Query<&Window, With<PrimaryWindow>>, cameras: &Query<(&Camera, &GlobalTransform)>) -> Option<Cell> {
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cameras.single()) else { return None };
    let cursor = window.cursor_position()?;
    let world_pos = camera.viewport_to_world_2d(cam_tf, cursor).ok()?;
    Some(world_to_cell(world_pos))
}

fn mouse_actions(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    data: Res<GameData>,
    mut player: ResMut<Player>,
    mut sim: ResMut<Sim>,
) {
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    let Some(cell) = cursor_cell(&windows, &cameras) else { return };
    if left {
        // Click on a unit selects it; on a geyser, the selected unit harvests; otherwise it moves.
        if let Some(i) = sim.world.units.iter().position(|u| u.alive && u.pos.cell() == cell) {
            player.selected = i;
            info!("selected unit {i} ({})", sim.world.units[i].kind);
        } else if let Some(g) = sim.world.structures.iter().position(|s| s.stock > 0 && s.covers(cell)) {
            let sel = player.selected;
            if sim.world.order_harvest(sel, g) {
                info!("unit {sel} harvesting geyser {g}");
            } else {
                info!("unit {sel} cannot harvest geyser {g}: needs a temple and a bridge connection");
            }
        } else if sim.world.command_move(player.selected, cell) {
            info!("unit {} ordered to {cell:?}", player.selected);
        } else {
            info!("{cell:?} is unreachable");
        }
        return;
    }
    match player.tool.clone() {
        Tool::Bridge(slot, piece) => match sim.world.place_piece(&piece.cells_at(cell)) {
            Ok(()) => {
                info!("{} piece placed at {cell:?}", piece.name);
                sim.world.queue.refill(slot);
                // The slot now holds a fresh piece; pick it up so building can continue.
                player.tool = Tool::Bridge(slot, sim.world.queue.slots[slot].clone());
            }
            Err(e) => info!("cannot place {}: {e}", piece.name),
        },
        Tool::Drop(stem) => match sim.world.drop_structure(stem, &data.rules(stem), cell) {
            Ok(_) => info!("{stem} dropped at {cell:?}"),
            Err(e) => info!("cannot drop {stem}: {e}"),
        },
        Tool::Spawn(stem) => match sim.world.spawn_unit(stem, &data.rules(stem), cell) {
            Some(i) => {
                player.selected = i;
                info!("{stem} spawned at {cell:?} as unit {i}");
            }
            None => info!("cannot spawn {stem} at {cell:?}"),
        },
    }
}

fn tool_keys(keys: Res<ButtonInput<KeyCode>>, sim: Res<Sim>, mut player: ResMut<Player>) {
    if keys.just_pressed(KeyCode::KeyR) {
        if let Tool::Bridge(slot, piece) = &player.tool {
            player.tool = Tool::Bridge(*slot, piece.rotated());
        }
        return;
    }
    if let Some(slot) = SLOT_KEYS.iter().position(|k| keys.just_pressed(*k)).filter(|&i| i < sim.world.queue.slots.len()) {
        player.tool = Tool::Bridge(slot, sim.world.queue.slots[slot].clone());
        info!("tool: bridge piece {} from slot {slot}", sim.world.queue.slots[slot].name);
        return;
    }
    if keys.just_pressed(KeyCode::KeyU) {
        player.tool = Tool::Spawn(UNIT_TOOL);
    } else if let Some((_, stem)) = BUILD_TOOLS.iter().find(|(k, _)| keys.just_pressed(*k)) {
        player.tool = Tool::Drop(stem);
    } else {
        return;
    }
    info!("tool: {:?}", player.tool);
}

/// Crack, harden or destroy the bridge cell under the cursor.
fn bridge_keys(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut sim: ResMut<Sim>,
) {
    let (crack, harden, destroy) = (keys.just_pressed(KeyCode::KeyC), keys.just_pressed(KeyCode::KeyH), keys.just_pressed(KeyCode::Delete));
    let salvage = keys.just_pressed(KeyCode::KeyV);
    if !crack && !harden && !destroy && !salvage {
        return;
    }
    let Some(cell) = cursor_cell(&windows, &cameras) else { return };
    if salvage {
        if let Some(i) = sim.world.structures.iter().position(|s| s.owner == 0 && s.covers(cell)) {
            let kind = sim.world.structures[i].kind.clone();
            match sim.world.salvage(i) {
                Some(refund) => info!("salvaged {kind} for {refund} Storm Power"),
                None => info!("cannot salvage {kind}"),
            }
        }
        return;
    }
    if crack {
        info!("crack {cell:?}: {}", sim.world.crack_bridge(cell));
    } else if harden {
        info!("harden {cell:?}: {}", sim.world.harden_bridge(cell));
    } else {
        let before = sim.world.bridges.len();
        if sim.world.destroy_bridge(cell) {
            info!("destroyed {cell:?}; {} bridge cells fell", before - 1 - sim.world.bridges.len());
        }
    }
}

/// Show the current piece or footprint under the cursor, green when it can go there.
#[allow(clippy::too_many_arguments)]
fn ghost(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    data: Res<GameData>,
    player: Res<Player>,
    sim: Res<Sim>,
    existing: Query<Entity, With<Ghost>>,
) {
    for e in &existing {
        commands.entity(e).despawn();
    }
    let Some(cell) = cursor_cell(&windows, &cameras) else { return };
    let (cells, ok) = match &player.tool {
        Tool::Bridge(_, piece) => {
            let cells = piece.cells_at(cell);
            let ok = sim.world.can_place_piece(&cells).is_ok();
            (cells, ok)
        }
        Tool::Drop(stem) => {
            let rules = data.rules(stem);
            let probe = islefall_sim::Structure::new(*stem, cell, rules.foot_x, rules.foot_y, islefall_sim::Walk::Free);
            (probe.cells().collect(), sim.world.can_drop(&rules, cell).is_ok())
        }
        Tool::Spawn(_) => (vec![cell], sim.world.is_walkable(cell)),
    };
    let color = if ok { Color::srgba(0.2, 1.0, 0.2, 0.35) } else { Color::srgba(1.0, 0.2, 0.2, 0.35) };
    for c in cells {
        let (x, y) = c.top_left_px();
        commands.spawn((
            Sprite::from_color(color, Vec2::new(CELL_W as f32, CELL_H as f32)),
            Transform::from_translation(Vec3::new(x as f32 + CELL_W as f32 / 2.0, -(y as f32 + CELL_H as f32 / 2.0), 50.0)),
            Ghost,
        ));
    }
}

/// Marks per-frame overlay sprites: health bars and shot flashes.
#[derive(Component)]
struct Overlay;

/// Draw a health bar over every damaged structure and unit, and a flash on
/// the target of each shot fired this tick.
fn overlays(mut commands: Commands, sim: Res<Sim>, existing: Query<Entity, With<Overlay>>) {
    for e in &existing {
        commands.entity(e).despawn();
    }
    let bar = |commands: &mut Commands, pos: Vec2, width: f32, frac: f32| {
        let back = Color::srgba(0.1, 0.1, 0.1, 0.8);
        let front = if frac > 0.6 { Color::srgb(0.2, 0.9, 0.2) } else if frac > 0.3 { Color::srgb(0.9, 0.9, 0.2) } else { Color::srgb(0.9, 0.2, 0.2) };
        commands.spawn((Sprite::from_color(back, Vec2::new(width, 3.0)), Transform::from_translation(pos.extend(60.0)), Overlay));
        let w = (width - 1.0) * frac.clamp(0.0, 1.0);
        commands.spawn((
            Sprite::from_color(front, Vec2::new(w.max(0.5), 2.0)),
            Transform::from_translation(Vec3::new(pos.x - (width - 1.0 - w) / 2.0, pos.y, 60.1)),
            Overlay,
        ));
    };
    for s in &sim.world.structures {
        if s.max_hp > 0 && s.hp < s.max_hp {
            let (x, y) = s.cell.top_left_px();
            let width = (s.foot_x * CELL_W) as f32;
            let top = Vec2::new(x as f32 + CELL_W as f32 - width / 2.0, -(y as f32 - ((s.foot_y - 1) * CELL_H) as f32) + 4.0);
            bar(&mut commands, top, width, s.hp as f32 / s.max_hp as f32);
        }
    }
    for u in &sim.world.units {
        if u.alive && u.hp < u.max_hp {
            let p = unit_to_world(u);
            bar(&mut commands, Vec2::new(p.x, p.y + 22.0), 12.0, u.hp as f32 / u.max_hp as f32);
        }
    }
    for &(_, target) in &sim.world.last_shots {
        let pos = match target {
            islefall_sim::world::Target::Structure(j) => sim.world.structures.get(j).map(|s| {
                let (x, y) = s.centre().top_left_px();
                Vec2::new(x as f32 + CELL_W as f32 / 2.0, -(y as f32 + CELL_H as f32 / 2.0))
            }),
            islefall_sim::world::Target::Unit(j) => sim.world.units.get(j).map(unit_to_world),
        };
        if let Some(pos) = pos {
            commands.spawn((Sprite::from_color(Color::srgba(1.0, 0.9, 0.3, 0.8), Vec2::new(6.0, 6.0)), Transform::from_translation(pos.extend(61.0)), Overlay));
        }
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

/// Rebuild structure sprites whenever the simulation's structures change.
fn sync_structures(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    existing: Query<Entity, With<StructureSprite>>,
    mut seen: Local<Option<u64>>,
) {
    if *seen == Some(sim.world.structure_version) {
        return;
    }
    *seen = Some(sim.world.structure_version);
    for e in &existing {
        commands.entity(e).despawn();
    }
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    world::spawn_structures(&mut commands, &data.install, &mut lib, palette, &mut images, &mut layouts, &sim.world);
}

/// Rebuild platform terrain whenever buildings create new ground.
fn sync_platforms(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    existing: Query<(Entity, &TerrainTile)>,
    mut seen: Local<Option<u64>>,
) {
    if *seen == Some(sim.world.terrain_version) {
        return;
    }
    *seen = Some(sim.world.terrain_version);
    for (e, t) in &existing {
        if t.platform {
            commands.entity(e).despawn();
        }
    }
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    world::spawn_island(&mut commands, &data.install, &mut lib, palette, &mut images, &mut layouts, &sim.world.platforms, Theme::Sun, true);
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
