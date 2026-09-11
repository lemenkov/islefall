// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! The island terrain set (`isle.type`).
//!
//! The 317 frames are grouped by animation label into terrain pieces, and
//! within each label the frames come in four equal blocks, one per theme in
//! the order sun, thunder, wind, rain. Filled pieces are `AA01`..`AA24`;
//! the many `AA00` frames after them are extra interior variants the
//! original game's "terrain scrambler" used, and are ignored here.

use crate::typefile::TypeDef;

/// The four weather themes, in the order the terrain frames are blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Theme {
    Sun,
    Thunder,
    Wind,
    Rain,
}

impl Theme {
    pub const ALL: [Theme; 4] = [Theme::Sun, Theme::Thunder, Theme::Wind, Theme::Rain];

    pub fn index(self) -> usize {
        match self {
            Theme::Sun => 0,
            Theme::Thunder => 1,
            Theme::Wind => 2,
            Theme::Rain => 3,
        }
    }

    /// Parse a `theme = "..."` property value.
    pub fn parse(s: &str) -> Option<Theme> {
        match s.to_ascii_lowercase().as_str() {
            "sun" => Some(Theme::Sun),
            "thunder" => Some(Theme::Thunder),
            "wind" => Some(Theme::Wind),
            "rain" => Some(Theme::Rain),
            _ => None,
        }
    }
}

/// One kind of terrain tile. Edges and corners are named by the side of the
/// island they sit on; an inside corner is a filled cell whose diagonal
/// neighbour in the named direction is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Piece {
    Filled,
    EdgeLeft,
    EdgeTop,
    EdgeRight,
    EdgeBottom,
    CornerTopLeft,
    CornerTopRight,
    CornerBottomRight,
    CornerBottomLeft,
    InsideTopLeft,
    InsideTopRight,
    InsideBottomRight,
    InsideBottomLeft,
}

impl Piece {
    pub const ALL: [Piece; 13] = [
        Piece::Filled,
        Piece::EdgeLeft,
        Piece::EdgeTop,
        Piece::EdgeRight,
        Piece::EdgeBottom,
        Piece::CornerTopLeft,
        Piece::CornerTopRight,
        Piece::CornerBottomRight,
        Piece::CornerBottomLeft,
        Piece::InsideTopLeft,
        Piece::InsideTopRight,
        Piece::InsideBottomRight,
        Piece::InsideBottomLeft,
    ];

    /// Animation label in `isle.type`. The inside-corner assignment follows
    /// the file's comment order and the `fringe` flag (fringe pieces face
    /// the bottom); it has not been confirmed visually yet.
    pub fn label(self) -> &'static str {
        match self {
            Piece::Filled => "AA",
            Piece::EdgeLeft => "BF",
            Piece::EdgeTop => "CG",
            Piece::EdgeRight => "DH",
            Piece::EdgeBottom => "EI",
            Piece::CornerTopLeft => "FL",
            Piece::CornerTopRight => "GM",
            Piece::CornerBottomRight => "HN",
            Piece::CornerBottomLeft => "IO",
            Piece::InsideTopLeft => "AC",
            Piece::InsideTopRight => "AD",
            Piece::InsideBottomRight => "AE",
            Piece::InsideBottomLeft => "AB",
        }
    }
}

/// Frame indices (into `def.frames`) of one piece for one theme.
pub fn frames(def: &TypeDef, theme: Theme, piece: Piece) -> Vec<usize> {
    let label = piece.label();
    let all: Vec<usize> = def
        .frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f.animation.eq_ignore_ascii_case(label) && f.number >= 1)
        .map(|(i, _)| i)
        .collect();
    let per = all.len() / Theme::ALL.len();
    if per == 0 {
        return Vec::new();
    }
    all[theme.index() * per..(theme.index() + 1) * per].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_isle_if_available() {
        let Ok(dir) = std::env::var("NETSTORM_DIR") else {
            eprintln!("NETSTORM_DIR not set; skipping real-file test");
            return;
        };
        let inst = crate::Installation::load(dir).unwrap();
        let isle = inst.type_def("isle").unwrap();
        for theme in Theme::ALL {
            for piece in Piece::ALL {
                let f = frames(isle, theme, piece);
                assert!(!f.is_empty(), "{theme:?} {piece:?} has no frames");
                let file = &isle.frames[f[0]].image().unwrap().file.to_ascii_lowercase();
                let expected = match theme {
                    Theme::Sun => "r",
                    Theme::Thunder => "th",
                    Theme::Wind => "wi",
                    Theme::Rain => "ra",
                };
                assert!(file.starts_with(expected), "{theme:?} {piece:?} uses {file}");
            }
        }
        assert_eq!(frames(isle, Theme::Sun, Piece::Filled).len(), 6);
        assert_eq!(frames(isle, Theme::Rain, Piece::EdgeBottom).len(), 6);
        assert_eq!(frames(isle, Theme::Wind, Piece::CornerTopLeft).len(), 2);
    }
}
