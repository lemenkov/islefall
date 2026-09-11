// SPDX-License-Identifier: Apache-2.0
//! `*.COL` palette files: an 8-byte header followed by 256 RGB triplets.

use std::path::Path;
use thiserror::Error;

const HEADER_LEN: usize = 8;
const ENTRIES: usize = 256;
pub const FILE_LEN: usize = HEADER_LEN + ENTRIES * 3;

/// A 256-entry RGB palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub colors: [[u8; 3]; ENTRIES],
}

#[derive(Debug, Error)]
pub enum ColError {
    /// The file is not exactly [`FILE_LEN`] bytes long.
    #[error("palette file is {0} bytes, expected {FILE_LEN}")]
    BadLength(usize),
    #[error("{0}")]
    Io(std::io::Error),
}

impl From<std::io::Error> for ColError {
    fn from(e: std::io::Error) -> Self {
        ColError::Io(e)
    }
}

impl Palette {
    pub fn parse(data: &[u8]) -> Result<Palette, ColError> {
        if data.len() != FILE_LEN {
            return Err(ColError::BadLength(data.len()));
        }
        let mut colors = [[0u8; 3]; ENTRIES];
        for (i, c) in colors.iter_mut().enumerate() {
            let o = HEADER_LEN + 3 * i;
            c.copy_from_slice(&data[o..o + 3]);
        }
        Ok(Palette { colors })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Palette, ColError> {
        Palette::parse(&std::fs::read(path)?)
    }

    /// A grey ramp, useful when the real palette for a shape is unknown.
    pub fn grey() -> Palette {
        let mut colors = [[0u8; 3]; ENTRIES];
        for (i, c) in colors.iter_mut().enumerate() {
            *c = [i as u8; 3];
        }
        Palette { colors }
    }

    #[inline]
    pub fn rgb(&self, index: u8) -> [u8; 3] {
        self.colors[index as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthetic_palette() {
        let mut data = vec![0u8; FILE_LEN];
        data[HEADER_LEN + 3 * 7] = 10;
        data[HEADER_LEN + 3 * 7 + 1] = 20;
        data[HEADER_LEN + 3 * 7 + 2] = 30;
        let pal = Palette::parse(&data).unwrap();
        assert_eq!(pal.rgb(7), [10, 20, 30]);
        assert_eq!(pal.rgb(8), [0, 0, 0]);
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(matches!(Palette::parse(&[0; 100]), Err(ColError::BadLength(100))));
    }
}
