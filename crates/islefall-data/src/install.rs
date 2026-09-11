// SPDX-License-Identifier: Apache-2.0
//! A NetStorm installation as one loaded object.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::col::{ColError, Palette};
use crate::shapes::{self, ShapeIndexError, ShapeRecords};
use crate::shp::{ShapeFile, ShpError};
use crate::tarc::{Archive, TarcError};
use crate::typefile::{self, ParseError, TypeDef};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("_shapes.shp: {0}")]
    Shp(ShpError),
    #[error("netstorm.tarc: {0}")]
    Tarc(TarcError),
    #[error("{file}: {error}")]
    Col { file: String, error: ColError },
    #[error("{file}: {error}")]
    Type { file: String, error: ParseError },
    #[error("{0}")]
    Index(ShapeIndexError),
}

/// Everything Islefall needs from the game directory, loaded up front.
pub struct Installation {
    pub root: PathBuf,
    pub shapes: ShapeFile,
    pub archive: Archive,
    /// Type definitions keyed by lowercase file stem, e.g. `sunwalker`.
    pub types: BTreeMap<String, TypeDef>,
    /// Sprite cache records per type, keyed like `types`.
    pub records: BTreeMap<String, ShapeRecords>,
    /// Palettes keyed by lowercase file stem, e.g. `suncannon`.
    pub palettes: BTreeMap<String, Palette>,
}

impl Installation {
    /// Load from the directory that contains `d/` and `netstorm.tarc`.
    pub fn load(root: impl AsRef<Path>) -> Result<Installation, InstallError> {
        let root = root.as_ref().to_path_buf();
        let d = root.join("d");
        let shapes = ShapeFile::load(d.join("_shapes.shp")).map_err(InstallError::Shp)?;
        let archive = Archive::load(root.join("netstorm.tarc")).map_err(InstallError::Tarc)?;

        let mut types = BTreeMap::new();
        for (i, e) in archive.entries().iter().enumerate() {
            if e.extension() != "type" {
                continue;
            }
            let file = e.basename().to_string();
            let stem = file[..file.len() - 5].to_ascii_lowercase();
            let def = typefile::parse(&archive.read_text(i)).map_err(|error| InstallError::Type { file, error })?;
            types.insert(stem, def);
        }
        let records = shapes::index(&shapes, &types).map_err(InstallError::Index)?;

        let mut palettes = BTreeMap::new();
        if let Ok(dir) = std::fs::read_dir(&d) {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(stem) = name.strip_suffix(".COL").or_else(|| name.strip_suffix(".col")) {
                    let pal = Palette::load(entry.path()).map_err(|error| InstallError::Col { file: name.clone(), error })?;
                    palettes.insert(stem.to_ascii_lowercase(), pal);
                }
            }
        }
        Ok(Installation { root, shapes, archive, types, records, palettes })
    }

    /// Type stems in container order.
    pub fn type_names(&self) -> impl Iterator<Item = &'static str> {
        shapes::SHAPE_ORDER.iter().copied()
    }

    pub fn type_def(&self, stem: &str) -> Option<&TypeDef> {
        self.types.get(&stem.to_ascii_lowercase())
    }

    pub fn shape_records(&self, stem: &str) -> Option<&ShapeRecords> {
        self.records.get(&stem.to_ascii_lowercase())
    }

    pub fn palette(&self, stem: &str) -> Option<&Palette> {
        self.palettes.get(&stem.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_install_if_available() {
        let Ok(dir) = std::env::var("NETSTORM_DIR") else {
            eprintln!("NETSTORM_DIR not set; skipping real-file test");
            return;
        };
        let inst = Installation::load(dir).expect("load installation");
        assert_eq!(inst.types.len(), 124);
        assert_eq!(inst.records.len(), 119);
        assert!(inst.palette("SUNCANNON").is_some());
        let walker = inst.type_def("sunwalker").unwrap();
        assert_eq!(walker.get_str("description"), Some("Golem"));
        assert_eq!(inst.shape_records("sunwalker").unwrap().images.len(), walker.frames.len());
    }
}
