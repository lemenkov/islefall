// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Generated fire: flames that stand on a burning structure's picture,
//! the smoke they give off, and an explosion, all from `islefall-art`.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use islefall_art::Picture;
use islefall_art::fire::{Blast, Flame, Smoke, strip};
use rand::RngExt as _;
use rand_pcg::Pcg32;

use super::world::{StructureSprite, Z_STRUCTURE, Z_UNIT};
use super::{GameData, Sim, frame_pixels};

/// Clear pixels between the frames of a sheet, so that none shows at its
/// neighbour's edge when the view is zoomed.
const GAP: u32 = 2;

/// One run of frames as an atlas.
pub struct Strip {
    image: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
    frames: usize,
    size: UVec2,
}

impl Strip {
    fn sprite(&self, frame: usize) -> Sprite {
        Sprite::from_atlas_image(self.image.clone(), TextureAtlas { layout: self.layout.clone(), index: frame.min(self.frames.saturating_sub(1)) })
    }
}

/// Every sheet of generated fire, made once at start-up.
#[derive(Resource, Default)]
pub struct FireArt {
    /// By size, smallest first, then by variant.
    flames: Vec<Vec<Strip>>,
    smoke: Vec<Strip>,
    /// By size, smallest first.
    blasts: Vec<Strip>,
}

/// A sprite stepping through a strip: round and round until its life is
/// over, or once.
#[derive(Component)]
pub struct Flicker {
    frames: usize,
    fps: f32,
    clock: f32,
    /// Seconds left for one that loops; one that plays once ends with its frames.
    life: Option<f32>,
    rise: f32,
}

/// A flame on the structure with this id, and the time to its next puff of smoke.
#[derive(Component)]
pub struct FlameOf {
    id: u32,
    puff: f32,
    height: f32,
}

fn make_strip(frames: &[Picture], images: &mut Assets<Image>, layouts: &mut Assets<TextureAtlasLayout>) -> Strip {
    let size = UVec2::new(frames[0].width, frames[0].height);
    let sheet = strip(frames, GAP);
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d { width: sheet.width, height: sheet.height, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        sheet.rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::nearest();
    let layout = TextureAtlasLayout::from_grid(size, frames.len() as u32, 1, Some(UVec2::new(GAP, 0)), None);
    Strip { image: images.add(image), layout: layouts.add(layout), frames: frames.len(), size }
}

pub fn make_fire_art(mut commands: Commands, data: Res<GameData>, mut images: ResMut<Assets<Image>>, mut layouts: ResMut<Assets<TextureAtlasLayout>>) {
    let r = &data.cfg.effects.fire;
    let palette = data.install.palette(&data.palette).expect("palette checked at start-up");
    let snap = |ramp: &[[u8; 3]]| ramp.iter().map(|&c| super::world::snap(palette, c)).collect::<Vec<_>>();
    let (fire, grey) = (snap(&r.ramp), snap(&r.smoke_ramp));
    let mut art = FireArt::default();
    for (i, &[width, height]) in r.flame_sizes.iter().enumerate() {
        let flame = Flame { ramp: fire.clone(), width, height, frames: r.flame_frames, flicker: r.flicker, lick: r.lick, ..Flame::default() };
        art.flames.push((0..r.variants.max(1)).map(|v| make_strip(&flame.frames(1 + i as u32 * 31 + v * 7), &mut images, &mut layouts)).collect());
    }
    let smoke = Smoke { ramp: grey.clone(), size: r.smoke_size, frames: r.smoke_frames };
    art.smoke = (0..r.variants.max(1)).map(|v| make_strip(&smoke.frames(101 + v * 13), &mut images, &mut layouts)).collect();
    for (i, &size) in r.blast_sizes.iter().enumerate() {
        let blast = Blast { fire: fire.clone(), smoke: grey.clone(), size, frames: r.blast_frames, sparks: r.sparks };
        art.blasts.push(make_strip(&blast.frames(211 + i as u32 * 17), &mut images, &mut layouts));
    }
    commands.insert_resource(art);
}

impl FireArt {
    /// An explosion for a structure whose longer side is `cells`, centred on `pos`.
    pub fn blast(&self, commands: &mut Commands, data: &GameData, cells: i32, pos: Vec2, z: f32) {
        let r = &data.cfg.effects.fire;
        let pick = if cells <= 1 { 0 } else if cells <= r.blast_medium_cells { 1 } else { 2 };
        let Some(strip) = self.blasts.get(pick.min(self.blasts.len().saturating_sub(1))) else { return };
        commands.spawn((strip.sprite(0), Anchor::CENTER, Transform::from_translation(pos.extend(z)), Flicker { frames: strip.frames, fps: r.blast_fps, clock: 0.0, life: None, rise: 0.0 }));
    }
}

/// A point of the structure's picture a flame can stand on: an opaque
/// pixel in the allowed part of the largest sprite drawn for it.
fn stand_point(images: &Assets<Image>, layouts: &Assets<TextureAtlasLayout>, sprite: &Sprite, anchor: &Anchor, tf: &Transform, area: [f32; 4], next: &mut impl FnMut() -> f32) -> Option<Vec2> {
    let atlas = sprite.texture_atlas.as_ref()?;
    let (size, px) = frame_pixels(images, layouts, &sprite.image, atlas)?;
    let (w, h) = (size.x as f32, size.y as f32);
    for _ in 0..24 {
        let x = ((area[0] + next() * (area[2] - area[0])) * w).clamp(0.0, w - 1.0) as u32;
        let y = ((area[1] + next() * (area[3] - area[1])) * h).clamp(0.0, h - 1.0) as u32;
        if px.get(((y * size.x + x) * 4 + 3) as usize).is_some_and(|&a| a > 0) {
            let a = anchor.as_vec();
            return Some(Vec2::new(tf.translation.x - a.x * w + (x as f32 + 0.5 - w / 2.0), tf.translation.y - a.y * h + (h / 2.0 - y as f32 - 0.5)));
        }
    }
    None
}

/// Keep as many flames standing on every damaged structure as its damage
/// and size call for; a flame that has burnt out is replaced elsewhere.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn flames(
    commands: &mut Commands,
    sim: &Sim,
    data: &GameData,
    art: &FireArt,
    images: &Assets<Image>,
    layouts: &Assets<TextureAtlasLayout>,
    sprites: &Query<(&StructureSprite, &Sprite, &Anchor, &Transform), (Without<Flicker>, Without<super::Ember>)>,
    burning: &Query<(Entity, &FlameOf)>,
    next: &mut impl FnMut() -> f32,
) {
    let w = sim.world();
    let fx = &data.cfg.effects;
    let r = &fx.fire;
    let g = data.grid();
    let mut have: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (_, f) in burning {
        *have.entry(f.id).or_default() += 1;
    }
    let mut wanted: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (i, s) in w.structures.iter().enumerate() {
        if s.max_hp == 0 || !s.complete() {
            continue;
        }
        let share = s.hp as f32 / s.max_hp as f32;
        let (per_cell, most, sizes) = if share < fx.blazing_below {
            (r.blaze_sites_per_cell, r.max_sites[1], art.flames.len())
        } else if share < fx.burning_below {
            (r.sites_per_cell, r.max_sites[0], art.flames.len().saturating_sub(1).max(1))
        } else {
            continue;
        };
        let want = ((per_cell * (s.foot_x * s.foot_y) as f32).round() as usize).clamp(1, most.max(1));
        wanted.insert(s.id, want);
        // One new flame a frame, so that a fire catches rather than appears.
        if have.get(&s.id).copied().unwrap_or(0) >= want || art.flames.is_empty() {
            continue;
        }
        let main = sprites
            .iter()
            .filter(|(key, sprite, _, tf)| key.0 == i && tf.translation.z >= Z_STRUCTURE && sprite.texture_atlas.is_some())
            .max_by_key(|(_, sprite, _, _)| sprite.texture_atlas.as_ref().and_then(|a| layouts.get(&a.layout)?.textures.get(a.index).map(|t| t.width() * t.height())).unwrap_or(0));
        let at = main.and_then(|(_, sprite, anchor, tf)| stand_point(images, layouts, sprite, anchor, tf, r.stand_on, next)).unwrap_or_else(|| {
            // No picture to read: somewhere over the footprint, which runs up and left from the hotspot cell.
            let (hx, hy) = s.cell.top_left_px(g);
            let left = hx as f32 - ((s.foot_x - 1) * g.cell_w) as f32;
            let top = hy as f32 - ((s.foot_y - 1) * g.cell_h) as f32;
            Vec2::new(left + next() * (s.foot_x * g.cell_w) as f32, -(top + next() * (s.foot_y * g.cell_h) as f32))
        });
        let variants = &art.flames[((next() * sizes as f32) as usize).min(sizes.saturating_sub(1))];
        let strip = &variants[((next() * variants.len() as f32) as usize).min(variants.len() - 1)];
        let life = r.site_seconds[0] + next() * (r.site_seconds[1] - r.site_seconds[0]).max(0.0);
        // Lower flames are drawn over higher ones.
        let z = Z_UNIT + 1.0 - at.y * 0.0001;
        commands.spawn((
            strip.sprite((next() * strip.frames as f32) as usize),
            Anchor::BOTTOM_CENTER,
            Transform::from_translation(at.extend(z)),
            Flicker { frames: strip.frames, fps: r.flame_fps, clock: next(), life: Some(life), rise: 0.0 },
            FlameOf { id: s.id, puff: next() * r.smoke_every, height: strip.size.y as f32 },
        ));
    }
    // Flames of structures that are gone, mended or over their number go out.
    let mut over: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (e, f) in burning {
        let want = wanted.get(&f.id).copied().unwrap_or(0);
        let seen = over.entry(f.id).or_default();
        *seen += 1;
        if *seen > want {
            commands.entity(e).despawn();
        }
    }
}

/// Step every flame, puff and explosion; flames give off smoke.
pub fn run_flickers(
    mut commands: Commands,
    data: Res<GameData>,
    art: Option<Res<FireArt>>,
    time: Res<Time>,
    mut all: Query<(Entity, &mut Flicker, &mut Sprite, &mut Transform, Option<&mut FlameOf>)>,
    mut rng: Local<Option<Pcg32>>,
) {
    use rand::SeedableRng;
    let rng = rng.get_or_insert_with(|| Pcg32::seed_from_u64(0x1405_7B7E_F767_814F));
    let fx = &data.cfg.effects;
    let dt = time.delta_secs();
    for (e, mut f, mut sprite, mut tf, flame) in &mut all {
        f.clock += dt;
        let mut frame = (f.clock * f.fps) as usize;
        let frames = f.frames.max(1);
        let over = match f.life.as_mut() {
            Some(life) => {
                *life -= dt;
                frame %= frames;
                *life <= 0.0
            }
            None => frame >= frames,
        };
        if over {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = frame;
        }
        tf.translation.y += f.rise * dt;
        let (Some(mut flame), Some(art)) = (flame, art.as_ref()) else { continue };
        flame.puff -= dt;
        if flame.puff <= 0.0 && !art.smoke.is_empty() {
            flame.puff = fx.fire.smoke_every * (0.6 + rng.random::<f32>() * 0.8);
            let strip = &art.smoke[(rng.random::<f32>() * art.smoke.len() as f32) as usize % art.smoke.len()];
            let at = Vec3::new(tf.translation.x + rng.random::<f32>() * 4.0 - 2.0, tf.translation.y + flame.height, tf.translation.z - 0.05);
            commands.spawn((strip.sprite(0), Anchor::CENTER, Transform::from_translation(at), Flicker { frames: strip.frames, fps: strip.frames as f32 / fx.smoke_seconds.max(0.1), clock: 0.0, life: None, rise: fx.smoke_rise_px }));
        }
    }
}
