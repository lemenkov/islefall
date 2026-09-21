// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! The surface of an island: one picture of ground for the whole island,
//! so that it has the large shapes tiles cannot: drifts of lighter and
//! darker growth, worn patches, and only then the grain of single pixels.

use noise::{NoiseFn, Perlin};

use crate::Picture;

/// A region of another material within the ground: dry grass, moss, bare
/// earth, snow. Where a noise field of size `scale` rises above the level
/// that leaves `coverage` of the area, the ground takes this ramp.
#[derive(Clone, Debug, PartialEq)]
pub struct Accent {
    pub ramp: Vec<[u8; 3]>,
    pub scale: f64,
    pub coverage: f64,
}

/// Single pixels of a colour scattered over the ground: flowers, stones,
/// glints, embers.
#[derive(Clone, Debug, PartialEq)]
pub struct Speck {
    pub colour: [u8; 3],
    pub per_1000px: f64,
}

/// How the ground is made. Lengths are source pixels; a cell is wider
/// than it is tall, so the vertical scale is squeezed by `squash`.
#[derive(Clone, Debug, PartialEq)]
pub struct Ground {
    /// Colours from the darkest to the lightest.
    pub ramp: Vec<[u8; 3]>,
    /// Size of the large drifts and of the smaller patches inside them.
    pub drift: f64,
    pub patch: f64,
    /// How far the drifts and the patches move the brightness (0..1).
    pub drift_strength: f64,
    pub patch_strength: f64,
    /// How much single pixels differ from their neighbours.
    pub grain: f64,
    /// Rows to columns: 11 to 16 for the original's cells.
    pub squash: f64,
    /// Tufts and clumps: small blobs, most darker, some lighter, one to a
    /// square of `clump_spacing` with probability `clump_density`.
    pub clump_spacing: f64,
    pub clump_density: f64,
    pub clump_strength: f64,
    /// Regions of other materials, the later ones over the earlier.
    pub accents: Vec<Accent>,
    pub specks: Vec<Speck>,
    /// Cracks along the borders of cells about `crack_spacing` across (0
    /// for none), a pixel wide.
    pub crack_spacing: f64,
    pub crack_colour: [u8; 3],
}

impl Default for Ground {
    fn default() -> Ground {
        Ground {
            ramp: vec![[38, 62, 22], [52, 82, 30], [66, 100, 38], [82, 118, 46], [100, 136, 56], [120, 152, 68]],
            drift: 70.0,
            patch: 17.0,
            drift_strength: 0.22,
            patch_strength: 0.12,
            grain: 0.7,
            squash: 11.0 / 16.0,
            clump_spacing: 9.0,
            clump_density: 0.45,
            clump_strength: 0.3,
            accents: vec![
                Accent { ramp: vec![[84, 106, 40], [106, 126, 50], [130, 146, 62], [150, 160, 74]], scale: 55.0, coverage: 0.16 },
                Accent { ramp: vec![[34, 58, 24], [44, 72, 30], [56, 86, 36], [68, 100, 42]], scale: 38.0, coverage: 0.14 },
            ],
            specks: vec![Speck { colour: [236, 236, 220], per_1000px: 0.7 }, Speck { colour: [240, 210, 70], per_1000px: 0.6 }, Speck { colour: [170, 120, 200], per_1000px: 0.4 }],
            crack_spacing: 0.0,
            crack_colour: [20, 20, 20],
        }
    }
}

fn hash(a: u32, b: u32, c: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B) ^ c.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

fn unit(h: u32) -> f64 {
    (h % 100_000) as f64 / 100_000.0
}

impl Ground {
    /// What the clumps add to the brightness at a pixel.
    fn clumps(&self, seed: u32, x: f64, y: f64) -> f64 {
        if self.clump_spacing <= 1.0 || self.clump_density <= 0.0 {
            return 0.0;
        }
        let (gx, gy) = ((x / self.clump_spacing).floor() as i64, (y / self.clump_spacing).floor() as i64);
        let mut sum = 0.0;
        for cy in gy - 1..=gy + 1 {
            for cx in gx - 1..=gx + 1 {
                let h = hash(seed ^ 0x00C1, cx as u32, cy as u32);
                if unit(h) >= self.clump_density {
                    continue;
                }
                let px = (cx as f64 + unit(hash(h, 1, 0))) * self.clump_spacing;
                let py = (cy as f64 + unit(hash(h, 2, 0))) * self.clump_spacing;
                let r = 1.5 + 3.0 * unit(hash(h, 3, 0));
                let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                if d < r {
                    let sign = if unit(hash(h, 4, 0)) < 0.7 { -1.0 } else { 1.0 };
                    sum += sign * self.clump_strength * (1.0 - d / r);
                }
            }
        }
        sum
    }

    /// Whether a pixel lies on a crack: near the border between the two
    /// nearest points of a jittered grid.
    fn cracked(&self, seed: u32, x: f64, y: f64) -> bool {
        if self.crack_spacing <= 2.0 {
            return false;
        }
        let s = self.crack_spacing;
        let (gx, gy) = ((x / s).floor() as i64, (y / s).floor() as i64);
        let (mut d1, mut d2) = (f64::MAX, f64::MAX);
        for cy in gy - 1..=gy + 1 {
            for cx in gx - 1..=gx + 1 {
                let h = hash(seed ^ 0x0CAC, cx as u32, cy as u32);
                let px = (cx as f64 + 0.15 + 0.7 * unit(hash(h, 1, 0))) * s;
                let py = (cy as f64 + 0.15 + 0.7 * unit(hash(h, 2, 0))) * s;
                let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                if d < d1 {
                    d2 = d1;
                    d1 = d;
                } else if d < d2 {
                    d2 = d;
                }
            }
        }
        d2 - d1 < 0.9
    }

    /// The ground of an island `width` by `height` pixels; `covered` says
    /// which pixels are ground at all. Pixels elsewhere stay clear.
    pub fn surface(&self, width: u32, height: u32, seed: u32, covered: impl Fn(u32, u32) -> bool) -> Picture {
        let mut pic = Picture::new(width, height);
        if self.ramp.is_empty() {
            return pic;
        }
        let (big, mid) = (Perlin::new(seed), Perlin::new(seed.wrapping_add(53)));
        let fields: Vec<Perlin> = (0..self.accents.len()).map(|k| Perlin::new(seed.wrapping_add(1000 + 97 * k as u32))).collect();
        for y in 0..height {
            for x in 0..width {
                if !covered(x, y) {
                    continue;
                }
                let (xf, yf) = (x as f64, y as f64 / self.squash);
                if self.cracked(seed, xf, yf) {
                    pic.put(x, y, self.crack_colour);
                    continue;
                }
                let speck = self.specks.iter().enumerate().find(|(k, s)| unit(hash(seed ^ (0x5EC + *k as u32), x, y)) < s.per_1000px / 1000.0);
                if let Some((_, s)) = speck {
                    pic.put(x, y, s.colour);
                    continue;
                }
                let drift = big.get([xf / self.drift, yf / self.drift]) + 0.5 * big.get([xf / (self.drift * 0.5), yf / (self.drift * 0.5) + 9.0]);
                let patch = mid.get([xf / self.patch, yf / self.patch]);
                // The grain is a hash, not a lattice: ordered dithering
                // shows as a weave on flat ground, and grass has none.
                let grain = unit(hash(seed, x, y)) - 0.5;
                let b = 0.5 + self.drift_strength * drift / 1.5 + self.patch_strength * patch + self.grain * grain + self.clumps(seed, xf, yf);
                // The last accent whose field rises here gives the material;
                // the grain frays its border so it is no contour line.
                let mut ramp = &self.ramp;
                for (a, field) in self.accents.iter().zip(&fields) {
                    if a.ramp.is_empty() {
                        continue;
                    }
                    let v = 0.5 + 0.5 * (field.get([xf / a.scale, yf / a.scale]) + 0.4 * field.get([xf / (a.scale * 0.4), yf / (a.scale * 0.4) + 3.0])) / 1.4;
                    // Perlin's values crowd the middle: this maps a share of area to a level.
                    let level = 0.5 + (0.5 - a.coverage.clamp(0.0, 1.0)) * 0.62;
                    // Across the border the two materials mix pixel by
                    // pixel, more of the new one the further in.
                    let inside = ((v - level) / 0.1 + 0.5).clamp(0.0, 1.0);
                    if unit(hash(seed ^ 0xACCE, x, y)) < inside * inside * (3.0 - 2.0 * inside) {
                        ramp = &a.ramp;
                    }
                }
                let steps = (ramp.len() - 1) as f64;
                let level = (b.clamp(0.0, 1.0) * steps).round() as usize;
                pic.put(x, y, ramp[level.min(ramp.len() - 1)]);
            }
        }
        pic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ground_covers_what_it_is_told_to_and_nothing_else() {
        let g = Ground::default();
        let p = g.surface(64, 44, 5, |x, y| x >= 16 && y < 33);
        for y in 0..p.height {
            for x in 0..p.width {
                let i = ((y * p.width + x) * 4) as usize;
                if x >= 16 && y < 33 {
                    assert_eq!(p.rgba[i + 3], 255);
                    let px = [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2]];
                    let known = g.ramp.contains(&px) || g.accents.iter().any(|a| a.ramp.contains(&px)) || g.specks.iter().any(|s| s.colour == px) || px == g.crack_colour;
                    assert!(known, "{px:?}");
                } else {
                    assert_eq!(p.rgba[i + 3], 0);
                }
            }
        }
        assert_eq!(p, g.surface(64, 44, 5, |x, y| x >= 16 && y < 33), "the same seed, the same ground");
    }

    #[test]
    fn the_ground_has_large_shapes_not_only_grain() {
        // Average brightness of far-apart blocks differs more than grain alone would give.
        let g = Ground::default();
        let p = g.surface(320, 220, 9, |_, _| true);
        let level = |x0: u32, y0: u32| {
            let mut sum = 0u64;
            for y in y0..y0 + 20 {
                for x in x0..x0 + 20 {
                    let i = ((y * p.width + x) * 4) as usize;
                    sum += p.rgba[i + 1] as u64;
                }
            }
            sum as f64 / 400.0
        };
        let blocks: Vec<f64> = (0..5).flat_map(|by| (0..7).map(move |bx| (bx * 44, by * 40))).map(|(x, y)| level(x, y)).collect();
        let (lo, hi) = (blocks.iter().cloned().fold(f64::MAX, f64::min), blocks.iter().cloned().fold(f64::MIN, f64::max));
        assert!(hi - lo > 8.0, "drifts of lighter and darker ground: {lo:.1}..{hi:.1}");
    }
}
