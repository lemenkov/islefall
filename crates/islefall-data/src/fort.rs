// SPDX-License-Identifier: Apache-2.0
//! Campaign scenarios: the `*.fort` files in `netstorm.tarc`, a saved
//! starting position for each mission of the original game.
//!
//! The layout below was worked out by comparing the 32 shipped files with
//! their mission texts; nothing documents it. A file is
//!
//! - `F`, a byte, the format version, four bytes, a `u16` name length and
//!   the name (`Player`, `Fred`, `Unnamed` or nothing), then
//! - a stream of chunks, each a little-endian `u16` length (counting
//!   itself) and the payload.
//!
//! The first chunk of any size whose payload is a kind byte and then `c`
//! (0x63) records is the world table: 256 records, one per 16 x 16-cell
//! block of a 256 x 256-cell world in row-major order. A record is `c`, a
//! `u16` item count and the items: a cell byte (`column << 4 | row` within
//! the block), a type code and a payload whose length depends on the code
//! (see [`payload_len`]). Ground cells, geysers, bridges and structures
//! of every island but the players' fortresses live here.
//!
//! A later chunk of twenty six-byte objects lists the players' fortresses
//! (tag 9) among other objects (tag 3), and after the tech table three
//! empty chunks are followed by one chunk per object: a bare kind byte
//! for the tag-3 objects, and for a fortress its structures as `c` records
//! over a canvas of 16 x 16 blocks two blocks wide, top-left block first,
//! trailing empty blocks left out. The fortress's ground shape and its
//! place in the world are not in the file: the original drew every
//! fortress on the same island and put it where the player's seat was.

use thiserror::Error;

/// Cells per block side; the world is `BLOCKS` blocks square.
pub const BLOCK: i32 = 16;
pub const BLOCKS: i32 = 16;
/// A fortress canvas is this many blocks wide.
pub const CANVAS_BLOCKS_W: i32 = 2;
/// And at most this many tall in the files seen.
pub const CANVAS_BLOCKS_H: i32 = 3;

/// Codes with a fixed meaning the parser relies on.
pub const CODE_GROUND: u8 = 0x9d;
pub const CODE_GEYSER: u8 = 0x7a;
pub const CODE_TEMPLE: u8 = 0x9e;
pub const CODE_PRIEST: u8 = 0x59;
pub const CODE_ALTARS: std::ops::RangeInclusive<u8> = 0x7b..=0x7d;
pub const CODE_BRIDGES: std::ops::RangeInclusive<u8> = 0xfb..=0xfe;

/// Bytes of payload after an item's cell and code. Found by requiring
/// every record chunk to parse to exactly its own length.
pub fn payload_len(code: u8) -> usize {
    match code {
        0x9e => 3,
        0x7b..=0x7d => 4,
        0x55..=0x58 => 3,
        0x6e => 3,
        0x4f | 0x5c | 0x6f | 0x81 | 0x83 | 0x85 | 0x87 | 0x89 | 0x8c | 0x8d | 0x8e | 0x92 | 0x9c => 2,
        _ => 1,
    }
}

/// One thing at a cell: ground, a geyser, a bridge cell, a structure or a
/// unit, by its type code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub x: i32,
    pub y: i32,
    pub code: u8,
    pub payload: Vec<u8>,
}

impl Item {
    /// The player number the payload names (1 = the first player), if it
    /// carries one: a Temple's second byte, an altar's first, a Workshop's
    /// second, otherwise a small byte.
    pub fn owner(&self) -> Option<u8> {
        let small = |b: u8| (1..=8).contains(&b).then_some(b);
        match self.code {
            CODE_GROUND | CODE_GEYSER => None,
            c if CODE_BRIDGES.contains(&c) => None,
            CODE_TEMPLE => self.payload.get(1).copied().and_then(small),
            c if CODE_ALTARS.contains(&c) => self.payload.first().copied().and_then(small),
            0x55..=0x58 => self.payload.get(1).copied().and_then(small),
            _ => match self.payload.len() {
                2 => self.payload.get(1).copied().and_then(small).or_else(|| self.payload.first().copied().and_then(small)),
                _ => self.payload.first().copied().and_then(small),
            },
        }
    }

    /// A Temple written for the seat the file was made from has its top
    /// bit set on the first payload byte.
    pub fn is_local_temple(&self) -> bool {
        self.code == CODE_TEMPLE && self.payload.first().is_some_and(|b| b & 0x80 != 0)
    }
}

/// A player's fortress: its structures on a canvas of blocks, coordinates
/// relative to the canvas's top-left cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fortress {
    pub items: Vec<Item>,
    /// Blocks the file listed (two per row).
    pub blocks: usize,
}

impl Fortress {
    pub fn canvas_size(&self) -> (i32, i32) {
        let rows = ((self.blocks as i32 + CANVAS_BLOCKS_W - 1) / CANVAS_BLOCKS_W).max(1);
        (CANVAS_BLOCKS_W * BLOCK, rows * BLOCK)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fort {
    pub version: u8,
    pub name: String,
    /// Everything in the world table, in world cells.
    pub items: Vec<Item>,
    pub fortresses: Vec<Fortress>,
    /// Record chunks that did not parse to their own length and were skipped.
    pub skipped_chunks: usize,
}

#[derive(Debug, Error)]
pub enum FortError {
    #[error("file too short")]
    Short,
    #[error("not a fort file (no F magic)")]
    Magic,
    #[error("no world table of 256 blocks")]
    NoTable,
}

fn u16_at(d: &[u8], p: usize) -> Option<usize> {
    Some(d.get(p).copied()? as usize | (d.get(p + 1).copied()? as usize) << 8)
}

/// Parse the `c` records in `d[start..end]`; `None` unless they fill the
/// range exactly. Each record's items get block-local cells.
fn records(d: &[u8], start: usize, end: usize) -> Option<Vec<Vec<(u8, u8, Vec<u8>)>>> {
    let mut p = start;
    let mut out = Vec::new();
    while p < end && d[p] == 0x63 {
        let count = u16_at(d, p + 1)?;
        p += 3;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let cell = *d.get(p)?;
            let code = *d.get(p + 1)?;
            let n = payload_len(code);
            let payload = d.get(p + 2..p + 2 + n)?.to_vec();
            items.push((cell, code, payload));
            p += 2 + n;
        }
        out.push(items);
    }
    (p == end).then_some(out)
}

impl Fort {
    pub fn parse(d: &[u8]) -> Result<Fort, FortError> {
        if d.len() < 12 {
            return Err(FortError::Short);
        }
        if d[0] != b'F' {
            return Err(FortError::Magic);
        }
        let version = d[2];
        let name_len = u16_at(d, 8).ok_or(FortError::Short)?;
        let name = String::from_utf8_lossy(d.get(10..10 + name_len).ok_or(FortError::Short)?).into_owned();
        let mut p = 10 + name_len;
        let mut fort = Fort { version, name, ..Fort::default() };
        let mut table_seen = false;
        while let Some(len) = u16_at(d, p) {
            if len < 2 || p + len > d.len() {
                break;
            }
            let (start, end) = (p + 2, p + len);
            p = end;
            // A record chunk: a kind byte, then 'c' records.
            if end - start < 4 || d[start + 1] != 0x63 {
                continue;
            }
            let Some(recs) = records(d, start + 1, end) else {
                fort.skipped_chunks += 1;
                continue;
            };
            if !table_seen && recs.len() == BLOCKS as usize * BLOCKS as usize {
                table_seen = true;
                for (k, items) in recs.iter().enumerate() {
                    let (bx, by) = (k as i32 % BLOCKS, k as i32 / BLOCKS);
                    for (cell, code, payload) in items {
                        fort.items.push(Item { x: bx * BLOCK + (cell >> 4) as i32, y: by * BLOCK + (cell & 15) as i32, code: *code, payload: payload.clone() });
                    }
                }
            } else if table_seen {
                let mut f = Fortress { items: Vec::new(), blocks: recs.len() };
                for (k, items) in recs.iter().enumerate() {
                    let (bx, by) = (k as i32 % CANVAS_BLOCKS_W, k as i32 / CANVAS_BLOCKS_W);
                    for (cell, code, payload) in items {
                        f.items.push(Item { x: bx * BLOCK + (cell >> 4) as i32, y: by * BLOCK + (cell & 15) as i32, code: *code, payload: payload.clone() });
                    }
                }
                fort.fortresses.push(f);
            }
        }
        if !table_seen {
            return Err(FortError::NoTable);
        }
        Ok(fort)
    }

    /// Ground cells of the world table with their theme byte.
    pub fn ground(&self) -> impl Iterator<Item = (i32, i32, u8)> + '_ {
        self.items.iter().filter(|i| i.code == CODE_GROUND).map(|i| (i.x, i.y, i.payload.first().copied().unwrap_or(0)))
    }

    /// Bounds of everything in the world table: (min x, min y, max x, max y).
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for i in &self.items {
            b = Some(match b {
                None => (i.x, i.y, i.x, i.y),
                Some((x0, y0, x1, y1)) => (x0.min(i.x), y0.min(i.y), x1.max(i.x), y1.max(i.y)),
            });
        }
        b
    }
}

/// Build a fort file from parts, for tests and for writing our own.
pub fn encode(version: u8, name: &str, table: &[Vec<(u8, u8, Vec<u8>)>], fortresses: &[Vec<Vec<(u8, u8, Vec<u8>)>>]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, payload: &[u8]) {
        let len = payload.len() + 2;
        out.extend_from_slice(&(len as u16).to_le_bytes());
        out.extend_from_slice(payload);
    }
    fn recs(recs: &[Vec<(u8, u8, Vec<u8>)>]) -> Vec<u8> {
        let mut p = vec![2u8];
        for r in recs {
            p.push(0x63);
            p.extend_from_slice(&(r.len() as u16).to_le_bytes());
            for (cell, code, payload) in r {
                p.push(*cell);
                p.push(*code);
                p.extend_from_slice(payload);
            }
        }
        p
    }
    let mut out = vec![b'F', 0, version, 0, 0, 0, 0, 0];
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    chunk(&mut out, &[0; 20]);
    chunk(&mut out, &[]);
    chunk(&mut out, &[]);
    let mut table: Vec<Vec<(u8, u8, Vec<u8>)>> = table.to_vec();
    table.resize(256, Vec::new());
    chunk(&mut out, &recs(&table));
    chunk(&mut out, &[0; 122]);
    chunk(&mut out, &[]);
    for f in fortresses {
        chunk(&mut out, &recs(f));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_world_table_and_fortresses() {
        // Block 17 (row 1, column 1): a 3 x 3 geyser island with the geyser at its bottom-right.
        let mut table = vec![Vec::new(); 18];
        for c in 0..3u8 {
            for r in 0..3u8 {
                table[17].push((c << 4 | r, CODE_GROUND, vec![0]));
            }
        }
        table[17].push((0x22, CODE_GEYSER, vec![0]));
        let fortress = vec![vec![(0x7d, 0x7b, vec![1, 1, 0x85, 0])], vec![(0x41, CODE_TEMPLE, vec![0x81, 1, 0]), (0x40, CODE_PRIEST, vec![1])]];
        let bytes = encode(14, "Player", &table, &[fortress]);
        let f = Fort::parse(&bytes).unwrap();
        assert_eq!((f.version, f.name.as_str()), (14, "Player"));
        assert_eq!(f.ground().count(), 9);
        assert!(f.ground().all(|(x, y, _)| (16..19).contains(&x) && (16..19).contains(&y)));
        let geyser = f.items.iter().find(|i| i.code == CODE_GEYSER).unwrap();
        assert_eq!((geyser.x, geyser.y), (18, 18));
        assert_eq!(f.fortresses.len(), 1);
        let fo = &f.fortresses[0];
        assert_eq!(fo.blocks, 2);
        assert_eq!(fo.canvas_size(), (32, 16));
        let altar = fo.items.iter().find(|i| CODE_ALTARS.contains(&i.code)).unwrap();
        assert_eq!(((altar.x, altar.y), altar.owner()), ((7, 13), Some(1)));
        let temple = fo.items.iter().find(|i| i.code == CODE_TEMPLE).unwrap();
        assert_eq!(((temple.x, temple.y), temple.owner(), temple.is_local_temple()), ((16 + 4, 1), Some(1), true));
        assert_eq!(f.skipped_chunks, 0);
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(Fort::parse(b"hello"), Err(FortError::Short)));
        assert!(matches!(Fort::parse(b"typename x\n{\n}\n"), Err(FortError::Magic)));
    }
}
