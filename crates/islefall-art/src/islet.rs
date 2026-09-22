// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! The islet a unit makes for itself in the sky: a knob of ground with a
//! rim in its owner's colour, and rock hanging underneath with a band of
//! that colour where the two meet.

use noise::{NoiseFn, Perlin};

use crate::Picture;
use crate::ground::Ground;
use crate::rock::Rock;

#[derive(Clone, Debug, PartialEq)]
pub struct Islet {
    pub ground: Ground,
    pub rock: Rock,
    /// The owner's colour, lit and shaded.
    pub light: [u8; 3],
    pub dark: [u8; 3],
    /// The footprint the top covers.
    pub width: u32,
    pub height: u32,
    /// How far the outline wanders in from a full ellipse, 0..1.
    pub wobble: f64,
    /// Rows of the owner's band at the top of the rock.
    pub band: u32,
}

impl Islet {
    /// Whether a pixel of the top is ground: a rounded shape filling the
    /// footprint, its outline wandering with noise.
    fn covered(&self, x: u32, y: u32, wander: &Perlin) -> bool {
        let (w, h) = (self.width as f64, self.height as f64);
        let (u, v) = ((x as f64 + 0.5 - w / 2.0) / (w / 2.0), (y as f64 + 0.5 - h / 2.0) / (h / 2.0));
        let angle = v.atan2(u);
        let edge = 1.0 - self.wobble * (0.5 + 0.5 * wander.get([angle.cos() * 1.3, angle.sin() * 1.3]));
        u.abs().powf(2.6) + v.abs().powf(2.6) < edge.powf(2.6)
    }

    /// The top and the rock under it, both `width` wide: the rock picture
    /// starts at the top's bottom row and hangs from its outline.
    pub fn pictures(&self, seed: u32) -> (Picture, Picture) {
        let wander = Perlin::new(seed.wrapping_add(41));
        let (w, h) = (self.width.max(4), self.height.max(4));
        let mask: Vec<bool> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| self.covered(x, y, &wander)).collect();
        let at = |x: u32, y: u32| x < w && y < h && mask[(y * w + x) as usize];
        let mut top = self.ground.surface(w, h, seed, |x, y| at(x, y));
        // The rim: ground at the outline, lit above and to the left.
        for y in 0..h {
            for x in 0..w {
                if !at(x, y) {
                    continue;
                }
                let open_up = y == 0 || !at(x, y - 1);
                let open_left = x == 0 || !at(x - 1, y);
                let open_down = y + 1 >= h || !at(x, y + 1);
                let open_right = x + 1 >= w || !at(x + 1, y);
                if open_up || open_left {
                    top.put(x, y, self.light);
                } else if open_down || open_right {
                    top.put(x, y, self.dark);
                } else if y + 2 >= h || !at(x, y + 2) {
                    // A second row of shade at the lower lip.
                    if (x + y) % 2 == 0 {
                        top.put(x, y, self.dark);
                    }
                }
            }
        }
        // The rock hangs from the lowest ground of each column.
        let bottom: Vec<Option<u32>> = (0..w).map(|x| (0..h).rev().find(|&y| at(x, y))).collect();
        let rock = self.rock.underside(w, seed);
        let mut under = Picture::new(w, h + rock.height);
        for x in 0..w {
            let Some(b) = bottom[x as usize] else { continue };
            for ry in 0..rock.height {
                let i = ((ry * rock.width + x) * 4) as usize;
                if rock.rgba[i + 3] == 0 {
                    continue;
                }
                let y = b + 1 + ry;
                let colour = if ry < self.band {
                    if (x + ry) % 2 == 0 { self.dark } else { self.light }
                } else {
                    [rock.rgba[i], rock.rgba[i + 1], rock.rgba[i + 2]]
                };
                under.put(x, y, colour);
            }
        }
        (top, under)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn islet() -> Islet {
        Islet {
            ground: Ground { ramp: vec![[40, 70, 30], [60, 100, 40], [90, 140, 60]], ..Ground::default() },
            rock: Rock { windows_per_100px: 0.0, ..Rock::default() },
            light: [240, 200, 60],
            dark: [140, 100, 20],
            width: 48,
            height: 33,
            wobble: 0.2,
            band: 3,
        }
    }

    #[test]
    fn the_top_is_a_rounded_knob_with_a_coloured_rim() {
        let (top, under) = islet().pictures(5);
        assert_eq!((top.width, top.height), (48, 33));
        assert_eq!(top.alpha(0, 0), 0, "the corners are sky");
        assert!(top.alpha(24, 16) > 0, "the middle is ground");
        let lit = top.rgba.chunks(4).filter(|c| c[3] > 0 && [c[0], c[1], c[2]] == [240, 200, 60]).count();
        assert!(lit > 20, "a lit rim");
        assert!(under.height > top.height && under.rgba.chunks(4).any(|c| c[3] > 0), "rock hangs underneath");
        assert_eq!(islet().pictures(5), islet().pictures(5));
    }
}
