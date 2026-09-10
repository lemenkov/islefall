// SPDX-License-Identifier: Apache-2.0
//! Rule hooks in Rhai. The engine is sandboxed: no file, time or random
//! access, so the same inputs always give the same result.

use std::path::Path;

use islefall_data::isle::Theme;
use rhai::{AST, Array, Dynamic, Engine, Map, Scope};

use crate::rules::{AirAttack, EnergyNeed};

pub struct Scripts {
    engine: Engine,
    ast: AST,
}

#[derive(Debug)]
pub enum ScriptError {
    Io(std::io::Error),
    Compile(String),
    /// A hook failed or returned something unexpected.
    Call { hook: &'static str, message: String },
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptError::Io(e) => write!(f, "{e}"),
            ScriptError::Compile(e) => write!(f, "script error: {e}"),
            ScriptError::Call { hook, message } => write!(f, "hook {hook}: {message}"),
        }
    }
}

impl std::error::Error for ScriptError {}

fn theme_name(t: Theme) -> &'static str {
    match t {
        Theme::Sun => "sun",
        Theme::Thunder => "thunder",
        Theme::Wind => "wind",
        Theme::Rain => "rain",
    }
}

impl Scripts {
    pub fn compile(source: &str) -> Result<Scripts, ScriptError> {
        let mut engine = Engine::new();
        engine.set_max_operations(200_000);
        engine.set_max_call_levels(32);
        let ast = engine.compile(source).map_err(|e| ScriptError::Compile(e.to_string()))?;
        Ok(Scripts { engine, ast })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Scripts, ScriptError> {
        Scripts::compile(&std::fs::read_to_string(path).map_err(ScriptError::Io)?)
    }

    fn call(&self, hook: &'static str, args: impl rhai::FuncArgs) -> Result<Dynamic, ScriptError> {
        let mut scope = Scope::new();
        self.engine
            .call_fn::<Dynamic>(&mut scope, &self.ast, hook, args)
            .map_err(|e| ScriptError::Call { hook, message: e.to_string() })
    }

    /// `energy_need(level, theme, mana, class)` -> map or unit.
    pub fn energy_need(&self, level: i64, theme: Theme, mana: &str, class: &str) -> Result<Option<EnergyNeed>, ScriptError> {
        let r = self.call("energy_need", (level, theme_name(theme).to_string(), mana.to_string(), class.to_string()))?;
        if r.is_unit() {
            return Ok(None);
        }
        let map: Map = r.try_cast().ok_or(ScriptError::Call { hook: "energy_need", message: "expected a map or ()".into() })?;
        let get = |k: &str| map.get(k).and_then(|v| v.as_int().ok()).unwrap_or(0);
        let theme = map
            .get("theme")
            .and_then(|v| v.clone().into_string().ok())
            .and_then(|s| Theme::parse(&s))
            .unwrap_or(Theme::Sun);
        Ok(Some(EnergyNeed { theme, themed: get("themed").clamp(0, 255) as u8, any: get("any").clamp(0, 255) as u8 }))
    }

    pub fn fires_straight(&self, name: &str, flags: &[String]) -> Result<bool, ScriptError> {
        let arr: Array = flags.iter().map(|f| Dynamic::from(f.clone())).collect();
        self.call("fires_straight", (name.to_string(), arr))?.as_bool().map_err(|t| ScriptError::Call { hook: "fires_straight", message: format!("expected bool, got {t}") })
    }

    pub fn damage_per_shot(&self, hp_per_sec: i64, delay: f64) -> Result<i32, ScriptError> {
        let v = self.call("damage_per_shot", (hp_per_sec, delay))?;
        v.as_int().map(|i| i as i32).map_err(|t| ScriptError::Call { hook: "damage_per_shot", message: format!("expected int, got {t}") })
    }

    pub fn salvage_refund(&self, cost: i64, hp: i64, max_hp: i64, refund_percent: i64) -> Result<i32, ScriptError> {
        let v = self.call("salvage_refund", (cost, hp, max_hp, refund_percent))?;
        v.as_int().map(|i| i as i32).map_err(|t| ScriptError::Call { hook: "salvage_refund", message: format!("expected int, got {t}") })
    }

    pub fn knowledge_grant(&self, known: &[u8], all: &[u8]) -> Result<Option<u8>, ScriptError> {
        let to_arr = |v: &[u8]| -> Array { v.iter().map(|&b| Dynamic::from(b as i64)).collect() };
        let v = self.call("knowledge_grant", (to_arr(known), to_arr(all)))?;
        if v.is_unit() {
            return Ok(None);
        }
        v.as_int().map(|i| Some(i.clamp(0, 255) as u8)).map_err(|t| ScriptError::Call { hook: "knowledge_grant", message: format!("expected int or (), got {t}") })
    }

    pub fn workshop_can_produce(&self, workshop: Theme, kind: Theme, level: i64) -> Result<bool, ScriptError> {
        self.call("workshop_can_produce", (theme_name(workshop).to_string(), theme_name(kind).to_string(), level))?
            .as_bool()
            .map_err(|t| ScriptError::Call { hook: "workshop_can_produce", message: format!("expected bool, got {t}") })
    }

    /// `air_attack(class, use_air_damage, air_range, air_damage, range, hp_per_sec)` -> map or unit.
    pub fn air_attack(&self, class: &str, use_air_damage: i64, air_range: i64, air_damage: i64, range: i64, hp_per_sec: i64) -> Result<Option<AirAttack>, ScriptError> {
        let r = self.call("air_attack", (class.to_string(), use_air_damage, air_range, air_damage, range, hp_per_sec))?;
        if r.is_unit() {
            return Ok(None);
        }
        let map: Map = r.try_cast().ok_or(ScriptError::Call { hook: "air_attack", message: "expected a map or ()".into() })?;
        let get = |k: &str| map.get(k).and_then(|v| v.as_int().ok()).unwrap_or(0).clamp(0, i32::MAX as i64) as i32;
        let ground = map.get("ground").and_then(|v| v.as_bool().ok()).unwrap_or(true);
        Ok(Some(AirAttack { air_range: get("air_range"), air_damage: get("air_damage"), ground }))
    }

    /// `air_target_priority(distance, is_unit, is_transport, hunts_transports)` -> int or unit.
    pub fn air_target_priority(&self, distance: i64, is_unit: bool, is_transport: bool, hunts_transports: bool) -> Result<Option<i64>, ScriptError> {
        let v = self.call("air_target_priority", (distance, is_unit, is_transport, hunts_transports))?;
        if v.is_unit() {
            return Ok(None);
        }
        v.as_int().map(Some).map_err(|t| ScriptError::Call { hook: "air_target_priority", message: format!("expected int or (), got {t}") })
    }

    /// `sound_gain(edge, zoom, base_db, edge_db, reference_zoom, db_per_halving)` -> decibels.
    pub fn sound_gain(&self, edge: f64, zoom: f64, base_db: f64, edge_db: f64, reference_zoom: f64, db_per_halving: f64) -> Result<f64, ScriptError> {
        let v = self.call("sound_gain", (edge, zoom, base_db, edge_db, reference_zoom, db_per_halving))?;
        v.as_float().map_err(|t| ScriptError::Call { hook: "sound_gain", message: format!("expected float, got {t}") })
    }

    /// `shot_delay(name, class, range, hp_per_sec, usual)` -> seconds
    /// between shots for a type without `delayBetweenShots`.
    pub fn shot_delay(&self, name: &str, class: &str, range: i64, hp_per_sec: i64, default: f64) -> Result<f64, ScriptError> {
        let v = self.call("shot_delay", (name.to_string(), class.to_string(), range, hp_per_sec, default))?;
        v.as_float().map_err(|t| ScriptError::Call { hook: "shot_delay", message: format!("expected float, got {t}") })
    }

    pub fn construction_seconds(&self, cost: i64, construction_rate: f64, power_per_rate: f64) -> Result<f64, ScriptError> {
        let v = self.call("construction_seconds", (cost, construction_rate, power_per_rate))?;
        v.as_float().map_err(|t| ScriptError::Call { hook: "construction_seconds", message: format!("expected float, got {t}") })
    }

    pub fn kill_reward(&self, cost: i64, percent: i64) -> Result<i32, ScriptError> {
        let v = self.call("kill_reward", (cost, percent))?;
        v.as_int().map(|i| i as i32).map_err(|t| ScriptError::Call { hook: "kill_reward", message: format!("expected int, got {t}") })
    }

    /// `bridge_hit(state, damage)` -> "crack", "destroy" or "none".
    pub fn bridge_hit(&self, state: &str, damage: i64) -> Result<String, ScriptError> {
        let v = self.call("bridge_hit", (state.to_string(), damage))?;
        v.into_string().map_err(|t| ScriptError::Call { hook: "bridge_hit", message: format!("expected a string, got {t}") })
    }

    pub fn target_priority(&self, threat: i64, distance: i64, is_unit: bool, is_current: bool) -> Result<i64, ScriptError> {
        let v = self.call("target_priority", (threat, distance, is_unit, is_current))?;
        v.as_int().map_err(|t| ScriptError::Call { hook: "target_priority", message: format!("expected int, got {t}") })
    }
}

/// The repository's script, for tests.
#[cfg(test)]
pub fn test_scripts() -> Scripts {
    Scripts::compile(include_str!("../../../data/scripts/rules.rhai")).expect("data/scripts/rules.rhai compiles")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_follow_the_manual_examples() {
        let s = test_scripts();
        assert_eq!(s.energy_need(3, Theme::Thunder, "", "Shooter").unwrap(), Some(EnergyNeed { theme: Theme::Thunder, themed: 2, any: 1 }));
        assert_eq!(s.energy_need(1, Theme::Thunder, "", "Ground Transport").unwrap(), Some(EnergyNeed { theme: Theme::Thunder, themed: 1, any: 0 }));
        assert_eq!(s.energy_need(2, Theme::Sun, "", "Shooter").unwrap(), Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 2 }));
        assert_eq!(s.energy_need(1, Theme::Wind, "s", "Shooter").unwrap(), Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 1 }));
        assert_eq!(s.energy_need(2, Theme::Sun, "", "").unwrap(), None, "no class, no need");
        assert_eq!(s.energy_need(1, Theme::Wind, "", "Source of Energy").unwrap(), Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 1 }), "a generator asks for any Energy");
        assert!(s.fires_straight("sunCannon", &[]).unwrap());
        assert!(!s.fires_straight("sunArcher", &[]).unwrap());
        assert_eq!(s.damage_per_shot(16, 5.0).unwrap(), 80);
        assert_eq!(s.salvage_refund(400, 50, 100, 25).unwrap(), 50);
        assert_eq!(s.knowledge_grant(&[0, 2], &[0, 2, 3, 4]).unwrap(), Some(3));
        assert_eq!(s.knowledge_grant(&[0], &[0]).unwrap(), None);
        assert!(s.target_priority(25, 3, true, false).unwrap() > s.target_priority(5, 1, false, false).unwrap());
        assert!(s.target_priority(5, 9, false, true).unwrap() > s.target_priority(25, 1, true, false).unwrap(), "a shooter stays on its target");
        assert!(s.workshop_can_produce(Theme::Sun, Theme::Wind, 1).unwrap(), "a Sun Workshop builds a Wind Generator");
        assert!(!s.workshop_can_produce(Theme::Sun, Theme::Wind, 2).unwrap());
        assert!(s.workshop_can_produce(Theme::Wind, Theme::Wind, 3).unwrap());
        assert_eq!(s.air_attack("Anti-Air", 0, 18, 35, 18, 35).unwrap(), Some(AirAttack { air_range: 18, air_damage: 35, ground: false }));
        assert_eq!(s.air_attack("Shooter", 1, 12, 24, 8, 12).unwrap(), Some(AirAttack { air_range: 12, air_damage: 24, ground: true }));
        assert_eq!(s.air_attack("Shooter", 0, 0, 0, 22, 16).unwrap(), Some(AirAttack { air_range: 0, air_damage: 0, ground: true }));
        assert_eq!(s.air_attack("Defense", 0, 0, 0, 0, 0).unwrap(), None);
        assert_eq!(s.bridge_hit("normal", 80).unwrap(), "crack");
        assert_eq!(s.bridge_hit("cracked", 80).unwrap(), "destroy");
        assert_eq!(s.bridge_hit("hard", 80).unwrap(), "none");
        assert_eq!(s.construction_seconds(400, 10.0, 5.0).unwrap(), 8.0, "a Sun Cannon stands in eight seconds");
        assert_eq!(s.construction_seconds(400, 0.0, 5.0).unwrap(), 1.0, "no rate: a moment");
        assert_eq!(s.kill_reward(1200, 25).unwrap(), 300);
        assert_eq!(s.sound_gain(0.0, 4.0, -5.0, -9.0, 4.0, 6.0).unwrap(), -5.0, "centre at the reference zoom: the file's own gain");
        assert_eq!(s.sound_gain(1.0, 4.0, 0.0, -9.0, 4.0, 6.0).unwrap(), -9.0, "the edge loses edge_db");
        assert!((s.sound_gain(0.0, 2.0, 0.0, -9.0, 4.0, 6.0).unwrap() + 6.0).abs() < 1e-9, "half the zoom loses one halving");
        assert_eq!(s.sound_gain(0.0, 8.0, 0.0, -9.0, 4.0, 6.0).unwrap(), 0.0, "zooming in adds nothing");
        assert_eq!(s.air_target_priority(3, true, true, false).unwrap(), None, "a Whirligig never targets a Transport");
        assert!(s.air_target_priority(3, true, true, true).unwrap() > s.air_target_priority(1, false, false, true).unwrap(), "a Man o'War prefers Transports");
    }

    #[test]
    fn broken_scripts_are_reported() {
        assert!(Scripts::compile("fn energy_need( {").is_err());
        let s = Scripts::compile("fn damage_per_shot(a, b) { \"nope\" }").unwrap();
        assert!(s.damage_per_shot(1, 1.0).is_err());
    }
}
