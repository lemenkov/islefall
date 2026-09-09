// SPDX-License-Identifier: Apache-2.0
//! Islefall entry point.
//!
//! Every rule, number and key comes from the data directory: `rules.toml`
//! for parameters, `scripts/rules.rhai` for decision hooks, `maps/*.toml`
//! for scenes. Nothing about the game is hardcoded here.
//!
//! Two modes, chosen with `ISLEFALL_MODE`:
//! - `island` (default): the map from `ISLEFALL_MAP` (default `demo`),
//!   driven by the simulation. Left-click selects a unit, sends the selected
//!   unit somewhere, on a Storm Geyser sets it harvesting, on a stunned enemy
//!   priest sends a Transport to capture him, and on your Altar sacrifices a
//!   carried priest; right-click acts with the current tool. Tools: the slot
//!   keys pick a bridge piece (`R` rotates it), number keys drop a building,
//!   `U` spawns a unit. `Delete` destroys, `C` cracks and `H` hardens the
//!   bridge cell under the cursor; `V` salvages your structure under the
//!   cursor. Arrow keys or WASD pan the camera, `-` and `=` zoom.
//! - `viewer`: animates one object type at a time. `[` and `]` step
//!   through types, `,` and `.` through animations.
//!
//! Environment:
//! - `NETSTORM_DIR`: directory containing the game's `d/` and `netstorm.tarc`.
//! - `ISLEFALL_DATA`: the data directory (default `data` next to the
//!   working directory, or the repository's).
//! - `ISLEFALL_MAP`: map name inside `maps/` (default `demo`).
//! - `ISLEFALL_PALETTE`: palette file stem from `d/`, overriding the rules.
//! - `ISLEFALL_SCREENSHOT=file.png`: save a screenshot after start-up,
//!   after `ISLEFALL_SCREENSHOT_AT` seconds (default from the rules).
//! - `ISLEFALL_CAMERA=x,y`: start the camera centred on that cell.
//!
//! Keys in both modes: `Space` pauses, `P` saves a screenshot.

mod sprites;
mod world;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::sprite::Anchor;
use bevy::window::PrimaryWindow;
use islefall_data::isle::Theme;
use islefall_data::shapes::SHAPE_ORDER;
use islefall_data::{Installation, TypeDef};
use bevy::audio::{AudioPlayer, AudioSource, GlobalVolume, PlaybackSettings, Volume};
use islefall_sim::config::Grid;
use islefall_sim::map::{self, MapDef};
use islefall_sim::{Ai, AiMove, Cell, Config, Dir8, IslandMap, Piece, Scripts, TypeRules, Unit, World};
use sprites::FrameInfo;
use world::{BridgeTile, LoadedShape, ShapeLibrary, StructureSprite, TerrainTile, Z_SHADOW, Z_STRUCTURE, Z_UNIT};

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
    Drop(String),
    Spawn(String),
}

/// The user-supplied NetStorm data and the game's own rules, loaded once.
#[derive(Resource)]
struct GameData {
    install: Installation,
    cfg: Config,
    scripts: Scripts,
    palette: String,
    map: MapDef,
}

/// Sound files by lowercase name: the install's sound directory plus
/// overrides from the rules, decoded on first use.
#[derive(Resource, Default)]
struct SoundBank {
    files: HashMap<String, PathBuf>,
    loaded: HashMap<String, Option<Handle<AudioSource>>>,
}

impl SoundBank {
    fn scan(dir: &Path, snd: &islefall_sim::config::Sounds, data_dir: &Path) -> SoundBank {
        let mut files = HashMap::new();
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for e in entries.flatten() {
                    files.insert(e.file_name().to_string_lossy().to_lowercase(), e.path());
                }
            }
            Err(e) => warn!("no sounds: {}: {e}", dir.display()),
        }
        for (name, target) in &snd.aliases {
            match files.get(&target.to_lowercase()).cloned() {
                Some(path) => {
                    files.insert(name.to_lowercase(), path);
                }
                None => warn!("sound alias {name}: {target} is not in {}", dir.display()),
            }
        }
        for (name, path) in &snd.overrides {
            files.insert(name.to_lowercase(), data_dir.join(path));
        }
        info!("{} sound files", files.len());
        SoundBank { files, loaded: HashMap::new() }
    }

    fn get_or_load(&mut self, name: &str, sources: &mut Assets<AudioSource>) -> Option<Handle<AudioSource>> {
        let key = name.to_lowercase();
        if let Some(h) = self.loaded.get(&key) {
            return h.clone();
        }
        let handle = match self.files.get(&key).map(std::fs::read) {
            Some(Ok(bytes)) => Some(sources.add(AudioSource { bytes: bytes.into() })),
            Some(Err(e)) => {
                warn!("sound {name}: {e}");
                None
            }
            None => {
                warn!("sound {name}: no such file");
                None
            }
        };
        self.loaded.insert(key, handle.clone());
        handle
    }
}

impl GameData {
    fn rules(&self, stem: &str) -> TypeRules {
        match self.install.type_def(stem) {
            Some(def) => TypeRules::from_type(def, &self.cfg, &self.scripts).unwrap_or_else(|e| {
                warn!("{stem}: {e}; using plain rules");
                TypeRules::plain()
            }),
            None => TypeRules::plain(),
        }
    }

    fn grid(&self) -> &Grid {
        &self.cfg.grid
    }
}

/// The simulation state; stepped on the fixed schedule only.
#[derive(Resource, Default)]
struct Sim {
    world: Option<World>,
    /// Computer opponents, acting after each tick.
    ais: Vec<Ai>,
}

impl Sim {
    fn world(&self) -> &World {
        self.world.as_ref().expect("world is created at start-up")
    }

    fn world_mut(&mut self) -> &mut World {
        self.world.as_mut().expect("world is created at start-up")
    }
}

/// Player-side interaction state.
#[derive(Resource)]
struct Player {
    tool: Tool,
    selected: usize,
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

/// Marks the placement preview sprites.
#[derive(Component)]
struct Ghost;

/// The ring image used to show Energy circles, built once.
#[derive(Resource)]
struct RingImage(Handle<Image>);

/// Marks Energy circle sprites.
#[derive(Component)]
struct EnergyRing;

/// Marks per-frame overlay sprites: health bars and shot flashes.
#[derive(Component)]
struct Overlay;

/// Where the game's own data lives.
fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("ISLEFALL_DATA") {
        return PathBuf::from(d);
    }
    let local = PathBuf::from("data");
    if local.join("rules.toml").exists() {
        return local;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

fn main() {
    let dir = match std::env::var_os("NETSTORM_DIR") {
        Some(d) => PathBuf::from(d),
        None => fail("NETSTORM_DIR is not set; point it at a NetStorm installation (the directory holding d/ and netstorm.tarc)"),
    };
    let data = data_dir();
    let cfg = Config::load(data.join("rules.toml")).unwrap_or_else(|e| fail(format!("{}: {e}", data.join("rules.toml").display())));
    let scripts = Scripts::load(data.join("scripts/rules.rhai")).unwrap_or_else(|e| fail(format!("{}: {e}", data.join("scripts/rules.rhai").display())));
    let map_name = std::env::var("ISLEFALL_MAP").unwrap_or_else(|_| "demo".into());
    let map_path = data.join("maps").join(format!("{map_name}.toml"));
    let map = MapDef::load(&map_path).unwrap_or_else(|e| fail(format!("{}: {e}", map_path.display())));
    let install = Installation::load(&dir).unwrap_or_else(|e| fail(format!("failed to load NetStorm data from {}: {e}", dir.display())));
    let palette = std::env::var("ISLEFALL_PALETTE").unwrap_or_else(|_| cfg.controls.palette.clone());
    if install.palette(&palette).is_none() {
        fail(format!("palette {palette} not found in {}/d; available: {:?}", dir.display(), install.palettes.keys().collect::<Vec<_>>()));
    }
    let mode = match std::env::var("ISLEFALL_MODE").as_deref() {
        Ok("viewer") => Mode::Viewer,
        _ => Mode::Island,
    };
    let tick_hz = cfg.sim.tick_hz;
    let frame_seconds = 1.0 / cfg.controls.animation_fps.max(0.1);
    let screenshot_at = std::env::var("ISLEFALL_SCREENSHOT_AT").ok().and_then(|s| s.parse().ok()).unwrap_or(cfg.controls.screenshot_seconds);
    let unit_tool = cfg.controls.unit_tools[0].clone();
    let volume = std::env::var("ISLEFALL_VOLUME").ok().and_then(|s| s.parse().ok()).unwrap_or(cfg.sounds.volume);

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window { title: "Islefall".into(), ..default() }),
                ..default()
            }),
    )
    .insert_resource(GameData { install, cfg, scripts, palette, map })
    .insert_resource(GlobalVolume::new(Volume::Linear(volume)))
    .insert_resource(mode)
    .insert_resource(Time::<Fixed>::from_hz(tick_hz as f64))
    .init_resource::<ShapeLibrary>()
    .init_resource::<Sim>()
    .insert_resource(Player { tool: Tool::Spawn(unit_tool), selected: 0 })
    .insert_resource(Viewer {
        type_index: 0,
        animation: 0,
        timer: Timer::from_seconds(frame_seconds, TimerMode::Repeating),
        paused: false,
    })
    .add_systems(Startup, setup)
    .add_systems(Startup, move |mut commands: Commands, game: Res<GameData>| {
        commands.insert_resource(SoundBank::scan(&game.install.root.join(&game.cfg.sounds.dir), &game.cfg.sounds, &data));
    })
    .add_systems(Startup, |mut commands: Commands, data: Res<GameData>, mut images: ResMut<Assets<Image>>| {
        let ring = make_ring(&mut images, data.cfg.energy.range_px);
        commands.insert_resource(RingImage(ring));
    })
    .add_systems(FixedUpdate, sim_step.run_if(resource_equals(Mode::Island)))
    .add_systems(Update, (common_keys, animate, auto_screenshot))
    .add_systems(
        Update,
        (camera_keys, tool_keys, mouse_actions, bridge_keys, ghost, title, grant_knowledge, overlays, sync_units, sync_bridges, sync_structures, sync_platforms, sync_energy_rings)
            .run_if(resource_equals(Mode::Island)),
    )
    .add_systems(Update, play_sounds.after(mouse_actions).after(bridge_keys).run_if(resource_equals(Mode::Island)))
    .add_systems(Update, viewer_keys.run_if(resource_equals(Mode::Viewer)));
    if let Ok(path) = std::env::var("ISLEFALL_SCREENSHOT") {
        app.insert_resource(AutoScreenshot { path, delay: Timer::from_seconds(screenshot_at, TimerMode::Once) });
    }
    app.run();
}

/// A translucent ring of the Energy radius; drawn around every own source.
fn make_ring(images: &mut Assets<Image>, radius: i32) -> Handle<Image> {
    let r = radius as f32;
    let size = (radius * 2 + 2) as u32;
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - size as f32 / 2.0;
            let dy = y as f32 + 0.5 - size as f32 / 2.0;
            let d = (dx * dx + dy * dy).sqrt();
            let alpha = if (d - r).abs() <= 1.0 { 160 } else if d < r { 18 } else { 0 };
            let o = ((y * size + x) * 4) as usize;
            data[o..o + 4].copy_from_slice(&[255, 240, 120, alpha]);
        }
    }
    images.add(Image::new(
        bevy::render::render_resource::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    ))
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
        Mode::Island => setup_map(&mut commands, &data, &mut sim, &mut lib, &mut images, &mut layouts),
        Mode::Viewer => {
            commands.spawn((Camera2d, zoomed_projection(data.cfg.controls.zoom)));
            spawn_viewer_shape(&mut commands, &data, &viewer, &mut lib, &mut images, &mut layouts);
        }
    }
}

fn zoomed_projection(zoom: f32) -> Projection {
    Projection::Orthographic(OrthographicProjection { scale: 1.0 / zoom.max(0.1), ..OrthographicProjection::default_2d() })
}

/// Build the scene from the map file. The layout is free and needs no
/// Energy; costs and Energy apply once it stands.
fn setup_map(
    commands: &mut Commands,
    data: &GameData,
    sim: &mut Sim,
    lib: &mut ShapeLibrary,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let install = &data.install;
    let palette = install.palette(&data.palette).expect("palette checked at start-up");
    let mut w = World::new(data.cfg.clone());
    for (stem, def) in &data.install.types {
        if let Ok(rules) = TypeRules::from_type(def, &data.cfg, &data.scripts) {
            w.register_type(stem, rules);
        }
    }
    w.powers = vec![i32::MAX / 2; data.cfg.sim.max_players];
    w.energy_enforced = false;

    for isl in &data.map.islands {
        let mut island = IslandMap::rect(map::cell(isl.origin), isl.size[0], isl.size[1]);
        for c in &isl.remove {
            island.remove(map::cell(*c));
        }
        let theme = Theme::parse(&isl.theme).unwrap_or(Theme::Sun);
        world::spawn_island(commands, install, lib, palette, images, layouts, &island, theme, false, data.grid());
        w.push_island(island, isl.owner);
    }
    for b in &data.map.bridges {
        for c in &b.cells {
            if !w.place_map_bridge(b.owner, map::cell(*c)) {
                warn!("map bridge cell {c:?} could not be placed");
            }
        }
    }
    for st in &data.map.structures {
        if let Err(e) = w.drop_structure_for(st.owner, &st.kind, &data.rules(&st.kind), map::cell(st.at)) {
            warn!("map: {} at {:?}: {e}", st.kind, st.at);
        }
    }
    w.powers = vec![data.map.start_power; data.cfg.sim.max_players];
    w.energy_enforced = true;
    // The layout was placed, not built: nothing to hear.
    w.take_events();
    for u in &data.map.units {
        match w.spawn_unit_for(u.owner, &u.kind, &data.rules(&u.kind), map::cell(u.at)) {
            Some(i) => {
                if let Some(to) = u.move_to {
                    w.order_move(i, map::cell(to));
                }
            }
            None => warn!("map: unit {} at {:?} could not be placed", u.kind, u.at),
        }
    }
    let altar = w.structures.iter().find(|s| s.is_altar && s.owner == 0).map(|s| s.centre()).unwrap_or(data.map.camera_cell());
    for op in &data.map.opponents {
        let target = op.target.map(map::cell).unwrap_or(altar);
        sim.ais.push(Ai::new(op.owner, target, &data.cfg));
    }

    let mut centre = w.cfg.grid.clone();
    let _ = &mut centre;
    let mut cam = data.map.camera_cell();
    if let Ok(spec) = std::env::var("ISLEFALL_CAMERA") {
        if let Some((x, y)) = spec.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<i32>().ok()?, y.trim().parse::<i32>().ok()?))) {
            cam = Cell::new(x, y);
        }
    }
    let centre = world::cell_to_world(cam, data.grid());
    commands.spawn((Camera2d, zoomed_projection(data.cfg.controls.zoom), Transform::from_translation(centre.extend(0.0))));
    info!(
        "map {}: {} islands, {} structures, {} bridge cells, {} Storm Power, {} opponents",
        data.map.name,
        w.islands.len(),
        w.structures.len(),
        w.bridges.len(),
        w.storm_power(),
        sim.ais.len()
    );
    sim.world = Some(w);
}

/// World position of a unit's feet: cells to source pixels, y flipped.
fn unit_to_world(unit: &Unit, g: &Grid) -> Vec2 {
    let (x, y) = unit.pos_f32();
    Vec2::new(x * g.cell_w as f32, -(y * g.cell_h as f32))
}

/// Cell under a world position.
fn world_to_cell(p: Vec2, g: &Grid) -> Cell {
    Cell::new((p.x / g.cell_w as f32).floor() as i32, (-p.y / g.cell_h as f32).floor() as i32)
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
    let seq: Vec<usize> = def.frames.iter().enumerate().filter(|(_, f)| f.animation.eq_ignore_ascii_case(animation)).map(|(i, _)| i).collect();
    if !seq.is_empty() || animation.eq_ignore_ascii_case("A") {
        return seq;
    }
    // Types without per-facing animations (flyers spin, balloons drift) use their first one.
    def.frames.iter().enumerate().filter(|(_, f)| f.animation.eq_ignore_ascii_case("A")).map(|(i, _)| i).collect()
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

fn sim_step(mut sim: ResMut<Sim>, viewer: Res<Viewer>, data: Res<GameData>) {
    if viewer.paused {
        return;
    }
    let Sim { world: Some(world), ais } = &mut *sim else { return };
    world.step(&data.scripts);
    let shooter = data.cfg.ai.shooter.clone();
    let rules = data.rules(&shooter);
    for ai in ais.iter_mut() {
        match ai.tick(world, (&shooter, &rules)) {
            Some(AiMove::Piece { name, at }) => info!("opponent {} lays a {name} piece at {at:?}", ai.owner),
            Some(AiMove::Shooter { kind, at }) => info!("opponent {} drops a {kind} at {at:?}", ai.owner),
            _ => {}
        }
    }
}

/// Play the sound each event names: the type's property first, then the
/// rules' fallback file. The world's events are drained here.
fn play_sounds(
    mut commands: Commands,
    mut sim: ResMut<Sim>,
    data: Res<GameData>,
    mut bank: ResMut<SoundBank>,
    mut sources: ResMut<Assets<AudioSource>>,
    volume: Res<GlobalVolume>,
) {
    let Some(world) = sim.world.as_mut() else { return };
    let events = world.take_events();
    if volume.volume == Volume::Linear(0.0) {
        return;
    }
    let snd = &data.cfg.sounds;
    let mut started = 0;
    for e in events {
        if started >= snd.max_per_frame {
            break;
        }
        let cue = snd.events.cue(e.what);
        let named = |stem: &str| cue.property.as_deref().and_then(|p| data.install.type_def(stem).and_then(|d| d.get_str(p))).map(str::to_string);
        let from_type = named(&e.kind).or_else(|| snd.projectiles.get(&e.kind).and_then(|p| named(p)));
        let Some(name) = from_type.or_else(|| cue.file.clone()).filter(|n| !n.is_empty()) else { continue };
        let Some(handle) = bank.get_or_load(&name, &mut sources) else { continue };
        commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
        started += 1;
    }
}

/// Keep the window title showing the Storm Power reserve and Knowledge.
fn title(sim: Res<Sim>, mut windows: Query<&mut Window, With<PrimaryWindow>>, mut last: Local<Option<(i32, u32, usize)>>) {
    let w = sim.world();
    let now = (w.storm_power(), w.knowledge, w.known_tech[0].len());
    if *last == Some(now) {
        return;
    }
    *last = Some(now);
    if let Ok(mut win) = windows.single_mut() {
        win.title = format!("Islefall - Storm Power {} - Knowledge {} ({} techs)", now.0, now.1, now.2);
    }
}

/// Turn each sacrifice into the Knowledge bit the `knowledge_grant` hook picks.
fn grant_knowledge(mut sim: ResMut<Sim>, data: Res<GameData>) {
    let w = sim.world_mut();
    if w.pending_knowledge.is_empty() {
        return;
    }
    let mut bits: Vec<u8> = data.install.types.values().filter_map(|t| TypeRules::from_type(t, &data.cfg, &data.scripts).ok().and_then(|r| r.tech_bit)).collect();
    bits.sort_unstable();
    bits.dedup();
    let pending = std::mem::take(&mut w.pending_knowledge);
    for owner in pending {
        let known: Vec<u8> = w.known_tech[owner as usize % data.cfg.sim.max_players].iter().copied().collect();
        match data.scripts.knowledge_grant(&known, &bits) {
            Ok(Some(bit)) => {
                w.grant_tech(owner, bit);
                let unlocked: Vec<&str> = data
                    .install
                    .types
                    .iter()
                    .filter(|(_, t)| t.get_i64("techBit") == Some(bit as i64))
                    .map(|(k, _)| k.as_str())
                    .collect();
                info!("owner {owner} gains Knowledge {bit}: {unlocked:?}");
            }
            Ok(None) => info!("owner {owner} already knows everything"),
            Err(e) => warn!("{e}"),
        }
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
    let w = sim.world();
    let g = data.grid();
    while *known < w.units.len() {
        let i = *known;
        *known += 1;
        let unit = &w.units[i];
        let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
        let (Some(def), Some(shape)) = (data.install.type_def(&unit.kind), lib.get_or_load(&data.install, palette, &unit.kind, &mut images, &mut layouts)) else {
            warn!("unit {i}: no sprites for {}", unit.kind);
            continue;
        };
        let entities = spawn_animated(&mut commands, shape, def, unit.facing.animation(), unit_to_world(unit, g), Z_UNIT, false);
        for (k, e) in entities.into_iter().enumerate() {
            commands.entity(e).insert(UnitLayer { unit: i, facing: unit.facing, shadow: k == 1 });
        }
    }
    for (entity, mut tf, mut sprite, mut anchor, mut shape, mut layer) in &mut layers {
        let Some(unit) = w.units.get(layer.unit) else { continue };
        if !unit.alive {
            commands.entity(entity).despawn();
            continue;
        }
        let pos = unit_to_world(unit, g);
        tf.translation.x = pos.x;
        tf.translation.y = pos.y;
        let (_, row) = unit.pos_f32();
        tf.translation.z = if layer.shadow {
            Z_SHADOW
        } else if unit.is_air {
            // Flyers pass over everything on the ground.
            Z_UNIT
        } else {
            depth_z(Z_STRUCTURE, row) + 0.005
        };
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
fn cursor_cell(windows: &Query<&Window, With<PrimaryWindow>>, cameras: &Query<(&Camera, &GlobalTransform)>, g: &Grid) -> Option<Cell> {
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cameras.single()) else { return None };
    let cursor = window.cursor_position()?;
    let world_pos = camera.viewport_to_world_2d(cam_tf, cursor).ok()?;
    Some(world_to_cell(world_pos, g))
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
    let Some(cell) = cursor_cell(&windows, &cameras, data.grid()) else { return };
    let w = sim.world_mut();
    if left {
        let sel = player.selected;
        if let Some(i) = w.units.iter().position(|u| u.alive && u.owner == 0 && u.carried_by.is_none() && u.cell() == cell) {
            player.selected = i;
            info!("selected unit {i} ({})", w.units[i].kind);
        } else if let Some(p) = w.units.iter().position(|u| u.alive && u.owner != 0 && u.is_priest && u.cell() == cell) {
            if w.order_capture(sel, p) {
                info!("unit {sel} sent to capture the priest");
            } else {
                info!("unit {sel} cannot capture: needs a Transport and a stunned priest");
            }
        } else if let Some(a) = w.structures.iter().position(|s| s.is_altar && s.owner == 0 && s.covers(cell)) {
            if w.order_sacrifice(sel, a) {
                info!("unit {sel} carries the priest to the altar");
            } else if w.command_move(sel, cell) {
                info!("unit {sel} ordered to {cell:?}");
            }
        } else if let Some(g) = w.structures.iter().position(|s| s.stock > 0 && s.covers(cell)) {
            if w.order_harvest(sel, g) {
                info!("unit {sel} harvesting geyser {g}");
            } else {
                info!("unit {sel} cannot harvest geyser {g}: needs a temple and a bridge connection");
            }
        } else if w.command_move(sel, cell) {
            info!("unit {sel} ordered to {cell:?}");
        } else {
            info!("{cell:?} is unreachable");
        }
        return;
    }
    match player.tool.clone() {
        Tool::Bridge(slot, piece) => match w.place_piece(&piece.cells_at(cell)) {
            Ok(()) => {
                info!("{} piece placed at {cell:?}", piece.name);
                w.queue.refill(slot);
                player.tool = Tool::Bridge(slot, w.queue.slots[slot].clone());
            }
            Err(e) => info!("cannot place {}: {e}", piece.name),
        },
        Tool::Drop(stem) => match w.drop_structure(&stem, &data.rules(&stem), cell) {
            Ok(_) => info!("{stem} dropped at {cell:?}"),
            Err(e) => info!("cannot drop {stem}: {e}"),
        },
        Tool::Spawn(stem) => match w.place_unit_for(0, &stem, &data.rules(&stem), cell) {
            Ok(i) => {
                player.selected = i;
                info!("{stem} placed at {cell:?} as unit {i}");
            }
            Err(e) => info!("cannot place {stem}: {e}"),
        },
    }
}

/// Key name from the rules file to a key code: single letters and digits.
fn key_code(name: &str) -> Option<KeyCode> {
    let c = name.chars().next()?.to_ascii_uppercase();
    Some(match c {
        'A' => KeyCode::KeyA, 'B' => KeyCode::KeyB, 'C' => KeyCode::KeyC, 'D' => KeyCode::KeyD, 'E' => KeyCode::KeyE,
        'F' => KeyCode::KeyF, 'G' => KeyCode::KeyG, 'H' => KeyCode::KeyH, 'I' => KeyCode::KeyI, 'J' => KeyCode::KeyJ,
        'K' => KeyCode::KeyK, 'L' => KeyCode::KeyL, 'M' => KeyCode::KeyM, 'N' => KeyCode::KeyN, 'O' => KeyCode::KeyO,
        'P' => KeyCode::KeyP, 'Q' => KeyCode::KeyQ, 'R' => KeyCode::KeyR, 'S' => KeyCode::KeyS, 'T' => KeyCode::KeyT,
        'U' => KeyCode::KeyU, 'V' => KeyCode::KeyV, 'W' => KeyCode::KeyW, 'X' => KeyCode::KeyX, 'Y' => KeyCode::KeyY,
        'Z' => KeyCode::KeyZ,
        _ => return None,
    })
}

const DIGIT_KEYS: [KeyCode; 9] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9];

fn tool_keys(keys: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>, data: Res<GameData>, mut player: ResMut<Player>) {
    let w = sim.world();
    if keys.just_pressed(KeyCode::KeyR) {
        if let Tool::Bridge(slot, piece) = &player.tool {
            player.tool = Tool::Bridge(*slot, piece.rotated());
        }
        return;
    }
    let slot_keys: Vec<Option<KeyCode>> = data.cfg.controls.slot_keys.iter().map(|k| key_code(k)).collect();
    if let Some(slot) = slot_keys.iter().position(|k| k.is_some_and(|k| keys.just_pressed(k))).filter(|&i| i < w.queue.slots.len()) {
        player.tool = Tool::Bridge(slot, w.queue.slots[slot].clone());
        info!("tool: bridge piece {} from slot {slot}", w.queue.slots[slot].name);
        return;
    }
    let unit_key = data.cfg.controls.unit_keys.iter().position(|k| key_code(k).is_some_and(|k| keys.just_pressed(k)));
    if let Some(i) = unit_key {
        let stem = data.cfg.controls.unit_tools[i].clone();
        let rules = data.rules(&stem);
        let w = sim.world_mut();
        match w.put_into_production(0, &stem, &rules, &data.scripts) {
            Ok(ws) => info!("{stem} in production at {} ({} of {} slots used)", w.structures[ws].kind, w.structures[ws].production.len(), w.structures[ws].slots),
            Err(islefall_sim::ProductionError::NotProducible) => {}
            Err(e) => info!("{stem}: {e}"),
        }
        player.tool = Tool::Spawn(stem);
    } else if let Some(i) = DIGIT_KEYS.iter().position(|k| keys.just_pressed(*k)).filter(|&i| i < data.cfg.controls.build_tools.len()) {
        let stem = data.cfg.controls.build_tools[i].clone();
        // Picking a Battle unit puts it into production at a Workshop, as the manual's menu would.
        let rules = data.rules(&stem);
        let w = sim.world_mut();
        match w.put_into_production(0, &stem, &rules, &data.scripts) {
            Ok(ws) => info!("{stem} in production at {} ({} of {} slots used)", w.structures[ws].kind, w.structures[ws].production.len(), w.structures[ws].slots),
            Err(islefall_sim::ProductionError::NotProducible) => {}
            Err(e) => info!("{stem}: {e}"),
        }
        player.tool = Tool::Drop(stem);
    } else {
        return;
    }
    info!("tool: {:?}", player.tool);
}

/// Crack, harden, destroy or salvage under the cursor.
fn bridge_keys(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    data: Res<GameData>,
    mut sim: ResMut<Sim>,
) {
    let (crack, harden, destroy) = (keys.just_pressed(KeyCode::KeyC), keys.just_pressed(KeyCode::KeyH), keys.just_pressed(KeyCode::Delete));
    let salvage = keys.just_pressed(KeyCode::KeyV);
    if !crack && !harden && !destroy && !salvage {
        return;
    }
    let Some(cell) = cursor_cell(&windows, &cameras, data.grid()) else { return };
    let w = sim.world_mut();
    if salvage {
        if let Some(i) = w.structures.iter().position(|s| s.owner == 0 && s.covers(cell)) {
            let kind = w.structures[i].kind.clone();
            match w.salvage(i, &data.scripts) {
                Some(refund) => info!("salvaged {kind} for {refund} Storm Power"),
                None => info!("cannot salvage {kind}"),
            }
        }
        return;
    }
    if crack {
        info!("crack {cell:?}: {}", w.crack_bridge(cell));
    } else if harden {
        info!("harden {cell:?}: {}", w.harden_bridge(cell));
    } else {
        let before = w.bridges.len();
        if w.destroy_bridge(cell) {
            info!("destroyed {cell:?}; {} bridge cells fell", before - 1 - w.bridges.len());
        }
    }
}

/// Show the current piece or footprint under the cursor, green when it can go there.
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
    let g = data.grid();
    let Some(cell) = cursor_cell(&windows, &cameras, g) else { return };
    let w = sim.world();
    let (cells, ok) = match &player.tool {
        Tool::Bridge(_, piece) => {
            let cells = piece.cells_at(cell);
            let ok = w.can_place_piece_for(0, &cells).is_ok();
            (cells, ok)
        }
        Tool::Drop(stem) => {
            let rules = data.rules(stem);
            let probe = islefall_sim::Structure::new(stem.as_str(), cell, rules.foot_x, rules.foot_y, islefall_sim::Walk::Free);
            let ok = w.can_drop(&rules, cell).is_ok()
                && w.check_ownership(0, &rules, cell).is_ok()
                && rules.tech_bit.is_none_or(|b| w.knows_tech(0, b))
                && w.check_energy(0, &rules, cell).is_ok()
                && w.check_production(0, stem, &rules).is_ok();
            (probe.cells().collect(), ok)
        }
        Tool::Spawn(stem) => {
            let rules = data.rules(stem);
            let ok = (rules.is_air || w.is_walkable(cell))
                && rules.tech_bit.is_none_or(|b| w.knows_tech(0, b))
                && w.check_energy(0, &rules, cell).is_ok()
                && w.check_production(0, stem, &rules).is_ok();
            (vec![cell], ok)
        }
    };
    let color = if ok { Color::srgba(0.2, 1.0, 0.2, 0.35) } else { Color::srgba(1.0, 0.2, 0.2, 0.35) };
    for c in cells {
        let (x, y) = c.centre_px(g);
        commands.spawn((
            Sprite::from_color(color, Vec2::new(g.cell_w as f32, g.cell_h as f32)),
            Transform::from_translation(Vec3::new(x as f32, -(y as f32), 50.0)),
            Ghost,
        ));
    }
}

/// Draw a health bar over every damaged structure and unit, a ring around
/// stunned priests, and a flash on the target of each shot fired this tick.
fn overlays(mut commands: Commands, sim: Res<Sim>, data: Res<GameData>, existing: Query<Entity, With<Overlay>>) {
    for e in &existing {
        commands.entity(e).despawn();
    }
    let w = sim.world();
    let g = data.grid();
    let bar = |commands: &mut Commands, pos: Vec2, width: f32, frac: f32| {
        let back = Color::srgba(0.1, 0.1, 0.1, 0.8);
        let front = if frac > 0.6 { Color::srgb(0.2, 0.9, 0.2) } else if frac > 0.3 { Color::srgb(0.9, 0.9, 0.2) } else { Color::srgb(0.9, 0.2, 0.2) };
        commands.spawn((Sprite::from_color(back, Vec2::new(width, 3.0)), Transform::from_translation(pos.extend(60.0)), Overlay));
        let wdt = (width - 1.0) * frac.clamp(0.0, 1.0);
        commands.spawn((
            Sprite::from_color(front, Vec2::new(wdt.max(0.5), 2.0)),
            Transform::from_translation(Vec3::new(pos.x - (width - 1.0 - wdt) / 2.0, pos.y, 60.1)),
            Overlay,
        ));
    };
    for s in &w.structures {
        if s.max_hp > 0 && s.hp < s.max_hp {
            let (x, y) = s.cell.top_left_px(g);
            let width = (s.foot_x * g.cell_w) as f32;
            let top = Vec2::new(x as f32 + g.cell_w as f32 - width / 2.0, -(y as f32 - ((s.foot_y - 1) * g.cell_h) as f32) + 4.0);
            bar(&mut commands, top, width, s.hp as f32 / s.max_hp as f32);
        }
    }
    for u in &w.units {
        if !u.alive {
            continue;
        }
        let p = unit_to_world(u, g);
        if u.hp < u.max_hp {
            bar(&mut commands, Vec2::new(p.x, p.y + 22.0), 12.0, u.hp as f32 / u.max_hp as f32);
        }
        if u.stunned {
            commands.spawn((
                Sprite::from_color(Color::srgba(1.0, 0.95, 0.3, 0.45), Vec2::new(18.0, 12.0)),
                Transform::from_translation(Vec3::new(p.x, p.y - 2.0, 59.0)),
                Overlay,
            ));
        }
    }
    for &(_, target) in &w.last_shots {
        let pos = match target {
            islefall_sim::world::Target::Structure(j) => w.structures.get(j).map(|s| {
                let (x, y) = s.centre().centre_px(g);
                Vec2::new(x as f32, -(y as f32))
            }),
            islefall_sim::world::Target::Unit(j) => w.units.get(j).map(|u| unit_to_world(u, g)),
        };
        if let Some(pos) = pos {
            commands.spawn((Sprite::from_color(Color::srgba(1.0, 0.9, 0.3, 0.8), Vec2::new(6.0, 6.0)), Transform::from_translation(pos.extend(61.0)), Overlay));
        }
    }
}

/// Draw the Energy circle of every source the player owns whenever structures change.
fn sync_energy_rings(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    ring: Res<RingImage>,
    existing: Query<Entity, With<EnergyRing>>,
    mut seen: Local<Option<u64>>,
) {
    let w = sim.world();
    if *seen == Some(w.structure_version) {
        return;
    }
    *seen = Some(w.structure_version);
    for e in &existing {
        commands.entity(e).despawn();
    }
    for s in w.structures.iter().filter(|s| s.owner == 0 && s.produces.is_some()) {
        let (x, y) = s.centre().centre_px(data.grid());
        commands.spawn((Sprite::from_image(ring.0.clone()), Transform::from_translation(Vec3::new(x as f32, -(y as f32), 0.9)), EnergyRing));
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
    let w = sim.world();
    if *seen == Some(w.bridge_version) {
        return;
    }
    *seen = Some(w.bridge_version);
    for e in &existing {
        commands.entity(e).despawn();
    }
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    world::spawn_bridges(&mut commands, &data.install, &mut lib, palette, &mut images, &mut layouts, w);
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
    let w = sim.world();
    if *seen == Some(w.structure_version) {
        return;
    }
    *seen = Some(w.structure_version);
    for e in &existing {
        commands.entity(e).despawn();
    }
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    world::spawn_structures(&mut commands, &data.install, &mut lib, palette, &mut images, &mut layouts, w);
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
    let w = sim.world();
    if *seen == Some(w.terrain_version) {
        return;
    }
    *seen = Some(w.terrain_version);
    for (e, t) in &existing {
        if t.platform {
            commands.entity(e).despawn();
        }
    }
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    world::spawn_island(&mut commands, &data.install, &mut lib, palette, &mut images, &mut layouts, &w.platforms, Theme::Sun, true, data.grid());
}

fn camera_keys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    data: Res<GameData>,
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
        let step = d.normalize() * data.cfg.controls.pan_speed * time.delta_secs();
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
