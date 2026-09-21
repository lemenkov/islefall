// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Generated art: pictures made from a seed, in the manner of the pixel
//! art they stand beside (source-pixel resolution, a short colour ramp,
//! ordered dithering, light from the top left, no smoothing). Nothing here
//! knows about the engine: a picture is a buffer of RGBA bytes.

pub mod ground;
pub mod rock;

/// A picture: `width * height` RGBA pixels, row by row from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Picture {
    pub fn new(width: u32, height: u32) -> Picture {
        Picture { width, height, rgba: vec![0; (width * height * 4) as usize] }
    }

    pub fn put(&mut self, x: u32, y: u32, rgb: [u8; 3]) {
        if x < self.width && y < self.height {
            let i = ((y * self.width + x) * 4) as usize;
            self.rgba[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }

    pub fn alpha(&self, x: u32, y: u32) -> u8 {
        if x < self.width && y < self.height { self.rgba[((y * self.width + x) * 4 + 3) as usize] } else { 0 }
    }
}

/// The colour of `ramp` (darkest first) for a brightness of 0..1, dithered
/// by the pixel's place so that few colours carry a gradient.
pub fn shade(ramp: &[[u8; 3]], brightness: f64, x: u32, y: u32) -> [u8; 3] {
    const BAYER: [[f64; 4]; 4] = [[0.0, 8.0, 2.0, 10.0], [12.0, 4.0, 14.0, 6.0], [3.0, 11.0, 1.0, 9.0], [15.0, 7.0, 13.0, 5.0]];
    if ramp.is_empty() {
        return [0, 0, 0];
    }
    let steps = (ramp.len() - 1) as f64;
    let threshold = (BAYER[(y % 4) as usize][(x % 4) as usize] + 0.5) / 16.0 - 0.5;
    let level = (brightness.clamp(0.0, 1.0) * steps + threshold * 0.9).round().clamp(0.0, steps);
    ramp[level as usize]
}
