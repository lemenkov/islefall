// SPDX-License-Identifier: Apache-2.0
//! Islefall entry point. For now: a shape viewer that animates one container
//! of the NetStorm sprite cache, to prove the data path end to end.
//!
//! Set `NETSTORM_DIR` to the directory containing the game's `d/` folder.
//! Keys: `[` and `]` step through containers, `Space` pauses, `P` saves a
//! screenshot. Set `ISLEFALL_SCREENSHOT=file.png` to save one automatically
//! shortly after start-up (used for visual checks without a desktop).

mod sprites;

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::sprite::Anchor;
use islefall_data::{Palette, ShapeFile};
use sprites::FrameInfo;

const START_CONTAINER: usize = 88;
const FRAME_SECONDS: f32 = 0.1;
const ZOOM: f32 = 4.0;
const AUTO_SCREENSHOT_SECONDS: f32 = 1.5;

/// The user-supplied NetStorm data, loaded once at startup.
#[derive(Resource)]
struct GameData {
    shp: ShapeFile,
    palette: Palette,
}

#[derive(Resource)]
struct Viewer {
    container: usize,
    timer: Timer,
    paused: bool,
}

/// Pending automatic screenshot requested through `ISLEFALL_SCREENSHOT`.
#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    delay: Timer,
}

/// An animated shape; the entity's transform marks the hotspot.
#[derive(Component)]
struct ShapeSprite {
    frames: Vec<FrameInfo>,
}

fn main() {
    let dir = match std::env::var_os("NETSTORM_DIR") {
        Some(d) => PathBuf::from(d),
        None => {
            eprintln!("NETSTORM_DIR is not set; point it at a NetStorm installation (the directory holding d/)");
            std::process::exit(2);
        }
    };
    let data = match load_data(&dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("failed to load NetStorm data from {}: {e}", dir.display());
            std::process::exit(1);
        }
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
        .insert_resource(data)
        .insert_resource(Viewer {
            container: START_CONTAINER,
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

fn load_data(dir: &std::path::Path) -> Result<GameData, Box<dyn std::error::Error>> {
    let d = dir.join("d");
    let shp = ShapeFile::load(d.join("_shapes.shp"))?;
    let palette = Palette::load(d.join("SUNCANNON.COL"))?;
    Ok(GameData { shp, palette })
}

fn setup(
    mut commands: Commands,
    data: Res<GameData>,
    viewer: Res<Viewer>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    commands.spawn(Camera2d);
    spawn_shape(&mut commands, &data, viewer.container, &mut images, &mut layouts);
}

fn spawn_shape(
    commands: &mut Commands,
    data: &GameData,
    container: usize,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let atlas = match sprites::build_atlas(&data.shp, &data.palette, container) {
        Ok(a) => a,
        Err(e) => {
            warn!("container {container}: {e}");
            return;
        }
    };
    info!(
        "container {container}: {} frames, atlas {}x{}",
        atlas.frames.len(),
        atlas.layout.size.x,
        atlas.layout.size.y
    );
    let image = images.add(atlas.image);
    let layout = layouts.add(atlas.layout);
    let anchor = atlas.frames[0].anchor();
    commands.spawn((
        Sprite::from_atlas_image(image, TextureAtlas { layout, index: 0 }),
        anchor,
        Transform::from_scale(Vec3::splat(ZOOM)),
        ShapeSprite { frames: atlas.frames },
    ));
}

fn animate(
    time: Res<Time>,
    mut viewer: ResMut<Viewer>,
    mut sprites: Query<(&mut Sprite, &mut Anchor, &ShapeSprite)>,
) {
    if viewer.paused || !viewer.timer.tick(time.delta()).just_finished() {
        return;
    }
    for (mut sprite, mut anchor, shape) in &mut sprites {
        let Some(atlas) = sprite.texture_atlas.as_mut() else { continue };
        atlas.index = (atlas.index + 1) % shape.frames.len().max(1);
        *anchor = shape.frames[atlas.index].anchor();
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
    let count = data.shp.container_count();
    let next = if keys.just_pressed(KeyCode::BracketRight) {
        Some((viewer.container + 1) % count)
    } else if keys.just_pressed(KeyCode::BracketLeft) {
        Some((viewer.container + count - 1) % count)
    } else {
        None
    };
    if let Some(next) = next {
        viewer.container = next;
        for e in &existing {
            commands.entity(e).despawn();
        }
        spawn_shape(&mut commands, &data, next, &mut images, &mut layouts);
    }
}
