// SPDX-License-Identifier: Apache-2.0
//! Pictures outside the sprite cache: the palette-indexed GIF cloud
//! textures the sky is made of (first frame only), and PNGs a mod may
//! bring instead.

use std::path::Path;

/// An 8-bit image with its own palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    /// One palette index per pixel, row-major.
    pub indices: Vec<u8>,
    /// The file's palette, RGB triples.
    pub palette: Vec<[u8; 3]>,
    /// Index drawn as transparent, if the file names one.
    pub transparent: Option<u8>,
}

#[derive(Debug)]
pub enum GifError {
    Io(std::io::Error),
    Decode(String),
    Empty,
}

impl std::fmt::Display for GifError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GifError::Io(e) => write!(f, "{e}"),
            GifError::Decode(e) => write!(f, "gif: {e}"),
            GifError::Empty => write!(f, "gif has no frames"),
        }
    }
}

impl std::error::Error for GifError {}

impl IndexedImage {
    pub fn load(path: impl AsRef<Path>) -> Result<IndexedImage, GifError> {
        let file = std::fs::File::open(path).map_err(GifError::Io)?;
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::Indexed);
        let mut decoder = options.read_info(file).map_err(|e| GifError::Decode(e.to_string()))?;
        let global = decoder.global_palette().map(<[u8]>::to_vec);
        let frame = decoder.read_next_frame().map_err(|e| GifError::Decode(e.to_string()))?.ok_or(GifError::Empty)?;
        let palette_bytes = frame.palette.clone().or(global).unwrap_or_default();
        let palette = palette_bytes.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        Ok(IndexedImage {
            width: frame.width as u32,
            height: frame.height as u32,
            indices: frame.buffer.to_vec(),
            palette,
            transparent: frame.transparent,
        })
    }

    /// RGBA pixels, row-major, through the file's own palette.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.indices.len() * 4);
        for &i in &self.indices {
            let [r, g, b] = self.palette.get(i as usize).copied().unwrap_or([0, 0, 0]);
            let a = if self.transparent == Some(i) { 0 } else { 255 };
            out.extend_from_slice(&[r, g, b, a]);
        }
        out
    }
}

/// A decoded picture ready for the GPU: RGBA, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Picture {
    /// Load a GIF (through its own palette) or, with the `png` feature, a PNG.
    pub fn load(path: impl AsRef<Path>) -> Result<Picture, GifError> {
        let path = path.as_ref();
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        match ext.as_str() {
            "gif" => {
                let img = IndexedImage::load(path)?;
                Ok(Picture { width: img.width, height: img.height, rgba: img.to_rgba() })
            }
            #[cfg(feature = "png")]
            "png" => Picture::load_png(path),
            other => Err(GifError::Decode(format!("unsupported picture format .{other}"))),
        }
    }

    #[cfg(feature = "png")]
    fn load_png(path: &Path) -> Result<Picture, GifError> {
        let file = std::fs::File::open(path).map_err(GifError::Io)?;
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().map_err(|e| GifError::Decode(e.to_string()))?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(|e| GifError::Decode(e.to_string()))?;
        buf.truncate(info.buffer_size());
        let rgba = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
            other => return Err(GifError::Decode(format!("png colour type {other:?} after expansion"))),
        };
        Ok(Picture { width: info.width, height: info.height, rgba })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_cloud_textures_when_the_data_is_there() {
        let Some(dir) = std::env::var_os("NETSTORM_DIR") else { return };
        let img = IndexedImage::load(Path::new(&dir).join("d").join("Gifcloud.gif")).unwrap();
        assert_eq!((img.width, img.height), (512, 512));
        assert_eq!(img.indices.len(), 512 * 512);
        assert_eq!(img.palette.len(), 256);
        assert_eq!(img.to_rgba().len(), 512 * 512 * 4);
        let pic = Picture::load(Path::new(&dir).join("d").join("Gifcloud2.GIF")).unwrap();
        assert_eq!((pic.width, pic.height, pic.rgba.len()), (512, 512, 512 * 512 * 4));
    }
}
