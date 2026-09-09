// SPDX-License-Identifier: Apache-2.0
//! Turn decoded NetStorm frames into a Bevy texture atlas.
//!
//! Each frame's stored block is packed as its own atlas rectangle, and the
//! frame remembers where its hotspot lies inside that block. A sprite showing
//! the frame uses an [`Anchor`] derived from the hotspot, so the entity's
//! `Transform` always marks the hotspot (a walker's feet, an explosion's
//! centre) whatever the block size of the current frame.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler, TextureAtlasLayout};
use bevy::math::{URect, UVec2, Vec2};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use islefall_data::{Frame, Palette, Picture, ShapeFile, Sheet};

/// Largest atlas edge we are willing to build.
const MAX_ATLAS_EDGE: u32 = 8192;
/// Blocks are packed on shelves no wider than this.
const SHELF_WIDTH: u32 = 2048;
/// Gap between packed blocks, to keep nearest-neighbour sampling clean.
const PADDING: u32 = 1;

/// Where one frame lives in the atlas and how to anchor it.
#[derive(Clone, Copy, Debug)]
pub struct FrameInfo {
    /// Block size in pixels.
    pub size: UVec2,
    /// Hotspot position measured from the block's top-left corner, in pixels.
    pub hotspot: Vec2,
}

impl FrameInfo {
    /// Sprite anchor that places the entity's transform on the hotspot.
    /// Bevy anchors are normalised to -0.5..0.5 with y pointing up.
    pub fn anchor(&self) -> Anchor {
        if self.size.x == 0 || self.size.y == 0 {
            return Anchor::CENTER;
        }
        Anchor(Vec2::new(
            self.hotspot.x / self.size.x as f32 - 0.5,
            0.5 - self.hotspot.y / self.size.y as f32,
        ))
    }
}

/// A packed shape: one atlas image plus per-frame placement.
pub struct ShapeAtlas {
    pub image: Image,
    pub layout: TextureAtlasLayout,
    /// One entry per container entry, in animation order.
    pub frames: Vec<FrameInfo>,
}

#[derive(Debug)]
pub enum AtlasError {
    NoFrames,
    TooLarge { width: u32, height: u32 },
    /// A sheet frame's rect lies outside its picture.
    BadRect(usize),
}

impl std::fmt::Display for AtlasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtlasError::NoFrames => write!(f, "container has no decodable frames"),
            AtlasError::TooLarge { width, height } => write!(f, "atlas {width}x{height} exceeds limits"),
            AtlasError::BadRect(i) => write!(f, "sheet frame {i} lies outside the picture"),
        }
    }
}

impl std::error::Error for AtlasError {}

/// Pack the records at `offsets` into an atlas, one cell per offset in
/// order. Records that fail to decode or are empty become 1x1 transparent
/// cells so atlas indices stay aligned with frame indices.
pub fn build_atlas(shp: &ShapeFile, palette: &Palette, offsets: &[usize]) -> Result<ShapeAtlas, AtlasError> {
    let frames: Vec<Option<Frame>> = offsets.iter().map(|&o| shp.decode(o).ok()).collect();
    if frames.iter().all(|f| f.as_ref().is_none_or(Frame::is_empty)) {
        return Err(AtlasError::NoFrames);
    }

    // Shelf packing in entry order: blocks of one shape are similar in size.
    let sizes: Vec<UVec2> = frames
        .iter()
        .map(|f| match f {
            Some(fr) if !fr.is_empty() => UVec2::new(fr.width as u32, fr.height as u32),
            _ => UVec2::ONE,
        })
        .collect();
    let mut rects = Vec::with_capacity(sizes.len());
    let (mut x, mut y, mut shelf_h, mut width) = (0u32, 0u32, 0u32, 0u32);
    for s in &sizes {
        if x > 0 && x + s.x > SHELF_WIDTH {
            y += shelf_h + PADDING;
            x = 0;
            shelf_h = 0;
        }
        rects.push(URect::new(x, y, x + s.x, y + s.y));
        x += s.x + PADDING;
        shelf_h = shelf_h.max(s.y);
        width = width.max(x - PADDING);
    }
    let height = y + shelf_h;
    if width > MAX_ATLAS_EDGE || height > MAX_ATLAS_EDGE {
        return Err(AtlasError::TooLarge { width, height });
    }

    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut layout = TextureAtlasLayout::new_empty(UVec2::new(width, height));
    let mut infos = Vec::with_capacity(frames.len());
    for (fr, rect) in frames.iter().zip(&rects) {
        layout.add_texture(*rect);
        let Some(fr) = fr.as_ref().filter(|f| !f.is_empty()) else {
            infos.push(FrameInfo { size: UVec2::ZERO, hotspot: Vec2::ZERO });
            continue;
        };
        for py in 0..fr.height {
            for px in 0..fr.width {
                let Some(idx) = fr.pixel(px, py) else { continue };
                let [r, g, b] = palette.rgb(idx);
                let o = (((rect.min.y as usize + py) * width as usize) + rect.min.x as usize + px) * 4;
                rgba[o..o + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
        infos.push(FrameInfo {
            size: UVec2::new(fr.width as u32, fr.height as u32),
            hotspot: Vec2::new(
                (fr.header.hot_x as i32 - fr.block_x) as f32,
                (fr.header.hot_y as i32 - fr.block_y) as f32,
            ),
        });
    }

    let mut image = Image::new(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    Ok(ShapeAtlas { image, layout, frames: infos })
}

/// A mod's sheet as atlases: the picture as it is, with one layout for the
/// frames and, when any frame has one, another for the shadows.
pub fn build_sheet_atlases(pic: &Picture, sheet: &Sheet) -> Result<(ShapeAtlas, Option<ShapeAtlas>), AtlasError> {
    if sheet.frames.is_empty() {
        return Err(AtlasError::NoFrames);
    }
    let size = UVec2::new(pic.width, pic.height);
    let place = |layout: &mut TextureAtlasLayout, i: usize, rect: Option<[u32; 4]>, hotspot: Option<[i32; 2]>| -> Result<FrameInfo, AtlasError> {
        match rect {
            Some([x, y, w, h]) if w > 0 && h > 0 => {
                if x + w > pic.width || y + h > pic.height {
                    return Err(AtlasError::BadRect(i));
                }
                layout.add_texture(URect::new(x, y, x + w, y + h));
                let hs = hotspot.unwrap_or([0, 0]);
                Ok(FrameInfo { size: UVec2::new(w, h), hotspot: Vec2::new(hs[0] as f32, hs[1] as f32) })
            }
            _ => {
                layout.add_texture(URect::new(0, 0, 1, 1));
                Ok(FrameInfo { size: UVec2::ZERO, hotspot: Vec2::ZERO })
            }
        }
    };
    let image = || {
        let mut image = Image::new(
            Extent3d { width: pic.width, height: pic.height, depth_or_array_layers: 1 },
            TextureDimension::D2,
            pic.rgba.clone(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::nearest();
        image
    };
    let mut layout = TextureAtlasLayout::new_empty(size);
    let mut frames = Vec::with_capacity(sheet.frames.len());
    for (i, f) in sheet.frames.iter().enumerate() {
        frames.push(place(&mut layout, i, Some(f.rect), Some(f.hotspot))?);
    }
    let main = ShapeAtlas { image: image(), layout, frames };
    let shadow = if sheet.frames.iter().any(|f| f.shadow.is_some()) {
        let mut layout = TextureAtlasLayout::new_empty(size);
        let mut frames = Vec::with_capacity(sheet.frames.len());
        for (i, f) in sheet.frames.iter().enumerate() {
            frames.push(place(&mut layout, i, f.shadow, f.shadow_hotspot)?);
        }
        Some(ShapeAtlas { image: image(), layout, frames })
    } else {
        None
    };
    Ok((main, shadow))
}
