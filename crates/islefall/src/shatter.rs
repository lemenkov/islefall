// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! The catastrophe: a picture that has lost its ground breaks into chunks
//! that burst outward, tumble and fall out of the sky, in a cloud of dust.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use rand::RngExt as _;
use rand_pcg::Pcg32;

use super::fire::FireArt;
use super::world::Z_TERRAIN;
use super::{GameData, frame_pixels};

/// A chunk of something that fell, on its way down.
#[derive(Component)]
pub struct Debris {
    age: f32,
    velocity: Vec2,
    spin: f32,
}

/// Break the picture of `sprite` into chunks and set them falling from
/// where it stood. `heft` scales the burst: 1 for an islet, less for a
/// bridge cell.
#[allow(clippy::too_many_arguments)]
pub fn shatter(
    commands: &mut Commands,
    data: &GameData,
    images: &mut Assets<Image>,
    layouts: &Assets<TextureAtlasLayout>,
    art: Option<&FireArt>,
    sprite: &Sprite,
    anchor: &Anchor,
    tf: &Transform,
    heft: f32,
    rng: &mut Pcg32,
) -> bool {
    let fr = &data.cfg.fringe;
    let Some(atlas) = sprite.texture_atlas.as_ref() else { return false };
    let Some((size, px)) = frame_pixels(images, layouts, &sprite.image, atlas) else { return false };
    let (w, h) = (size.x as usize, size.y as usize);
    let pieces = ((fr.shatter_pieces as f32 * heft.max(0.3)).round() as usize).max(2);
    // Seeds of the chunks: scattered points of the picture.
    let seeds: Vec<(f32, f32)> = (0..pieces).map(|_| (rng.random::<f32>() * w as f32, rng.random::<f32>() * h as f32)).collect();
    let a = anchor.as_vec();
    let top_left = Vec2::new(tf.translation.x - (a.x + 0.5) * w as f32, tf.translation.y + (0.5 - a.y) * h as f32);
    let centre = top_left + Vec2::new(w as f32 / 2.0, -(h as f32) / 2.0);
    let mut owner = vec![usize::MAX; w * h];
    let mut bounds: Vec<(usize, usize, usize, usize)> = vec![(w, h, 0, 0); pieces];
    for y in 0..h {
        for x in 0..w {
            if px[(y * w + x) * 4 + 3] == 0 {
                continue;
            }
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let k = (0..pieces).min_by(|&i, &j| ((seeds[i].0 - fx).powi(2) + (seeds[i].1 - fy).powi(2)).total_cmp(&((seeds[j].0 - fx).powi(2) + (seeds[j].1 - fy).powi(2)))).unwrap_or(0);
            owner[y * w + x] = k;
            let b = &mut bounds[k];
            b.0 = b.0.min(x);
            b.1 = b.1.min(y);
            b.2 = b.2.max(x);
            b.3 = b.3.max(y);
        }
    }
    let mut any = false;
    for (k, &(x0, y0, x1, y1)) in bounds.iter().enumerate() {
        if x1 < x0 || y1 < y0 {
            continue;
        }
        let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
        let mut rgba = vec![0u8; cw * ch * 4];
        for y in y0..=y1 {
            for x in x0..=x1 {
                if owner[y * w + x] == k {
                    let from = (y * w + x) * 4;
                    let to = ((y - y0) * cw + (x - x0)) * 4;
                    rgba[to..to + 4].copy_from_slice(&px[from..from + 4]);
                }
            }
        }
        let mut image = Image::new(
            bevy::render::render_resource::Extent3d { width: cw as u32, height: ch as u32, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            rgba,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = bevy::image::ImageSampler::nearest();
        let at = top_left + Vec2::new(x0 as f32 + cw as f32 / 2.0, -(y0 as f32 + ch as f32 / 2.0));
        let away = (at - centre).normalize_or_zero();
        let burst = fr.shatter_burst_px * heft * (0.4 + rng.random::<f32>() * 0.9);
        let velocity = away * burst + Vec2::new((rng.random::<f32>() - 0.5) * burst * 0.5, fr.shatter_lift_px * heft * rng.random::<f32>());
        let spin = (rng.random::<f32>() - 0.5) * 2.0 * fr.shatter_spin;
        commands.spawn((
            Sprite::from_image(images.add(image)),
            Anchor::CENTER,
            Transform::from_translation(at.extend(tf.translation.z.min(Z_TERRAIN - 0.5) - k as f32 * 0.001)),
            Debris { age: 0.0, velocity, spin },
        ));
        any = true;
    }
    if let Some(art) = art {
        let puffs = (fr.shatter_dust as f32 * heft).round() as u32;
        for _ in 0..puffs {
            let at = centre + Vec2::new((rng.random::<f32>() - 0.5) * w as f32 * 0.8, (rng.random::<f32>() - 0.5) * h as f32 * 0.6);
            art.dust(commands, data, at, tf.translation.z + 0.5, rng.random::<f32>() * 0.4);
        }
    }
    any
}

/// Let the debris fall: faster and faster, tumbling, fading towards the end.
pub fn fall(mut commands: Commands, data: Res<GameData>, time: Res<Time>, mut debris: Query<(Entity, &mut Debris, &mut Transform, &mut Sprite)>) {
    let fr = &data.cfg.fringe;
    let dt = time.delta_secs();
    for (e, mut d, mut tf, mut sprite) in &mut debris {
        d.age += dt;
        if d.age >= fr.fall_seconds {
            commands.entity(e).despawn();
            continue;
        }
        d.velocity.y -= fr.fall_gravity_px * dt;
        // The burst dies away; the fall does not.
        d.velocity.x *= 1.0 - (2.5 * dt).min(1.0);
        tf.translation += (d.velocity * dt).extend(0.0);
        tf.rotate_z(d.spin * dt);
        let fade_from = fr.fall_seconds * (1.0 - fr.fall_fade_share.clamp(0.0, 1.0));
        if d.age > fade_from {
            let left = 1.0 - (d.age - fade_from) / (fr.fall_seconds - fade_from).max(0.01);
            sprite.color = Color::srgba(1.0, 1.0, 1.0, left.clamp(0.0, 1.0));
        }
    }
}
