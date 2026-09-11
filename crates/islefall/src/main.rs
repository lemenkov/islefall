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
//! - `ISLEFALL_MAP`: map name inside `maps/` (default `demo`); else a
//!   campaign scenario of that name from `netstorm.tarc` (`bridgethegap`,
//!   `capturethepriest`, ...); `random` or `random:<seed>` makes one up
//!   for `ISLEFALL_PLAYERS` players. `ISLEFALL_MAP_EXPORT=file.toml`
//!   writes whatever was loaded as a map file of our own.
//! - `ISLEFALL_PALETTE`: palette file stem from `d/`, overriding the rules.
//! - `ISLEFALL_SCREENSHOT=file.png`: save a screenshot after start-up,
//!   after `ISLEFALL_SCREENSHOT_AT` seconds (default from the rules).
//! - `ISLEFALL_CAMERA=x,y`: start the camera centred on that cell.
//! - `ISLEFALL_ZOOM=z`: start at that zoom instead of the rules' (1 is
//!   source pixels, smaller is further away).
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
use islefall_data::Installation;
use rand::{RngExt as _, SeedableRng};
use rand_pcg::Pcg32;
use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, GlobalVolume, PlaybackSettings, Volume};
use islefall_sim::config::Grid;
use islefall_sim::map::{self, MapDef};
use islefall_sim::{Ai, AiMove, Applied, Cell, Command, Config, Dir8, IslandMap, Replay, Scripts, TypeRules, Unit, World};
use sprites::FrameInfo;
use world::{BridgeTile, LoadedShape, ShapeLibrary, StructureBase, StructureFrames, StructureSprite, TerrainTile, Z_SHADOW, Z_SKY, Z_STRUCTURE, Z_UNIT};

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
    /// Nothing in hand: clicks select and order units.
    Empty,
    /// A bridge piece taken from queue slot `.0`, turned `.1` quarter turns.
    Bridge(usize, u8),
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
    /// Where rules, scripts, maps and a mod's own files live.
    data_dir: PathBuf,
}

impl GameData {
    /// A picture or sound by name: the data directory first, so a mod can
    /// bring its own, then the installation's `d/` directory.
    #[allow(dead_code)]
    fn asset_path(&self, name: &str) -> Option<PathBuf> {
        let own = self.data_dir.join(name);
        if own.is_file() {
            return Some(own);
        }
        find_file(&self.install.root.join("d"), name)
    }
}

/// Sound files by lowercase name: the install's sound directory plus
/// overrides from the rules, decoded on first use.
#[derive(Resource, Default)]
struct SoundBank {
    files: HashMap<String, PathBuf>,
    /// Decoded sources with the base gain in decibels their file name carries.
    loaded: HashMap<String, Option<(Handle<AudioSource>, f32)>>,
    /// Numbered siblings of a name (`golemMove1` to `golemMove5`), by lowercase name.
    variants: HashMap<String, Vec<String>>,
}

/// A name without its extension and trailing digits: `golemmove1.wav` -> `golemmove`.
fn sound_stem(name: &str) -> String {
    let lower = name.to_lowercase();
    let base = lower.rsplit_once('.').map(|(b, _)| b).unwrap_or(&lower);
    base.trim_end_matches(|c: char| c.is_ascii_digit()).to_string()
}

/// The gain a NetStorm sound file name carries: `name-500.wav` is -5 dB,
/// DirectSound's hundredths of a decibel.
fn file_gain_db(path: &Path) -> f32 {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    match stem.rsplit_once('-') {
        Some((_, digits)) if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) => -(digits.parse::<f32>().unwrap_or(0.0)) / 100.0,
        _ => 0.0,
    }
}

/// One drifting cloud layer under the map.
#[derive(Component)]
struct SkyLayer {
    speed: Vec2,
    tile: Vec2,
}

/// A file in a directory by case-insensitive name.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

/// One cloud tile from fractal noise on a torus: the four-dimensional
/// noise is sampled around two circles, so the tile wraps in both
/// directions without a seam. Noise above `cover` is cloud, fading in
/// over `softness`.
fn cloud_tile(layer: &islefall_sim::config::SkyLayer) -> Vec<u8> {
    use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
    let fbm = Fbm::<Perlin>::new(layer.seed)
        .set_octaves(layer.octaves.max(1) as usize)
        .set_frequency(1.0)
        .set_lacunarity(layer.lacunarity.max(1.0))
        .set_persistence(layer.gain.clamp(0.05, 1.0));
    let n = layer.tile as usize;
    let radius = layer.scale / std::f64::consts::TAU;
    let [r, g, b] = layer.tint;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        let v = y as f64 / n as f64 * std::f64::consts::TAU;
        for x in 0..n {
            let u = x as f64 / n as f64 * std::f64::consts::TAU;
            let p = [radius * u.cos(), radius * u.sin(), radius * v.cos(), radius * v.sin()];
            // Fractal noise lands mostly within -1..1; map to 0..1.
            let value = (fbm.get(p) as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
            let threshold = 1.0 - layer.cover.clamp(0.0, 1.0);
            let t = ((value - threshold) / layer.softness.max(0.01)).clamp(0.0, 1.0);
            let alpha = t * t * (3.0 - 2.0 * t);
            rgba.extend_from_slice(&[(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (alpha * 255.0) as u8]);
        }
    }
    rgba
}

/// Make the sky's cloud layers and lay them out around the origin;
/// `drift_sky` keeps them under the camera and moving.
fn spawn_sky(commands: &mut Commands, data: &GameData, images: &mut Assets<Image>) {
    for (i, layer) in data.cfg.sky.layers.iter().enumerate() {
        let started = std::time::Instant::now();
        let rgba = cloud_tile(layer);
        info!("sky layer {i}: {}x{} tile from seed {} in {:?}", layer.tile, layer.tile, layer.seed, started.elapsed());
        let mut image = Image::new(
            bevy::render::render_resource::Extent3d { width: layer.tile, height: layer.tile, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            rgba,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = bevy::image::ImageSampler::linear();
        let handle = images.add(image);
        let tile = Vec2::splat(layer.tile as f32);
        let extent = tile * data.cfg.sky.extent_tiles as f32;
        let mut sprite = Sprite::from_image(handle);
        sprite.custom_size = Some(extent);
        sprite.image_mode = SpriteImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 1.0 };
        sprite.color = Color::srgba(1.0, 1.0, 1.0, layer.opacity);
        commands.spawn((
            sprite,
            Transform::from_translation(Vec3::new(0.0, 0.0, Z_SKY + i as f32 * 0.1)),
            SkyLayer { speed: Vec2::new(layer.speed[0], -layer.speed[1]), tile },
        ));
    }
}

/// Drift each cloud layer with time and keep it centred on the camera, in
/// whole tiles so the texture never jumps.
fn drift_sky(time: Res<Time>, cameras: Query<&Transform, (With<WorldCamera>, Without<SkyLayer>)>, mut layers: Query<(&SkyLayer, &mut Transform)>) {
    let Ok(cam) = cameras.single() else { return };
    let t = time.elapsed_secs();
    for (layer, mut tf) in layers.iter_mut() {
        let offset = (layer.speed * t).rem_euclid(layer.tile);
        let cam2 = cam.translation.truncate();
        let centre = offset + ((cam2 - offset) / layer.tile).round() * layer.tile;
        tf.translation.x = centre.x;
        tf.translation.y = centre.y;
    }
}

/// A looping object sound tied to a structure or unit.
#[derive(Component)]
struct ObjectLoop(LoopKey, String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum LoopKey {
    Structure(String, Cell),
    Unit(usize),
}

/// The camera as a listener: world centre, half extent of the view and zoom.
struct View {
    centre: Vec2,
    half: Vec2,
    zoom: f32,
}

impl View {
    fn of(cameras: &Query<(&Camera, &GlobalTransform, &Projection), With<WorldCamera>>) -> Option<View> {
        let (camera, tf, proj) = cameras.single().ok()?;
        let size = camera.logical_viewport_size()?;
        let scale = match proj {
            Projection::Orthographic(o) => o.scale,
            _ => 1.0,
        };
        Some(View { centre: tf.translation().truncate(), half: size * 0.5 * scale, zoom: 1.0 / scale.max(1e-6) })
    }

    /// How far out `pos` lies: 0 at the centre, 1 at the view's edge, more beyond.
    fn edge(&self, pos: Vec2) -> f32 {
        let d = (pos - self.centre).abs();
        (d.x / self.half.x.max(1.0)).max(d.y / self.half.y.max(1.0))
    }
}

/// Gain of a sound at `pos`, or `None` when it lies outside the view.
fn gain_at(view: &View, pos: Vec2, base_db: f32, data: &GameData) -> Option<Volume> {
    let edge = view.edge(pos);
    if edge > 1.0 {
        return None;
    }
    let a = &data.cfg.sounds.attenuation;
    let db = data
        .scripts
        .sound_gain(edge as f64, view.zoom as f64, base_db as f64, a.edge_db as f64, a.reference_zoom as f64, a.db_per_halving as f64)
        .unwrap_or(base_db as f64);
    Some(Volume::Decibels(db as f32))
}

fn cell_to_world(cell: Cell, g: &Grid) -> Vec2 {
    let (x, y) = cell.centre_px(g);
    Vec2::new(x as f32, -(y as f32))
}

/// Sky noises on a random timer.
#[derive(Resource)]
struct Sky {
    timer: Timer,
    rng: Pcg32,
}

impl Sky {
    fn wait(&mut self, range: [f32; 2]) -> f32 {
        let t = self.rng.random::<f32>();
        range[0] + (range[1] - range[0]) * t
    }
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
        SoundBank { files, loaded: HashMap::new(), variants: HashMap::new() }
    }

    /// The name and every file that differs from it only by a number, sorted.
    fn variants(&mut self, name: &str) -> Vec<String> {
        let key = name.to_lowercase();
        if let Some(v) = self.variants.get(&key) {
            return v.clone();
        }
        let stem = sound_stem(name);
        let ext = key.rsplit_once('.').map(|(_, e)| e.to_string()).unwrap_or_default();
        let mut out: Vec<String> = self
            .files
            .keys()
            .filter(|k| k.rsplit_once('.').is_some_and(|(_, e)| e == ext) && sound_stem(k) == stem)
            .filter(|k| {
                let base = k.rsplit_once('.').map(|(b, _)| b).unwrap_or(k);
                base.len() > stem.len() || *k == &key
            })
            .cloned()
            .collect();
        out.sort();
        if out.is_empty() {
            out.push(key.clone());
        }
        self.variants.insert(key, out.clone());
        out
    }

    fn get_or_load(&mut self, name: &str, sources: &mut Assets<AudioSource>) -> Option<(Handle<AudioSource>, f32)> {
        let key = name.to_lowercase();
        if let Some(h) = self.loaded.get(&key) {
            return h.clone();
        }
        let handle = match self.files.get(&key).map(|p| (std::fs::read(p), file_gain_db(p))) {
            Some((Ok(bytes), gain)) => Some((sources.add(AudioSource { bytes: bytes.into() }), gain)),
            Some((Err(e), _)) => {
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

/// Commands recorded to a file as they are given, and a file being played back.
#[derive(Resource, Default)]
struct Recording {
    record: Option<(PathBuf, Replay)>,
    replay: Option<Replay>,
    /// The server connection when playing online.
    net: Option<Net>,
    /// The local player, for recording and applying.
    owner: u8,
}

impl Recording {
    /// Give a command for the local player: apply it, tell the status line,
    /// and record it if recording.
    fn issue(&mut self, w: &mut World, scripts: &Scripts, status: &mut Status, cmd: Command) -> Option<Applied> {
        if let Some(net) = &self.net {
            // Online, the server orders every command; it comes back in a turn.
            net.send(islefall_net::ClientMsg::Command(cmd));
            return None;
        }
        let tick = w.tick;
        let result = w.apply(self.owner, &cmd, scripts);
        if let Some((path, replay)) = self.record.as_mut() {
            replay.record(tick, self.owner, cmd, result.is_ok());
            if let Err(e) = replay.save(&*path) {
                warn!("cannot write {}: {e}", path.display());
            }
        }
        match result {
            Ok(a) => {
                status.say(a.message.clone());
                Some(a)
            }
            Err(e) => {
                status.say(e);
                None
            }
        }
    }
}

/// The connection to a server: commands go out, the server's messages come in.
#[derive(Resource)]
struct Net {
    tx: std::sync::mpsc::Sender<islefall_net::ClientMsg>,
    rx: std::sync::Mutex<std::sync::mpsc::Receiver<islefall_net::ServerMsg>>,
    turn_ticks: u32,
    hash_every_turns: u64,
    started: bool,
    turns_done: u64,
}

impl Net {
    /// Connect in the background: one thread writes what we send, another
    /// reads what the server says; both hand over through channels.
    fn connect(addr: &str, hello: islefall_net::ClientMsg) -> Result<Net, String> {
        let stream = std::net::TcpStream::connect(addr).map_err(|e| format!("{addr}: {e}"))?;
        stream.set_nodelay(true).ok();
        let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
        let mut reader = stream;
        let (tx, out_rx) = std::sync::mpsc::channel::<islefall_net::ClientMsg>();
        let (in_tx, rx) = std::sync::mpsc::channel::<islefall_net::ServerMsg>();
        islefall_net::write_frame(&mut writer, &hello).map_err(|e| e.to_string())?;
        std::thread::spawn(move || {
            while let Ok(msg) = out_rx.recv() {
                if islefall_net::write_frame(&mut writer, &msg).is_err() {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            loop {
                match islefall_net::read_frame::<_, islefall_net::ServerMsg>(&mut reader) {
                    Ok(msg) => {
                        if in_tx.send(msg).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Net { tx, rx: std::sync::Mutex::new(rx), turn_ticks: 1, hash_every_turns: 0, started: false, turns_done: 0 })
    }

    fn send(&self, msg: islefall_net::ClientMsg) {
        let _ = self.tx.send(msg);
    }

    fn drain(&self) -> Vec<islefall_net::ServerMsg> {
        let mut out = Vec::new();
        if let Ok(rx) = self.rx.lock() {
            while let Ok(m) = rx.try_recv() {
                out.push(m);
            }
        }
        out
    }
}

/// The last thing the game had to say, shown on screen.
#[derive(Resource, Default)]
struct Status(String);

impl Status {
    fn say(&mut self, msg: String) {
        info!("{msg}");
        self.0 = msg;
    }
}

/// The corner text.
#[derive(Component)]
struct Hud;

/// A missile in flight from a shooter to its target.
#[derive(Component)]
struct Projectile {
    from: Vec2,
    to: Vec2,
    /// 0 at the muzzle, 1 at the target.
    progress: f32,
    seconds: f32,
    bearings: bool,
}

/// Where missiles landed this frame, for the overlay's flashes.
#[derive(Resource, Default)]
struct Landed(Vec<Vec2>);

/// The world's events of this frame, drained once for every system that reacts to them.
#[derive(Resource, Default)]
struct Happenings(Vec<islefall_sim::Event>);

fn collect_events(mut sim: ResMut<Sim>, mut happenings: ResMut<Happenings>) {
    happenings.0.clear();
    if let Some(world) = sim.world.as_mut() {
        happenings.0 = world.take_events();
    }
}

/// A one-shot animation from the sprite cache, stepped at the effects' own
/// pace and gone when it ends.
#[derive(Component)]
struct Effect {
    sequence: Vec<usize>,
    frames: Vec<FrameInfo>,
    step: usize,
    clock: f32,
}

/// Start an effect animation at `pos`; none when the type or label is missing.
fn spawn_effect(commands: &mut Commands, data: &GameData, lib: &mut ShapeLibrary, images: &mut Assets<Image>, layouts: &mut Assets<TextureAtlasLayout>, fx: &islefall_sim::config::EffectSprite, pos: Vec2, z: f32) {
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    let Some(shape) = lib.get_or_load(&data.install, palette, &fx.kind, images, layouts) else { return };
    let sequence: Vec<usize> = (0..shape.labels.len())
        .filter(|&i| shape.labels[i].eq_ignore_ascii_case(&fx.label) && !shape.flags[i].iter().any(|f| data.cfg.animation.hidden_flags.iter().any(|h| h.eq_ignore_ascii_case(f))))
        .collect();
    let Some(&first) = sequence.first() else { return };
    commands.spawn((
        Sprite::from_atlas_image(shape.image.clone(), TextureAtlas { layout: shape.layout.clone(), index: first }),
        shape.frames[first].anchor(),
        Transform::from_translation(pos.extend(z)),
        Effect { sequence, frames: shape.frames.clone(), step: 0, clock: 0.0 },
    ));
}

/// Step every effect and remove the ones that have ended.
fn run_effects(mut commands: Commands, data: Res<GameData>, time: Res<Time>, mut effects: Query<(Entity, &mut Effect, &mut Sprite, &mut Anchor)>) {
    let frame_time = 1.0 / data.cfg.effects.fps.max(1.0);
    for (e, mut fx, mut sprite, mut anchor) in &mut effects {
        fx.clock += time.delta_secs();
        while fx.clock >= frame_time {
            fx.clock -= frame_time;
            fx.step += 1;
        }
        if fx.step >= fx.sequence.len() {
            commands.entity(e).despawn();
            continue;
        }
        let frame = fx.sequence[fx.step];
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            if atlas.index != frame {
                atlas.index = frame;
                *anchor = fx.frames[frame].anchor();
            }
        }
    }
}

/// Explosions where structures die, and sparkles over the ones being built.
#[allow(clippy::too_many_arguments)]
fn structure_effects(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    time: Res<Time>,
    happenings: Res<Happenings>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut rng: Local<Option<Pcg32>>,
) {
    let g = data.grid();
    let fx = &data.cfg.effects;
    if let Some(boom) = &fx.destroyed {
        for e in happenings.0.iter().filter(|e| e.what == islefall_sim::EventKind::Destroyed) {
            spawn_effect(&mut commands, &data, &mut lib, &mut images, &mut layouts, boom, cell_to_world(e.at, g), Z_UNIT + 3.0);
        }
    }
    let Some(sparkle) = &fx.building else { return };
    let Some(w) = sim.world.as_ref() else { return };
    let rng = rng.get_or_insert_with(|| Pcg32::seed_from_u64(0x5851_F42D_4C95_7F2D));
    let mut next = || rng.random::<f32>();
    let dt = time.delta_secs();
    for s in w.structures.iter().filter(|s| !s.complete()) {
        let expected = fx.sparkles_per_cell * (s.foot_x * s.foot_y) as f32 * dt;
        if next() > expected.min(1.0) {
            continue;
        }
        let (hx, hy) = s.cell.top_left_px(g);
        let left = hx as f32 - ((s.foot_x - 1) * g.cell_w) as f32;
        let top = hy as f32 - ((s.foot_y - 1) * g.cell_h) as f32;
        let pos = Vec2::new(left + next() * (s.foot_x * g.cell_w) as f32, -(top + next() * (s.foot_y * g.cell_h) as f32));
        spawn_effect(&mut commands, &data, &mut lib, &mut images, &mut layouts, sparkle, pos, Z_UNIT + 1.5);
    }
}

/// Player-side interaction state.
#[derive(Resource)]
struct Player {
    tool: Tool,
    selected: usize,
    /// Which player this machine is: 0 alone, whatever the server says online.
    id: u8,
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
    /// Stop instead of looping, and then show `rest` if there is one.
    once: bool,
    rest: Option<usize>,
}

/// An all-round turret's place on its ring of bearings, and where it is
/// swinging to.
#[derive(Component)]
struct Turret {
    pos: usize,
    goal: usize,
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

/// One star drifting round an Energy source: its centre and its place in the ring.
#[derive(Component)]
struct EnergyRing {
    centre: Vec2,
    index: u32,
}

/// Marks per-frame overlay sprites: health bars, the selection mark, flashes.
#[derive(Component)]
struct Overlay;

/// A one-pixel white image for sprites that are just a colour. The
/// default image handle draws nothing here, so `Sprite::from_color` is
/// never used; see [`solid`].
#[derive(Resource)]
struct Solid(Handle<Image>);

/// A sprite of one colour and size.
fn solid(white: &Solid, color: Color, size: Vec2) -> Sprite {
    Sprite { image: white.0.clone(), color, custom_size: Some(size), ..default() }
}

/// Where the game's own data lives.
fn whoami() -> String {
    std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "player".into())
}

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
    let install = Installation::load(&dir).unwrap_or_else(|e| fail(format!("failed to load NetStorm data from {}: {e}", dir.display())));
    let map_name = std::env::var("ISLEFALL_MAP").unwrap_or_else(|_| "demo".into());
    let map = load_map(&data, &cfg, &scripts, &install).unwrap_or_else(|e| fail(e));
    if let Ok(out) = std::env::var("ISLEFALL_MAP_EXPORT") {
        match std::fs::write(&out, map.to_toml()) {
            Ok(()) => info!("wrote the map as {out}"),
            Err(e) => warn!("cannot write {out}: {e}"),
        }
    }
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
    let volume = std::env::var("ISLEFALL_VOLUME").ok().and_then(|s| s.parse().ok()).unwrap_or(cfg.sounds.volume);
    let sky_first = cfg.sounds.ambient.sky_seconds[0];
    let background = cfg.sky.background;
    let mut recording = Recording::default();
    if let Ok(addr) = std::env::var("ISLEFALL_JOIN") {
        let name = std::env::var("ISLEFALL_NAME").unwrap_or_else(|_| whoami());
        let mut bytes = std::fs::read(data.join("rules.toml")).unwrap_or_default();
        bytes.extend(std::fs::read(data.join("scripts/rules.rhai")).unwrap_or_default());
        // The map as loaded, so a scenario or a seeded map agrees between clients too.
        bytes.extend(format!("{map:?}").into_bytes());
        let hello = islefall_net::ClientMsg::Hello { protocol: islefall_net::PROTOCOL, name: name.clone(), map: map_name.clone(), data_hash: islefall_net::data_hash(&bytes) };
        let net = Net::connect(&addr, hello).unwrap_or_else(|e| fail(format!("cannot join {addr}: {e}")));
        net.send(islefall_net::ClientMsg::Ready(true));
        info!("joined {addr} as {name}; waiting for the others");
        recording.net = Some(net);
    }
    if let Some(path) = std::env::var_os("ISLEFALL_RECORD") {
        recording.record = Some((PathBuf::from(path), Replay { map: map_name.clone(), commands: Vec::new() }));
    }
    if let Some(path) = std::env::var_os("ISLEFALL_REPLAY") {
        let replay = Replay::load(&path).unwrap_or_else(|e| fail(format!("{}: {e}", PathBuf::from(&path).display())));
        if replay.map != map_name {
            warn!("replay was recorded on map {} but this is {map_name}", replay.map);
        }
        info!("replaying {} commands from {}", replay.commands.len(), PathBuf::from(&path).display());
        recording.replay = Some(replay);
    }

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window { title: "Islefall".into(), ..default() }),
                ..default()
            }),
    )
    .insert_resource(GameData { install, cfg, scripts, palette, map, data_dir: data.clone() })
    .insert_resource(GlobalVolume::new(Volume::Linear(volume)))
    .insert_resource(ClearColor(Color::srgb(background[0], background[1], background[2])))
    .insert_resource(Sky {
        timer: Timer::from_seconds(sky_first, TimerMode::Once),
        // Presentation only, never the simulation: seeded from the clock.
        rng: Pcg32::seed_from_u64(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7)),
    })
    .insert_resource(mode)
    .insert_resource(Time::<Fixed>::from_hz(tick_hz as f64))
    .init_resource::<ShapeLibrary>()
    .init_resource::<Sim>()
    .init_resource::<Status>()
    .insert_resource(recording)
    .insert_resource(Player { tool: Tool::Empty, selected: 0, id: 0 })
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
    .add_systems(Startup, |mut commands: Commands, mut images: ResMut<Assets<Image>>| {
        let star = make_star(&mut images);
        commands.insert_resource(RingImage(star));
    })
    .add_systems(FixedUpdate, sim_step.run_if(resource_equals(Mode::Island)))
    .add_systems(Update, (common_keys, animate, auto_screenshot))
    .add_systems(
        Update,
        (camera_keys, tool_keys, mouse_actions, bridge_keys, save_keys, ghost, title, hud, grant_knowledge, overlays, sync_units, sync_bridges, sync_structures, sync_platforms, sync_energy_rings)
            .run_if(resource_equals(Mode::Island)),
    )
    .add_systems(
        Update,
        (sidebar_update, sidebar_clicks.before(mouse_actions), minimap.after(camera_keys)).run_if(resource_equals(Mode::Island)),
    )
    .add_systems(Update, (tint_shells, animate_structures, turn_turrets.after(animate).after(animate_structures), drift_stars, burning, projectiles.before(overlays), structure_effects, run_effects).after(sync_structures).run_if(resource_equals(Mode::Island)))
    .add_systems(Update, collect_events.after(mouse_actions).after(bridge_keys).after(tool_keys).before(play_sounds).before(structure_effects).run_if(resource_equals(Mode::Island)))
    .init_resource::<Landed>()
    .init_resource::<Happenings>()
    .init_resource::<MinimapFrame>()
    .add_systems(Update, (play_sounds.after(mouse_actions).after(bridge_keys), sync_loops, ambient, footsteps.after(animate), drift_sky.after(camera_keys)).run_if(resource_equals(Mode::Island)))
    .add_systems(Update, viewer_keys.run_if(resource_equals(Mode::Viewer)));
    if let Ok(path) = std::env::var("ISLEFALL_SCREENSHOT") {
        app.insert_resource(AutoScreenshot { path, delay: Timer::from_seconds(screenshot_at, TimerMode::Once) });
    }
    app.run();
}

/// One frame of a type as an image of its own, for the panel's art.
fn frame_image(data: &GameData, stem: &str, frame: usize, images: &mut Assets<Image>) -> Option<(Handle<Image>, UVec2)> {
    let palette = data.install.palette(&data.palette)?;
    let records = data.install.shape_records(stem)?;
    let offset = *records.images.get(frame)?;
    let fr = data.install.shapes.decode(offset).ok()?;
    if fr.is_empty() {
        return None;
    }
    let (w, h) = (fr.width as u32, fr.height as u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..fr.height {
        for x in 0..fr.width {
            if let Some(idx) = fr.pixel(x, y) {
                let [r, g, b] = palette.rgb(idx);
                let o = (y * fr.width + x) * 4;
                rgba[o..o + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::nearest();
    Some((images.add(image), UVec2::new(w, h)))
}

/// The camera that draws the world (the UI has one of its own).
#[derive(Component)]
struct WorldCamera;

/// The panel and its parts.
#[derive(Component)]
struct SidebarRoot;
#[derive(Component)]
struct PowerText;
#[derive(Component)]
struct PieceBox;
#[derive(Component)]
struct PieceSlotButton(usize);
#[derive(Component)]
struct BuildButton {
    stem: String,
    spawn: bool,
}
#[derive(Component)]
struct MinimapNode;
#[derive(Component)]
struct ViewBox;

/// The minimap's world bounds and scale, for placing the view box and clicks.
#[derive(Resource, Default)]
struct MinimapFrame {
    /// Source pixel at the map's top-left, and map pixels per source pixel.
    origin_px: Vec2,
    scale: f32,
    size: Vec2,
}

fn colour(cfg: &islefall_sim::config::Sidebar, name: &str) -> Color {
    let [r, g, b] = cfg.colours.get(name).copied().unwrap_or([1.0, 0.0, 1.0]);
    Color::srgb(r, g, b)
}

/// Build the panel: counter, piece queue, build list, minimap.
fn spawn_sidebar(commands: &mut Commands, data: &GameData, lib: &mut ShapeLibrary, images: &mut Assets<Image>, layouts: &mut Assets<TextureAtlasLayout>) {
    let sb = &data.cfg.sidebar;
    let width = sb.width;
    let back = frame_image(data, &sb.art, sb.back_frame, images);
    let counter = frame_image(data, &sb.art, sb.counter_frame, images);
    let ruler = frame_image(data, &sb.art, sb.ruler_frame, images);
    let crystal = frame_image(data, &sb.crystal, 0, images);
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    let mut root = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: px(width),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(6)),
            row_gap: px(6),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgb(0.25, 0.24, 0.22)),
        SidebarRoot,
    ));
    root.with_children(|panel| {
        if let Some((back, _)) = &back {
            panel.spawn((
                ImageNode { image: back.clone(), image_mode: NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 1.0 }, ..default() },
                Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() },
            ));
        }
        if let Some((ruler, size)) = &ruler {
            panel.spawn((
                ImageNode { image: ruler.clone(), color: Color::srgba(1.0, 1.0, 1.0, 0.6), ..default() },
                Node { position_type: PositionType::Absolute, left: px((width - size.x as f32) / 2.0), top: px(0), width: px(size.x as f32), height: px(size.y as f32), ..default() },
            ));
        }
        // Storm Power.
        let mut counter_node = Node { width: percent(100), height: px(30), align_items: AlignItems::Center, justify_content: JustifyContent::Center, column_gap: px(6), ..default() };
        let mut row = match &counter {
            Some((img, _)) => {
                counter_node.padding = UiRect::axes(px(8), px(2));
                panel.spawn((ImageNode { image: img.clone(), image_mode: NodeImageMode::Stretch, ..default() }, counter_node))
            }
            None => panel.spawn((BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), counter_node)),
        };
        row.with_children(|r| {
            r.spawn((Text::new("0"), TextFont { font_size: 20.0.into(), ..default() }, TextColor(Color::srgb(0.95, 0.2, 0.2)), PowerText));
            if let Some((c, size)) = &crystal {
                r.spawn((ImageNode::new(c.clone()), Node { width: px(size.x as f32), height: px(size.y as f32), ..default() }));
            }
        });
        // The piece queue.
        panel.spawn((Node { width: percent(100), flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(6), justify_content: JustifyContent::Center, ..default() }, PieceBox));
        // The build list.
        let icon = sb.icon_size;
        let entries: Vec<(String, bool)> = data.cfg.controls.build_tools.iter().map(|t| (t.clone(), false)).chain(data.cfg.controls.unit_tools.iter().map(|t| (t.clone(), true))).collect();
        panel.spawn((Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(2), flex_grow: 1.0, overflow: Overflow::clip(), ..default() },)).with_children(|list| {
            for (stem, spawn) in entries {
                let def = data.install.type_def(&stem);
                let name = def.and_then(|d| d.get_str("description")).unwrap_or(stem.as_str()).to_string();
                let cost = def.and_then(|d| d.get_i64("cost")).unwrap_or(0);
                let shape = lib.get_or_load(&data.install, palette, &stem, images, layouts);
                let icon_frame = shape.and_then(|sh| {
                    let n = sh.labels.len();
                    let gump = (0..n).find(|&i| sh.flags[i].iter().any(|f| f.eq_ignore_ascii_case("gumpframe")));
                    let default = (0..n).find(|&i| sh.flags[i].iter().any(|f| f.eq_ignore_ascii_case("default")));
                    gump.or(default).map(|i| (sh.image.clone(), sh.layout.clone(), i, sh.frames[i].size))
                });
                let mut b = list.spawn((
                    Button,
                    Node { width: percent(100), height: px(icon + 4.0), align_items: AlignItems::Center, column_gap: px(6), padding: UiRect::all(px(2)), ..default() },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
                    BuildButton { stem: stem.clone(), spawn },
                ));
                b.with_children(|row| {
                    match icon_frame {
                        Some((image, layout, index, size)) => {
                            let scale = (icon / size.x.max(1) as f32).min(icon / size.y.max(1) as f32).min(1.5);
                            row.spawn((
                                ImageNode { image, texture_atlas: Some(TextureAtlas { layout, index }), image_mode: NodeImageMode::Stretch, ..default() },
                                Node { width: px(size.x as f32 * scale), height: px(size.y as f32 * scale), flex_shrink: 0.0, ..default() },
                            ));
                        }
                        None => {
                            row.spawn((Node { width: px(icon), height: px(icon), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3))));
                        }
                    }
                    row.spawn((Node { flex_direction: FlexDirection::Column, ..default() },)).with_children(|col| {
                        col.spawn((Text::new(name), TextFont { font_size: sb.font_size.into(), ..default() }, TextColor(Color::WHITE)));
                        col.spawn((Text::new(if cost > 0 { cost.to_string() } else { String::new() }), TextFont { font_size: sb.font_size.into(), ..default() }, TextColor(Color::srgb(1.0, 0.85, 0.3))));
                    });
                });
            }
        });
        // The minimap.
        let map_w = width - 12.0;
        panel.spawn((
            Node { width: px(map_w), height: px(sb.minimap_height), flex_shrink: 0.0, ..default() },
            BackgroundColor(colour(sb, "sky")),
            ImageNode { image: Handle::default(), image_mode: NodeImageMode::Stretch, ..default() },
            bevy::ui::RelativeCursorPosition::default(),
            Interaction::None,
            MinimapNode,
        ))
        .with_children(|m| {
            m.spawn((
                Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: px(10), height: px(10), border: UiRect::all(px(1)), ..default() },
                BorderColor::all(colour(sb, "view")),
                ViewBox,
            ));
        });
    });
}

/// Keep the panel's figures and lists current: the Storm Power, the piece
/// queue and which build entry is the tool in hand.
#[allow(clippy::too_many_arguments)]
fn sidebar_update(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    player: Res<Player>,
    mut texts: Query<&mut Text, With<PowerText>>,
    boxes: Query<Entity, With<PieceBox>>,
    slots: Query<Entity, With<PieceSlotButton>>,
    mut slot_buttons: Query<(&PieceSlotButton, &mut BackgroundColor), Without<BuildButton>>,
    mut buttons: Query<(&BuildButton, &mut BackgroundColor)>,
    mut last_queue: Local<Vec<String>>,
) {
    let w = sim.world();
    let me = player.id as usize % data.cfg.sim.max_players;
    for mut t in &mut texts {
        let s = w.powers[me].to_string();
        if t.0 != s {
            t.0 = s;
        }
    }
    // The piece in hand is drawn turned as it is, so the slots are redrawn
    // when the queue or the turn changes.
    let in_hand = match &player.tool {
        Tool::Bridge(slot, rotations) => Some((*slot, *rotations)),
        _ => None,
    };
    let mut names: Vec<String> = w.queue.slots.iter().map(|p| p.name.clone()).collect();
    names.push(format!("{in_hand:?}"));
    if *last_queue != names {
        *last_queue = names;
        for e in &slots {
            commands.entity(e).despawn();
        }
        if let Ok(boxe) = boxes.single() {
            let sb = &data.cfg.sidebar;
            let cell = sb.piece_cell;
            commands.entity(boxe).with_children(|b| {
                for (i, piece) in w.queue.slots.iter().enumerate() {
                    let mut piece = piece.clone();
                    if let Some((slot, rotations)) = in_hand {
                        if slot == i {
                            for _ in 0..rotations % 4 {
                                piece = piece.rotated();
                            }
                        }
                    }
                    let (pw, ph) = piece.size();
                    let key = data.cfg.controls.slot_keys.get(i).cloned().unwrap_or_default();
                    b.spawn((
                        Button,
                        Node { width: px((sb.width - 12.0) / 4.0 - 6.0), height: px(cell * 4.0 + 22.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, padding: UiRect::all(px(2)), ..default() },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
                        PieceSlotButton(i),
                    ))
                    .with_children(|slot| {
                        slot.spawn((Node { width: px(cell * pw as f32), height: px(cell * ph as f32), position_type: PositionType::Relative, ..default() },)).with_children(|grid| {
                            for &(ox, oy) in &piece.cells {
                                grid.spawn((
                                    Node { position_type: PositionType::Absolute, left: px(ox as f32 * cell), top: px(oy as f32 * cell), width: px(cell - 1.0), height: px(cell - 1.0), ..default() },
                                    BackgroundColor(colour(sb, "bridge")),
                                ));
                            }
                        });
                        slot.spawn((Text::new(key), TextFont { font_size: sb.font_size.into(), ..default() }, TextColor(Color::srgba(1.0, 1.0, 1.0, 0.8))));
                    });
                }
            });
        }
    }
    for (b, mut bg) in &mut buttons {
        let chosen = match &player.tool {
            Tool::Drop(stem) => !b.spawn && *stem == b.stem,
            Tool::Spawn(stem) => b.spawn && *stem == b.stem,
            Tool::Bridge(..) | Tool::Empty => false,
        };
        let wanted = if chosen { Color::srgba(1.0, 0.9, 0.4, 0.35) } else { Color::srgba(0.0, 0.0, 0.0, 0.25) };
        if bg.0 != wanted {
            bg.0 = wanted;
        }
    }
    // The piece in hand lights its slot up.
    for (slot, mut bg) in &mut slot_buttons {
        let chosen = matches!(&player.tool, Tool::Bridge(i, _) if *i == slot.0);
        let wanted = if chosen { Color::srgba(1.0, 0.9, 0.4, 0.35) } else { Color::srgba(0.0, 0.0, 0.0, 0.25) };
        if bg.0 != wanted {
            bg.0 = wanted;
        }
    }
}

/// Clicks on the panel: a piece slot or a build entry becomes the tool.
fn sidebar_clicks(
    mut sim: ResMut<Sim>,
    data: Res<GameData>,
    mut player: ResMut<Player>,
    mut status: ResMut<Status>,
    mut rec: ResMut<Recording>,
    slots: Query<(&Interaction, &PieceSlotButton), Changed<Interaction>>,
    builds: Query<(&Interaction, &BuildButton), Changed<Interaction>>,
) {
    for (i, slot) in &slots {
        if *i == Interaction::Pressed {
            player.tool = Tool::Bridge(slot.0, 0);
            status.say(format!("tool: bridge piece {} from slot {}", sim.world().queue.slots.get(slot.0).map(|p| p.name.as_str()).unwrap_or("?"), slot.0 + 1));
        }
    }
    for (i, b) in &builds {
        if *i != Interaction::Pressed {
            continue;
        }
        let rules = data.rules(&b.stem);
        if rules.energy.is_some() && !sim.world().in_production(player.id, &b.stem) {
            rec.issue(sim.world_mut(), &data.scripts, &mut status, Command::Produce { kind: b.stem.clone() });
        }
        player.tool = if b.spawn { Tool::Spawn(b.stem.clone()) } else { Tool::Drop(b.stem.clone()) };
        status.say(format!("tool: {}", b.stem));
    }
}

/// Redraw the minimap now and then, place the view box, and jump the
/// camera where the map is clicked.
#[allow(clippy::too_many_arguments)]
fn minimap(
    sim: Res<Sim>,
    data: Res<GameData>,
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut images: ResMut<Assets<Image>>,
    mut frame: ResMut<MinimapFrame>,
    mut map: Query<(&mut ImageNode, &bevy::ui::RelativeCursorPosition), With<MinimapNode>>,
    mut view_box: Query<&mut Node, With<ViewBox>>,
    mut cameras: Query<(&Camera, &mut Transform, &Projection), With<WorldCamera>>,
    mut timer: Local<f32>,
) {
    let w = sim.world();
    let g = data.grid();
    let sb = &data.cfg.sidebar;
    let Ok((mut node, cursor)) = map.single_mut() else { return };
    let size = Vec2::new(sb.width - 12.0, sb.minimap_height);
    *timer -= time.delta_secs();
    if *timer <= 0.0 || frame.size == Vec2::ZERO {
        *timer = 0.3;
        // Bounds of everything that is ground, with a margin.
        let mut min = Cell::new(i32::MAX, i32::MAX);
        let mut max = Cell::new(i32::MIN, i32::MIN);
        for c in w.islands.iter().flat_map(|i| i.cells()).chain(w.platforms.cells()).chain(w.bridges.keys().copied()) {
            min = Cell::new(min.x.min(c.x), min.y.min(c.y));
            max = Cell::new(max.x.max(c.x), max.y.max(c.y));
        }
        if min.x > max.x {
            return;
        }
        let (min, max) = (Cell::new(min.x - 2, min.y - 2), Cell::new(max.x + 3, max.y + 3));
        let cells = Vec2::new((max.x - min.x) as f32, (max.y - min.y) as f32);
        // Cells are wider than tall; keep the map's shape in source pixels.
        let world_px = Vec2::new(cells.x * g.cell_w as f32, cells.y * g.cell_h as f32);
        let scale = (size.x / world_px.x).min(size.y / world_px.y);
        let (mw, mh) = (size.x as u32, size.y as u32);
        let mut rgba = vec![0u8; (mw * mh * 4) as usize];
        let sky = sb.colours.get("sky").copied().unwrap_or([0.0, 0.0, 0.0]);
        for px_ in rgba.chunks_exact_mut(4) {
            px_.copy_from_slice(&[(sky[0] * 255.0) as u8, (sky[1] * 255.0) as u8, (sky[2] * 255.0) as u8, 255]);
        }
        let mut fill = |c: Cell, col: [f32; 3]| {
            let x0 = ((c.x - min.x) as f32 * g.cell_w as f32 * scale) as u32;
            let y0 = ((c.y - min.y) as f32 * g.cell_h as f32 * scale) as u32;
            let x1 = (((c.x - min.x + 1) as f32 * g.cell_w as f32 * scale) as u32).max(x0 + 1);
            let y1 = (((c.y - min.y + 1) as f32 * g.cell_h as f32 * scale) as u32).max(y0 + 1);
            for y in y0..y1.min(mh) {
                for x in x0..x1.min(mw) {
                    let o = ((y * mw + x) * 4) as usize;
                    rgba[o..o + 4].copy_from_slice(&[(col[0] * 255.0) as u8, (col[1] * 255.0) as u8, (col[2] * 255.0) as u8, 255]);
                }
            }
        };
        let col = |name: &str| sb.colours.get(name).copied().unwrap_or([1.0, 0.0, 1.0]);
        for (i, island) in w.islands.iter().enumerate() {
            let theme = w.island_themes.get(i).copied().unwrap_or(Theme::Sun);
            let c = col(theme_name(theme));
            for cell in island.cells() {
                fill(cell, c);
            }
        }
        for cell in w.platforms.cells() {
            fill(cell, col("sun"));
        }
        for cell in w.bridges.keys() {
            fill(*cell, col("bridge"));
        }
        for s in &w.structures {
            if s.max_hp == 0 {
                continue;
            }
            let c = if s.owner == 0 { col("mine") } else { col("enemy") };
            fill(s.centre(), c);
        }
        for u in w.units.iter().filter(|u| u.alive) {
            fill(u.cell(), col("unit"));
        }
        let image = images.add(Image::new(
            bevy::render::render_resource::Extent3d { width: mw, height: mh, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            rgba,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::RENDER_WORLD,
        ));
        let old = std::mem::replace(&mut node.image, image);
        images.remove(&old);
        *frame = MinimapFrame { origin_px: Vec2::new(min.x as f32 * g.cell_w as f32, min.y as f32 * g.cell_h as f32), scale, size };
    }
    let Ok((camera, mut cam_tf, proj)) = cameras.single_mut() else { return };
    let scale = match proj {
        Projection::Orthographic(o) => o.scale,
        _ => 1.0,
    };
    // The view box: the camera's world rectangle on the map.
    if let (Some(view), Ok(mut vb)) = (camera.logical_viewport_size(), view_box.single_mut()) {
        let half = view * 0.5 * scale;
        let centre = cam_tf.translation.truncate();
        // World y grows upwards, the map's downwards.
        let to_map = |world: Vec2| (Vec2::new(world.x, -world.y) - frame.origin_px) * frame.scale;
        let a = to_map(centre - half);
        let b = to_map(centre + half);
        let (l, t) = (a.x.min(b.x), a.y.min(b.y));
        let (r, btm) = (a.x.max(b.x), a.y.max(b.y));
        vb.left = px(l.clamp(0.0, size.x));
        vb.top = px(t.clamp(0.0, size.y));
        vb.width = px((r - l).min(size.x - l.max(0.0)).max(2.0));
        vb.height = px((btm - t).min(size.y - t.max(0.0)).max(2.0));
    }
    // A click on the map centres the camera there.
    if buttons.pressed(MouseButton::Left) && cursor.cursor_over && frame.scale > 0.0 {
        if let Some(n) = cursor.normalized {
            let map_px = (n + Vec2::splat(0.5)) * size;
            let source = frame.origin_px + map_px / frame.scale;
            cam_tf.translation.x = source.x;
            cam_tf.translation.y = -source.y;
        }
    }
}

/// Whether the cursor is over the panel rather than the world.
fn over_sidebar(windows: &Query<&Window, With<PrimaryWindow>>, data: &GameData) -> bool {
    windows.single().ok().and_then(|w| w.cursor_position()).is_some_and(|c| c.x < data.cfg.sidebar.width)
}

/// A little four-pointed star, for the drifting marks of an Energy reach.
fn make_star(images: &mut Assets<Image>) -> Handle<Image> {
    let size = 5u32;
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let on_arm = x == size / 2 || y == size / 2;
            let centre = x == size / 2 && y == size / 2;
            let alpha = if centre { 255 } else if on_arm { 170 } else { 0 };
            let o = ((y * size + x) * 4) as usize;
            data[o..o + 4].copy_from_slice(&[255, 250, 200, alpha]);
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
    lib.sheets = Some(data.data_dir.join(&data.cfg.sprites.dir));
    let white = images.add(Image::new(
        bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        vec![255, 255, 255, 255],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    ));
    commands.insert_resource(Solid(white));
    match *mode {
        Mode::Island => {
            setup_map(&mut commands, &data, &mut sim, &mut lib, &mut images, &mut layouts);
            spawn_sky(&mut commands, &data, &mut images);
            spawn_sidebar(&mut commands, &data, &mut lib, &mut images, &mut layouts);
            commands.spawn((
                Text::new(""),
                TextFont { font_size: data.cfg.hud.font_size.into(), ..default() },
                TextColor(Color::WHITE),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                Node { position_type: PositionType::Absolute, top: px(8), left: px(data.cfg.sidebar.width + 8.0), padding: UiRect::all(px(6)), ..default() },
                Hud,
            ));
        }
        Mode::Viewer => {
            commands.spawn((Camera2d, zoomed_projection(data.cfg.controls.zoom), WorldCamera, IsDefaultUiCamera));
            spawn_viewer_shape(&mut commands, &data, &viewer, &mut lib, &mut images, &mut layouts);
        }
    }
}

fn zoomed_projection(zoom: f32) -> Projection {
    Projection::Orthographic(OrthographicProjection { scale: 1.0 / zoom.max(0.1), ..OrthographicProjection::default_2d() })
}

/// The map named by `ISLEFALL_MAP`: a file in `maps/`, a made-up one, or
/// a campaign scenario from the archive.
fn load_map(data: &Path, cfg: &Config, scripts: &Scripts, install: &Installation) -> Result<MapDef, String> {
    let name = std::env::var("ISLEFALL_MAP").unwrap_or_else(|_| "demo".into());
    let path = data.join("maps").join(format!("{name}.toml"));
    if path.exists() {
        return MapDef::load(&path).map_err(|e| format!("{}: {e}", path.display()));
    }
    let foot = |kind: &str| -> (i32, i32) {
        install.type_def(kind).and_then(|d| TypeRules::from_type(d, cfg, scripts).ok()).map(|r| (r.foot_x, r.foot_y)).unwrap_or((1, 1))
    };
    if let Some(rest) = name.strip_prefix("random") {
        let seed = rest.strip_prefix(':').and_then(|s| s.parse().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(1));
        let players = std::env::var("ISLEFALL_PLAYERS").ok().and_then(|p| p.parse().ok()).unwrap_or(cfg.generate.players);
        info!("making a map up: seed {seed}, {players} players (ISLEFALL_MAP=random:{seed} brings it back)");
        return Ok(islefall_sim::generate::skirmish(cfg, seed, players, &foot));
    }
    let stem = name.to_ascii_lowercase();
    let Some(index) = install.archive.find(&format!("{stem}.fort")) else {
        return Err(format!("{}: no such map, and no {stem}.fort in the archive", path.display()));
    };
    let fortfile = islefall_data::fort::Fort::parse(&install.archive.read(index)).map_err(|e| format!("{stem}.fort: {e}"))?;
    let mission = islefall_data::mission::Mission::for_fort(&install.archive, &stem);
    let (map, report) = islefall_sim::campaign::fort_to_map(&fortfile, mission.as_ref(), cfg, &stem, &foot);
    for note in &report.notes {
        info!("{stem}.fort: {note}");
    }
    if !report.unknown_codes.is_empty() {
        warn!("{stem}.fort: skipped things with codes the rules do not name: {}", report.unknown_codes.iter().map(|(c, n)| format!("{c:02x} x{n}")).collect::<Vec<_>>().join(", "));
    }
    if fortfile.skipped_chunks > 0 {
        warn!("{stem}.fort: {} record chunks could not be read", fortfile.skipped_chunks);
    }
    Ok(map)
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
    w.rims_enforced = false;

    for isl in &data.map.islands {
        let mut island = if isl.cells.is_empty() { IslandMap::rect(map::cell(isl.origin), isl.size[0], isl.size[1]) } else { IslandMap::new() };
        for c in &isl.cells {
            island.insert(map::cell(*c));
        }
        for c in &isl.remove {
            island.remove(map::cell(*c));
        }
        let theme = Theme::parse(&isl.theme).unwrap_or(Theme::Sun);
        world::spawn_island(commands, install, lib, palette, images, layouts, &island, theme, false, data.grid());
        match isl.owner {
            Some(owner) => w.push_island(island, owner),
            None => w.push_neutral_island(island),
        }
        let last = w.islands.len() - 1;
        w.set_island_theme(last, theme);
    }
    for b in &data.map.bridges {
        // Cells that touch nothing yet may be reached by later ones: keep
        // trying until a pass places nothing more.
        let mut left: Vec<Cell> = b.cells.iter().map(|c| map::cell(*c)).collect();
        loop {
            let before = left.len();
            left.retain(|&c| !w.place_map_bridge(b.owner, c));
            if left.is_empty() || left.len() == before {
                break;
            }
        }
        if !left.is_empty() {
            warn!("map: {} bridge cells of owner {} touch no island or bridge and were left out (first at {:?})", left.len(), b.owner, left[0]);
        }
    }
    for st in &data.map.structures {
        // A map may stack things the way the original did (its Temple
        // rose from the altar): nudge to the nearest free cell.
        let rules = data.rules(&st.kind);
        let wanted = map::cell(st.at);
        let reach = rules.foot_x.max(rules.foot_y).max(4);
        let mut spots: Vec<Cell> = (-reach..=reach).flat_map(|dy| (-reach..=reach).map(move |dx| wanted.offset(dx, dy))).collect();
        spots.sort_by_key(|c| (c.x - wanted.x).abs() + (c.y - wanted.y).abs());
        let mut placed = Err(String::new());
        for at in spots {
            match w.drop_structure_for(st.owner, &st.kind, &rules, at) {
                Ok(i) => {
                    if at != wanted {
                        info!("map: {} moved from {:?} to {:?}", st.kind, wanted, at);
                    }
                    placed = Ok(i);
                    break;
                }
                Err(e) if placed.is_err() && placed.as_ref().err().is_some_and(String::is_empty) => placed = Err(e.to_string()),
                Err(_) => {}
            }
        }
        match placed {
            Ok(i) => {
                w.structures[i].variant = st.frame;
                if w.structures[i].is_obelisk {
                    w.structures[i].spell = st.spell.clone();
                    w.assign_obelisk_spell(i);
                }
            }
            Err(e) => warn!("map: {} at {:?}: {e}", st.kind, st.at),
        }
    }
    w.powers = vec![data.map.start_power; data.cfg.sim.max_players];
    w.energy_enforced = true;
    w.rims_enforced = true;
    // The layout was placed, not built: nothing to hear.
    w.take_events();
    for u in &data.map.units {
        let rules = data.rules(&u.kind);
        let wanted = map::cell(u.at);
        let mut spots: Vec<Cell> = (-4..=4).flat_map(|dy| (-4..=4).map(move |dx| wanted.offset(dx, dy))).collect();
        spots.sort_by_key(|c| (c.x - wanted.x).abs() + (c.y - wanted.y).abs());
        match spots.iter().find_map(|&at| w.spawn_unit_for(u.owner, &u.kind, &rules, at).map(|i| (i, at))) {
            Some((i, at)) => {
                if at != wanted {
                    info!("map: unit {} moved from {:?} to {:?}", u.kind, wanted, at);
                }
                if let Some(to) = u.move_to {
                    w.order_move(i, map::cell(to));
                }
            }
            None => warn!("map: unit {} at {:?} could not be placed", u.kind, u.at),
        }
    }
    for t in &data.map.player_tech {
        if let Some(bit) = data.rules(t).tech_bit {
            w.grant_tech(0, bit);
        }
    }
    let altar = w.structures.iter().find(|s| s.is_altar && s.owner == 0).map(|s| s.centre()).unwrap_or(data.map.camera_cell());
    for op in &data.map.opponents {
        let target = op.target.map(map::cell).unwrap_or(altar);
        let mut ai = Ai::new(op.owner, target, &data.cfg);
        if let Some(sh) = &op.shooter {
            ai.shooter = sh.clone();
        }
        if let Some(gn) = &op.generator {
            ai.generator = gn.clone();
        }
        for &bit in &op.knowledge {
            w.grant_tech(op.owner, bit);
        }
        for t in &op.tech {
            if let Some(bit) = data.rules(t).tech_bit {
                w.grant_tech(op.owner, bit);
            }
        }
        if let Some(p) = op.power {
            let slot = op.owner as usize % data.cfg.sim.max_players;
            w.powers[slot] = p;
        }
        sim.ais.push(ai);
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
    let zoom = std::env::var("ISLEFALL_ZOOM").ok().and_then(|z| z.trim().parse::<f32>().ok()).unwrap_or(data.cfg.controls.zoom);
    commands.spawn((Camera2d, zoomed_projection(zoom), Transform::from_translation(centre.extend(0.0)), WorldCamera, IsDefaultUiCamera));
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
    // The simulation keeps a unit at its cell's middle; its feet belong on
    // the cell's bottom edge, where structures put their hotspots too.
    let (x, y) = unit.pos_f32();
    Vec2::new(x * g.cell_w as f32, -((y + 0.5) * g.cell_h as f32 - 1.0))
}

/// Cell under a world position.
fn world_to_cell(p: Vec2, g: &Grid) -> Cell {
    Cell::new((p.x / g.cell_w as f32).floor() as i32, (-p.y / g.cell_h as f32).floor() as i32)
}

/// Spawn image and shadow layers of one animation, looping. Returns the entities.
fn spawn_animated(
    commands: &mut Commands,
    shape: &LoadedShape,
    animation: &str,
    pos: Vec2,
    z: f32,
    viewer: bool,
) -> Vec<Entity> {
    let sequence = animation_sequence(&shape.labels, animation);
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
            ShapeSprite { sequence: sequence.clone(), frames, step: 0, playing: true, once: false, rest: None },
        ));
        if viewer {
            e.insert(ViewerEntity);
        }
        out.push(e.id());
    }
    out
}

/// Frame indices of one animation label, in file order.
fn animation_sequence(labels: &[String], animation: &str) -> Vec<usize> {
    let seq: Vec<usize> = labels.iter().enumerate().filter(|(_, l)| l.eq_ignore_ascii_case(animation)).map(|(i, _)| i).collect();
    if !seq.is_empty() || animation.eq_ignore_ascii_case("A") {
        return seq;
    }
    // Types without per-facing animations (flyers spin, balloons drift) use their first one.
    labels.iter().enumerate().filter(|(_, l)| l.eq_ignore_ascii_case("A")).map(|(i, _)| i).collect()
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
    let description = install.type_def(stem).and_then(|d| d.get_str("description")).unwrap_or("-").to_string();
    let Some(shape) = lib.get_or_load(install, palette, stem, images, layouts) else {
        warn!("{stem}: no atlas");
        return;
    };
    let animations = shape.animations();
    let Some(&animation) = animations.get(viewer.animation) else { return };
    info!("{stem} ({description}): {} frames, {} animations, showing {animation}", shape.frames.len(), animations.len());
    spawn_animated(commands, shape, animation, Vec2::ZERO, Z_UNIT, true);
}

fn sim_step(mut sim: ResMut<Sim>, viewer: Res<Viewer>, data: Res<GameData>, mut rec: ResMut<Recording>, mut player: ResMut<Player>, mut status: ResMut<Status>) {
    if viewer.paused {
        return;
    }
    let Sim { world: Some(world), ais } = &mut *sim else { return };
    if rec.net.is_some() {
        online_step(world, ais, &data, &mut rec, &mut player, &mut status);
        return;
    }
    if let Some(replay) = rec.replay.as_ref() {
        let refused = replay.play_tick(world, &data.scripts);
        if refused > 0 {
            warn!("tick {}: {refused} replayed command(s) came out differently: the game has drifted from the recording", world.tick);
        }
    }
    let every = if data.cfg.controls.hash_every_seconds > 0.0 { data.cfg.ticks(data.cfg.controls.hash_every_seconds) as u64 } else { 0 };
    if every > 0 && world.tick % every == 0 {
        info!("tick {} hash {:016x}", world.tick, world.hash());
    }
    world.step(&data.scripts);
    for ai in ais.iter_mut() {
        let kit = islefall_sim::ai::AiKit { shooter: (ai.shooter.clone(), data.rules(&ai.shooter)), generator: (ai.generator.clone(), data.rules(&ai.generator)) };
        match ai.tick(world, &kit, &data.scripts) {
            Some(AiMove::Piece { name, at }) => info!("opponent {} lays a {name} piece at {at:?}", ai.owner),
            Some(AiMove::Shooter { kind, at }) => info!("opponent {} drops a {kind} at {at:?}", ai.owner),
            Some(AiMove::Generator { kind, at }) => info!("opponent {} drops a {kind} for Energy at {at:?}", ai.owner),
            Some(AiMove::Refused { kind, why }) => info!("opponent {} could not drop a {kind}: {why}", ai.owner),
            _ => {}
        }
    }
}

/// Play the sound each event names: the type's property first, then the
/// rules' fallback file. The world's events are drained here.
fn play_sounds(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    mut bank: ResMut<SoundBank>,
    mut sources: ResMut<Assets<AudioSource>>,
    volume: Res<GlobalVolume>,
    cameras: Query<(&Camera, &GlobalTransform, &Projection), With<WorldCamera>>,
    mut turns: Local<HashMap<String, usize>>,
    happenings: Res<Happenings>,
) {
    if sim.world.is_none() || volume.volume == Volume::Linear(0.0) {
        return;
    }
    let events = happenings.0.clone();
    let Some(view) = View::of(&cameras) else { return };
    let snd = &data.cfg.sounds;
    let mut started = 0;
    for e in events {
        if started >= snd.max_per_frame {
            break;
        }
        let cue = snd.events.cue(e.what);
        let named = |stem: &str| cue.property.as_deref().and_then(|p| data.install.type_def(stem).and_then(|d| d.get_str(p))).map(str::to_string);
        let from_type = named(&e.kind).or_else(|| snd.projectiles.get(&e.kind).and_then(|p| named(p)));
        let Some(mut name) = from_type.or_else(|| cue.file.clone()).filter(|n| !n.is_empty()) else { continue };
        if cue.variants {
            // One of the numbered siblings in turn, so a voice does not say the same line twice running.
            let names = bank.variants(&name);
            let n = turns.entry(name.to_lowercase()).or_insert(0);
            name = names[*n % names.len()].clone();
            *n += 1;
        }
        let Some((handle, base_db)) = bank.get_or_load(&name, &mut sources) else { continue };
        // Out of view, out of earshot.
        let Some(gain) = gain_at(&view, cell_to_world(e.at, data.grid()), base_db, &data) else { continue };
        commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN.with_volume(gain)));
        started += 1;
    }
}

/// Keep a loop playing for each object in view that names one, nearest
/// the centre first up to the cap, with its volume following the camera.
fn sync_loops(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    mut bank: ResMut<SoundBank>,
    mut sources: ResMut<Assets<AudioSource>>,
    cameras: Query<(&Camera, &GlobalTransform, &Projection), With<WorldCamera>>,
    mut existing: Query<(Entity, &ObjectLoop, Option<&mut AudioSink>)>,
) {
    let Some(world) = sim.world.as_ref() else { return };
    let Some(view) = View::of(&cameras) else { return };
    let a = &data.cfg.sounds.attenuation;
    let g = data.grid();
    let names = |kind: &str| data.install.type_def(kind).and_then(|d| d.get_str(&a.loop_property)).map(str::to_string);
    // (key, name, position, distance from the centre)
    let mut wanted: Vec<(LoopKey, String, Vec2, f32)> = Vec::new();
    for s in &world.structures {
        let name = if s.complete() { names(&s.kind) } else { a.building_loop.clone() };
        if let Some(name) = name {
            let pos = cell_to_world(s.centre(), g);
            wanted.push((LoopKey::Structure(s.kind.clone(), s.cell), name, pos, view.edge(pos)));
        }
    }
    for (i, u) in world.units.iter().enumerate() {
        if !u.alive || u.carried_by.is_some() {
            continue;
        }
        if let Some(name) = names(&u.kind) {
            let pos = unit_to_world(u, g);
            wanted.push((LoopKey::Unit(i), name, pos, view.edge(pos)));
        }
    }
    wanted.retain(|w| w.3 <= 1.0);
    wanted.sort_by(|x, y| x.3.total_cmp(&y.3));
    wanted.truncate(a.max_loops);
    let mut keep: HashMap<LoopKey, (String, Vec2)> = wanted.into_iter().map(|(k, n, p, _)| (k, (n, p))).collect();
    for (entity, l, sink) in existing.iter_mut() {
        match keep.get(&l.0).filter(|(n, _)| n.eq_ignore_ascii_case(&l.1)).cloned() {
            Some((name, pos)) => {
                keep.remove(&l.0);
                let base_db = bank.get_or_load(&name, &mut sources).map(|(_, db)| db).unwrap_or(0.0);
                if let (Some(gain), Some(mut sink)) = (gain_at(&view, pos, base_db, &data), sink) {
                    sink.set_volume(gain);
                }
            }
            None => commands.entity(entity).despawn(),
        }
    }
    for (key, (name, pos)) in keep {
        let Some((handle, base_db)) = bank.get_or_load(&name, &mut sources) else { continue };
        let Some(gain) = gain_at(&view, pos, base_db, &data) else { continue };
        commands.spawn((AudioPlayer::new(handle), PlaybackSettings::LOOP.with_volume(gain), ObjectLoop(key, name)));
    }
}

/// A walker's steps follow its walk animation: whenever the frame passes a
/// step point, one of the type's numbered step sounds plays at the unit.
#[allow(clippy::too_many_arguments)]
fn footsteps(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    mut bank: ResMut<SoundBank>,
    mut sources: ResMut<Assets<AudioSource>>,
    cameras: Query<(&Camera, &GlobalTransform, &Projection), With<WorldCamera>>,
    layers: Query<(&UnitLayer, &ShapeSprite)>,
    mut last: Local<HashMap<usize, (usize, usize)>>,
) {
    let Some(world) = sim.world.as_ref() else { return };
    let Some(view) = View::of(&cameras) else { return };
    let fs = &data.cfg.sounds.footsteps;
    let g = data.grid();
    for (layer, shape) in &layers {
        if layer.shadow || !shape.playing || shape.sequence.is_empty() {
            continue;
        }
        let Some(unit) = world.units.get(layer.unit).filter(|u| u.alive && !u.is_air) else { continue };
        let entry = last.entry(layer.unit).or_insert((usize::MAX, 0));
        if entry.0 == shape.step {
            continue;
        }
        entry.0 = shape.step;
        let stride = (shape.sequence.len() / fs.per_cycle as usize).max(1);
        if shape.step % stride != 0 {
            continue;
        }
        let Some(name) = fs.steps.get(&unit.kind).cloned() else { continue };
        let names = bank.variants(&name);
        let pick = names[entry.1 % names.len()].clone();
        entry.1 += 1;
        let Some((handle, base_db)) = bank.get_or_load(&pick, &mut sources) else { continue };
        let Some(gain) = gain_at(&view, unit_to_world(unit, g), base_db, &data) else { continue };
        commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN.with_volume(gain)));
    }
}

/// The bed loop, started once, and sky noises at random intervals.
fn ambient(mut commands: Commands, data: Res<GameData>, mut bank: ResMut<SoundBank>, mut sources: ResMut<Assets<AudioSource>>, time: Res<Time>, mut sky: ResMut<Sky>, mut started: Local<bool>) {
    let amb = &data.cfg.sounds.ambient;
    if !*started {
        *started = true;
        if let Some((handle, db)) = amb.bed.as_deref().and_then(|b| bank.get_or_load(b, &mut sources)) {
            commands.spawn((AudioPlayer::new(handle), PlaybackSettings::LOOP.with_volume(Volume::Decibels(db))));
        }
    }
    if amb.sky.is_empty() {
        return;
    }
    sky.timer.tick(time.delta());
    if sky.timer.just_finished() {
        let pick = sky.rng.random_range(0..amb.sky.len());
        if let Some((handle, db)) = bank.get_or_load(&amb.sky[pick], &mut sources) {
            commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN.with_volume(Volume::Decibels(db))));
        }
        let wait = sky.wait(amb.sky_seconds);
        sky.timer = Timer::from_seconds(wait, TimerMode::Once);
    }
}

/// How the game stands for the local player, if it is decided.
fn verdict(w: &World, data: &GameData, me: u8) -> Option<String> {
    let hud = &data.cfg.hud;
    match w.winner {
        Some(id) if id == me => Some(hud.victory.clone()),
        Some(_) => Some(hud.defeat.clone()),
        None if w.out.contains(&me) => Some(hud.defeat.clone()),
        None => None,
    }
}

/// Online, the world moves only by the server's turns: every command of
/// a turn is applied, then the turn's ticks run, the opponents act, and
/// every so many turns the hash goes back to the server.
fn online_step(world: &mut World, ais: &mut [Ai], data: &GameData, rec: &mut Recording, player: &mut Player, status: &mut Status) {
    use islefall_net::{ClientMsg, ServerMsg};
    let Some(net) = rec.net.as_mut() else { return };
    let messages = net.drain();
    for msg in messages {
        match msg {
            ServerMsg::Welcome { player: id, turn_ticks, hash_every_turns } => {
                player.id = id;
                rec.owner = id;
                net.turn_ticks = turn_ticks.max(1);
                net.hash_every_turns = hash_every_turns as u64;
                status.say(format!("the server made us player {id}"));
            }
            ServerMsg::Reject(why) => status.say(format!("the server refused us: {why}")),
            ServerMsg::Lobby { players } => {
                let names: Vec<String> = players.iter().map(|p| format!("{}{}", p.name, if p.ready { " (ready)" } else { "" })).collect();
                status.say(format!("in the lobby: {}", names.join(", ")));
            }
            ServerMsg::Start { map } => {
                net.started = true;
                status.say(format!("the game starts on {map}"));
            }
            ServerMsg::Turn { turn, commands } => {
                if turn != net.turns_done {
                    warn!("turn {turn} arrived, expected {}", net.turns_done);
                }
                for (owner, cmd) in &commands {
                    match world.apply(*owner, cmd, &data.scripts) {
                        Ok(a) if *owner == player.id => status.say(a.message),
                        Err(e) if *owner == player.id => status.say(e),
                        _ => {}
                    }
                    if let Some((path, replay)) = rec.record.as_mut() {
                        replay.record(world.tick, *owner, cmd.clone(), true);
                        let _ = replay.save(&*path);
                    }
                }
                for _ in 0..net.turn_ticks {
                    world.step(&data.scripts);
                    for ai in ais.iter_mut() {
                        let kit = islefall_sim::ai::AiKit { shooter: (ai.shooter.clone(), data.rules(&ai.shooter)), generator: (ai.generator.clone(), data.rules(&ai.generator)) };
                        ai.tick(world, &kit, &data.scripts);
                    }
                }
                net.turns_done = turn + 1;
                if net.hash_every_turns > 0 && (turn + 1) % net.hash_every_turns == 0 {
                    let hash = world.hash();
                    info!("turn {turn} (tick {}) hash {hash:016x}", world.tick);
                    net.send(ClientMsg::Hash { turn, hash });
                }
            }
            ServerMsg::Desync { turn, hashes } => {
                warn!("DESYNC at turn {turn}: {hashes:?}");
                status.say(format!("DESYNC at turn {turn}: the players' worlds differ"));
            }
            ServerMsg::Left(id) => status.say(format!("player {id} left")),
            ServerMsg::Pong(_) => {}
        }
    }
}

/// Keep the window title showing the Storm Power reserve and Knowledge.
fn title(sim: Res<Sim>, data: Res<GameData>, player: Res<Player>, mut windows: Query<&mut Window, With<PrimaryWindow>>, mut last: Local<Option<String>>) {
    let w = sim.world();
    let me = player.id as usize % data.cfg.sim.max_players;
    let now = match verdict(w, &data, player.id) {
        Some(v) => format!("Islefall - {v}"),
        None => format!("Islefall - Storm Power {} - Knowledge {} ({} techs)", w.powers[me], w.knowledge, w.known_tech[me].len()),
    };
    if last.as_deref() == Some(now.as_str()) {
        return;
    }
    if let Ok(mut win) = windows.single_mut() {
        win.title = now.clone();
    }
    *last = Some(now);
}

/// Fill the corner text from the rules' templates.
fn hud(sim: Res<Sim>, data: Res<GameData>, player: Res<Player>, status: Res<Status>, mut texts: Query<&mut Text, With<Hud>>) {
    let w = sim.world();
    let tool = match &player.tool {
        Tool::Empty => "nothing in hand".to_string(),
        Tool::Bridge(slot, _) => format!("bridge piece {} (slot {}); left-click drops it, right-click turns it", w.queue.slots.get(*slot).map(|p| p.name.as_str()).unwrap_or("?"), slot + 1),
        Tool::Drop(stem) | Tool::Spawn(stem) => stem.clone(),
    };
    let me = player.id as usize % data.cfg.sim.max_players;
    let opponents = w.players.iter().filter(|&&o| o != player.id && !w.out.contains(&o)).count();
    let fill = |t: &str| {
        t.replace("{power}", &w.powers[me].to_string())
            .replace("{knowledge}", &w.knowledge.to_string())
            .replace("{techs}", &w.known_tech[me].len().to_string())
            .replace("{tool}", &tool)
            .replace("{status}", &status.0)
            .replace("{opponents}", &opponents.to_string())
    };
    let mut lines: Vec<String> = data.cfg.hud.lines.iter().map(|l| fill(l)).collect();
    if let Some(v) = verdict(w, &data, player.id) {
        lines.push(v);
    }
    let text = lines.join("\n");
    for mut t in &mut texts {
        if t.0 != text {
            t.0 = text.clone();
        }
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
        let Some(shape) = lib.get_or_load(&data.install, palette, &unit.kind, &mut images, &mut layouts) else {
            warn!("unit {i}: no sprites for {}", unit.kind);
            continue;
        };
        let entities = spawn_animated(&mut commands, shape, unit.facing.animation(), unit_to_world(unit, g), Z_UNIT, false);
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
            if let Some(labels) = lib.labels(&unit.kind) {
                let seq = animation_sequence(labels, unit.facing.animation());
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
fn cursor_cell(windows: &Query<&Window, With<PrimaryWindow>>, cameras: &Query<(&Camera, &GlobalTransform), With<WorldCamera>>, g: &Grid) -> Option<Cell> {
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cameras.single()) else { return None };
    let cursor = window.cursor_position()?;
    let world_pos = camera.viewport_to_world_2d(cam_tf, cursor).ok()?;
    Some(world_to_cell(world_pos, g))
}

/// World position under the cursor.
fn cursor_world(windows: &Query<&Window, With<PrimaryWindow>>, cameras: &Query<(&Camera, &GlobalTransform), With<WorldCamera>>) -> Option<Vec2> {
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cameras.single()) else { return None };
    camera.viewport_to_world_2d(cam_tf, window.cursor_position()?).ok()
}

/// The unit whose drawn picture lies under `at`, nearest the front, among
/// those `wanted` accepts. A walker's picture stands above its cell, so
/// clicks land on the body, not the feet.
fn unit_under(at: Vec2, layers: &Query<(&UnitLayer, &GlobalTransform, &Sprite, &ShapeSprite)>, wanted: impl Fn(usize) -> bool) -> Option<usize> {
    let mut best: Option<(f32, usize)> = None;
    for (layer, tf, sprite, shape) in layers {
        if layer.shadow || !wanted(layer.unit) {
            continue;
        }
        let Some(atlas) = &sprite.texture_atlas else { continue };
        let Some(frame) = shape.frames.get(atlas.index) else { continue };
        let (w, h) = (frame.size.x as f32, frame.size.y as f32);
        let x0 = tf.translation().x - frame.hotspot.x;
        let top = tf.translation().y + frame.hotspot.y;
        if at.x >= x0 && at.x <= x0 + w && at.y <= top && at.y >= top - h && best.is_none_or(|(y, _)| tf.translation().y < y) {
            best = Some((tf.translation().y, layer.unit));
        }
    }
    best.map(|(_, i)| i)
}

#[allow(clippy::too_many_arguments)]
fn mouse_actions(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    data: Res<GameData>,
    mut player: ResMut<Player>,
    mut sim: ResMut<Sim>,
    mut status: ResMut<Status>,
    mut rec: ResMut<Recording>,
    layers: Query<(&UnitLayer, &GlobalTransform, &Sprite, &ShapeSprite)>,
) {
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if (!left && !right) || over_sidebar(&windows, &data) {
        return;
    }
    let Some(cell) = cursor_cell(&windows, &cameras, data.grid()) else { return };
    let Some(at) = cursor_world(&windows, &cameras) else { return };
    let w = sim.world_mut();
    // Something in hand: the left button drops it, the right turns a piece.
    if !matches!(player.tool, Tool::Empty) {
        if right {
            if let Tool::Bridge(slot, rotations) = &player.tool {
                player.tool = Tool::Bridge(*slot, (rotations + 1) % 4);
            }
            return;
        }
        let cmd = match player.tool.clone() {
            Tool::Bridge(slot, rotations) => Command::PlacePiece { slot, rotations, at: cell },
            Tool::Drop(stem) => Command::Drop { kind: stem, at: cell },
            Tool::Spawn(stem) => Command::PlaceUnit { kind: stem, at: cell },
            Tool::Empty => return,
        };
        if let Some(applied) = rec.issue(w, &data.scripts, &mut status, cmd) {
            if let Some(i) = applied.unit {
                player.selected = i;
            }
            // Placed: the hand is empty again, as the original's was.
            player.tool = Tool::Empty;
        }
        return;
    }
    if left {
        let sel = player.selected;
        // Selecting is local; everything else is a command.
        let me = player.id;
        let mine = unit_under(at, &layers, |i| w.units.get(i).is_some_and(|u| u.alive && u.owner == me && u.carried_by.is_none()));
        if let Some(i) = mine.or_else(|| w.units.iter().position(|u| u.alive && u.owner == me && u.carried_by.is_none() && u.cell() == cell)) {
            player.selected = i;
            status.say(format!("selected unit {i} ({})", w.units[i].kind));
            return;
        }
        let enemy_priest = unit_under(at, &layers, |i| w.units.get(i).is_some_and(|u| u.alive && u.owner != me && u.is_priest));
        let cmd = if let Some(p) = enemy_priest.or_else(|| w.units.iter().position(|u| u.alive && u.owner != me && u.is_priest && u.cell() == cell)) {
            Command::Capture { unit: sel, priest: p }
        } else if let Some(a) = w.structures.iter().position(|s| s.is_altar && s.owner == me && s.covers(cell)).filter(|_| w.units.get(sel).is_some_and(|u| u.carrying.is_some())) {
            Command::Sacrifice { unit: sel, altar: a }
        } else if let Some(o) = w.structures.iter().position(|s| s.is_obelisk && s.covers(cell)) {
            Command::Read { unit: sel, obelisk: o }
        } else if let Some(g) = w.structures.iter().position(|s| s.stock > 0 && s.covers(cell)) {
            Command::Harvest { unit: sel, geyser: g }
        } else {
            Command::Move { unit: sel, to: cell }
        };
        rec.issue(w, &data.scripts, &mut status, cmd);
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

fn tool_keys(keys: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>, data: Res<GameData>, mut player: ResMut<Player>, mut status: ResMut<Status>, mut rec: ResMut<Recording>) {
    if keys.just_pressed(KeyCode::KeyX) || keys.just_pressed(KeyCode::KeyY) {
        let sel = player.selected;
        let cmd = if keys.just_pressed(KeyCode::KeyX) { Command::Cast { unit: sel } } else { Command::Pray { unit: sel } };
        rec.issue(sim.world_mut(), &data.scripts, &mut status, cmd);
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        if let Tool::Bridge(slot, rotations) = &player.tool {
            player.tool = Tool::Bridge(*slot, (rotations + 1) % 4);
        }
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if !matches!(player.tool, Tool::Empty) {
            player.tool = Tool::Empty;
            status.say("put back".to_string());
        }
        return;
    }
    let slot_keys: Vec<Option<KeyCode>> = data.cfg.controls.slot_keys.iter().map(|k| key_code(k)).collect();
    if let Some(slot) = slot_keys.iter().position(|k| k.is_some_and(|k| keys.just_pressed(k))).filter(|&i| i < sim.world().queue.slots.len()) {
        player.tool = Tool::Bridge(slot, 0);
        status.say(format!("tool: bridge piece {} from slot {}", sim.world().queue.slots[slot].name, slot + 1));
        return;
    }
    let unit_key = data.cfg.controls.unit_keys.iter().position(|k| key_code(k).is_some_and(|k| keys.just_pressed(k)));
    let build_key = DIGIT_KEYS.iter().position(|k| keys.just_pressed(*k)).filter(|&i| i < data.cfg.controls.build_tools.len());
    let (stem, spawn) = match (unit_key, build_key) {
        (Some(i), _) => (data.cfg.controls.unit_tools[i].clone(), true),
        (None, Some(i)) => (data.cfg.controls.build_tools[i].clone(), false),
        _ => return,
    };
    // Picking a Battle unit puts it into production at a Workshop, as the manual's menu would.
    let rules = data.rules(&stem);
    if rules.energy.is_some() && !sim.world().in_production(player.id, &stem) {
        rec.issue(sim.world_mut(), &data.scripts, &mut status, Command::Produce { kind: stem.clone() });
    }
    player.tool = if spawn { Tool::Spawn(stem) } else { Tool::Drop(stem) };
    info!("tool: {:?}", player.tool);
}

/// Crack, harden, destroy or salvage under the cursor.
fn bridge_keys(
    mut status: ResMut<Status>,
    mut rec: ResMut<Recording>,
    player: Res<Player>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    data: Res<GameData>,
    mut sim: ResMut<Sim>,
) {
    let (crack, harden, destroy) = (keys.just_pressed(KeyCode::KeyC), keys.just_pressed(KeyCode::KeyH), keys.just_pressed(KeyCode::Delete));
    let salvage = keys.just_pressed(KeyCode::KeyV);
    let upgrade = keys.just_pressed(KeyCode::KeyG);
    if !crack && !harden && !destroy && !salvage && !upgrade {
        return;
    }
    let Some(cell) = cursor_cell(&windows, &cameras, data.grid()) else { return };
    let w = sim.world_mut();
    let cmd = if upgrade {
        let Some(i) = w.structures.iter().position(|s| s.owner == player.id && s.slots > 0 && s.covers(cell)) else { return };
        Command::Upgrade { structure: i }
    } else if salvage {
        let Some(i) = w.structures.iter().position(|s| s.owner == player.id && s.covers(cell)) else { return };
        Command::Salvage { structure: i }
    } else if crack {
        Command::CrackBridge { at: cell }
    } else if harden {
        Command::HardenBridge { at: cell }
    } else {
        Command::DestroyBridge { at: cell }
    };
    rec.issue(w, &data.scripts, &mut status, cmd);
}

/// Show the current piece or footprint under the cursor, green when it can go there.
fn ghost(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    data: Res<GameData>,
    player: Res<Player>,
    sim: Res<Sim>,
    existing: Query<Entity, With<Ghost>>,
    white: Res<Solid>,
) {
    for e in &existing {
        commands.entity(e).despawn();
    }
    let g = data.grid();
    if over_sidebar(&windows, &data) {
        return;
    }
    let Some(cell) = cursor_cell(&windows, &cameras, g) else { return };
    let w = sim.world();
    let (cells, ok) = match &player.tool {
        Tool::Empty => return,
        Tool::Bridge(slot, rotations) => {
            let Some(mut piece) = w.queue.slots.get(*slot).cloned() else { return };
            for _ in 0..(*rotations % 4) {
                piece = piece.rotated();
            }
            let cells = piece.cells_at(cell);
            let ok = w.can_place_piece_for(player.id, &cells).is_ok();
            (cells, ok)
        }
        Tool::Drop(stem) => {
            let rules = data.rules(stem);
            let probe = islefall_sim::Structure::new(stem.as_str(), cell, rules.foot_x, rules.foot_y, islefall_sim::Walk::Free);
            let ok = w.can_drop(&rules, cell).is_ok()
                && w.check_ownership(player.id, &rules, cell).is_ok()
                && rules.tech_bit.is_none_or(|b| w.knows_tech(player.id, b))
                && w.check_energy(player.id, &rules, cell).is_ok()
                && w.check_production(player.id, stem, &rules).is_ok();
            (probe.cells().collect(), ok)
        }
        Tool::Spawn(stem) => {
            let rules = data.rules(stem);
            let ok = (rules.is_air || w.is_walkable(cell))
                && rules.tech_bit.is_none_or(|b| w.knows_tech(player.id, b))
                && w.check_energy(player.id, &rules, cell).is_ok()
                && w.check_production(player.id, stem, &rules).is_ok();
            (vec![cell], ok)
        }
    };
    let color = if ok { Color::srgba(0.2, 1.0, 0.2, 0.35) } else { Color::srgba(1.0, 0.2, 0.2, 0.35) };
    for c in cells {
        let (x, y) = c.centre_px(g);
        commands.spawn((
            solid(&white, color, Vec2::new(g.cell_w as f32, g.cell_h as f32)),
            Transform::from_translation(Vec3::new(x as f32, -(y as f32), 50.0)),
            Ghost,
        ));
    }
}

/// Draw a health bar over every damaged structure and unit, a ring around
/// stunned priests, and a flash on the target of each shot fired this tick.
#[allow(clippy::too_many_arguments)]
fn overlays(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    player: Res<Player>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    existing: Query<Entity, With<Overlay>>,
    time: Res<Time>,
    mut tracers: Local<Vec<(Vec2, Vec2, f32)>>,
    mut landed: ResMut<Landed>,
    white: Res<Solid>,
) {
    for e in &existing {
        commands.entity(e).despawn();
    }
    let w = sim.world();
    let g = data.grid();
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    // The selected unit: a pulsing ring on the ground round its feet.
    if let Some(u) = w.units.get(player.selected).filter(|u| u.alive && u.owner == player.id) {
        let hud = &data.cfg.hud;
        let pulse = 0.65 + 0.35 * (time.elapsed_secs() * hud.selection_pulse * std::f32::consts::TAU).sin();
        let c = Color::srgba(hud.selection_colour[0], hud.selection_colour[1], hud.selection_colour[2], pulse);
        let feet = unit_to_world(u, g);
        let (rw, rh) = (hud.selection_ring[0] / 2.0, hud.selection_ring[1] / 2.0);
        let dots = 28;
        for k in 0..dots {
            let a = k as f32 / dots as f32 * std::f32::consts::TAU;
            let (x, y) = (feet.x + rw * a.cos(), feet.y + rh * a.sin());
            commands.spawn((solid(&white, c, Vec2::new(2.0, 2.0)), Transform::from_translation(Vec3::new(x, y, Z_UNIT - 0.5)), Overlay));
        }
    }
    // Spell icons over their bearers, the selected caster's reach, and casting, prayer, paralysis and invisibility marks.
    for (i, u) in w.units.iter().enumerate() {
        if !u.alive || u.carried_by.is_some() {
            continue;
        }
        let p = unit_to_world(u, g);
        if let Some(spell) = &u.spell {
            if let Some(shape) = lib.get_or_load(&data.install, palette, spell, &mut images, &mut layouts) {
                let frame = data.install.type_def(spell).and_then(|d| d.frames.iter().position(|f| f.has_flag("baseframe") || f.has_flag("default"))).unwrap_or(0);
                if frame < shape.frames.len() {
                    let e = world::spawn_frame(&mut commands, shape, frame, Vec2::new(p.x, p.y + 30.0), 62.0);
                    commands.entity(e).insert(Overlay);
                }
                if i == player.selected && u.owner == player.id {
                    let range = data.rules(spell).spell_range;
                    let size = Vec2::new(((2 * range + 1) * g.cell_w) as f32, ((2 * range + 1) * g.cell_h) as f32);
                    commands.spawn((solid(&white, Color::srgba(0.6, 0.4, 1.0, 0.12), size), Transform::from_translation(Vec3::new(p.x, p.y + g.cell_h as f32 / 2.0, 58.0)), Overlay));
                }
            }
        }
        let mark = if u.casting > 0 || u.praying > 0 {
            Some(Color::srgba(1.0, 1.0, 0.6, 0.6))
        } else if u.paralysed > 0 {
            Some(Color::srgba(0.4, 0.9, 1.0, 0.5))
        } else if u.invisible > 0 {
            Some(Color::srgba(1.0, 1.0, 1.0, 0.25))
        } else {
            None
        };
        if let Some(c) = mark {
            commands.spawn((solid(&white, c, Vec2::new(16.0, 16.0)), Transform::from_translation(Vec3::new(p.x, p.y + 8.0, 59.5)), Overlay));
        }
    }
    let bar = |commands: &mut Commands, pos: Vec2, width: f32, frac: f32| {
        let back = Color::srgba(0.1, 0.1, 0.1, 0.8);
        let front = if frac > 0.6 { Color::srgb(0.2, 0.9, 0.2) } else if frac > 0.3 { Color::srgb(0.9, 0.9, 0.2) } else { Color::srgb(0.9, 0.2, 0.2) };
        commands.spawn((solid(&white, back, Vec2::new(width, 3.0)), Transform::from_translation(pos.extend(60.0)), Overlay));
        let wdt = (width - 1.0) * frac.clamp(0.0, 1.0);
        commands.spawn((
            solid(&white, front, Vec2::new(wdt.max(0.5), 2.0)),
            Transform::from_translation(Vec3::new(pos.x - (width - 1.0 - wdt) / 2.0, pos.y, 60.1)),
            Overlay,
        ));
    };
    for s in &w.structures {
        let (x, y) = s.cell.top_left_px(g);
        let width = (s.foot_x * g.cell_w) as f32;
        let top = Vec2::new(x as f32 + g.cell_w as f32 - width / 2.0, -(y as f32 - ((s.foot_y - 1) * g.cell_h) as f32) + 4.0);
        if !s.complete() {
            // Build progress in blue while the stream works.
            let frac = s.progress();
            commands.spawn((solid(&white, Color::srgba(0.1, 0.1, 0.1, 0.8), Vec2::new(width, 3.0)), Transform::from_translation(top.extend(60.0)), Overlay));
            let wdt = (width - 1.0) * frac.clamp(0.0, 1.0);
            commands.spawn((
                solid(&white, Color::srgb(0.3, 0.6, 1.0), Vec2::new(wdt.max(0.5), 2.0)),
                Transform::from_translation(Vec3::new(top.x - (width - 1.0 - wdt) / 2.0, top.y, 60.1)),
                Overlay,
            ));
        } else if s.max_hp > 0 && s.hp < s.max_hp {
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
                solid(&white, Color::srgba(1.0, 0.95, 0.3, 0.45), Vec2::new(18.0, 12.0)),
                Transform::from_translation(Vec3::new(p.x, p.y - 2.0, 59.0)),
                Overlay,
            ));
        }
    }
    // Flashes where shots have landed.
    let dt = time.delta_secs();
    tracers.retain_mut(|t| {
        t.2 -= dt;
        t.2 > 0.0
    });
    for &(_, to, left) in tracers.iter() {
        let alpha = (left / data.cfg.projectiles.flash_seconds.max(0.01)).clamp(0.0, 1.0);
        commands.spawn((solid(&white, Color::srgba(1.0, 0.8, 0.3, alpha), Vec2::new(7.0, 7.0)), Transform::from_translation(to.extend(61.0)), Overlay));
    }
    for hit in landed.0.drain(..) {
        tracers.push((hit, hit, data.cfg.projectiles.flash_seconds));
    }
}

/// Draw the Energy circle of every source the player owns whenever structures change.
#[allow(clippy::too_many_arguments)]
fn sync_energy_rings(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    player: Res<Player>,
    ring: Res<RingImage>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
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
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    let energy = &data.cfg.energy;
    for s in w.structures.iter().filter(|s| s.owner == player.id && s.produces.is_some() && s.complete()) {
        let (x, y) = s.centre().centre_px(data.grid());
        let centre = Vec2::new(x as f32, -(y as f32));
        // The original's twinkling stars in the theme's colour, else a generated one.
        let theme_name = theme_name(s.produces.unwrap_or(s.theme));
        let star = energy.star_type.as_deref().and_then(|t| {
            let label = energy.star_labels.get(theme_name)?;
            let shape = lib.get_or_load(&data.install, palette, t, &mut images, &mut layouts)?;
            let seq: Vec<usize> = (0..shape.labels.len()).filter(|&i| shape.labels[i].eq_ignore_ascii_case(label)).collect();
            (!seq.is_empty()).then(|| (shape.image.clone(), shape.layout.clone(), shape.frames.clone(), seq))
        });
        for index in 0..energy.stars {
            match &star {
                Some((image, layout, frames, seq)) => {
                    let first = seq[(index as usize) % seq.len()];
                    commands.spawn((
                        Sprite::from_atlas_image(image.clone(), TextureAtlas { layout: layout.clone(), index: first }),
                        frames[first].anchor(),
                        Transform::from_translation(centre.extend(0.9)),
                        ShapeSprite { sequence: seq.clone(), frames: frames.clone(), step: (index as usize) % seq.len(), playing: true, once: false, rest: None },
                        EnergyRing { centre, index },
                    ));
                }
                None => {
                    commands.spawn((Sprite::from_image(ring.0.clone()), Transform::from_translation(centre.extend(0.9)), EnergyRing { centre, index }));
                }
            }
        }
    }
}

fn theme_name(t: Theme) -> &'static str {
    match t {
        Theme::Sun => "sun",
        Theme::Thunder => "thunder",
        Theme::Wind => "wind",
        Theme::Rain => "rain",
    }
}

/// The stars circle their source, one turn every `star_seconds`.
fn drift_stars(time: Res<Time>, data: Res<GameData>, mut stars: Query<(&EnergyRing, &mut Transform)>) {
    let count = data.cfg.energy.stars.max(1) as f32;
    let radius = data.cfg.energy.range_px as f32;
    let turn = time.elapsed_secs() / data.cfg.energy.star_seconds.max(0.1) * std::f32::consts::TAU;
    for (star, mut tf) in &mut stars {
        let a = turn + star.index as f32 / count * std::f32::consts::TAU;
        tf.translation.x = star.centre.x + radius * a.cos();
        tf.translation.y = star.centre.y + radius * a.sin();
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
    mut existing: Query<(Entity, &world::StructureKey, &mut StructureSprite)>,
    mut seen: Local<Option<u64>>,
) {
    let w = sim.world();
    if *seen == Some(w.structure_version) {
        return;
    }
    *seen = Some(w.structure_version);
    // Sprites of structures still standing with the same look stay, so a
    // turret keeps its bearing and a burning shell its flames; the rest go,
    // and structures without a sprite get one.
    let index_of: HashMap<u32, usize> = w.structures.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let mut drawn: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for (e, key, mut sprite) in &mut existing {
        match index_of.get(&key.id) {
            Some(&i) if world::StructureKey::of(&w.structures[i]) == *key => {
                sprite.0 = i;
                drawn.insert(key.id);
            }
            _ => commands.entity(e).despawn(),
        }
    }
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    for i in 0..w.structures.len() {
        if !drawn.contains(&w.structures[i].id) {
            world::spawn_structure(&mut commands, &data.install, &mut lib, palette, &mut images, &mut layouts, w, i);
        }
    }
}

/// A shell under construction shows through until its stream has built it.
fn tint_shells(sim: Res<Sim>, mut sprites: Query<(&StructureSprite, &mut Sprite)>) {
    let w = sim.world();
    for (s, mut sprite) in &mut sprites {
        let Some(st) = w.structures.get(s.0) else { continue };
        let alpha = if st.complete() { 1.0 } else { 0.3 + 0.7 * st.progress() };
        let wanted = Color::srgba(1.0, 1.0, 1.0, alpha);
        if sprite.color != wanted {
            sprite.color = wanted;
        }
    }
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
    mut cameras: Query<(&mut Transform, &mut Projection), With<WorldCamera>>,
) {
    let Ok((mut tf, mut proj)) = cameras.single_mut() else { return };
    let mut d = Vec2::ZERO;
    if keys.any_pressed([KeyCode::ArrowLeft]) {
        d.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::ArrowRight]) {
        d.x += 1.0;
    }
    if keys.any_pressed([KeyCode::ArrowUp]) {
        d.y += 1.0;
    }
    if keys.any_pressed([KeyCode::ArrowDown]) {
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
        if shape.once && shape.step + 1 >= shape.sequence.len() {
            shape.playing = false;
            if let Some(rest) = shape.rest.take() {
                shape.sequence = vec![rest];
                shape.step = 0;
                atlas.index = rest;
                *anchor = shape.frames[rest].anchor();
            }
            continue;
        }
        shape.step = (shape.step + 1) % shape.sequence.len().max(1);
        let frame = shape.sequence[shape.step];
        atlas.index = frame;
        *anchor = shape.frames[frame].anchor();
    }
}

/// Launch a missile for every shot of a new tick and fly the ones in the
/// air: a lob from muzzle to target, spinning through its frames or turned
/// to its bearing, and a flash where it lands.
#[allow(clippy::too_many_arguments)]
fn projectiles(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    time: Res<Time>,
    mut lib: ResMut<ShapeLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut landed: ResMut<Landed>,
    mut flying: Query<(Entity, &mut Projectile, &mut Transform, &mut Sprite, &mut Anchor, &ShapeSprite)>,
    mut seen_tick: Local<Option<u64>>,
) {
    let w = sim.world();
    let g = data.grid();
    let rules = &data.cfg.projectiles;
    if *seen_tick != Some(w.tick) {
        *seen_tick = Some(w.tick);
        let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
        for &(shooter, target) in &w.last_shots {
            let Some(s) = w.structures.get(shooter) else { continue };
            let to = match target {
                islefall_sim::world::Target::Structure(j) => w.structures.get(j).map(|t| cell_to_world(t.centre(), g)),
                islefall_sim::world::Target::Unit(j) => w.units.get(j).map(|u| unit_to_world(u, g)),
                islefall_sim::world::Target::Bridge(c) => Some(cell_to_world(c, g)),
            };
            let Some(to) = to else { continue };
            let from = cell_to_world(s.centre(), g) + Vec2::new(0.0, g.cell_h as f32);
            let missile = rules.types.get(&s.kind).cloned().unwrap_or_else(|| rules.default.clone());
            let Some(shape) = lib.get_or_load(&data.install, palette, &missile, &mut images, &mut layouts) else { continue };
            let sequence: Vec<usize> = (0..shape.labels.len())
                .filter(|&i| !shape.flags[i].iter().any(|f| data.cfg.animation.hidden_flags.iter().any(|h| h.eq_ignore_ascii_case(f))))
                .collect();
            if sequence.is_empty() {
                continue;
            }
            // Four runs of frames, one per direction: keep the run of this flight's.
            let sequence = match rules.cardinal_frames.get(&missile) {
                Some(&per) if per > 0 && sequence.len() >= per * rules.cardinal_order.len() => {
                    let d = to - from;
                    let dir = if d.x.abs() >= d.y.abs() { if d.x >= 0.0 { "east" } else { "west" } } else if d.y >= 0.0 { "north" } else { "south" };
                    let run = rules.cardinal_order.iter().position(|o| o.eq_ignore_ascii_case(dir)).unwrap_or(0);
                    sequence[run * per..(run + 1) * per].to_vec()
                }
                _ => sequence,
            };
            let bearings = sequence.len() >= rules.bearing_min_frames;
            let seconds = ((to - from).length() / rules.speed_px.max(1.0)).max(0.05);
            debug!("tick {}: {} fires {} at {:?}", w.tick, s.kind, missile, target);
            let first = sequence[0];
            commands.spawn((
                Sprite::from_atlas_image(shape.image.clone(), TextureAtlas { layout: shape.layout.clone(), index: first }),
                shape.frames[first].anchor(),
                Transform::from_translation(from.extend(Z_UNIT + 2.0)),
                ShapeSprite { sequence, frames: shape.frames.clone(), step: 0, playing: !bearings, once: false, rest: None },
                Projectile { from, to, progress: 0.0, seconds, bearings },
            ));
        }
    }
    let dt = time.delta_secs();
    let rules_fx = &data.cfg.effects;
    for (e, mut p, mut tf, mut sprite, mut anchor, shape) in &mut flying {
        p.progress += dt / p.seconds;
        if p.progress >= 1.0 {
            match &rules_fx.impact {
                Some(hit) => spawn_effect(&mut commands, &data, &mut lib, &mut images, &mut layouts, hit, p.to, Z_UNIT + 2.5),
                None => landed.0.push(p.to),
            }
            commands.entity(e).despawn();
            continue;
        }
        let d = p.to - p.from;
        let lob = rules.arc_px * 4.0 * p.progress * (1.0 - p.progress);
        let at = p.from + d * p.progress + Vec2::new(0.0, lob);
        tf.translation.x = at.x;
        tf.translation.y = at.y;
        if p.bearings {
            // Frame 0 points north; the rest turn clockwise on screen.
            let n = shape.sequence.len() as f32;
            let bearing = d.x.atan2(d.y).rem_euclid(std::f32::consts::TAU);
            let frame = shape.sequence[((bearing / std::f32::consts::TAU * n).round() as usize) % shape.sequence.len()];
            if let Some(atlas) = sprite.texture_atlas.as_mut() {
                if atlas.index != frame {
                    atlas.index = frame;
                    *anchor = shape.frames[frame].anchor();
                }
            }
        }
    }
}

/// A flame or a puff of smoke rising from a damaged structure.
#[derive(Component)]
struct Ember {
    life: f32,
    seconds: f32,
    rise: f32,
    smoke: bool,
}

/// Damaged structures burn: flames start at random points of the
/// footprint at a rate set by the damage, rise, fade, and leave smoke.
fn burning(mut commands: Commands, sim: Res<Sim>, data: Res<GameData>, time: Res<Time>, mut embers: Query<(Entity, &mut Ember, &mut Transform, &mut Sprite)>, mut rng: Local<Option<Pcg32>>, white: Res<Solid>) {
    let w = sim.world();
    let fx = &data.cfg.effects;
    let g = data.grid();
    let dt = time.delta_secs();
    let rng = rng.get_or_insert_with(|| Pcg32::seed_from_u64(0x2545_F491_4F6C_DD1D));
    let mut next = || rng.random::<f32>();
    for s in &w.structures {
        if s.max_hp == 0 || !s.complete() {
            continue;
        }
        let share = s.hp as f32 / s.max_hp as f32;
        let per_cell = if share < fx.blazing_below { fx.blaze_per_cell } else if share < fx.burning_below { fx.flames_per_cell } else { continue };
        let cells = (s.foot_x * s.foot_y) as f32;
        let mut expected = per_cell * cells * dt;
        while expected > 0.0 {
            if next() > expected.min(1.0) {
                break;
            }
            expected -= 1.0;
            // The footprint runs up and left from the hotspot cell.
            let (hx, hy) = s.cell.top_left_px(g);
            let left = hx as f32 - ((s.foot_x - 1) * g.cell_w) as f32;
            let top = hy as f32 - ((s.foot_y - 1) * g.cell_h) as f32;
            let x = left + next() * (s.foot_x * g.cell_w) as f32;
            let y = -(top + next() * (s.foot_y * g.cell_h) as f32);
            let size = 2.0 + next() * 2.0;
            commands.spawn((
                solid(&white, Color::srgb(1.0, 0.85, 0.3), Vec2::splat(size)),
                Transform::from_translation(Vec3::new(x, y, Z_UNIT + 1.0)),
                Ember { life: 0.0, seconds: fx.flame_seconds * (0.7 + next() * 0.6), rise: fx.flame_rise_px, smoke: false },
            ));
            if next() < fx.smoke_per_flame {
                commands.spawn((
                    solid(&white, Color::srgba(0.3, 0.3, 0.3, 0.5), Vec2::splat(size + 2.0)),
                    Transform::from_translation(Vec3::new(x + next() * 4.0 - 2.0, y + 6.0, Z_UNIT + 0.9)),
                    Ember { life: 0.0, seconds: fx.smoke_seconds * (0.7 + next() * 0.6), rise: fx.smoke_rise_px, smoke: true },
                ));
            }
        }
    }
    for (e, mut ember, mut tf, mut sprite) in &mut embers {
        ember.life += dt;
        let t = ember.life / ember.seconds;
        if t >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation.y += ember.rise * dt;
        sprite.color = if ember.smoke {
            Color::srgba(0.3, 0.3, 0.3, 0.45 * (1.0 - t))
        } else {
            // Yellow at birth, orange, then red as it dies.
            Color::srgba(1.0, 0.9 - 0.7 * t, 0.3 * (1.0 - t), 1.0 - t * t)
        };
    }
}

/// Give each new structure sprite its idle animation, and turn turrets to
/// their targets: an all-round turret swings along its ring of bearings
/// to the target and stays there, a cannon plays its direction's sequence
/// once per shot and rests facing that way, a thrower spins its arm once.
#[allow(clippy::type_complexity)]
fn animate_structures(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    fresh: Query<(Entity, &StructureFrames), Without<ShapeSprite>>,
    mut turrets: Query<(&StructureSprite, &StructureFrames, &mut ShapeSprite, &mut Sprite, &mut Anchor, Option<&mut Turret>)>,
    mut bases: Query<(&StructureSprite, &mut Sprite, &mut Anchor), (With<StructureBase>, Without<StructureFrames>)>,
    mut seen_tick: Local<Option<u64>>,
) {
    for (e, plan) in &fresh {
        commands.entity(e).insert(ShapeSprite { sequence: plan.sequence.clone(), frames: plan.frames.clone(), step: 0, playing: plan.sequence.len() > 1, once: false, rest: None });
        if !plan.ring.is_empty() {
            let pos = plan.ring.iter().position(|&f| f == plan.sequence[0]).unwrap_or(0);
            commands.entity(e).insert(Turret { pos, goal: pos });
        }
    }
    let w = sim.world();
    let fired: Vec<usize> = if *seen_tick == Some(w.tick) {
        Vec::new()
    } else {
        *seen_tick = Some(w.tick);
        w.last_shots.iter().map(|&(i, _)| i).collect()
    };
    let g = data.grid();
    for (s, plan, mut shape, mut sprite, mut anchor, turret) in &mut turrets {
        let Some(st) = w.structures.get(s.0) else { continue };
        if plan.stages > 1 && plan.sequence.len() >= plan.stages as usize {
            // A well shows the run of its remaining stock: full, then half, then low.
            let share = if st.cost > 0 { st.stock as f32 / st.cost as f32 } else { 1.0 };
            let stage = (((1.0 - share.clamp(0.0, 1.0)) * plan.stages as f32).floor() as usize).min(plan.stages as usize - 1);
            let run = plan.sequence.len() / plan.stages as usize;
            let frames = &plan.sequence[stage * run..(stage + 1) * run];
            if shape.sequence != frames {
                shape.sequence = frames.to_vec();
                shape.step = 0;
                shape.playing = frames.len() > 1;
            }
        }
        if !fired.contains(&s.0) {
            continue;
        }
        let (cx, cy) = st.centre().centre_px(g);
        let aim = st.aim.map(|a| a.centre_px(g)).map(|(ax, ay)| ((ax - cx) as f32, (ay - cy) as f32));
        if !plan.fire.is_empty() {
            // The arm spins a throw over the dome, then rests.
            let rest = plan.base.unwrap_or(plan.sequence[0]);
            play_once(&mut shape, &mut sprite, &mut anchor, plan.fire.clone(), rest);
        } else if let (Some(mut turret), Some((dx, dy))) = (turret, aim) {
            // Swing to the target's bearing; fire on arrival.
            let goal = nearest_bearing(&plan.ring_bearings, bearing_deg(dx, dy));
            turret.goal = goal;
            if turret.pos == goal {
                fire_on_ring(plan, goal, &mut shape, &mut sprite, &mut anchor);
            }
        } else if let (Some((dx, dy)), false) = (aim, plan.cardinal.is_empty()) {
            let dir = if dx.abs() >= dy.abs() { if dx >= 0.0 { "east" } else { "west" } } else if dy >= 0.0 { "south" } else { "north" };
            if let Some(seq) = plan.cardinal.get(dir).cloned() {
                let rest = seq[0];
                // The picture underneath, for directions whose frames are sparks over it.
                if let Some((_, mut bs, mut ba)) = bases.iter_mut().find(|(b, _, _)| b.0 == s.0) {
                    show_frame(&mut bs, &mut ba, &plan.frames, rest);
                }
                play_once(&mut shape, &mut sprite, &mut anchor, seq, rest);
            }
        }
    }
}

/// Swing every all-round turret a step along its ring towards its goal,
/// once per animation tick, and fire when it gets there.
fn turn_turrets(
    viewer: Res<Viewer>,
    data: Res<GameData>,
    mut turrets: Query<(&mut Turret, &StructureFrames, &mut ShapeSprite, &mut Sprite, &mut Anchor)>,
) {
    if !viewer.timer.just_finished() {
        return;
    }
    let fps = data.cfg.controls.animation_fps.max(0.1);
    let seconds = data.cfg.animation.turn_seconds.max(0.01);
    for (mut turret, plan, mut shape, mut sprite, mut anchor) in &mut turrets {
        let n = plan.ring.len();
        let (pos, goal) = (turret.pos, turret.goal);
        if n == 0 || pos == goal {
            continue;
        }
        // Frames per tick for a quarter turn to take `turn_seconds`.
        let step = ((n as f32 / 4.0 / (fps * seconds)).round() as usize).max(1);
        let clockwise = (goal + n - pos) % n;
        let counter = (pos + n - goal) % n;
        let next = if clockwise <= counter {
            if clockwise <= step { goal } else { (pos + step) % n }
        } else if counter <= step {
            goal
        } else {
            (pos + n - step) % n
        };
        turret.pos = next;
        if next == goal {
            fire_on_ring(plan, goal, &mut shape, &mut sprite, &mut anchor);
        } else {
            shape.sequence = vec![plan.ring[next]];
            shape.step = 0;
            shape.playing = false;
            shape.once = false;
            shape.rest = None;
            show_frame(&mut sprite, &mut anchor, &plan.frames, plan.ring[next]);
        }
    }
}

/// Bearing in degrees clockwise from north of a screen offset (y down).
fn bearing_deg(dx: f32, dy: f32) -> f32 {
    dx.atan2(-dy).to_degrees().rem_euclid(360.0)
}

/// Index of the bearing nearest to `bearing` round the circle.
fn nearest_bearing(bearings: &[f32], bearing: f32) -> usize {
    let apart = |b: f32| {
        let d = (b - bearing).rem_euclid(360.0);
        d.min(360.0 - d)
    };
    (0..bearings.len()).min_by(|&a, &b| apart(bearings[a]).total_cmp(&apart(bearings[b]))).unwrap_or(0)
}

/// A turret that has arrived at ring position `at` fires: a direction
/// group's frames play once and it rests on the position's frame; a frame
/// between directions just shows.
fn fire_on_ring(plan: &StructureFrames, at: usize, shape: &mut ShapeSprite, sprite: &mut Sprite, anchor: &mut Anchor) {
    let frame = plan.ring[at];
    match plan.cardinal.values().find(|seq| seq.contains(&frame)) {
        Some(seq) => play_once(shape, sprite, anchor, seq.clone(), frame),
        None => {
            shape.sequence = vec![frame];
            shape.step = 0;
            shape.playing = false;
            shape.once = false;
            shape.rest = None;
            show_frame(sprite, anchor, &plan.frames, frame);
        }
    }
}

/// Play `seq` through once from its first frame, then show `rest`.
fn play_once(shape: &mut ShapeSprite, sprite: &mut Sprite, anchor: &mut Anchor, seq: Vec<usize>, rest: usize) {
    let first = seq[0];
    shape.sequence = seq;
    shape.step = 0;
    shape.playing = true;
    shape.once = true;
    shape.rest = Some(rest);
    show_frame(sprite, anchor, &shape.frames, first);
}

fn show_frame(sprite: &mut Sprite, anchor: &mut Anchor, frames: &[FrameInfo], frame: usize) {
    if let Some(atlas) = sprite.texture_atlas.as_mut() {
        atlas.index = frame;
    }
    *anchor = frames[frame].anchor();
}

/// F5 saves the world as a snapshot, F9 restores it.
fn save_keys(keys: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>, data: Res<GameData>, mut status: ResMut<Status>) {
    let path = data.data_dir.join(&data.cfg.controls.save_file);
    if keys.just_pressed(KeyCode::F5) {
        let Some(w) = sim.world.as_ref() else { return };
        match std::fs::write(&path, w.snapshot()) {
            Ok(()) => status.say(format!("saved tick {} to {}", w.tick, path.display())),
            Err(e) => status.say(format!("cannot save to {}: {e}", path.display())),
        }
    } else if keys.just_pressed(KeyCode::F9) {
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| World::restore(&b)) {
            Ok(w) => {
                status.say(format!("loaded tick {} from {}", w.tick, path.display()));
                sim.world = Some(w);
            }
            Err(e) => status.say(format!("cannot load {}: {e}", path.display())),
        }
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
