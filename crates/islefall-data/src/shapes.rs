// SPDX-License-Identifier: Apache-2.0
//! Which `_shapes.shp` container belongs to which type.
//!
//! The sprite cache has one container per object type, in the order the
//! game registers its types. That order is not stored in the data files; it
//! was recovered from the string table of the game's own binary and verified
//! against every container's entry count, so it is kept here as a constant.
//!
//! Inside a container the entries are the type's frames in file order. If
//! the type carries the `shadow` or `flyershadow` flag, the container holds
//! twice as many entries: first every frame's image, then every frame's
//! shadow. Frames that reference the same image share the same record.

use std::collections::BTreeMap;

use crate::shp::ShapeFile;
use crate::typefile::TypeDef;
use thiserror::Error;

/// Type file stems in container order, for the 8.2 data (119 containers).
pub const SHAPE_ORDER: [&str; 119] = [
    "dude", "sunarcher", "sunaviary", "sunflyer", "banner", "windbattery", "rainbattery",
    "thunderbattery", "sunblocker", "thunderblocker", "blankmissile", "bolt", "bridge", "sundisc",
    "emptygeyser", "sunfactory", "windfactory", "rainfactory", "thunderfactory", "residence",
    "fencemark", "flag", "fortgump", "icon", "island", "islandstalag", "playerbanner", "isle",
    "islebig", "geyserbrightener", "treetwo", "treethree", "fringe", "mana", "range", "manabolt",
    "flare", "puzzlepiece", "playerisland", "battleisland", "particleplaceholder", "sunballoon",
    "buried", "bombexplodesmall", "bombexplodemedium", "bombexplodelarge", "bombheal",
    "bombinvisible", "bombparalyze", "bombhardener", "bombtreason", "edgefarm", "geyser",
    "windvortex", "rainvortex", "thundervortex", "outpost", "mcloud", "suncannon", "raincannon",
    "raincannonmissile", "thundercannon", "thundercannonmissile", "sunwalker", "teleporteffect",
    "bulf", "sunfence", "windwalker", "windflyer", "windaviary", "windarcher", "windballoon",
    "windblocker", "rainaviary", "rainflyer", "rainfence", "rainballoon", "rainblocker",
    "thunderarcher", "thunderfence", "growingrainblocker", "platform", "player", "mog", "nugget",
    "bridgeconnector", "rainwalker", "noisland", "priest", "windwhirl", "thunderaviary", "ruin",
    "monument", "sunbattery", "lightning", "anim", "challengeisland", "fakethreebythreesurface",
    "sunflyerbomb", "altar", "dais", "rune", "forcefield", "bombspecialone", "daisextraframes",
    "fenceshield", "bombtwister", "bombiitwister", "bombiiitwister", "gravitationeffect",
    "bombgraviton", "healeffect", "meteoreffect", "bombmeteor", "bombimano", "bombiimano",
    "bombiiimano", "bomblightingzap", "bomblightingwave",
];

/// Whether a type's container carries a shadow block.
pub fn has_shadow_block(def: &TypeDef) -> bool {
    def.has_flag("shadow") || def.has_flag("flyershadow")
}

/// Expected number of container entries for a type.
pub fn expected_entries(def: &TypeDef) -> usize {
    def.frames.len() * if has_shadow_block(def) { 2 } else { 1 }
}

/// Container index for a type, by file stem (case-insensitive).
pub fn container_of(stem: &str) -> Option<usize> {
    SHAPE_ORDER.iter().position(|s| s.eq_ignore_ascii_case(stem))
}

/// A type's frames resolved to sprite cache records.
#[derive(Clone, Debug)]
pub struct ShapeRecords {
    pub container: usize,
    /// Record offset of each frame's image, in frame order.
    pub images: Vec<usize>,
    /// Record offset of each frame's shadow, empty if the type has none.
    pub shadows: Vec<usize>,
}

#[derive(Debug, Error)]
pub enum ShapeIndexError {
    /// The cache does not have the expected number of containers.
    #[error("sprite cache has {found} containers, expected {expected}")]
    ContainerCount { expected: usize, found: usize },
    /// A container's entry count does not match its type's frames.
    #[error("container {container} ({stem}) has {found} entries, type has {expected} frames")]
    EntryCount { stem: &'static str, container: usize, expected: usize, found: usize },
    /// A type from [`SHAPE_ORDER`] was not provided.
    #[error("no type definition for {0}")]
    MissingType(&'static str),
}

/// Resolve every type in [`SHAPE_ORDER`] against the sprite cache.
///
/// `types` maps lowercase file stems to parsed definitions. Every container
/// is checked against its type's frame count, so a mismatched data set fails
/// loudly instead of showing the wrong sprites.
pub fn index(shp: &ShapeFile, types: &BTreeMap<String, TypeDef>) -> Result<BTreeMap<String, ShapeRecords>, ShapeIndexError> {
    if shp.container_count() != SHAPE_ORDER.len() {
        return Err(ShapeIndexError::ContainerCount { expected: SHAPE_ORDER.len(), found: shp.container_count() });
    }
    let mut out = BTreeMap::new();
    for (container, stem) in SHAPE_ORDER.iter().enumerate() {
        let def = types.get(*stem).ok_or(ShapeIndexError::MissingType(stem))?;
        let entries = shp.container(container);
        let expected = expected_entries(def);
        if entries.len() != expected {
            return Err(ShapeIndexError::EntryCount { stem, container, expected, found: entries.len() });
        }
        let n = def.frames.len();
        out.insert(
            stem.to_string(),
            ShapeRecords {
                container,
                images: entries[..n].to_vec(),
                shadows: if has_shadow_block(def) { entries[n..].to_vec() } else { Vec::new() },
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_is_unique_and_lowercase() {
        let mut seen = std::collections::BTreeSet::new();
        for s in SHAPE_ORDER {
            assert!(seen.insert(s), "duplicate {s}");
            assert_eq!(s, s.to_ascii_lowercase());
        }
        assert_eq!(container_of("Priest"), Some(88));
        assert_eq!(container_of("isle"), Some(27));
        assert_eq!(container_of("nope"), None);
    }

    #[test]
    fn real_index_if_available() {
        let Ok(dir) = std::env::var("NETSTORM_DIR") else {
            eprintln!("NETSTORM_DIR not set; skipping real-file test");
            return;
        };
        let shp = ShapeFile::load(format!("{dir}/d/_shapes.shp")).unwrap();
        let archive = crate::tarc::Archive::load(format!("{dir}/netstorm.tarc")).unwrap();
        let mut types = BTreeMap::new();
        for (i, e) in archive.entries().iter().enumerate() {
            if e.extension() == "type" {
                let stem = e.basename()[..e.basename().len() - 5].to_ascii_lowercase();
                types.insert(stem, crate::typefile::parse(&archive.read_text(i)).unwrap());
            }
        }
        let idx = index(&shp, &types).expect("every container matches its type");
        assert_eq!(idx.len(), 119);
        let priest = &idx["priest"];
        assert_eq!(priest.container, 88);
        assert_eq!(priest.images.len(), 168);
        assert_eq!(priest.shadows.len(), 168);
        assert!(idx["isle"].shadows.is_empty());
        // Frames sharing an image reference share a record.
        let t = &types["altar"];
        let r = &idx["altar"];
        for (a, fa) in t.frames.iter().enumerate() {
            for (b, fb) in t.frames.iter().enumerate() {
                if fa.image() == fb.image() {
                    assert_eq!(r.images[a], r.images[b], "altar frames {a} and {b}");
                }
            }
        }
    }
}
