// SPDX-License-Identifier: Apache-2.0
//! `netstorm.tarc`: a flat archive of text resources (`*.type`, `*.english`,
//! `*.fort`), each file XOR-obfuscated with a repeating 13-byte key.
//!
//! Header (little-endian):
//!
//! | Offset | Size | Field |
//! |-------:|-----:|-------|
//! | 0x00 | 10 | magic `TAFF v0.2\x1a` |
//! | 0x14 | 4 | number of files |
//! | 0x20 | 4 | offset of the name table |
//! | 0x24 | 4 | length of the name table |
//! | 0x28 | 4 | offset of the data area |
//!
//! The name table holds, per file, `u32 offset` (relative to the data
//! area), `u32 size`, then a NUL-terminated path such as `\d\bird.type`.

use std::path::Path;
use thiserror::Error;

const MAGIC: &[u8] = b"TAFF v0.2\x1a";
const KEY: &[u8] = b"mydoghasfleas";
const MAX_FILES: u32 = 100_000;
const MAX_NAME: usize = 260;

#[derive(Debug, Error)]
pub enum TarcError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("not a TAFF v0.2 archive")]
    BadMagic,
    /// A header field points outside the file.
    #[error("archive truncated: {what}")]
    Truncated { what: &'static str },
    #[error("implausible file count {0}")]
    ImplausibleCount(u32),
    /// The name table ended before all entries were read.
    #[error("name table ends inside entry {entry}")]
    BadNameTable { entry: usize },
    /// An entry's data lies outside the data area.
    #[error("entry {entry} lies outside the archive")]
    EntryOutOfFile { entry: usize },
}

impl From<std::io::Error> for TarcError {
    fn from(e: std::io::Error) -> Self {
        TarcError::Io(e)
    }
}

/// One file inside the archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Path as stored, e.g. `\d\bird.type`.
    pub name: String,
    /// Offset relative to the data area.
    pub offset: usize,
    pub size: usize,
}

impl Entry {
    /// File name without directories, e.g. `bird.type`.
    pub fn basename(&self) -> &str {
        self.name.rsplit(['\\', '/']).next().unwrap_or(&self.name)
    }

    /// Lowercase extension without the dot, e.g. `type`.
    pub fn extension(&self) -> String {
        self.basename().rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
    }
}

pub struct Archive {
    data: Vec<u8>,
    data_off: usize,
    entries: Vec<Entry>,
}

fn u32_at(data: &[u8], o: usize, what: &'static str) -> Result<u32, TarcError> {
    data.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(TarcError::Truncated { what })
}

impl Archive {
    pub fn load(path: impl AsRef<Path>) -> Result<Archive, TarcError> {
        Archive::parse(std::fs::read(path)?)
    }

    pub fn parse(data: Vec<u8>) -> Result<Archive, TarcError> {
        if data.len() < MAGIC.len() || &data[..MAGIC.len()] != MAGIC {
            return Err(TarcError::BadMagic);
        }
        let count = u32_at(&data, 0x14, "file count")?;
        if count > MAX_FILES {
            return Err(TarcError::ImplausibleCount(count));
        }
        let names_off = u32_at(&data, 0x20, "name table offset")? as usize;
        let names_len = u32_at(&data, 0x24, "name table length")? as usize;
        let data_off = u32_at(&data, 0x28, "data offset")? as usize;
        let names_end = names_off.checked_add(names_len).filter(|&e| e <= data.len());
        let Some(names_end) = names_end else {
            return Err(TarcError::Truncated { what: "name table" });
        };
        if data_off > data.len() {
            return Err(TarcError::Truncated { what: "data area" });
        }

        let mut entries = Vec::with_capacity(count as usize);
        let mut p = names_off;
        for i in 0..count as usize {
            let bad = || TarcError::BadNameTable { entry: i };
            if p + 8 > names_end {
                return Err(bad());
            }
            let offset = u32_at(&data, p, "entry offset")? as usize;
            let size = u32_at(&data, p + 4, "entry size")? as usize;
            p += 8;
            let name_end = data[p..names_end].iter().take(MAX_NAME).position(|&b| b == 0).map(|n| p + n).ok_or_else(bad)?;
            let name = String::from_utf8_lossy(&data[p..name_end]).into_owned();
            p = name_end + 1;
            let end = data_off.checked_add(offset).and_then(|s| s.checked_add(size));
            if end.is_none_or(|e| e > data.len()) {
                return Err(TarcError::EntryOutOfFile { entry: i });
            }
            entries.push(Entry { name, offset, size });
        }
        Ok(Archive { data, data_off, entries })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Find an entry by base name, case-insensitively.
    pub fn find(&self, basename: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.basename().eq_ignore_ascii_case(basename))
    }

    /// Decoded contents of entry `index`.
    pub fn read(&self, index: usize) -> Vec<u8> {
        let e = &self.entries[index];
        let start = self.data_off + e.offset;
        self.data[start..start + e.size]
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ KEY[i % KEY.len()])
            .collect()
    }

    /// Decoded contents of entry `index` as text (the archive holds Latin-1 text).
    pub fn read_text(&self, index: usize) -> String {
        self.read(index).iter().map(|&b| b as char).collect()
    }
}

/// Build an archive in memory; used by tests and useful for tooling.
pub fn encode(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut names = Vec::new();
    let mut data = Vec::new();
    for (name, content) in files {
        names.extend_from_slice(&(data.len() as u32).to_le_bytes());
        names.extend_from_slice(&(content.len() as u32).to_le_bytes());
        names.extend_from_slice(name.as_bytes());
        names.push(0);
        data.extend(content.iter().enumerate().map(|(i, &b)| b ^ KEY[i % KEY.len()]));
    }
    let mut out = vec![0u8; 0x2c];
    out[..MAGIC.len()].copy_from_slice(MAGIC);
    out[0x14..0x18].copy_from_slice(&(files.len() as u32).to_le_bytes());
    let names_off = out.len() as u32;
    out[0x20..0x24].copy_from_slice(&names_off.to_le_bytes());
    out[0x24..0x28].copy_from_slice(&(names.len() as u32).to_le_bytes());
    out[0x28..0x2c].copy_from_slice(&(names_off + names.len() as u32).to_le_bytes());
    out.extend_from_slice(&names);
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let bytes = encode(&[("\\d\\bird.type", b"typename Bird\n"), ("\\d\\help.english", b"[A]\nhello")]);
        let a = Archive::parse(bytes).unwrap();
        assert_eq!(a.entries().len(), 2);
        assert_eq!(a.entries()[0].basename(), "bird.type");
        assert_eq!(a.entries()[0].extension(), "type");
        assert_eq!(a.read_text(0), "typename Bird\n");
        assert_eq!(a.find("HELP.ENGLISH"), Some(1));
        assert_eq!(a.read_text(1), "[A]\nhello");
        assert_eq!(a.find("missing"), None);
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(matches!(Archive::parse(b"nope".to_vec()), Err(TarcError::BadMagic)));
    }

    #[test]
    fn rejects_entry_outside_file() {
        let mut bytes = encode(&[("a.type", b"x")]);
        let names_off = u32::from_le_bytes(bytes[0x20..0x24].try_into().unwrap()) as usize;
        bytes[names_off + 4..names_off + 8].copy_from_slice(&1000u32.to_le_bytes());
        assert!(matches!(Archive::parse(bytes), Err(TarcError::EntryOutOfFile { entry: 0 })));
    }

    #[test]
    fn real_archive_if_available() {
        let Ok(dir) = std::env::var("NETSTORM_DIR") else {
            eprintln!("NETSTORM_DIR not set; skipping real-file test");
            return;
        };
        let a = Archive::load(format!("{dir}/netstorm.tarc")).expect("load netstorm.tarc");
        assert_eq!(a.entries().len(), 246);
        let types = a.entries().iter().filter(|e| e.extension() == "type").count();
        assert_eq!(types, 124);
        let i = a.find("bird.type").expect("bird.type present");
        assert!(a.read_text(i).starts_with("typename Bird"));
    }
}
