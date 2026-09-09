// SPDX-License-Identifier: Apache-2.0
//! Per-type rules derived from the `typeflags` and properties of a type file.
//!
//! The flags are read as follows, from how they are distributed over the
//! shipped types (only `edgefarm` is `walkBlocking`; factories, trees,
//! ruins, outposts and vortexes are `yuckWalk`; every 3x3 emplacement is
//! `createsisland`; the altar carries no walking flag at all):
//!
//! - `walkBlocking`: units cannot enter the footprint.
//! - `yuckWalk`: units may enter but avoid it; path cost is multiplied.
//! - `dropBlocking`: nothing may be dropped onto the footprint.
//! - `createsisland`: dropping the type onto bridge cells turns its
//!   footprint into island ground.
//! - `mayDropOnRim`: the drop may include island rim cells.

use islefall_data::TypeDef;

/// How a footprint affects walking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Walk {
    Free,
    /// Passable but expensive: `yuckWalk`.
    Avoid,
    Blocked,
}

/// Path cost multiplier for [`Walk::Avoid`] cells.
pub const AVOID_COST: u32 = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct TypeRules {
    pub foot_x: i32,
    pub foot_y: i32,
    pub walk: Walk,
    pub drop_blocking: bool,
    pub creates_island: bool,
    pub may_drop_on_rim: bool,
    /// Whether the type is a mobile unit rather than something dropped in place.
    pub is_unit: bool,
    /// Cells per second for units.
    pub speed: f64,
}

impl TypeRules {
    pub fn from_type(def: &TypeDef) -> TypeRules {
        let walk = if def.has_flag("walkBlocking") {
            Walk::Blocked
        } else if def.has_flag("yuckWalk") {
            Walk::Avoid
        } else {
            Walk::Free
        };
        TypeRules {
            foot_x: def.get_i64("foot_x").unwrap_or(1).max(1) as i32,
            foot_y: def.get_i64("foot_y").unwrap_or(1).max(1) as i32,
            walk,
            drop_blocking: def.has_flag("dropBlocking"),
            creates_island: def.has_flag("createsisland"),
            may_drop_on_rim: def.has_flag("mayDropOnRim"),
            is_unit: def.has_flag("walker") || def.has_flag("flyer") || def.has_flag("balloon"),
            speed: def.get_f64("speed").unwrap_or(1.0),
        }
    }

    /// Rules for a plain, unknown type: one cell, walkable, droppable.
    pub fn plain() -> TypeRules {
        TypeRules {
            foot_x: 1,
            foot_y: 1,
            walk: Walk::Free,
            drop_blocking: false,
            creates_island: false,
            may_drop_on_rim: false,
            is_unit: false,
            speed: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use islefall_data::typefile;

    #[test]
    fn reads_flags_and_footprint() {
        let t = typefile::parse("typename x\ntypeflags yuckWalk dropBlocking factory;\n{\n foot_x = 8;\n foot_y = 6;\n}\nA00 : : \"a.gif\" #0;\n").unwrap();
        let r = TypeRules::from_type(&t);
        assert_eq!((r.foot_x, r.foot_y), (8, 6));
        assert_eq!(r.walk, Walk::Avoid);
        assert!(r.drop_blocking && !r.creates_island && !r.is_unit);
        let t = typefile::parse("typename y\ntypeflags walker shadow;\n{\n speed = 1.8;\n}\nA00 : : \"a.gif\" #0;\n").unwrap();
        let r = TypeRules::from_type(&t);
        assert!(r.is_unit);
        assert_eq!(r.speed, 1.8);
        assert_eq!(r.walk, Walk::Free);
    }
}
