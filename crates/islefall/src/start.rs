// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! The start screen: a small Bevy application that opens before the game
//! when no direct launch was asked for. It gathers the installation and
//! data directories, the player's name, and whether to play the campaign,
//! a local map or a network game, then starts the game with those choices.
//!
//! The game's data loading stays in `main`; the screen only validates the
//! paths and options and re-executes the program with the equivalent
//! environment variables. `ISLEFALL_START=0` skips the screen for scripts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use islefall_data::campaign::Difficulty;
use islefall_data::Installation;
use islefall_sim::{Config, Scripts};

const BG: Color = Color::srgb(0.035, 0.045, 0.085);
const PANEL: Color = Color::srgba(0.075, 0.085, 0.145, 0.98);
const FIELD: Color = Color::srgba(0.02, 0.025, 0.05, 0.85);
const BUTTON: Color = Color::srgba(0.16, 0.18, 0.27, 0.9);
const BUTTON_HOVER: Color = Color::srgba(0.24, 0.27, 0.4, 0.95);
const SELECTED: Color = Color::srgba(0.45, 0.34, 0.12, 0.9);
const GOLD: Color = Color::srgb(0.95, 0.78, 0.3);
const TEXT: Color = Color::srgb(0.92, 0.93, 0.96);
const MUTED: Color = Color::srgb(0.62, 0.66, 0.75);
const BORDER: Color = Color::srgba(0.45, 0.48, 0.6, 0.4);

/// How the next game is chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StartMode {
    Campaign,
    #[default]
    Skirmish,
    Online,
}

impl StartMode {
    fn label(self) -> &'static str {
        match self {
            StartMode::Campaign => "Campaign",
            StartMode::Skirmish => "Skirmish",
            StartMode::Online => "Online",
        }
    }
}

/// A text field on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldKind {
    Install,
    Data,
    Map,
    Players,
    Name,
    Server,
}

impl FieldKind {
    fn label(self) -> &'static str {
        match self {
            FieldKind::Install => "NetStorm folder",
            FieldKind::Data => "Data folder",
            FieldKind::Map => "Map",
            FieldKind::Players => "Players",
            FieldKind::Name => "Player name",
            FieldKind::Server => "Server",
        }
    }

    const ORDER: [FieldKind; 6] = [FieldKind::Install, FieldKind::Data, FieldKind::Map, FieldKind::Players, FieldKind::Name, FieldKind::Server];
}

/// What the screen is currently editing.
#[derive(Resource, Default)]
struct FocusedField(Option<FieldKind>);

#[derive(Component, Clone, Copy)]
struct StartField(FieldKind);

#[derive(Component, Clone, Copy)]
struct FieldText(FieldKind);

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct LibraryText;

#[derive(Component)]
struct MapButtons;

/// What a click on the screen does.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
enum StartAction {
    Mode(StartMode),
    Difficulty(Difficulty),
    Map(String),
    Scan,
    Start,
    Quit,
}

/// Everything the player can change before the game starts.
#[derive(Resource, Clone)]
struct Options {
    install: String,
    data: String,
    map: String,
    players: String,
    name: String,
    server: String,
    mode: StartMode,
    difficulty: Difficulty,
    message: String,
}

impl Default for Options {
    fn default() -> Self {
        let install = std::env::var("NETSTORM_DIR").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(default_install);
        let data = super::data_dir().display().to_string();
        let map = std::env::var("ISLEFALL_MAP").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "demo".into());
        let players = std::env::var("ISLEFALL_PLAYERS").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "4".into());
        let name = std::env::var("ISLEFALL_NAME").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(super::whoami);
        let server = std::env::var("ISLEFALL_JOIN").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "127.0.0.1:7777".into());
        let mode = if std::env::var("ISLEFALL_JOIN").is_ok_and(|v| !v.trim().is_empty()) {
            StartMode::Online
        } else if env_on("ISLEFALL_CAMPAIGN") {
            StartMode::Campaign
        } else {
            StartMode::Skirmish
        };
        let difficulty = std::env::var("ISLEFALL_DIFFICULTY").ok().and_then(|d| Difficulty::parse(&d)).unwrap_or_default();
        Options { install, data, map, players, name, server, mode, difficulty, message: "Set the folders, choose how to play, then press Start; Scan lists maps first. Tab moves between fields.".into() }
    }
}

impl Options {
    fn field(&self, kind: FieldKind) -> &str {
        match kind {
            FieldKind::Install => &self.install,
            FieldKind::Data => &self.data,
            FieldKind::Map => &self.map,
            FieldKind::Players => &self.players,
            FieldKind::Name => &self.name,
            FieldKind::Server => &self.server,
        }
    }

    fn field_mut(&mut self, kind: FieldKind) -> &mut String {
        match kind {
            FieldKind::Install => &mut self.install,
            FieldKind::Data => &mut self.data,
            FieldKind::Map => &mut self.map,
            FieldKind::Players => &mut self.players,
            FieldKind::Name => &mut self.name,
            FieldKind::Server => &mut self.server,
        }
    }

    fn difficulty_name(&self) -> &'static str {
        match self.difficulty {
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        }
    }

    /// The game this screen would start, as environment variables. The
    /// child never sees the screen again: `ISLEFALL_START=0`.
    fn environment(&self) -> BTreeMap<&'static str, String> {
        let mut env = BTreeMap::new();
        env.insert("NETSTORM_DIR", expand_path(&self.install).display().to_string());
        env.insert("ISLEFALL_DATA", expand_path(&self.data).display().to_string());
        env.insert("ISLEFALL_NAME", self.name.trim().to_string());
        env.insert("ISLEFALL_DIFFICULTY", self.difficulty_name().to_string());
        env.insert("ISLEFALL_START", "0".into());
        match self.mode {
            StartMode::Campaign => {
                env.insert("ISLEFALL_CAMPAIGN", "1".into());
            }
            StartMode::Skirmish => {
                env.insert("ISLEFALL_MAP", self.map.trim().to_string());
            }
            StartMode::Online => {
                env.insert("ISLEFALL_MAP", self.map.trim().to_string());
                env.insert("ISLEFALL_JOIN", self.server.trim().to_string());
            }
        }
        if self.map.trim().to_ascii_lowercase().starts_with("random") && matches!(self.mode, StartMode::Skirmish | StartMode::Online) {
            env.insert("ISLEFALL_PLAYERS", self.players.trim().to_string());
        }
        env
    }
}

/// What a successful scan found, for the list and the summary line.
#[derive(Debug, Resource, Default)]
struct Library {
    scanned: bool,
    key: Option<ScanKey>,
    local_maps: Vec<String>,
    forts: BTreeSet<String>,
    campaign_missions: usize,
    summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScanKey {
    install: String,
    data: String,
    difficulty: Difficulty,
}

/// Whether the program should open the start screen. An explicit
/// `ISLEFALL_START` wins; otherwise a direct launch request (a map,
/// campaign, mission, network game or viewer mode) opens the game itself.
pub fn requested() -> bool {
    let explicit = std::env::var("ISLEFALL_START").ok();
    let direct = env_nonempty("ISLEFALL_MAP") || env_nonempty("ISLEFALL_MISSION") || env_on("ISLEFALL_CAMPAIGN") || env_nonempty("ISLEFALL_JOIN");
    let viewer = std::env::var("ISLEFALL_MODE").is_ok_and(|m| m.trim().eq_ignore_ascii_case("viewer"));
    should_show(explicit.as_deref(), direct, viewer)
}

fn should_show(explicit: Option<&str>, direct: bool, viewer: bool) -> bool {
    match explicit.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("0" | "no" | "off" | "false") => false,
        Some("1" | "yes" | "on" | "true") => true,
        _ => !direct && !viewer,
    }
}

fn env_nonempty(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.trim().is_empty())
}

fn env_on(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "" | "0" | "no" | "off" | "false"))
}

/// A directory that looks like a NetStorm installation, for the first
/// run. The player can still type another one.
fn default_install() -> String {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(home) = &home {
        candidates.extend([home.join("games/NetStorm"), home.join("NetStorm"), home.join(".local/share/NetStorm")]);
    }
    candidates.push(PathBuf::from("NetStorm"));
    candidates.into_iter().find(|p| p.join("netstorm.tarc").is_file()).or_else(|| home.map(|h| h.join("games/NetStorm"))).unwrap_or_else(|| PathBuf::from("NetStorm")).display().to_string()
}

/// Expand the `~` a player may type; surrounding quotes are ignored.
fn expand_path(raw: &str) -> PathBuf {
    let path = raw.trim().trim_matches('"').trim_matches('\'');
    if path == "~" {
        return std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

/// The local map stems, sorted and case-preserving on disk.
fn local_maps(data: &Path) -> Vec<String> {
    let mut maps: Vec<String> = std::fs::read_dir(data.join("maps"))
        .map(|dir| {
            dir.flatten()
                .filter_map(|e| {
                    let path = e.path();
                    (path.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("toml"))).then(|| path.file_stem().and_then(|x| x.to_str()).map(|x| x.to_string())).flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    maps.sort_by_key(|m| m.to_ascii_lowercase());
    maps.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    maps
}

/// Load the installation and rules just far enough to name what can be
/// played. The game loads them again when it starts; this is the screen's
/// chance to say what is wrong before a window opens and closes.
fn scan(options: &Options) -> Result<Library, String> {
    let install_dir = expand_path(&options.install);
    let data_dir = expand_path(&options.data);
    if !install_dir.is_dir() {
        return Err(format!("NetStorm folder {} does not exist", install_dir.display()));
    }
    if !data_dir.join("rules.toml").is_file() {
        return Err(format!("Data folder {} has no rules.toml", data_dir.display()));
    }
    let cfg = Config::load(data_dir.join("rules.toml")).map_err(|e| format!("{}: {e}", data_dir.join("rules.toml").display()))?;
    Scripts::load(data_dir.join("scripts/rules.rhai")).map_err(|e| format!("{}: {e}", data_dir.join("scripts/rules.rhai").display()))?;
    let install = Installation::load(&install_dir).map_err(|e| format!("NetStorm data in {}: {e}", install_dir.display()))?;
    if install.palette(&cfg.controls.palette).is_none() {
        return Err(format!("palette {} is not in {}/d", cfg.controls.palette, install_dir.display()));
    }
    let local_maps = local_maps(&data_dir);
    let forts: BTreeSet<String> = install
        .archive
        .entries()
        .iter()
        .filter(|e| e.extension().eq_ignore_ascii_case("fort"))
        .map(|e| e.basename().to_ascii_lowercase())
        .collect();
    let chapters = islefall_data::campaign::chapters(&install.archive, options.difficulty);
    let campaign_missions: usize = chapters.iter().map(|c| c.missions.len()).sum();
    let first_mission = chapters.iter().flat_map(|c| &c.missions).next().map(|e| e.label.clone()).unwrap_or_default();
    let summary = format!(
        "{} types, {} palettes, {} local maps, {} archive scenarios. Campaign: {} chapters, {} missions{}.",
        install.types.len(),
        install.palettes.len(),
        local_maps.len(),
        forts.len(),
        chapters.len(),
        campaign_missions,
        if first_mission.is_empty() { String::new() } else { format!(", first: {first_mission}") }
    );
    Ok(Library {
        scanned: true,
        key: Some(ScanKey { install: options.install.trim().to_string(), data: options.data.trim().to_string(), difficulty: options.difficulty }),
        local_maps,
        forts,
        campaign_missions,
        summary,
    })
}

fn ensure_scanned(options: &Options, library: &mut Library) -> Result<(), String> {
    let key = ScanKey { install: options.install.trim().to_string(), data: options.data.trim().to_string(), difficulty: options.difficulty };
    if library.scanned && library.key.as_ref() == Some(&key) {
        return Ok(());
    }
    *library = scan(options)?;
    Ok(())
}

/// Check the choices after a scan, then start the game. A successful
/// launch replaces this process on Unix and exits it elsewhere.
fn try_launch(options: &mut Options, library: &mut Library) {
    let result = (|| -> Result<(), String> {
        ensure_scanned(options, library)?;
        validate_after_scan(options, library)?;
        launch(options)
    })();
    if let Err(e) = result {
        options.message = e;
    }
}

/// Check the choices against what the scan found. Kept separate from
/// `try_launch` so the rules can be tested without starting a process.
fn validate_after_scan(options: &Options, library: &Library) -> Result<(), String> {
    if options.name.trim().is_empty() {
        return Err("Give the player a name".into());
    }
    if options.mode == StartMode::Online && options.server.trim().is_empty() {
        return Err("Online play needs a server address".into());
    }
    if options.mode == StartMode::Campaign {
        return if library.campaign_missions == 0 { Err("This installation has no campaign chapters".into()) } else { Ok(()) };
    }
    let typed = options.map.trim().to_ascii_lowercase();
    let map = typed.strip_suffix(".toml").or_else(|| typed.strip_suffix(".fort")).unwrap_or(&typed);
    if map.is_empty() {
        return Err("Choose a map (demo, range, random, or a scenario name)".into());
    }
    let random = map == "random" || map.starts_with("random:");
    if !random {
        let local = library.local_maps.iter().any(|m| m.eq_ignore_ascii_case(&map));
        let scenario = library.forts.contains(map);
        if !local && !scenario {
            return Err(format!("Map \"{}\" is neither in {}/maps nor a scenario in the archive", options.map.trim(), expand_path(&options.data).display()));
        }
    } else {
        let players = options.players.trim().parse::<usize>().map_err(|_| "Players must be a number from 2 to 8".to_string())?;
        if !(2..=8).contains(&players) {
            return Err("Players must be a number from 2 to 8".into());
        }
    }
    Ok(())
}

/// Re-execute the program with the screen's choices. On Unix `exec`
/// replaces this process; elsewhere a child is started and this one leaves.
fn launch(options: &Options) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Cannot find the game's own program: {e}"))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(std::env::args_os().skip(1));
    for name in [
        "ISLEFALL_CAMPAIGN",
        "ISLEFALL_MISSION",
        "ISLEFALL_MAP",
        "ISLEFALL_JOIN",
        "ISLEFALL_MODE",
        "ISLEFALL_READY",
        "ISLEFALL_PLAYERS",
    ] {
        cmd.env_remove(name);
    }
    for (name, value) in options.environment() {
        cmd.env(name, value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        Err(format!("Cannot start the game: {e}"))
    }
    #[cfg(not(unix))]
    {
        match cmd.spawn() {
            Ok(_) => std::process::exit(0),
            Err(e) => Err(format!("Cannot start the game: {e}")),
        }
    }
}

/// Run the start screen until a game is started or the window is closed.
pub fn run() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins.set(ImagePlugin::default_nearest()).set(WindowPlugin {
            primary_window: Some(Window { title: "Islefall".into(), resolution: (1120, 760).into(), ..default() }),
            ..default()
        }),
    )
    .insert_resource(ClearColor(BG))
    .init_resource::<Options>()
    .init_resource::<FocusedField>()
    .init_resource::<Library>()
    .add_systems(Startup, (spawn_screen, super::window_icon))
    .add_systems(
        Update,
        (
            focus_fields,
            type_fields,
            actions,
            style_actions,
            style_fields,
            refresh_fields,
            refresh_status,
            refresh_library_text,
            refresh_map_buttons,
            super::window_icon,
            super::quit_keys,
        ),
    );
    if let Ok(path) = std::env::var("ISLEFALL_SCREENSHOT") {
        let delay = std::env::var("ISLEFALL_SCREENSHOT_AT").ok().and_then(|s| s.parse().ok()).unwrap_or(2.0);
        app.insert_resource(super::AutoScreenshot { path, delay: Timer::from_seconds(delay, TimerMode::Once) });
        app.add_systems(Update, super::auto_screenshot);
    }
    app.run();
}

fn spawn_screen(mut commands: Commands, options: Res<Options>) {
    commands.spawn((Camera2d, IsDefaultUiCamera));
    commands
        .spawn((Node { width: percent(100), height: percent(100), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() }, BackgroundColor(BG)))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(940),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(9),
                    padding: UiRect::all(px(26)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(BORDER),
            ))
            .with_children(|panel| {
                panel.spawn((Text::new("ISLEFALL"), TextFont { font_size: 54.0.into(), ..default() }, TextColor(GOLD)));
                panel.spawn((
                    Text::new("Floating islands, bridges and priests. Choose a game; the original data stays in your NetStorm folder."),
                    TextFont { font_size: 16.0.into(), ..default() },
                    TextColor(MUTED),
                ));
                panel.spawn((Node { height: px(4), ..default() },));
                panel.spawn((Text::new("GAME DATA"), TextFont { font_size: 14.0.into(), ..default() }, TextColor(GOLD)));
                spawn_field(panel, FieldKind::Install);
                spawn_field(panel, FieldKind::Data);
                panel.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(10), ..default() },)).with_children(|row| {
                    spawn_action(row, "Scan folder", StartAction::Scan);
                    row.spawn((Text::new(""), TextFont { font_size: 14.0.into(), ..default() }, TextColor(MUTED), LibraryText));
                });
                panel.spawn((Node { height: px(2), ..default() },));
                panel.spawn((Text::new("PLAY"), TextFont { font_size: 14.0.into(), ..default() }, TextColor(GOLD)));
                panel.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(8), flex_wrap: FlexWrap::Wrap, ..default() },)).with_children(|row| {
                    for mode in [StartMode::Campaign, StartMode::Skirmish, StartMode::Online] {
                        spawn_action(row, mode.label(), StartAction::Mode(mode));
                    }
                });
                spawn_field(panel, FieldKind::Map);
                panel.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8), flex_wrap: FlexWrap::Wrap, ..default() },)).with_children(|row| {
                    row.spawn((Text::new("Difficulty"), TextFont { font_size: 14.0.into(), ..default() }, TextColor(MUTED)));
                    for (difficulty, label) in [(Difficulty::Easy, "Easy"), (Difficulty::Normal, "Normal"), (Difficulty::Hard, "Hard")] {
                        spawn_action(row, label, StartAction::Difficulty(difficulty));
                    }
                    row.spawn((Text::new("Players"), TextFont { font_size: 14.0.into(), ..default() }, TextColor(MUTED)));
                    spawn_small_field(row, FieldKind::Players);
                });
                spawn_field(panel, FieldKind::Name);
                spawn_field(panel, FieldKind::Server);
                panel.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8), flex_wrap: FlexWrap::Wrap, min_height: px(30), ..default() },)).with_children(|row| {
                    row.spawn((Text::new("Maps"), TextFont { font_size: 14.0.into(), ..default() }, TextColor(MUTED)));
                    row.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), flex_wrap: FlexWrap::Wrap, ..default() }, MapButtons));
                });
                panel.spawn((Node { height: px(2), ..default() },));
                panel.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), ..default() },)).with_children(|row| {
                    spawn_action(row, "Start", StartAction::Start);
                    spawn_action(row, "Quit", StartAction::Quit);
                    row.spawn((
                        Text::new("Tab: next field   Enter: start   Esc: leave field"),
                        TextFont { font_size: 13.0.into(), ..default() },
                        TextColor(MUTED),
                    ));
                });
                panel.spawn((Text::new(&options.message), TextFont { font_size: 15.0.into(), ..default() }, TextColor(TEXT), Node { width: percent(100), ..default() }, StatusText));
            });
        });
}

fn spawn_field(parent: &mut ChildSpawnerCommands, kind: FieldKind) {
    parent
        .spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(10), ..default() },))
        .with_children(|row| {
            row.spawn((Text::new(kind.label()), TextFont { font_size: 15.0.into(), ..default() }, TextColor(MUTED), Node { width: px(145), ..default() }));
            spawn_field_box(row, kind, true);
        });
}

fn spawn_small_field(parent: &mut ChildSpawnerCommands, kind: FieldKind) {
    spawn_field_box(parent, kind, false);
}

fn spawn_field_box(parent: &mut ChildSpawnerCommands, kind: FieldKind, wide: bool) {
    parent
        .spawn((
            Button,
            StartField(kind),
            Node {
                width: if wide { Val::Auto } else { px(42) },
                flex_grow: if wide { 1.0 } else { 0.0 },
                height: px(28),
                padding: UiRect::axes(px(7), px(3)),
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(FIELD),
            BorderColor::all(BORDER),
        ))
        .with_children(|field| {
            field.spawn((Text::new(""), TextFont { font_size: 15.0.into(), ..default() }, TextColor(TEXT), FieldText(kind)));
        });
}

fn spawn_action(parent: &mut ChildSpawnerCommands, label: &str, action: StartAction) {
    parent
        .spawn((
            Button,
            action,
            Node { height: px(28), padding: UiRect::axes(px(10), px(3)), align_items: AlignItems::Center, justify_content: JustifyContent::Center, border: UiRect::all(px(1)), ..default() },
            BackgroundColor(BUTTON),
            BorderColor::all(BORDER),
        ))
        .with_children(|b| {
            b.spawn((Text::new(label), TextFont { font_size: 15.0.into(), ..default() }, TextColor(TEXT)));
        });
}

fn focus_fields(fields: Query<(&Interaction, &StartField), Changed<Interaction>>, mut focus: ResMut<FocusedField>) {
    for (interaction, field) in &fields {
        if *interaction == Interaction::Pressed {
            focus.0 = Some(field.0);
        }
    }
}

fn type_fields(
    mut keys: MessageReader<KeyboardInput>,
    key_state: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<FocusedField>,
    mut options: ResMut<Options>,
    mut library: ResMut<Library>,
) {
    let shift = key_state.pressed(KeyCode::ShiftLeft) || key_state.pressed(KeyCode::ShiftRight);
    for event in keys.read() {
        if !event.state.is_pressed() {
            continue;
        }
        match &event.logical_key {
            Key::Tab => {
                focus.0 = Some(next_field(focus.0, shift));
                continue;
            }
            Key::Enter => {
                try_launch(&mut options, &mut library);
                continue;
            }
            Key::Escape => {
                focus.0 = None;
                continue;
            }
            _ => {}
        }
        let Some(kind) = focus.0 else { continue };
        if event.key_code == KeyCode::Backspace {
            let value = options.field_mut(kind);
            value.pop();
            continue;
        }
        if event.key_code == KeyCode::Delete {
            options.field_mut(kind).clear();
            continue;
        }
        let Some(text) = &event.text else { continue };
        for ch in text.chars() {
            if ch.is_control() {
                continue;
            }
            if kind == FieldKind::Players && !ch.is_ascii_digit() {
                continue;
            }
            let value = options.field_mut(kind);
            if value.chars().count() < 300 {
                value.push(ch);
            }
        }
    }
}

fn next_field(current: Option<FieldKind>, backwards: bool) -> FieldKind {
    let order = FieldKind::ORDER;
    let at = current.and_then(|k| order.iter().position(|f| *f == k)).unwrap_or_else(|| if backwards { 0 } else { order.len() - 1 });
    let next = if backwards { (at + order.len() - 1) % order.len() } else { (at + 1) % order.len() };
    order[next]
}

fn actions(
    mut options: ResMut<Options>,
    mut library: ResMut<Library>,
    mut focus: ResMut<FocusedField>,
    clicks: Query<(&Interaction, &StartAction), Changed<Interaction>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action) in &clicks {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            StartAction::Mode(mode) => {
                options.mode = *mode;
                focus.0 = None;
            }
            StartAction::Difficulty(difficulty) => {
                options.difficulty = *difficulty;
                if library.scanned && library.key.as_ref().is_some_and(|k| k.difficulty != *difficulty) {
                    library.scanned = false;
                    options.message = "Difficulty changed; scan the data again to refresh the campaign.".into();
                }
            }
            StartAction::Map(map) => {
                options.map = map.clone();
                options.message = format!("Map: {map}");
            }
            StartAction::Scan => match ensure_scanned(&options, &mut library) {
                Ok(()) => options.message = "Game data found; choose a game and press Start.".into(),
                Err(e) => {
                    *library = Library::default();
                    options.message = e;
                }
            },
            StartAction::Start => try_launch(&mut options, &mut library),
            StartAction::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}

fn style_actions(options: Res<Options>, mut buttons: Query<(&StartAction, &Interaction, &mut BackgroundColor, &mut BorderColor)>) {
    for (action, interaction, mut background, mut border) in &mut buttons {
        let selected = match action {
            StartAction::Mode(mode) => options.mode == *mode,
            StartAction::Difficulty(difficulty) => options.difficulty == *difficulty,
            StartAction::Map(map) => options.map.eq_ignore_ascii_case(map),
            _ => false,
        };
        let hovered = !matches!(interaction, Interaction::None);
        background.0 = if selected { SELECTED } else if hovered { BUTTON_HOVER } else { BUTTON };
        *border = BorderColor::all(if selected { GOLD } else { BORDER });
    }
}

fn style_fields(focus: Res<FocusedField>, mut fields: Query<(&StartField, &Interaction, &mut BackgroundColor, &mut BorderColor)>) {
    for (field, interaction, mut background, mut border) in &mut fields {
        let focused = focus.0 == Some(field.0);
        let hovered = !matches!(interaction, Interaction::None);
        background.0 = if focused { Color::srgba(0.08, 0.1, 0.18, 0.95) } else if hovered { BUTTON } else { FIELD };
        *border = BorderColor::all(if focused { GOLD } else { BORDER });
    }
}

fn refresh_fields(options: Res<Options>, focus: Res<FocusedField>, mut texts: Query<(&FieldText, &mut Text)>) {
    for (field, mut text) in &mut texts {
        let value = options.field(field.0);
        let shown = if value.is_empty() {
            if focus.0 == Some(field.0) { "|".to_string() } else { "(empty)".to_string() }
        } else if focus.0 == Some(field.0) {
            format!("{value}|")
        } else {
            value.to_string()
        };
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

fn refresh_status(options: Res<Options>, mut texts: Query<&mut Text, With<StatusText>>) {
    for mut text in &mut texts {
        if text.0 != options.message {
            text.0 = options.message.clone();
        }
    }
}

fn refresh_library_text(library: Res<Library>, mut texts: Query<&mut Text, With<LibraryText>>) {
    let shown = if library.scanned { library.summary.clone() } else { "Scan the game data to list maps and the campaign.".into() };
    for mut text in &mut texts {
        if text.0 != shown {
            text.0 = shown.clone();
        }
    }
}

fn refresh_map_buttons(mut commands: Commands, library: Res<Library>, buttons: Query<Entity, With<MapButtons>>) {
    if !library.is_changed() {
        return;
    }
    for entity in &buttons {
        commands.entity(entity).despawn_children();
        commands.entity(entity).with_children(|row| {
            if !library.scanned {
                row.spawn((Text::new("scan the folder to list them"), TextFont { font_size: 13.0.into(), ..default() }, TextColor(MUTED)));
                return;
            }
            for map in &library.local_maps {
                spawn_action(row, map, StartAction::Map(map.clone()));
            }
            spawn_action(row, "Random", StartAction::Map("random".into()));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Options {
        Options {
            install: "/install".into(),
            data: "/data".into(),
            map: "range".into(),
            players: "4".into(),
            name: "Tester".into(),
            server: "127.0.0.1:7777".into(),
            mode: StartMode::Skirmish,
            difficulty: Difficulty::Normal,
            message: String::new(),
        }
    }

    #[test]
    fn the_start_screen_yields_to_direct_launches_unless_forced() {
        assert!(should_show(None, false, false));
        assert!(!should_show(None, true, false));
        assert!(should_show(Some("1"), true, false));
        assert!(!should_show(Some("0"), false, false));
        assert!(!should_show(None, false, true));
    }

    #[test]
    fn a_skirmish_launch_sets_the_map_and_never_the_campaign() {
        let env = options().environment();
        assert_eq!(env.get("ISLEFALL_MAP"), Some(&"range".to_string()));
        assert!(!env.contains_key("ISLEFALL_CAMPAIGN"));
        assert!(!env.contains_key("ISLEFALL_JOIN"));
        assert_eq!(env.get("ISLEFALL_START"), Some(&"0".to_string()));
    }

    #[test]
    fn a_campaign_launch_sets_the_campaign_and_clear_the_map() {
        let mut options = options();
        options.mode = StartMode::Campaign;
        options.difficulty = Difficulty::Hard;
        let env = options.environment();
        assert_eq!(env.get("ISLEFALL_CAMPAIGN"), Some(&"1".to_string()));
        assert_eq!(env.get("ISLEFALL_DIFFICULTY"), Some(&"hard".to_string()));
        assert!(!env.contains_key("ISLEFALL_MAP"));
    }

    #[test]
    fn an_online_launch_carries_the_server_and_name() {
        let mut options = options();
        options.mode = StartMode::Online;
        options.name = "  Ada  ".into();
        let env = options.environment();
        assert_eq!(env.get("ISLEFALL_JOIN"), Some(&"127.0.0.1:7777".to_string()));
        assert_eq!(env.get("ISLEFALL_NAME"), Some(&"Ada".to_string()));
        assert_eq!(env.get("ISLEFALL_MAP"), Some(&"range".to_string()));
    }

    #[test]
    fn a_random_map_passes_the_player_count() {
        let mut options = options();
        options.map = "random:42".into();
        options.players = "3".into();
        assert_eq!(options.environment().get("ISLEFALL_PLAYERS"), Some(&"3".to_string()));
    }

    #[test]
    fn scan_names_a_missing_install_before_touching_the_data() {
        let mut options = options();
        options.install = "/definitely/not/an/islefall/install".into();
        options.data = "/definitely/not/an/islefall/data".into();
        let error = scan(&options).unwrap_err();
        assert!(error.contains("NetStorm folder"), "{error}");
    }

    #[test]
    fn the_repository_maps_are_found() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let maps = local_maps(&data);
        assert!(maps.iter().any(|m| m == "demo"), "{maps:?}");
        assert!(maps.iter().any(|m| m == "range"), "{maps:?}");
    }

    #[test]
    fn tab_walks_the_fields_round_and_round() {
        assert_eq!(next_field(None, false), FieldKind::Install);
        assert_eq!(next_field(Some(FieldKind::Install), false), FieldKind::Data);
        assert_eq!(next_field(Some(FieldKind::Server), false), FieldKind::Install);
        assert_eq!(next_field(Some(FieldKind::Install), true), FieldKind::Server);
    }

    fn library() -> Library {
        Library {
            scanned: true,
            key: None,
            local_maps: vec!["demo".into(), "range".into()],
            forts: BTreeSet::from(["capturethepriest".into()]),
            campaign_missions: 32,
            summary: String::new(),
        }
    }

    #[test]
    fn a_map_must_be_local_or_in_the_archive() {
        let library = library();
        assert!(validate_after_scan(&options(), &library).is_ok());
        let mut scenario = options();
        scenario.map = "CaptureThePriest.fort".into();
        assert!(validate_after_scan(&scenario, &library).is_ok());
        let mut missing = options();
        missing.map = "atlantis".into();
        assert!(validate_after_scan(&missing, &library).unwrap_err().contains("neither"));
    }

    #[test]
    fn campaign_needs_chapters_and_random_needs_a_sane_player_count() {
        let mut library = library();
        let mut campaign = options();
        campaign.mode = StartMode::Campaign;
        assert!(validate_after_scan(&campaign, &library).is_ok());
        library.campaign_missions = 0;
        assert!(validate_after_scan(&campaign, &library).is_err());

        let mut random = options();
        random.map = "random:7".into();
        random.players = "1".into();
        assert!(validate_after_scan(&random, &library).is_err());
        random.players = "3".into();
        assert!(validate_after_scan(&random, &library).is_ok());
    }

    #[test]
    fn online_needs_a_server_and_every_game_needs_a_name() {
        let mut online = options();
        online.mode = StartMode::Online;
        online.server = "  ".into();
        assert!(validate_after_scan(&online, &library()).unwrap_err().contains("server"));
        online.server = "host:7777".into();
        assert!(validate_after_scan(&online, &library()).is_ok());
        online.name = "  ".into();
        assert!(validate_after_scan(&online, &library()).unwrap_err().contains("name"));
    }
}
