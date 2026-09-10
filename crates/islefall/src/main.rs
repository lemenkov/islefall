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
use islefall_data::Installation;
use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, GlobalVolume, PlaybackSettings, Volume};
use islefall_sim::config::Grid;
use islefall_sim::map::{self, MapDef};
use islefall_sim::{Ai, AiMove, Applied, Cell, Command, Config, Dir8, IslandMap, Replay, Scripts, TypeRules, Unit, World};
use sprites::FrameInfo;
use world::{BridgeTile, LoadedShape, ShapeLibrary, StructureFrames, StructureSprite, TerrainTile, Z_SHADOW, Z_SKY, Z_STRUCTURE, Z_UNIT};

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
fn drift_sky(time: Res<Time>, cameras: Query<&Transform, (With<Camera2d>, Without<SkyLayer>)>, mut layers: Query<(&SkyLayer, &mut Transform)>) {
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
    fn of(cameras: &Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>) -> Option<View> {
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
    rng: u64,
}

impl Sky {
    fn next(&mut self) -> u64 {
        // xorshift; presentation only, never the simulation.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn wait(&mut self, range: [f32; 2]) -> f32 {
        let t = (self.next() % 10_000) as f32 / 10_000.0;
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
    /// Stop on the last frame instead of looping.
    once: bool,
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

/// Marks per-frame overlay sprites: health bars and shot flashes.
#[derive(Component)]
struct Overlay;

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
    let sky_first = cfg.sounds.ambient.sky_seconds[0];
    let background = cfg.sky.background;
    let mut recording = Recording::default();
    if let Ok(addr) = std::env::var("ISLEFALL_JOIN") {
        let name = std::env::var("ISLEFALL_NAME").unwrap_or_else(|_| whoami());
        let mut bytes = std::fs::read(data.join("rules.toml")).unwrap_or_default();
        bytes.extend(std::fs::read(data.join("scripts/rules.rhai")).unwrap_or_default());
        bytes.extend(std::fs::read(&map_path).unwrap_or_default());
        let hello = islefall_net::ClientMsg::Hello { protocol: islefall_net::PROTOCOL, name: name.clone(), map: map_name.clone(), data_hash: islefall_net::fnv64(&bytes) };
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
        rng: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x9E37_79B9_7F4A_7C15) | 1,
    })
    .insert_resource(mode)
    .insert_resource(Time::<Fixed>::from_hz(tick_hz as f64))
    .init_resource::<ShapeLibrary>()
    .init_resource::<Sim>()
    .init_resource::<Status>()
    .insert_resource(recording)
    .insert_resource(Player { tool: Tool::Spawn(unit_tool), selected: 0, id: 0 })
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
    .add_systems(Update, (tint_shells, animate_structures, drift_stars).after(sync_structures).run_if(resource_equals(Mode::Island)))
    .add_systems(Update, (play_sounds.after(mouse_actions).after(bridge_keys), sync_loops, ambient, footsteps.after(animate), drift_sky.after(camera_keys)).run_if(resource_equals(Mode::Island)))
    .add_systems(Update, viewer_keys.run_if(resource_equals(Mode::Viewer)));
    if let Ok(path) = std::env::var("ISLEFALL_SCREENSHOT") {
        app.insert_resource(AutoScreenshot { path, delay: Timer::from_seconds(screenshot_at, TimerMode::Once) });
    }
    app.run();
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
    match *mode {
        Mode::Island => {
            setup_map(&mut commands, &data, &mut sim, &mut lib, &mut images, &mut layouts);
            spawn_sky(&mut commands, &data, &mut images);
            commands.spawn((
                Text::new(""),
                TextFont { font_size: data.cfg.hud.font_size.into(), ..default() },
                TextColor(Color::WHITE),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                Node { position_type: PositionType::Absolute, top: px(8), left: px(8), padding: UiRect::all(px(6)), ..default() },
                Hud,
            ));
        }
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
        match isl.owner {
            Some(owner) => w.push_island(island, owner),
            None => w.push_neutral_island(island),
        }
    }
    for b in &data.map.bridges {
        for c in &b.cells {
            if !w.place_map_bridge(b.owner, map::cell(*c)) {
                warn!("map bridge cell {c:?} could not be placed");
            }
        }
    }
    for st in &data.map.structures {
        match w.drop_structure_for(st.owner, &st.kind, &data.rules(&st.kind), map::cell(st.at)) {
            Ok(i) if w.structures[i].is_obelisk => {
                w.structures[i].spell = st.spell.clone();
                w.assign_obelisk_spell(i);
            }
            Ok(_) => {}
            Err(e) => warn!("map: {} at {:?}: {e}", st.kind, st.at),
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
            ShapeSprite { sequence: sequence.clone(), frames, step: 0, playing: true, once: false },
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
    let (shooter, generator) = (data.cfg.ai.shooter.clone(), data.cfg.ai.generator.clone());
    let kit = islefall_sim::ai::AiKit { shooter: (shooter.clone(), data.rules(&shooter)), generator: (generator.clone(), data.rules(&generator)) };
    for ai in ais.iter_mut() {
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
    mut sim: ResMut<Sim>,
    data: Res<GameData>,
    mut bank: ResMut<SoundBank>,
    mut sources: ResMut<Assets<AudioSource>>,
    volume: Res<GlobalVolume>,
    cameras: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
    mut turns: Local<HashMap<String, usize>>,
) {
    let Some(world) = sim.world.as_mut() else { return };
    let events = world.take_events();
    if volume.volume == Volume::Linear(0.0) {
        return;
    }
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
    cameras: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
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
    cameras: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
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
        let pick = (sky.next() % amb.sky.len() as u64) as usize;
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
    let (shooter, generator) = (data.cfg.ai.shooter.clone(), data.cfg.ai.generator.clone());
    let kit = islefall_sim::ai::AiKit { shooter: (shooter.clone(), data.rules(&shooter)), generator: (generator.clone(), data.rules(&generator)) };
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
        Tool::Bridge(slot, _) => format!("bridge piece {} (slot {})", w.queue.slots.get(*slot).map(|p| p.name.as_str()).unwrap_or("?"), slot + 1),
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
    mut status: ResMut<Status>,
    mut rec: ResMut<Recording>,
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
        // Selecting is local; everything else is a command.
        let me = player.id;
        if let Some(i) = w.units.iter().position(|u| u.alive && u.owner == me && u.carried_by.is_none() && u.cell() == cell) {
            player.selected = i;
            status.say(format!("selected unit {i} ({})", w.units[i].kind));
            return;
        }
        let cmd = if let Some(p) = w.units.iter().position(|u| u.alive && u.owner != me && u.is_priest && u.cell() == cell) {
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
        return;
    }
    let cmd = match player.tool.clone() {
        Tool::Bridge(slot, rotations) => Command::PlacePiece { slot, rotations, at: cell },
        Tool::Drop(stem) => Command::Drop { kind: stem, at: cell },
        Tool::Spawn(stem) => Command::PlaceUnit { kind: stem, at: cell },
    };
    if let Some(Applied { unit: Some(i), .. }) = rec.issue(w, &data.scripts, &mut status, cmd) {
        player.selected = i;
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
    cameras: Query<(&Camera, &GlobalTransform)>,
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
            Sprite::from_color(color, Vec2::new(g.cell_w as f32, g.cell_h as f32)),
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
    mut seen_tick: Local<Option<u64>>,
) {
    for e in &existing {
        commands.entity(e).despawn();
    }
    let w = sim.world();
    let g = data.grid();
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    // A box round the selected unit.
    if let Some(u) = w.units.get(player.selected).filter(|u| u.alive && u.owner == player.id) {
        let p = unit_to_world(u, g) + Vec2::new(0.0, 10.0);
        let (wd, ht, c) = (20.0, 26.0, Color::srgba(1.0, 1.0, 1.0, 0.8));
        for (dx, dy, sw, sh) in [(0.0, ht / 2.0, wd, 1.0), (0.0, -ht / 2.0, wd, 1.0), (wd / 2.0, 0.0, 1.0, ht), (-wd / 2.0, 0.0, 1.0, ht)] {
            commands.spawn((Sprite::from_color(c, Vec2::new(sw, sh)), Transform::from_translation(Vec3::new(p.x + dx, p.y + dy, 58.5)), Overlay));
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
                    commands.spawn((Sprite::from_color(Color::srgba(0.6, 0.4, 1.0, 0.12), size), Transform::from_translation(Vec3::new(p.x, p.y + g.cell_h as f32 / 2.0, 58.0)), Overlay));
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
            commands.spawn((Sprite::from_color(c, Vec2::new(16.0, 16.0)), Transform::from_translation(Vec3::new(p.x, p.y + 8.0, 59.5)), Overlay));
        }
    }
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
        let (x, y) = s.cell.top_left_px(g);
        let width = (s.foot_x * g.cell_w) as f32;
        let top = Vec2::new(x as f32 + g.cell_w as f32 - width / 2.0, -(y as f32 - ((s.foot_y - 1) * g.cell_h) as f32) + 4.0);
        if !s.complete() {
            // Build progress in blue while the stream works.
            let frac = s.progress();
            commands.spawn((Sprite::from_color(Color::srgba(0.1, 0.1, 0.1, 0.8), Vec2::new(width, 3.0)), Transform::from_translation(top.extend(60.0)), Overlay));
            let wdt = (width - 1.0) * frac.clamp(0.0, 1.0);
            commands.spawn((
                Sprite::from_color(Color::srgb(0.3, 0.6, 1.0), Vec2::new(wdt.max(0.5), 2.0)),
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
                Sprite::from_color(Color::srgba(1.0, 0.95, 0.3, 0.45), Vec2::new(18.0, 12.0)),
                Transform::from_translation(Vec3::new(p.x, p.y - 2.0, 59.0)),
                Overlay,
            ));
        }
    }
    // Shots: a tracer from the muzzle to the target that lingers a moment, and a flash where it lands.
    if *seen_tick != Some(w.tick) {
        *seen_tick = Some(w.tick);
        for &(shooter, target) in &w.last_shots {
            let to = match target {
                islefall_sim::world::Target::Structure(j) => w.structures.get(j).map(|s| cell_to_world(s.centre(), g)),
                islefall_sim::world::Target::Unit(j) => w.units.get(j).map(|u| unit_to_world(u, g)),
                islefall_sim::world::Target::Bridge(c) => Some(cell_to_world(c, g)),
            };
            if let (Some(from), Some(to)) = (w.structures.get(shooter).map(|s| cell_to_world(s.centre(), g)), to) {
                tracers.push((from, to, data.cfg.controls.tracer_seconds));
            }
        }
    }
    let dt = time.delta_secs();
    tracers.retain_mut(|t| {
        t.2 -= dt;
        t.2 > 0.0
    });
    for &(from, to, left) in tracers.iter() {
        let d = to - from;
        let len = d.length().max(1.0);
        let alpha = (left / data.cfg.controls.tracer_seconds.max(0.01)).clamp(0.0, 1.0);
        commands.spawn((
            Sprite::from_color(Color::srgba(1.0, 0.95, 0.5, 0.9 * alpha), Vec2::new(len, 1.5)),
            Transform::from_translation(((from + to) / 2.0).extend(60.5)).with_rotation(Quat::from_rotation_z(d.y.atan2(d.x))),
            Overlay,
        ));
        commands.spawn((Sprite::from_color(Color::srgba(1.0, 0.8, 0.3, alpha), Vec2::new(7.0, 7.0)), Transform::from_translation(to.extend(61.0)), Overlay));
    }
}

/// Draw the Energy circle of every source the player owns whenever structures change.
fn sync_energy_rings(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    player: Res<Player>,
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
    for s in w.structures.iter().filter(|s| s.owner == player.id && s.produces.is_some() && s.complete()) {
        let (x, y) = s.centre().centre_px(data.grid());
        let centre = Vec2::new(x as f32, -(y as f32));
        for index in 0..data.cfg.energy.stars {
            commands.spawn((Sprite::from_image(ring.0.clone()), Transform::from_translation(centre.extend(0.9)), EnergyRing { centre, index }));
        }
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
        if shape.once && shape.step + 1 >= shape.sequence.len() {
            shape.playing = false;
            continue;
        }
        shape.step = (shape.step + 1) % shape.sequence.len().max(1);
        let frame = shape.sequence[shape.step];
        atlas.index = frame;
        *anchor = shape.frames[frame].anchor();
    }
}

/// Give each new structure sprite its idle animation, and turn turrets to
/// their targets: an all-round turret shows the frame of its bearing, a
/// cannon plays its direction's raise-and-fire once per shot.
#[allow(clippy::type_complexity)]
fn animate_structures(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<GameData>,
    fresh: Query<(Entity, &StructureFrames), Without<ShapeSprite>>,
    mut turrets: Query<(&StructureSprite, &StructureFrames, &mut ShapeSprite, &mut Sprite, &mut Anchor)>,
    mut seen_tick: Local<Option<u64>>,
) {
    for (e, plan) in &fresh {
        commands.entity(e).insert(ShapeSprite { sequence: plan.sequence.clone(), frames: plan.frames.clone(), step: 0, playing: plan.sequence.len() > 1, once: false });
    }
    let w = sim.world();
    let fired: Vec<usize> = if *seen_tick == Some(w.tick) {
        Vec::new()
    } else {
        *seen_tick = Some(w.tick);
        w.last_shots.iter().map(|&(i, _)| i).collect()
    };
    let rules = &data.cfg.animation;
    let g = data.grid();
    for (s, plan, mut shape, mut sprite, mut anchor) in &mut turrets {
        let Some(st) = w.structures.get(s.0) else { continue };
        let Some(aim) = st.aim else { continue };
        let (cx, cy) = st.centre().centre_px(g);
        let (ax, ay) = aim.centre_px(g);
        let (dx, dy) = ((ax - cx) as f32, (ay - cy) as f32);
        if !plan.turret.is_empty() {
            // Bearing clockwise from north on screen (y grows downwards).
            let mut bearing = dx.atan2(-dy);
            if !rules.turret_clockwise {
                bearing = -bearing;
            }
            let first = match rules.turret_first.as_str() {
                "east" => std::f32::consts::FRAC_PI_2,
                "south" => std::f32::consts::PI,
                "west" => -std::f32::consts::FRAC_PI_2,
                _ => 0.0,
            };
            let n = plan.turret.len() as f32;
            let turn = ((bearing - first).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * n).round() as usize % plan.turret.len();
            let frame = plan.turret[turn];
            if shape.sequence != [frame] {
                shape.sequence = vec![frame];
                shape.step = 0;
                shape.playing = false;
                if let Some(atlas) = sprite.texture_atlas.as_mut() {
                    atlas.index = frame;
                }
                *anchor = shape.frames[frame].anchor();
            }
        } else if !plan.cardinal.is_empty() && fired.contains(&s.0) {
            let dir = if dx.abs() >= dy.abs() { if dx >= 0.0 { "east" } else { "west" } } else if dy >= 0.0 { "south" } else { "north" };
            if let Some(seq) = plan.cardinal.get(dir) {
                shape.sequence = seq.clone();
                shape.step = 0;
                shape.playing = true;
                shape.once = true;
                let frame = seq[0];
                if let Some(atlas) = sprite.texture_atlas.as_mut() {
                    atlas.index = frame;
                }
                *anchor = shape.frames[frame].anchor();
            }
        }
    }
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
