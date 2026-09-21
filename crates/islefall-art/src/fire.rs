// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Things that burn: a flame that loops, a puff of smoke that thins out,
//! and an explosion that plays once. Each is a run of frames of one size,
//! made from a seed; nothing is half-clear, smoke thins by ordered
//! dithering as a pixel artist would thin it.

use noise::{NoiseFn, Perlin};

use crate::{Picture, shade};

const BAYER: [[f64; 4]; 4] = [[0.0, 8.0, 2.0, 10.0], [12.0, 4.0, 14.0, 6.0], [3.0, 11.0, 1.0, 9.0], [15.0, 7.0, 13.0, 5.0]];

/// Whether a pixel of a thinning cloud of the given density (0..1) is there.
fn dithered(density: f64, x: u32, y: u32) -> bool {
    density > (BAYER[(y % 4) as usize][(x % 4) as usize] + 0.5) / 16.0
}

/// The pictures of a run side by side with `gap` clear pixels between
/// them, the way an atlas wants them.
pub fn strip(frames: &[Picture], gap: u32) -> Picture {
    let (w, h) = frames.first().map(|f| (f.width, f.height)).unwrap_or((1, 1));
    let n = frames.len().max(1) as u32;
    let mut out = Picture::new(w * n + gap * (n - 1), h);
    for (k, f) in frames.iter().enumerate() {
        for y in 0..h {
            let from = (y * w * 4) as usize;
            let to = ((y * out.width + k as u32 * (w + gap)) * 4) as usize;
            out.rgba[to..to + (w * 4) as usize].copy_from_slice(&f.rgba[from..from + (w * 4) as usize]);
        }
    }
    out
}

/// A flame standing on the bottom middle of its frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Flame {
    /// Coolest colour first: the dark red of the edge up to the pale core.
    pub ramp: Vec<[u8; 3]>,
    pub width: u32,
    pub height: u32,
    /// Frames of the loop; the last runs into the first.
    pub frames: u32,
    /// How much the noise eats into the flame, growing towards the tip.
    pub flicker: f64,
    /// How far the tip leans from side to side, in widths.
    pub lick: f64,
    /// Heights the pattern climbs in one loop.
    pub climb: f64,
}

pub const FIRE_RAMP: [[u8; 3]; 6] = [[92, 20, 12], [168, 40, 12], [228, 92, 16], [248, 156, 28], [252, 208, 60], [255, 240, 150]];
pub const SMOKE_RAMP: [[u8; 3]; 4] = [[44, 42, 44], [76, 74, 76], [112, 110, 112], [152, 150, 152]];

impl Default for Flame {
    fn default() -> Flame {
        Flame { ramp: FIRE_RAMP.to_vec(), width: 12, height: 20, frames: 8, flicker: 0.9, lick: 0.35, climb: 1.5 }
    }
}

impl Flame {
    pub fn frames(&self, seed: u32) -> Vec<Picture> {
        let tongues = Perlin::new(seed);
        let lean = Perlin::new(seed.wrapping_add(59));
        let (w, h, n) = (self.width.max(3), self.height.max(4), self.frames.max(1));
        let scroll = self.climb * h as f64;
        (0..n)
            .map(|k| {
                let t = k as f64 / n as f64;
                let mut pic = Picture::new(w, h);
                // Two copies of the climbing pattern, one fading into the
                // other, so the last frame runs into the first.
                let field = |x: f64, y: f64| {
                    let at = |shift: f64| tongues.get([x * 0.34, (y + shift) * 0.2, 0.5]);
                    (1.0 - t) * at(t * scroll) + t * at((t - 1.0) * scroll)
                };
                let turn = std::f64::consts::TAU * t;
                for y in 0..h {
                    // 0 at the base, 1 at the tip.
                    let v = ((h - 1 - y) as f64 + 0.5) / h as f64;
                    let sway = self.lick * v * v * lean.get([turn.cos() * 0.8, turn.sin() * 0.8, v * 1.5]) * 2.0;
                    let half = (1.0 - v).powf(0.55) * 0.95 + 0.05;
                    for x in 0..w {
                        let u = ((x as f64 + 0.5) - w as f64 / 2.0) / (w as f64 / 2.0) - sway;
                        let body = (1.0 - (u / half).powi(2)) * (1.0 - v.powf(1.6));
                        let heat = body - self.flicker * (0.15 + v) * (0.5 + field(x as f64, y as f64)).clamp(0.0, 1.0);
                        if heat > 0.06 {
                            pic.put(x, y, shade(&self.ramp, (heat - 0.06) * 1.25, x, y));
                        }
                    }
                }
                pic
            })
            .collect()
    }
}

/// A puff of smoke: it swells, drifts apart and thins to nothing over its
/// frames, which play once.
#[derive(Clone, Debug, PartialEq)]
pub struct Smoke {
    /// Darkest first.
    pub ramp: Vec<[u8; 3]>,
    pub size: u32,
    pub frames: u32,
}

impl Default for Smoke {
    fn default() -> Smoke {
        Smoke { ramp: SMOKE_RAMP.to_vec(), size: 16, frames: 10 }
    }
}

impl Smoke {
    pub fn frames(&self, seed: u32) -> Vec<Picture> {
        let billow = Perlin::new(seed);
        let (s, n) = (self.size.max(4), self.frames.max(1));
        (0..n)
            .map(|k| {
                let t = (k as f64 + 0.5) / n as f64;
                let mut pic = Picture::new(s, s);
                let radius = 0.5 + 0.5 * t.sqrt();
                let density = (1.0 - t * t) * 1.3;
                for y in 0..s {
                    for x in 0..s {
                        let (u, v) = (((x as f64 + 0.5) / s as f64 - 0.5) * 2.0, ((y as f64 + 0.5) / s as f64 - 0.5) * 2.0);
                        let lump = billow.get([x as f64 * 0.3, y as f64 * 0.3, t * 1.5]);
                        let edge = radius * (0.8 + 0.4 * lump);
                        let d = (u * u + v * v).sqrt();
                        if d < edge && dithered(density * (1.0 - 0.45 * d / edge), x, y) {
                            // Lit from the top left.
                            let light = 0.5 - 0.3 * (u + v) + 0.3 * lump;
                            pic.put(x, y, shade(&self.ramp, light, x, y));
                        }
                    }
                }
                pic
            })
            .collect()
    }
}

/// An explosion: a fireball bursts out from the middle of the frame,
/// breaks up into smoke and throws sparks; its frames play once.
#[derive(Clone, Debug, PartialEq)]
pub struct Blast {
    pub fire: Vec<[u8; 3]>,
    pub smoke: Vec<[u8; 3]>,
    pub size: u32,
    pub frames: u32,
    pub sparks: u32,
}

impl Default for Blast {
    fn default() -> Blast {
        Blast { fire: FIRE_RAMP.to_vec(), smoke: SMOKE_RAMP.to_vec(), size: 64, frames: 16, sparks: 14 }
    }
}

fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 13)
}

fn unit(a: u32, b: u32) -> f64 {
    hash(a, b) as f64 / u32::MAX as f64
}

impl Blast {
    pub fn frames(&self, seed: u32) -> Vec<Picture> {
        let boil = Perlin::new(seed);
        let tear = Perlin::new(seed.wrapping_add(83));
        let (s, n) = (self.size.max(8), self.frames.max(2));
        let c = s as f64 / 2.0;
        (0..n)
            .map(|k| {
                let t = (k as f64 + 0.5) / n as f64;
                let mut pic = Picture::new(s, s);
                // Out fast, then it hangs; the fire dies before the smoke.
                let radius = c * 1.05 * (1.0 - (1.0 - t).powi(4));
                let fire = (1.0 - t * t * 1.9).max(0.0);
                let smoke = ((1.0 - t) * 2.2).min(1.0);
                // The cloud rises a little as it cools.
                let lift = t * t * s as f64 * 0.12;
                for y in 0..s {
                    for x in 0..s {
                        let (dx, dy) = (x as f64 + 0.5 - c, y as f64 + 0.5 - c + lift);
                        let lump = boil.get([x as f64 * 0.11, y as f64 * 0.11, t * 2.2]);
                        let fine = tear.get([x as f64 * 0.3, y as f64 * 0.3, t * 3.0]);
                        let edge = radius * (0.74 + 0.3 * lump + 0.08 * fine);
                        let d = (dx * dx + dy * dy).sqrt();
                        if edge <= 0.0 || d >= edge {
                            continue;
                        }
                        let inward = 1.0 - d / edge;
                        // The fire holds in the middle and in the lumps; the rest is smoke.
                        let heat = fire * (0.55 + 1.0 * inward + 0.45 * lump) - t * 0.45 * (0.5 + fine);
                        if heat > 0.12 {
                            pic.put(x, y, shade(&self.fire, (heat - 0.12) * 1.1, x, y));
                        } else if dithered(smoke * (0.55 + 0.6 * inward), x, y) {
                            let light = 0.45 - 0.3 * (dx + dy) / edge.max(1.0) + 0.35 * lump;
                            pic.put(x, y, shade(&self.smoke, light, x, y));
                        }
                    }
                }
                for i in 0..self.sparks {
                    let angle = unit(seed, i * 2 + 1) * std::f64::consts::TAU;
                    let reach = c * (0.55 + 0.42 * unit(seed, i * 2 + 2));
                    let life = 0.45 + 0.4 * unit(seed ^ 0xA5A5, i);
                    if t > life {
                        continue;
                    }
                    let out = reach * (1.0 - (1.0 - t / life).powi(2));
                    // Sparks fall as they fly.
                    let (px, py) = (c + angle.cos() * out, c + angle.sin() * out * 0.8 + t * t * s as f64 * 0.1);
                    let hot = self.fire.len().saturating_sub(if t / life < 0.5 { 1 } else { 3 }).min(self.fire.len().saturating_sub(1));
                    if let Some(&colour) = self.fire.get(hot) {
                        pic.put(px as u32, py as u32, colour);
                        if t / life < 0.35 {
                            pic.put(px as u32 + 1, py as u32, colour);
                        }
                    }
                }
                pic
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_frames() {
        assert_eq!(Flame::default().frames(3), Flame::default().frames(3));
        assert_ne!(Flame::default().frames(3), Flame::default().frames(4));
        assert_eq!(Blast::default().frames(3), Blast::default().frames(3));
    }

    #[test]
    fn a_flame_stands_on_its_base_and_never_fills_its_tip_row() {
        let flame = Flame::default();
        for pic in flame.frames(1) {
            let lit = |y: u32| (0..pic.width).filter(|&x| pic.alpha(x, y) > 0).count();
            assert!(lit(pic.height - 1) >= 3, "a base to stand on");
            assert!(lit(0) < pic.width as usize / 2, "a tip, not a block");
        }
    }

    #[test]
    fn smoke_thins_and_an_explosion_ends_in_nothing_much() {
        let count = |p: &Picture| p.rgba.chunks(4).filter(|c| c[3] > 0).count();
        let smoke = Smoke::default().frames(1);
        let thickest = smoke.iter().map(count).max().unwrap();
        assert!(thickest > count(smoke.last().unwrap()) * 2, "the puff thins out");
        let blast = Blast::default().frames(1);
        let peak = blast.iter().map(count).max().unwrap();
        assert!(count(&blast[0]) < peak / 4, "it starts small");
        assert!(count(blast.last().unwrap()) < peak / 10, "and ends thin");
    }

    #[test]
    fn nothing_is_half_clear_and_every_colour_is_from_a_ramp() {
        let blast = Blast::default();
        for pic in blast.frames(5) {
            for c in pic.rgba.chunks(4) {
                assert!(c[3] == 0 || c[3] == 255);
                assert!(c[3] == 0 || blast.fire.contains(&[c[0], c[1], c[2]]) || blast.smoke.contains(&[c[0], c[1], c[2]]));
            }
        }
    }

    #[test]
    fn a_strip_lays_the_frames_side_by_side() {
        let frames = Flame::default().frames(2);
        let sheet = strip(&frames, 0);
        assert_eq!((sheet.width, sheet.height), (frames[0].width * frames.len() as u32, frames[0].height));
        assert_eq!(&sheet.rgba[..(frames[0].width * 4) as usize], &frames[0].rgba[..(frames[0].width * 4) as usize]);
    }
}
