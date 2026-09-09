// SPDX-License-Identifier: Apache-2.0
//! Islefall entry point. For now: a shape viewer that animates one object
//! type from the NetStorm data, image and shadow, to prove the data path.
//!
//! Environment:
//! - `NETSTORM_DIR`: directory containing the game's `d/` and `netstorm.tarc`.
//! - `ISLEFALL_PALETTE`: palette file stem from `d/` (default `suncannon`).
//! - `ISLEFALL_SCREENSHOT=file.png`: save a screenshot shortly after start-up.
//!
//! Keys: `[` and `]` step through types, `,` and `.` through animations,
//! `Space` pauses, `P` saves a screenshot.

mod sprites;

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::sprite::Anchor;
use islefall_data::Installation;
use islefall_data::shapes::SHAPE_ORDER;
use sprites::{FrameInfo, ShapeAtlas};

const START_TYPE: &str = "priest";
const FRAME_SECONDS: f32 = 0.1;
const ZOOM: f32 = 4.0;
const AUTO_SCREENSHOT_SECONDS: f32 = 1.5;
const DEFAULT_PALETTE: &str = "suncannon";

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
    .insert_resource(Viewer {
        type_index: SHAPE_ORDER.iter().position(|s| *s == START_TYPE).unwrap_or(0),
        animation: 0,
        timer: Timer::from_seconds(FRAME_SECONDS, TimerMode::Repeating),
        paused: false,
    })
    .add_systems(Startup, setup)
    .add_systems(Update, (keys, animate, auto_screenshot));
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
    data: Res<GameData>,
    viewer: Res<Viewer>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    commands.spawn(Camera2d);
    spawn_shape(&mut commands, &data, &viewer, &mut images, &mut layouts);
}

fn spawn_shape(
    commands: &mut Commands,
    data: &GameData,
    viewer: &Viewer,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let stem = SHAPE_ORDER[viewer.type_index];
    let (Some(def), Some(records), Some(palette)) =
        (data.install.type_def(stem), data.install.shape_records(stem), data.install.palette(&data.palette))
    else {
        warn!("{stem}: missing type, records or palette");
        return;
    };
    let animations = def.animations();
    let Some(&animation) = animations.get(viewer.animation) else {
        warn!("{stem}: no animations");
        return;
    };
    let sequence: Vec<usize> = def.frames.iter().enumerate().filter(|(_, f)| f.animation == animation).map(|(i, _)| i).collect();
    info!(
        "{stem} ({}): {} frames, {} animations, showing {animation} with {} frames{}",
        def.get_str("description").unwrap_or("-"),
        def.frames.len(),
        animations.len(),
        sequence.len(),
        if records.shadows.is_empty() { "" } else { ", with shadows" }
    );

    let layers = [(&records.shadows, -1.0), (&records.images, 0.0)];
    for (offsets, z) in layers {
        if offsets.is_empty() {
            continue;
        }
        let atlas: ShapeAtlas = match sprites::build_atlas(&data.install.shapes, palette, offsets) {
            Ok(a) => a,
            Err(e) => {
                warn!("{stem}: {e}");
                continue;
            }
        };
        let image = images.add(atlas.image);
        let layout = layouts.add(atlas.layout);
        let first = sequence[0];
        let mut sprite = Sprite::from_atlas_image(image, TextureAtlas { layout, index: first });
        if z < 0.0 {
            // Shadow records only carry a silhouette; draw it as translucent black.
            sprite.color = Color::srgba(0.0, 0.0, 0.0, 0.5);
        }
        commands.spawn((
            sprite,
            atlas.frames[first].anchor(),
            Transform::from_translation(Vec3::new(0.0, 0.0, z)).with_scale(Vec3::splat(ZOOM)),
            ShapeSprite { sequence: sequence.clone(), frames: atlas.frames, step: 0 },
        ));
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
        let Some(atlas) = sprite.texture_atlas.as_mut() else { continue };
        shape.step = (shape.step + 1) % shape.sequence.len().max(1);
        let frame = shape.sequence[shape.step];
        atlas.index = frame;
        *anchor = shape.frames[frame].anchor();
    }
}

fn keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    data: Res<GameData>,
    mut viewer: ResMut<Viewer>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    existing: Query<Entity, With<ShapeSprite>>,
) {
    if keys.just_pressed(KeyCode::Space) {
        viewer.paused = !viewer.paused;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk("islefall.png"));
    }
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
        spawn_shape(&mut commands, &data, &viewer, &mut images, &mut layouts);
    }
}
