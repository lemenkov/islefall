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
    /// Storm Power to build, from the `cost` property (0 when absent).
    pub cost: i32,
    /// A Storm Geyser: holds `cost` worth of crystals to harvest.
    pub is_geyser: bool,
    /// A Temple (`residence`): where crystals become Storm Power.
    pub is_temple: bool,
    /// `maxHitPoints`; 0 means indestructible (terrain-like types).
    pub max_hit_points: i32,
    /// Firing range in cells; 0 for things that do not shoot.
    pub range: i32,
    /// `hpPerSec`: damage dealt per second while firing.
    pub hp_per_sec: i32,
    /// `delayBetweenShots` in seconds (1.0 when absent).
    pub delay_between_shots: f64,
    /// Cannons fire only straight north, south, east or west (the manual).
    pub cardinal_only: bool,
    /// `threat`: targeting priority, higher first.
    pub threat: i32,
    /// A High Priest (`priest` flag): can be stunned, captured and sacrificed.
    pub is_priest: bool,
    /// A Transport (`walker`, `flyer`, `balloon` without `priest`): can carry a captured priest.
    pub is_transport: bool,
    /// An Altar (`dais` flag): where captured priests are sacrificed.
    pub is_altar: bool,
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
            cost: def.get_i64("cost").unwrap_or(0).clamp(0, i32::MAX as i64) as i32,
            is_geyser: def.has_flag("geyser"),
            is_temple: def.has_flag("residence"),
            max_hit_points: def.get_i64("maxHitPoints").unwrap_or(0).clamp(0, i32::MAX as i64) as i32,
            range: if def.get_i64("hpPerSec").unwrap_or(0) > 0 { def.get_i64("range").unwrap_or(0) as i32 } else { 0 },
            hp_per_sec: def.get_i64("hpPerSec").unwrap_or(0) as i32,
            delay_between_shots: def.get_f64("delayBetweenShots").unwrap_or(1.0).max(0.1),
            // No flag marks this; the manual says Sun/Rain/Thunder Cannons shoot only straight.
            cardinal_only: def.name.to_ascii_lowercase().contains("cannon"),
            threat: def.get_i64("threat").unwrap_or(0) as i32,
            is_priest: def.has_flag("priest"),
            is_transport: !def.has_flag("priest") && (def.has_flag("walker") || def.has_flag("flyer") || def.has_flag("balloon")),
            is_altar: def.has_flag("dais"),
        }
    }

    /// Damage of one shot.
    pub fn damage_per_shot(&self) -> i32 {
        (self.hp_per_sec as f64 * self.delay_between_shots).round() as i32
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
            cost: 0,
            is_geyser: false,
            is_temple: false,
            max_hit_points: 0,
            range: 0,
            hp_per_sec: 0,
            delay_between_shots: 1.0,
            cardinal_only: false,
            threat: 0,
            is_priest: false,
            is_transport: false,
            is_altar: false,
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
        let t = typefile::parse("typename g\ntypeflags geyser dropBlocking;\n{\n cost = 2000;\n}\nA00 : : \"a.gif\" #0;\n").unwrap();
        let r = TypeRules::from_type(&t);
        assert!(r.is_geyser && r.cost == 2000);
        let t = typefile::parse("typename sunCannon\ntypeflags emplacement;\n{\n maxHitPoints = 600;\n range = 22;\n hpPerSec = 16;\n delayBetweenShots = 5.0;\n threat = 5;\n}\nA00 : : \"a.gif\" #0;\n").unwrap();
        let r = TypeRules::from_type(&t);
        assert_eq!((r.max_hit_points, r.range, r.damage_per_shot()), (600, 22, 80));
        assert!(r.cardinal_only);
    }
}
