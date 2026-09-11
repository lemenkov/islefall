// SPDX-License-Identifier: Apache-2.0
//! A sprite sheet a mod brings for a type: one picture plus a TOML index
//! of its frames in the type's animation order. `shpdump export` writes
//! the original sprites in this form as a starting point for new art.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// Picture file next to the index, GIF or PNG.
    pub image: String,
    #[serde(default)]
    pub frames: Vec<SheetFrame>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SheetFrame {
    /// Animation letter as in the type file: `A` to `H` are the eight
    /// facings clockwise from north, `P` poses, others as the type names them.
    pub animation: String,
    /// x, y, width, height in the picture; an empty frame has zero size.
    pub rect: [u32; 4],
    /// The hotspot (a walker's feet, a building's bottom-right cell)
    /// measured from the rect's top-left corner.
    pub hotspot: [i32; 2],
    /// The shadow's rect and hotspot in the same picture, if the frame has one.
    /// The frame's flags as in the type file (`default`, `unlit`, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<[u32; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_hotspot: Option<[i32; 2]>,
}

#[derive(Debug, Error)]
pub enum SheetError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("sheet: {0}")]
    Parse(String),
}

impl Sheet {
    pub fn parse(text: &str) -> Result<Sheet, SheetError> {
        toml::from_str(text).map_err(|e| SheetError::Parse(e.to_string()))
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Sheet, SheetError> {
        Sheet::parse(&std::fs::read_to_string(path).map_err(SheetError::Io)?)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Animation label of every frame, in order.
    pub fn labels(&self) -> Vec<String> {
        self.frames.iter().map(|f| f.animation.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let sheet = Sheet {
            image: "golem.png".into(),
            frames: vec![
                SheetFrame { animation: "A".into(), rect: [0, 0, 20, 30], hotspot: [10, 29], flags: vec!["default".into()], shadow: Some([21, 0, 24, 12]), shadow_hotspot: Some([12, 6]) },
                SheetFrame { animation: "B".into(), rect: [0, 0, 0, 0], hotspot: [0, 0], flags: Vec::new(), shadow: None, shadow_hotspot: None },
            ],
        };
        let text = sheet.to_toml();
        assert!(text.contains("[[frames]]"));
        assert_eq!(Sheet::parse(&text).unwrap(), sheet);
        assert_eq!(sheet.labels(), ["A", "B"]);
        assert!(Sheet::parse("image = \"x.png\"\n[[frames]]\nanimation = \"A\"\nrect = [0,0,1,1]\nhotspot = [0,0]\nbogus = 1\n").is_err());
    }
}
