// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! The underside of an island: a strip of rock hanging from a run of its
//! bottom edge, ending in stalactites.

use noise::{NoiseFn, Perlin};

use crate::{Picture, shade};

/// How the rock is made. Lengths are source pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Rock {
    /// Colours from the darkest to the lightest.
    pub ramp: Vec<[u8; 3]>,
    /// How far the rock hangs at least and at most, away from the ends.
    pub depth_min: u32,
    pub depth_max: u32,
    /// How far it hangs at the two ends of the run, and over how many
    /// pixels it grows from that to its full depth.
    pub end_depth: u32,
    pub taper: u32,
    /// Width of the big lobes and of the teeth along their lower edge.
    pub lobe: f64,
    pub tooth: f64,
    /// Windows of dwellings in the rock: how many per hundred pixels of
    /// run, whether they are lit, and the colour of a lit pane.
    pub windows_per_100px: f64,
    pub lit: bool,
    pub pane: [u8; 3],
}

impl Default for Rock {
    fn default() -> Rock {
        Rock {
            ramp: vec![[20, 10, 4], [44, 22, 8], [70, 36, 12], [98, 52, 18], [126, 70, 24], [152, 90, 34], [178, 112, 46], [200, 136, 64]],
            depth_min: 22,
            depth_max: 42,
            end_depth: 7,
            taper: 14,
            lobe: 46.0,
            tooth: 9.0,
            windows_per_100px: 1.2,
            lit: true,
            pane: [255, 222, 96],
        }
    }
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A cheap hash for the odd decision that noise would make too smooth.
fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

impl Rock {
    /// How far the rock hangs under each pixel column of a run `width` wide.
    pub fn depths(&self, width: u32, seed: u32) -> Vec<u32> {
        let big = Perlin::new(seed);
        let small = Perlin::new(seed.wrapping_add(101));
        (0..width)
            .map(|x| {
                let xf = x as f64;
                // Lobes swell and shrink slowly; teeth are ridges turned
                // downwards and sharpened, so they end in points.
                let lobes = 0.5 + 0.5 * (big.get([xf / self.lobe, 0.37]) + 0.5 * big.get([xf / (self.lobe * 0.45), 7.1])) / 1.5;
                let ridge = 1.0 - small.get([xf / self.tooth, 3.3]).abs();
                let teeth = ridge.powf(2.6);
                let edge = (x.min(width - 1 - x) as f64 / self.taper.max(1) as f64).min(1.0);
                let full = self.depth_min as f64 + (self.depth_max.saturating_sub(self.depth_min)) as f64 * (0.6 * lobes + 0.4 * teeth);
                let d = self.end_depth as f64 + smoothstep(edge) * (full - self.end_depth as f64).max(0.0);
                d.round().max(1.0) as u32
            })
            .collect()
    }

    /// The rock under a run of bottom edge `width` pixels wide: a shallow
    /// back wall in shade, and in front of it stalactites side by side,
    /// each its own width, length and tone, lit from the left, dark on
    /// the right, narrowing to a point.
    pub fn underside(&self, width: u32, seed: u32) -> Picture {
        let envelope = self.depths(width, seed);
        let height = self.depth_max.max(1);
        let mut pic = Picture::new(width, height);
        let grain = Perlin::new(seed.wrapping_add(211));
        let darkest = self.ramp.first().copied().unwrap_or([0, 0, 0]);
        // The mass of the rock: most of the envelope's depth, lit from the
        // left across each lobe, so the upper rock reads as one body.
        let at = |x: i64| envelope[x.clamp(0, width as i64 - 1) as usize] as f64;
        for x in 0..width {
            let d = (envelope[x as usize] as f64 * 0.72).round() as u32;
            let facing = ((at(x as i64 + 3) - at(x as i64 - 3)) / 6.0 * 0.16).clamp(-0.2, 0.16);
            let strata = 0.1 * grain.get([x as f64 / 3.1, 0.5]);
            for y in 0..d {
                let v = y as f64 / d.max(1) as f64;
                let mut b = 0.5 - 0.32 * v + facing + strata + 0.07 * grain.get([x as f64 / 2.2, y as f64 / 2.6]);
                if y < 2 {
                    b -= 0.18;
                }
                pic.put(x, y, if y + 1 == d { darkest } else { shade(&self.ramp, b, x, y) });
            }
        }
        // Stalactites grow out of the mass: overlapping, of many widths,
        // their sides wandering, their ends blunt. Drawn back to front.
        let wobble = Perlin::new(seed.wrapping_add(307));
        let count = (width as f64 / 5.5).ceil() as u32;
        let mut order: Vec<u32> = (0..count).collect();
        order.sort_by_key(|&k| hash(seed ^ 0x51, k));
        for k in order {
            let centre0 = (hash(seed, k * 5 + 1) % width.max(1)) as f64;
            let w = 4.0 + (hash(seed, k * 5 + 2) % 90) as f64 / 10.0;
            let mid = centre0.round().clamp(0.0, width as f64 - 1.0) as usize;
            let r = hash(seed, k * 5 + 3) % 100;
            let reach = if r < 14 { 1.0 } else { 0.62 + 0.3 * (r as f64 / 100.0) };
            let len = ((envelope[mid] as f64 * reach).round() as u32).clamp(3, height);
            let tone = 0.36 + 0.24 * (hash(seed, k * 5 + 4) % 100) as f64 / 100.0;
            let start = (envelope[mid] as f64 * 0.3) as u32;
            for y in start.min(len - 1)..len {
                let v = (y - start.min(len - 1)) as f64 / (len - start.min(len - 1)).max(1) as f64;
                // Blunt: it keeps a pixel or two of width to the very end.
                let half = (w / 2.0) * (1.0 - v.powf(1.7)) + 0.6;
                let centre = centre0 + 1.6 * wobble.get([k as f64 * 3.7, y as f64 / 6.0]);
                let (left, right) = ((centre - half).round() as i64, (centre + half).round() as i64 - 1);
                for x in left.max(0)..=right.min(width as i64 - 1) {
                    let u = if right > left { (x - left) as f64 / (right - left) as f64 } else { 0.5 };
                    let b = tone + 0.2 * (1.0 - 2.0 * u) - 0.3 * (y as f64 / len as f64) + 0.06 * grain.get([x as f64 / 1.5, y as f64 / 2.5]);
                    let last = y + 1 == len;
                    let edge = (x == right && right > left) || last;
                    pic.put(x as u32, y, if edge { darkest } else { shade(&self.ramp, b, x as u32, y) });
                }
            }
        }
        self.windows(&mut pic, &envelope, seed);
        pic
    }

    /// Little windows in the upper rock, four panes in a dark frame.
    fn windows(&self, pic: &mut Picture, depths: &[u32], seed: u32) {
        let width = pic.width;
        let count = (self.windows_per_100px * width as f64 / 100.0).floor() as u32;
        let frame = self.ramp.first().copied().unwrap_or([0, 0, 0]);
        let pane = if self.lit { self.pane } else { self.ramp.get(1).copied().unwrap_or(frame) };
        for k in 0..count {
            if width < self.taper * 2 + 8 {
                break;
            }
            let span = width - self.taper * 2 - 5;
            let x0 = self.taper + hash(seed, k * 2 + 1) % span.max(1);
            let y0 = 3 + hash(seed, k * 2 + 2) % 4;
            // Only where the rock is deep enough to hold a room.
            if (x0..x0 + 5).any(|x| depths[x as usize] < y0 + 9) {
                continue;
            }
            for dy in 0..5 {
                for dx in 0..5 {
                    let glass = dx % 2 == 1 && dy % 2 == 1;
                    pic.put(x0 + dx, y0 + dy, if glass { pane } else { frame });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_rock() {
        let r = Rock::default();
        assert_eq!(r.underside(160, 7), r.underside(160, 7));
        assert_ne!(r.underside(160, 7), r.underside(160, 8));
    }

    #[test]
    fn the_rock_tapers_at_its_ends_and_stays_within_its_depths() {
        let r = Rock::default();
        let d = r.depths(240, 3);
        assert_eq!(d.len(), 240);
        assert!(d[0] <= r.end_depth + 1 && d[239] <= r.end_depth + 1, "{} {}", d[0], d[239]);
        assert!(d.iter().all(|&v| v >= 1 && v <= r.depth_max));
        assert!(d[40..200].iter().any(|&v| v > r.depth_min), "the middle hangs deeper than the ends");
    }

    #[test]
    fn every_pixel_is_a_ramp_colour_a_pane_or_clear() {
        let r = Rock::default();
        let p = r.underside(200, 11);
        assert_eq!(p.height, r.depth_max);
        let mut opaque = 0;
        for x in 0..p.width {
            for y in 0..p.height {
                let i = ((y * p.width + x) * 4) as usize;
                let px = [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2]];
                match p.rgba[i + 3] {
                    255 => {
                        opaque += 1;
                        assert!(r.ramp.contains(&px) || px == r.pane, "{px:?} at {x},{y}");
                    }
                    0 => {}
                    a => panic!("half-clear pixel {a} at {x},{y}: pixel art has none"),
                }
            }
        }
        assert!(opaque > 1500, "there is rock: {opaque}");
        // The ends hang less far than the middle.
        let lowest = |xs: std::ops::Range<u32>| xs.map(|x| (0..p.height).rev().find(|&y| p.alpha(x, y) > 0).unwrap_or(0)).max().unwrap_or(0);
        assert!(lowest(0..6) < lowest(60..140), "{} {}", lowest(0..6), lowest(60..140));
    }
}
