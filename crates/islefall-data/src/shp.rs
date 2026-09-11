// SPDX-License-Identifier: Apache-2.0
//! Decoder for NetStorm's `_shapes.shp` sprite cache.
//!
//! Layout summary (full description in `docs/FORMATS.md`):
//!
//! * The file starts with containers packed back to back: magic `1.10`,
//!   `u32 count`, then `count` entries of `(u32 offset, u32 zero)`. Entry
//!   offsets are relative to the container's own start.
//! * Each frame record is a 24-byte header, run-length rows, 0..3 zero pad
//!   bytes to a 4-byte boundary, and a 36-byte trailer. The trailer is
//!   recognised by its content (six floats equal to six shorts scaled by
//!   16 and 11), which is what delimits the record.
//! * Rows store only the frame's bounding box, placed on the shape's canvas
//!   via the hotspot and the `dx`/`dy` header fields.
//!
//! Safety rules: the block size comes from the decoded rows, never from the
//! header; rasters are capped by [`MAX_DIM`] and [`MAX_PIXELS`]; every offset
//! is validated before use.

use std::path::Path;
use thiserror::Error;

/// Largest width or height accepted for any raster.
pub const MAX_DIM: usize = 2048;
/// Largest pixel count accepted for any raster.
pub const MAX_PIXELS: usize = 4_000_000;

const MAGIC: &[u8; 4] = b"1.10";
const HEADER_LEN: usize = 24;
const TRAILER_LEN: usize = 36;
const MAX_ENTRIES: u32 = 100_000;
/// Trailer floats must match their shorts to within this tolerance.
const TRAILER_TOLERANCE: f32 = 0.15;

#[derive(Debug, Error)]
pub enum ShpError {
    #[error("{0}")]
    Io(std::io::Error),
    /// No container magic at offset 0.
    #[error("no container magic at offset 0")]
    NoContainers,
    #[error("container {container}: implausible entry count {count}")]
    ImplausibleCount { container: usize, count: u32 },
    /// A container entry points outside the file.
    #[error("container {container} entry {entry}: offset {offset:#x} outside file")]
    EntryOutOfFile { container: usize, entry: usize, offset: u64 },
    /// The offset is not one referenced by any container table.
    #[error("{offset:#x}: not a table target")]
    UnknownRecord { offset: usize },
    /// The record cannot hold a header and a trailer before the next one.
    #[error("{offset:#x}: record too small")]
    RecordTooSmall { offset: usize },
    /// Rows continued to the end of the record without a trailer.
    #[error("{offset:#x}: no trailer found")]
    NoTrailer { offset: usize },
    /// A row's opcodes ran past the record's data.
    #[error("{offset:#x}: row {row} overruns record")]
    RowOverrun { offset: usize, row: usize },
    /// Decoded block exceeds [`MAX_DIM`] or [`MAX_PIXELS`].
    #[error("{offset:#x}: refusing {width}x{height} raster")]
    TooLarge { offset: usize, width: usize, height: usize },
}

impl From<std::io::Error> for ShpError {
    fn from(e: std::io::Error) -> Self {
        ShpError::Io(e)
    }
}

/// The 24-byte frame header. Field names are ours; see `docs/FORMATS.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    /// Height of the shape's full box, shared by all frames of the shape.
    pub box_h: u16,
    /// Width of the full box minus one.
    pub box_w1: u16,
    /// Hotspot row inside the box.
    pub hot_y: i16,
    /// Hotspot column inside the box.
    pub hot_x: i16,
    /// X of the stored block's left edge relative to the hotspot.
    pub dx: i32,
    /// Y of the stored block's top edge relative to the hotspot.
    pub dy: i32,
    /// Unknown small integer.
    pub n: u32,
    /// Unknown small integer.
    pub flag: u32,
}

impl FrameHeader {
    fn parse(b: &[u8]) -> FrameHeader {
        FrameHeader {
            box_h: u16::from_le_bytes([b[0], b[1]]),
            box_w1: u16::from_le_bytes([b[2], b[3]]),
            hot_y: i16::from_le_bytes([b[4], b[5]]),
            hot_x: i16::from_le_bytes([b[6], b[7]]),
            dx: i32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            dy: i32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            n: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
            flag: u32::from_le_bytes([b[20], b[21], b[22], b[23]]),
        }
    }

    /// Size of the shape's canvas, `(width, height)`.
    pub fn canvas_size(&self) -> (usize, usize) {
        (self.box_w1 as usize + 1, self.box_h as usize + 1)
    }

    /// Placeholder records mark empty frames with a zero box or sentinel offsets.
    pub fn is_placeholder(&self) -> bool {
        self.box_h == 0 || self.box_w1 == 0 || self.dx.unsigned_abs() >= 1 << 30 || self.dy.unsigned_abs() >= 1 << 30
    }
}

/// One decoded frame: the stored block plus where it sits on the shape canvas.
#[derive(Clone, Debug)]
pub struct Frame {
    pub header: FrameHeader,
    /// Width of the stored block (0 for an empty frame).
    pub width: usize,
    /// Height of the stored block (0 for an empty frame).
    pub height: usize,
    /// Column of the block's left edge on the canvas. May be negative.
    pub block_x: i32,
    /// Row of the block's top edge on the canvas. May be negative.
    pub block_y: i32,
    /// Palette indices, row-major, `width * height` entries.
    pub indices: Vec<u8>,
    /// `true` where a pixel was stored, `false` where transparent.
    pub opaque: Vec<bool>,
}

impl Frame {
    fn empty(header: FrameHeader) -> Frame {
        Frame {
            header,
            width: 0,
            height: 0,
            block_x: 0,
            block_y: 0,
            indices: Vec::new(),
            opaque: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Whether the block lies entirely inside the shape canvas.
    pub fn fits_canvas(&self) -> bool {
        let (cw, ch) = self.header.canvas_size();
        self.block_x >= 0
            && self.block_y >= 0
            && self.block_x as usize + self.width <= cw
            && self.block_y as usize + self.height <= ch
    }

    /// Palette index at block coordinates, `None` if transparent or outside.
    pub fn pixel(&self, x: usize, y: usize) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = y * self.width + x;
        self.opaque[i].then(|| self.indices[i])
    }
}

/// A parsed `_shapes.shp`: container tables plus the raw bytes.
pub struct ShapeFile {
    data: Vec<u8>,
    containers: Vec<Vec<usize>>,
    /// Sorted, deduplicated record offsets from all tables.
    records: Vec<usize>,
    table_end: usize,
}

impl ShapeFile {
    pub fn load(path: impl AsRef<Path>) -> Result<ShapeFile, ShpError> {
        ShapeFile::parse(std::fs::read(path)?)
    }

    /// Parse the container tables. Frames are decoded lazily by [`ShapeFile::decode`].
    pub fn parse(data: Vec<u8>) -> Result<ShapeFile, ShpError> {
        let len = data.len();
        let mut containers = Vec::new();
        let mut o = 0usize;
        while o + 8 <= len && &data[o..o + 4] == MAGIC {
            let ci = containers.len();
            let count = u32::from_le_bytes([data[o + 4], data[o + 5], data[o + 6], data[o + 7]]);
            let table_len = 8 + 8 * count as usize;
            if count > MAX_ENTRIES || o + table_len > len {
                return Err(ShpError::ImplausibleCount { container: ci, count });
            }
            let mut entries = Vec::with_capacity(count as usize);
            for i in 0..count as usize {
                let e = o + 8 + 8 * i;
                let rel = u32::from_le_bytes([data[e], data[e + 1], data[e + 2], data[e + 3]]) as u64;
                let abs = o as u64 + rel;
                if abs >= len as u64 {
                    return Err(ShpError::EntryOutOfFile { container: ci, entry: i, offset: abs });
                }
                entries.push(abs as usize);
            }
            containers.push(entries);
            o += table_len;
        }
        if containers.is_empty() {
            return Err(ShpError::NoContainers);
        }
        let mut records: Vec<usize> = containers.iter().flatten().copied().collect();
        records.sort_unstable();
        records.dedup();
        Ok(ShapeFile { data, containers, records, table_end: o })
    }

    pub fn container_count(&self) -> usize {
        self.containers.len()
    }

    /// Record offsets of one container, in animation order. Offsets may repeat.
    pub fn container(&self, index: usize) -> &[usize] {
        &self.containers[index]
    }

    pub fn containers(&self) -> &[Vec<usize>] {
        &self.containers
    }

    /// Sorted unique record offsets referenced by any table.
    pub fn records(&self) -> &[usize] {
        &self.records
    }

    /// Upper bound of the record at `offset`: the next referenced record or EOF.
    fn record_bound(&self, offset: usize) -> Result<usize, ShpError> {
        match self.records.binary_search(&offset) {
            Ok(i) => Ok(self.records.get(i + 1).copied().unwrap_or(self.data.len())),
            Err(_) => Err(ShpError::UnknownRecord { offset }),
        }
    }

    /// Decode the frame record at `offset` (a value from a container table).
    pub fn decode(&self, offset: usize) -> Result<Frame, ShpError> {
        let bound = self.record_bound(offset)?;
        if offset < self.table_end || offset + HEADER_LEN + TRAILER_LEN > bound {
            return Err(ShpError::RecordTooSmall { offset });
        }
        let data = &self.data;
        let header = FrameHeader::parse(&data[offset..offset + HEADER_LEN]);
        if header.is_placeholder() {
            return Ok(Frame::empty(header));
        }

        // Rows are decoded into spans first; the raster is sized afterwards.
        let limit = bound - TRAILER_LEN;
        let mut p = offset + HEADER_LEN;
        let mut rows: Vec<Vec<(usize, Vec<u8>)>> = Vec::new();
        let mut width = 0usize;
        loop {
            // End of record: zero pad up to a 4-byte boundary, then a trailer.
            let t = (p + 3) & !3;
            if t + TRAILER_LEN <= bound && data[p..t].iter().all(|&b| b == 0) && is_trailer(&data[t..t + TRAILER_LEN]) {
                break;
            }
            if p >= limit {
                return Err(ShpError::NoTrailer { offset });
            }
            let row = rows.len();
            let overrun = || ShpError::RowOverrun { offset, row };
            let mut x = 0usize;
            let mut spans: Vec<(usize, Vec<u8>)> = Vec::new();
            loop {
                if p >= limit {
                    return Err(overrun());
                }
                let op = data[p];
                p += 1;
                match op {
                    0 => break,
                    1 => {
                        if p >= limit {
                            return Err(overrun());
                        }
                        x += data[p] as usize;
                        p += 1;
                    }
                    op if op & 1 == 1 => {
                        let k = (op >> 1) as usize;
                        if p + k > limit {
                            return Err(overrun());
                        }
                        spans.push((x, data[p..p + k].to_vec()));
                        x += k;
                        p += k;
                    }
                    op => {
                        let k = (op >> 1) as usize;
                        if p >= limit {
                            return Err(overrun());
                        }
                        spans.push((x, vec![data[p]; k]));
                        x += k;
                        p += 1;
                    }
                }
                if x > MAX_DIM {
                    return Err(ShpError::TooLarge { offset, width: x, height: row + 1 });
                }
            }
            width = width.max(x);
            rows.push(spans);
            if rows.len() > MAX_DIM {
                return Err(ShpError::TooLarge { offset, width, height: rows.len() });
            }
        }

        let height = rows.len();
        if width == 0 || height == 0 {
            return Ok(Frame::empty(header));
        }
        if width * height > MAX_PIXELS {
            return Err(ShpError::TooLarge { offset, width, height });
        }
        let mut indices = vec![0u8; width * height];
        let mut opaque = vec![false; width * height];
        for (y, spans) in rows.iter().enumerate() {
            for (x, px) in spans {
                let start = y * width + x;
                indices[start..start + px.len()].copy_from_slice(px);
                opaque[start..start + px.len()].fill(true);
            }
        }
        Ok(Frame {
            header,
            width,
            height,
            block_x: header.hot_x as i32 + header.dx,
            block_y: header.hot_y as i32 + header.dy,
            indices,
            opaque,
        })
    }

    /// Decode every entry of a container, in animation order.
    pub fn decode_container(&self, index: usize) -> Vec<Result<Frame, ShpError>> {
        self.containers[index].iter().map(|&o| self.decode(o)).collect()
    }
}

/// The 36-byte trailer holds six floats followed by the six shorts they were
/// derived from: x-like values divided by 16, y-like values by 11.
fn is_trailer(t: &[u8]) -> bool {
    debug_assert_eq!(t.len(), TRAILER_LEN);
    (0..6).all(|i| {
        let f = f32::from_le_bytes([t[4 * i], t[4 * i + 1], t[4 * i + 2], t[4 * i + 3]]);
        let s = i16::from_le_bytes([t[24 + 2 * i], t[25 + 2 * i]]) as f32;
        let scale = if i % 2 == 0 { 16.0 } else { 11.0 };
        f.is_finite() && (f - s / scale).abs() < TRAILER_TOLERANCE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a file with one container and one record from raw row bytes.
    fn synthetic(header: [u8; HEADER_LEN], rows: &[u8]) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(MAGIC);
        f.extend_from_slice(&1u32.to_le_bytes());
        // Record follows the 16-byte table.
        f.extend_from_slice(&16u32.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&header);
        f.extend_from_slice(rows);
        while f.len() % 4 != 0 {
            f.push(0);
        }
        // Trailer: shorts (32, 22, 16, 11, 0, -11) and matching floats.
        let shorts: [i16; 6] = [32, 22, 16, 11, 0, -11];
        for (i, s) in shorts.iter().enumerate() {
            let scale = if i % 2 == 0 { 16.0 } else { 11.0 };
            f.extend_from_slice(&(*s as f32 / scale).to_le_bytes());
        }
        for s in shorts {
            f.extend_from_slice(&s.to_le_bytes());
        }
        f
    }

    fn header(box_h: u16, box_w1: u16, hot_y: i16, hot_x: i16, dx: i32, dy: i32) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        h[0..2].copy_from_slice(&box_h.to_le_bytes());
        h[2..4].copy_from_slice(&box_w1.to_le_bytes());
        h[4..6].copy_from_slice(&hot_y.to_le_bytes());
        h[6..8].copy_from_slice(&hot_x.to_le_bytes());
        h[8..12].copy_from_slice(&dx.to_le_bytes());
        h[12..16].copy_from_slice(&dy.to_le_bytes());
        h
    }

    #[test]
    fn decodes_small_frame() {
        // Row 0: skip 2, run of 2 colour 0x18. Row 1: skip 1, literal 0x10 0xd3.
        let rows = [1, 2, 4, 0x18, 0, 1, 1, 5, 0x10, 0xd3, 0];
        let shp = ShapeFile::parse(synthetic(header(15, 9, 14, 4, -3, -14), &rows)).unwrap();
        assert_eq!(shp.container_count(), 1);
        let fr = shp.decode(shp.container(0)[0]).unwrap();
        assert_eq!((fr.width, fr.height), (4, 2));
        assert_eq!((fr.block_x, fr.block_y), (1, 0));
        assert_eq!(fr.pixel(0, 0), None);
        assert_eq!(fr.pixel(2, 0), Some(0x18));
        assert_eq!(fr.pixel(3, 0), Some(0x18));
        assert_eq!(fr.pixel(1, 1), Some(0x10));
        assert_eq!(fr.pixel(2, 1), Some(0xd3));
        assert_eq!(fr.pixel(3, 1), None);
        assert!(fr.fits_canvas());
        assert_eq!(fr.header.canvas_size(), (10, 16));
    }

    #[test]
    fn huge_header_dimensions_do_not_allocate() {
        // Header claims a 65535 x 65535 box; only two tiny rows are stored.
        let rows = [3, 7, 0, 3, 8, 0];
        let shp = ShapeFile::parse(synthetic(header(65535, 65534, 0, 0, 0, 0), &rows)).unwrap();
        let fr = shp.decode(shp.container(0)[0]).unwrap();
        assert_eq!((fr.width, fr.height), (1, 2));
        assert_eq!(fr.indices.len(), 2);
    }

    #[test]
    fn placeholder_is_empty() {
        let shp = ShapeFile::parse(synthetic(header(0, 0, 0, 0, 0x7fff_ff45, 0x7fff_ff31), &[])).unwrap();
        let fr = shp.decode(shp.container(0)[0]).unwrap();
        assert!(fr.is_empty());
    }

    #[test]
    fn missing_trailer_is_an_error() {
        let mut f = synthetic(header(2, 2, 0, 0, 0, 0), &[3, 7, 0]);
        // Corrupt the trailer floats so the record never terminates.
        let n = f.len();
        f[n - 36..n - 12].fill(0xff);
        let shp = ShapeFile::parse(f).unwrap();
        assert!(matches!(shp.decode(shp.container(0)[0]), Err(ShpError::NoTrailer { .. })));
    }

    #[test]
    fn rejects_offsets_not_in_tables() {
        let shp = ShapeFile::parse(synthetic(header(2, 2, 0, 0, 0, 0), &[3, 7, 0])).unwrap();
        assert!(matches!(shp.decode(17), Err(ShpError::UnknownRecord { offset: 17 })));
    }

    #[test]
    fn rejects_entry_outside_file() {
        let mut f = synthetic(header(2, 2, 0, 0, 0, 0), &[3, 7, 0]);
        f[8..12].copy_from_slice(&0xffff_fff0u32.to_le_bytes());
        assert!(matches!(ShapeFile::parse(f), Err(ShpError::EntryOutOfFile { .. })));
    }

    /// Decode the real cache when `NETSTORM_DIR` points at a NetStorm install.
    #[test]
    fn real_file_if_available() {
        let Ok(dir) = std::env::var("NETSTORM_DIR") else {
            eprintln!("NETSTORM_DIR not set; skipping real-file test");
            return;
        };
        let shp = ShapeFile::load(format!("{dir}/d/_shapes.shp")).expect("load _shapes.shp");
        assert_eq!(shp.container_count(), 119);
        let (mut ok, mut bad, mut fits) = (0, 0, 0);
        for ci in 0..shp.container_count() {
            for r in shp.decode_container(ci) {
                match r {
                    Ok(fr) => {
                        ok += 1;
                        if fr.is_empty() || fr.fits_canvas() {
                            fits += 1;
                        }
                    }
                    Err(_) => bad += 1,
                }
            }
        }
        // Reference decoder (tools/shp_decode.py) reports 4121 ok, 3 bad, 3 not fitting.
        assert_eq!(ok + bad, 4124);
        assert!(bad <= 3, "{bad} records failed");
        assert!(fits >= ok - 3, "{} frames do not fit their canvas", ok - fits);
    }
}
