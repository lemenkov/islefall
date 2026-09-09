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

use crate::config::Config;
use crate::script::{ScriptError, Scripts};

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

/// What a shooting type can hit: flyers within `air_range` for `air_damage`
/// per second, and the ground when `ground`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AirAttack {
    pub air_range: i32,
    pub air_damage: i32,
    pub ground: bool,
}

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
    /// `delayBetweenShots` in seconds (the rules' default when absent).
    pub delay_between_shots: f64,
    /// Damage of one shot, from the `damage_per_shot` hook.
    pub damage_per_shot: i32,
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
    /// A Workshop: holds production slots.
    pub is_workshop: bool,
    /// An aerial attacker (`flyer` flag): launched by a base, never a Transport.
    pub is_flyer: bool,
    /// Flies: an attacker or an Aerial Transport; needs no ground and never falls.
    pub is_air: bool,
    /// What the type can shoot at, when it shoots.
    pub air_attack: Option<AirAttack>,
    /// Damage of one shot at a flyer, from `airdamage` and the delay.
    pub air_damage_per_shot: i32,
    /// A base launching the attacker named here.
    pub launches: Option<String>,
    /// The type's alignment.
    pub theme: Theme,
    /// The type's `level` (0 when absent).
    pub level: i64,
}

impl TypeRules {
    /// Read a type's rules: flag meanings come from the rules file, formulas
    /// from the script hooks.
    pub fn from_type(def: &TypeDef, cfg: &Config, scripts: &Scripts) -> Result<TypeRules, ScriptError> {
        let f = &cfg.flags;
        let has = |names: &[String]| Config::has_any(names, &def.flags);
        let walk = if has(&f.walk_blocked) {
            Walk::Blocked
        } else if has(&f.walk_avoid) {
            Walk::Avoid
        } else {
            Walk::Free
        };
        let is_priest = has(&f.priest);
        let is_unit = has(&f.unit);
        let is_flyer = has(&f.flyer);
        let is_air = is_flyer || has(&f.balloon);
        let class = def.get_str("class").unwrap_or("");
        let is_air_base = cfg.air.base_classes.iter().any(|c| c.eq_ignore_ascii_case(class));
        let launches = if is_air_base { cfg.air.launches.get(&def.name.to_lowercase()).cloned() } else { None };
        let hp_per_sec = def.get_i64("hpPerSec").unwrap_or(0).max(0);
        let delay = def.get_f64("delayBetweenShots").unwrap_or(cfg.combat.default_delay_between_shots).max(0.1);
        let theme = def.get_str("theme").and_then(Theme::parse).unwrap_or(Theme::Sun);
        let level = def.get_i64("level").unwrap_or(0).clamp(0, 9);
        let energy = scripts.energy_need(level, theme, def.get_str("mana").unwrap_or(""), def.get_str("class").is_some())?;
        let produces = if has(&f.temple) {
            Some(theme)
        } else if def.get_str("class").is_some_and(|c| f.energy_source_classes.iter().any(|e| e.eq_ignore_ascii_case(c))) {
            Some(theme)
        } else {
            None
        };
        let air_attack = scripts.air_attack(
            class,
            def.get_i64("useairdamage").unwrap_or(0),
            def.get_i64("airrange").unwrap_or(0),
            def.get_i64("airdamage").unwrap_or(0),
            def.get_i64("range").unwrap_or(0),
            hp_per_sec,
        )?;
        let air_damage_per_shot = match air_attack {
            Some(a) if a.air_damage > 0 => scripts.damage_per_shot(a.air_damage as i64, delay)?,
            _ => 0,
        };
        Ok(TypeRules {
            foot_x: def.get_i64("foot_x").unwrap_or(1).max(1) as i32,
            foot_y: def.get_i64("foot_y").unwrap_or(1).max(1) as i32,
            walk,
            drop_blocking: has(&f.drop_blocking),
            creates_island: has(&f.creates_island),
            may_drop_on_rim: has(&f.may_drop_on_rim),
            is_unit,
            speed: def.get_f64("speed").unwrap_or(1.0),
            cost: def.get_i64("cost").unwrap_or(0).clamp(0, i32::MAX as i64) as i32,
            is_geyser: has(&f.geyser),
            is_temple: has(&f.temple),
            max_hit_points: def.get_i64("maxHitPoints").unwrap_or(0).clamp(0, i32::MAX as i64) as i32,
            range: if air_attack.is_some() { def.get_i64("range").unwrap_or(0) as i32 } else { 0 },
            hp_per_sec: hp_per_sec as i32,
            delay_between_shots: delay,
            damage_per_shot: scripts.damage_per_shot(hp_per_sec, delay)?,
            cardinal_only: scripts.fires_straight(&def.name, &def.flags)?,
            threat: def.get_i64("threat").unwrap_or(0) as i32,
            is_priest,
            is_transport: !is_priest && is_unit && !is_flyer,
            is_altar: has(&f.altar),
            energy,
            produces,
            tech_bit: def.get_i64("techBit").filter(|b| (0..=255).contains(b)).map(|b| b as u8),
            is_workshop: has(&f.workshop),
            is_flyer,
            is_air,
            air_attack,
            air_damage_per_shot,
            launches,
            theme,
            level,
        })
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
            damage_per_shot: 0,
            cardinal_only: false,
            threat: 0,
            is_priest: false,
            is_transport: false,
            is_altar: false,
            energy: None,
            produces: None,
            tech_bit: None,
            is_workshop: false,
            is_flyer: false,
            is_air: false,
            air_attack: None,
            air_damage_per_shot: 0,
            launches: None,
            theme: Theme::Sun,
            level: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use islefall_data::typefile;

    fn rules_of(src: &str) -> TypeRules {
        let cfg = crate::config::test_config();
        let scripts = crate::script::test_scripts();
        TypeRules::from_type(&typefile::parse(src).unwrap(), &cfg, &scripts).unwrap()
    }

    #[test]
    fn reads_flags_and_footprint() {
        let r = rules_of("typename x\ntypeflags yuckWalk dropBlocking factory;\n{\n foot_x = 8;\n foot_y = 6;\n}\nA00 : : \"a.gif\" #0;\n");
        assert_eq!((r.foot_x, r.foot_y), (8, 6));
        assert_eq!(r.walk, Walk::Avoid);
        assert!(r.drop_blocking && !r.creates_island && !r.is_unit);
        let r = rules_of("typename y\ntypeflags walker shadow;\n{\n speed = 1.8;\n}\nA00 : : \"a.gif\" #0;\n");
        assert!(r.is_unit);
        assert_eq!(r.speed, 1.8);
        assert_eq!(r.walk, Walk::Free);
        let r = rules_of("typename g\ntypeflags geyser dropBlocking;\n{\n cost = 2000;\n}\nA00 : : \"a.gif\" #0;\n");
        assert!(r.is_geyser && r.cost == 2000);
        let r = rules_of("typename sunCannon\ntypeflags emplacement;\n{\n maxHitPoints = 600;\n range = 22;\n hpPerSec = 16;\n delayBetweenShots = 5.0;\n threat = 5;\n}\nA00 : : \"a.gif\" #0;\n");
        assert_eq!((r.max_hit_points, r.range, r.damage_per_shot), (600, 22, 80));
        assert!(r.cardinal_only);
    }

    #[test]
    fn energy_needs_follow_level_theme_and_mana() {
        let parse = rules_of;
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
