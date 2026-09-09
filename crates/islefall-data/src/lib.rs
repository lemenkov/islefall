// SPDX-License-Identifier: Apache-2.0
//! Loaders for the original NetStorm data files.
//!
//! Islefall does not ship any game data. These loaders read the files of a
//! user-supplied NetStorm installation: the sprite cache and palettes in
//! `d/`, and the unit definitions inside `netstorm.tarc`. File layouts are
//! documented in `docs/FORMATS.md` at the repository root.
//!
//! Every loader is written on the assumption that the input may be corrupt:
//! no allocation is ever sized from a header field alone, and all rasters are
//! capped by [`shp::MAX_DIM`] and [`shp::MAX_PIXELS`].

pub mod bridge;
pub mod col;
pub mod gif;
pub mod install;
pub mod isle;
pub mod shapes;
pub mod shp;
pub mod tarc;
pub mod typefile;

pub use col::Palette;
pub use gif::{IndexedImage, Picture};
pub use install::Installation;
pub use shapes::ShapeRecords;
pub use shp::{Frame, FrameHeader, ShapeFile, ShpError};
pub use tarc::Archive;
pub use typefile::TypeDef;
