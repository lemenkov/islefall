// SPDX-License-Identifier: Apache-2.0
//! Rule hooks in Rhai. The engine is sandboxed: no file, time or random
//! access, so the same inputs always give the same result.

use std::path::Path;

use islefall_data::isle::Theme;
use rhai::{AST, Array, Dynamic, Engine, Map, Scope};

use crate::rules::EnergyNeed;

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

    /// `energy_need(level, theme, mana, has_class)` -> map or unit.
    pub fn energy_need(&self, level: i64, theme: Theme, mana: &str, has_class: bool) -> Result<Option<EnergyNeed>, ScriptError> {
        let r = self.call("energy_need", (level, theme_name(theme).to_string(), mana.to_string(), has_class))?;
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

    pub fn target_priority(&self, threat: i64, distance: i64, is_unit: bool) -> Result<i64, ScriptError> {
        let v = self.call("target_priority", (threat, distance, is_unit))?;
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
        assert_eq!(s.energy_need(3, Theme::Thunder, "", true).unwrap(), Some(EnergyNeed { theme: Theme::Thunder, themed: 2, any: 1 }));
        assert_eq!(s.energy_need(1, Theme::Thunder, "", true).unwrap(), Some(EnergyNeed { theme: Theme::Thunder, themed: 1, any: 0 }));
        assert_eq!(s.energy_need(2, Theme::Sun, "", true).unwrap(), Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 2 }));
        assert_eq!(s.energy_need(1, Theme::Wind, "s", true).unwrap(), Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 1 }));
        assert_eq!(s.energy_need(2, Theme::Sun, "", false).unwrap(), None, "no class, no need");
        assert!(s.fires_straight("sunCannon", &[]).unwrap());
        assert!(!s.fires_straight("sunArcher", &[]).unwrap());
        assert_eq!(s.damage_per_shot(16, 5.0).unwrap(), 80);
        assert_eq!(s.salvage_refund(400, 50, 100, 25).unwrap(), 50);
        assert_eq!(s.knowledge_grant(&[0, 2], &[0, 2, 3, 4]).unwrap(), Some(3));
        assert_eq!(s.knowledge_grant(&[0], &[0]).unwrap(), None);
        assert!(s.target_priority(25, 3, true).unwrap() > s.target_priority(5, 1, false).unwrap());
        assert!(s.workshop_can_produce(Theme::Sun, Theme::Wind, 1).unwrap(), "a Sun Workshop builds a Wind Generator");
        assert!(!s.workshop_can_produce(Theme::Sun, Theme::Wind, 2).unwrap());
        assert!(s.workshop_can_produce(Theme::Wind, Theme::Wind, 3).unwrap());
    }

    #[test]
    fn broken_scripts_are_reported() {
        assert!(Scripts::compile("fn energy_need( {").is_err());
        let s = Scripts::compile("fn damage_per_shot(a, b) { \"nope\" }").unwrap();
        assert!(s.damage_per_shot(1, 1.0).is_err());
    }
}
