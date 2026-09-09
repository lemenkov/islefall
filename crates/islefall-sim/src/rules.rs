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
use islefall_data::isle::Theme;

/// Energy a type needs at its placement site. Sun means any kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnergyNeed {
    pub theme: Theme,
    /// Units that must be of `theme` (0 for Sun types).
    pub themed: u8,
    /// Units of any kind on top of the themed ones.
    pub any: u8,
}

impl EnergyNeed {
    pub fn total(&self) -> u8 {
        self.themed + self.any
    }
}

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
    /// Energy required to place the type, if any.
    pub energy: Option<EnergyNeed>,
    /// Energy produced (Generators and Temples).
    pub produces: Option<Theme>,
    /// `techBit`: Knowledge needed before the type can be built, if any.
    pub tech_bit: Option<u8>,
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
            energy: energy_need(def),
            produces: energy_produced(def),
            tech_bit: def.get_i64("techBit").filter(|b| (0..=255).contains(b)).map(|b| b as u8),
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
            energy: None,
            produces: None,
            tech_bit: None,
        }
    }
}

/// Read a type's Energy requirement.
///
/// A `mana` string lists units by letter (`s` any, `w` wind, `r` rain,
/// `t` thunder). Otherwise Battle units (those with a `class`) derive it
/// from `level` and `theme`, following the manual's examples: a Level One
/// Bulf needs one Thunder, a Level Two Whirlibase two Sun, and an Air Ship
/// (Level Three, Wind) two Wind and one Sun. So a Sun type needs `level`
/// units of any kind; a themed type needs its theme for all but one unit
/// from level two up.
fn energy_need(def: &TypeDef) -> Option<EnergyNeed> {
    if let Some(m) = def.get_str("mana") {
        let mut need = EnergyNeed { theme: Theme::Sun, themed: 0, any: 0 };
        for ch in m.chars() {
            match ch.to_ascii_lowercase() {
                's' => need.any += 1,
                'w' | 'r' | 't' => {
                    need.theme = match ch.to_ascii_lowercase() {
                        'w' => Theme::Wind,
                        'r' => Theme::Rain,
                        _ => Theme::Thunder,
                    };
                    need.themed += 1;
                }
                _ => {}
            }
        }
        return (need.total() > 0).then_some(need);
    }
    let level = def.get_i64("level")?.clamp(0, 9) as u8;
    if level == 0 || def.get_str("class").is_none() {
        return None;
    }
    let theme = def.get_str("theme").and_then(Theme::parse).unwrap_or(Theme::Sun);
    Some(match theme {
        Theme::Sun => EnergyNeed { theme, themed: 0, any: level },
        _ if level == 1 => EnergyNeed { theme, themed: 1, any: 0 },
        _ => EnergyNeed { theme, themed: level - 1, any: 1 },
    })
}

/// Generators ("Source of Energy") produce their theme; a Temple produces Sun.
fn energy_produced(def: &TypeDef) -> Option<Theme> {
    if def.has_flag("residence") {
        return Some(def.get_str("theme").and_then(Theme::parse).unwrap_or(Theme::Sun));
    }
    if def.get_str("class").is_some_and(|c| c.eq_ignore_ascii_case("Source of Energy")) {
        return Some(def.get_str("theme").and_then(Theme::parse).unwrap_or(Theme::Sun));
    }
    None
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

    #[test]
    fn energy_needs_follow_level_theme_and_mana() {
        let parse = |src: &str| TypeRules::from_type(&typefile::parse(src).unwrap());
        let cannon = parse("typename thunderCannon\ntypeflags emplacement;\n{\n class=\"Shooter\";\n theme=\"thunder\";\n level = 3;\n}\nA00 : : \"a.gif\" #0;\n");
        assert_eq!(cannon.energy, Some(EnergyNeed { theme: Theme::Thunder, themed: 2, any: 1 }));
        let bulf = parse("typename bulf\ntypeflags walker;\n{\n class=\"Ground Transport\";\n theme=\"thunder\";\n level = 1;\n}\nA00 : : \"a.gif\" #0;\n");
        assert_eq!(bulf.energy, Some(EnergyNeed { theme: Theme::Thunder, themed: 1, any: 0 }));
        let aviary = parse("typename sunAviary\ntypeflags emplacement;\n{\n class=\"Air Attack Base\";\n theme=\"sun\";\n level = 2;\n}\nA00 : : \"a.gif\" #0;\n");
        assert_eq!(aviary.energy, Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 2 }));
        let battery = parse("typename windBattery\ntypeflags emplacement;\n{\n class=\"Source of Energy\";\n theme=\"wind\";\n level = 1;\n mana = \"s\";\n}\nA00 : : \"a.gif\" #0;\n");
        assert_eq!(battery.energy, Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 1 }));
        assert_eq!(battery.produces, Some(Theme::Wind));
        let tree = parse("typename treeTwo\ntypeflags tree;\n{\n theme=\"sun\";\n level = 2;\n}\nA00 : : \"a.gif\" #0;\n");
        assert_eq!(tree.energy, None, "terrain has no class and needs no energy");
        let temple = parse("typename residence\ntypeflags residence;\n{\n}\nA00 : : \"a.gif\" #0;\n");
        assert_eq!(temple.produces, Some(Theme::Sun));
    }
}
