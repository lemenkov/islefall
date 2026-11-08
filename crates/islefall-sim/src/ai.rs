// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! A simple opponent: grows bridges from its own ground towards a target,
//! drops shooters at its bridge ends, and takes a stunned enemy priest to
//! its Altar. Deterministic: it only looks at the world and its own piece
//! queue.

use crate::config::Config;
use crate::grid::Cell;
use crate::pieces::{Piece, PieceQueue};
use crate::rules::TypeRules;
use crate::script::Scripts;
use serde::{Deserialize, Serialize};
use crate::unit::Task;
use crate::world::{DropError, World};

/// The types an opponent builds with, resolved by the app from the data.
pub struct AiKit {
    pub shooter: (String, TypeRules),
    pub generator: (String, TypeRules),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ai {
    pub owner: u8,
    /// Where the bridges are heading, usually the enemy altar.
    pub target: Cell,
    /// Ticks between moves.
    pub every: u32,
    /// Every this many moves, a shooter is dropped instead of a piece.
    pub shooter_every: u32,
    pub shooter_reach: i32,
    pub queue: PieceQueue,
    /// The types this opponent builds with.
    pub shooter: String,
    pub generator: String,
    ticks: u32,
    moves: u32,
}

/// What the opponent did in one move, for logging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiMove {
    Piece { name: String, at: Cell },
    Shooter { kind: String, at: Cell },
    /// A generator dropped so that a later shooter has Energy.
    Generator { kind: String, at: Cell },
    /// A Transport was sent to take a stunned or floating priest.
    Capture { unit: usize, priest: usize },
    /// A Transport carrying a priest was sent to an Altar.
    Sacrifice { unit: usize, altar: usize },
    /// A drop was tried and refused; why, for the log.
    Refused { kind: String, why: String },
    /// A sacrifice was spent: a type name, or "upgrade" for the Altar.
    Knowledge { choice: String },
    Nothing,
}

impl Ai {
    /// An opponent configured from the rules' `[ai]` section.
    pub fn new(owner: u8, target: Cell, cfg: &Config) -> Ai {
        Ai {
            owner,
            target,
            every: cfg.ticks(cfg.ai.move_seconds),
            shooter_every: cfg.ai.shooter_every.max(1),
            shooter_reach: cfg.ai.shooter_reach,
            queue: PieceQueue::from_defs(&cfg.bridges.pieces, cfg.bridges.piece_slots, cfg.ai.queue_seed.wrapping_add(owner as u64)),
            shooter: cfg.ai.shooter.clone(),
            generator: cfg.ai.generator.clone(),
            ticks: 0,
            moves: 0,
        }
    }

    /// Call once per simulation tick; acts every `every` ticks.
    pub fn tick(&mut self, world: &mut World, kit: &AiKit, scripts: &Scripts) -> Option<AiMove> {
        if world.pending_choice(self.owner) {
            if let Some(choice) = world.auto_choose_knowledge(self.owner, scripts) {
                return Some(AiMove::Knowledge { choice });
            }
        }
        self.ticks += 1;
        if self.ticks < self.every {
            return None;
        }
        self.ticks = 0;
        self.moves += 1;
        // A priest already in hand is worth more than any new work; then a
        // priest who can be picked up is worth more than harvesting.
        if let Some(m) = self.carry_priest_to_altar(world) {
            return Some(m);
        }
        if let Some(m) = self.capture_a_priest(world) {
            return Some(m);
        }
        self.send_idle_transports_harvesting(world);
        self.keep_in_production(world, kit, scripts);
        let m = if self.moves % self.shooter_every == 0 { self.drop_battle_unit(world, kit) } else { AiMove::Nothing };
        if !matches!(m, AiMove::Nothing) {
            return Some(m);
        }
        Some(self.extend_bridge(world))
    }

    /// Send a Transport carrying a priest to the nearest finished Altar of
    /// its owner. Altars are tried nearest first, then next move if the
    /// path is blocked.
    fn carry_priest_to_altar(&self, world: &mut World) -> Option<AiMove> {
        let carriers: Vec<usize> = world
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && u.owner == self.owner && u.is_transport && u.carrying.is_some() && !matches!(u.task, Task::Sacrifice { .. }))
            .map(|(i, _)| i)
            .collect();
        if carriers.is_empty() {
            return None;
        }
        let altars: Vec<usize> = world.structures.iter().enumerate().filter(|(_, s)| s.is_altar && s.owner == self.owner && s.complete()).map(|(i, _)| i).collect();
        for u in carriers {
            let at = world.units[u].cell();
            let mut altars = altars.clone();
            altars.sort_by_key(|&a| (Self::distance(at, world.structures[a].centre()), a));
            for altar in altars {
                if world.order_sacrifice(u, altar) {
                    return Some(AiMove::Sacrifice { unit: u, altar });
                }
            }
        }
        None
    }

    /// Find an idle Transport and a stunned enemy priest (or a floating
    /// priest, if the Transport flies) and send the closest pair together.
    fn capture_a_priest(&self, world: &mut World) -> Option<AiMove> {
        let priests: Vec<usize> = world
            .units
            .iter()
            .enumerate()
            .filter(|(_, p)| p.alive && p.is_priest && p.carried_by.is_none() && p.invisible == 0 && world.hostile(p.owner, self.owner) && (p.stunned || p.floating))
            .map(|(i, _)| i)
            .collect();
        if priests.is_empty() {
            return None;
        }
        let transports: Vec<usize> = world
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && u.owner == self.owner && u.is_transport && u.carrying.is_none() && u.carried_by.is_none() && !u.stunned && u.task == Task::Idle && !u.is_moving())
            .map(|(i, _)| i)
            .collect();
        let mut pairs = Vec::new();
        for &unit in &transports {
            let at = world.units[unit].cell();
            for &priest in &priests {
                let p = &world.units[priest];
                if !p.stunned && !(p.floating && world.units[unit].is_air) {
                    continue;
                }
                pairs.push((Self::distance(at, p.cell()), unit, priest));
            }
        }
        pairs.sort_by_key(|&(distance, unit, priest)| (distance, unit, priest));
        for (_, unit, priest) in pairs {
            if world.order_capture(unit, priest) {
                return Some(AiMove::Capture { unit, priest });
            }
        }
        None
    }

    /// Keep the shooter and the generator in production wherever a
    /// Workshop of the opponent's allows; nothing happens without one.
    fn keep_in_production(&self, world: &mut World, kit: &AiKit, scripts: &Scripts) {
        for (kind, rules) in [&kit.shooter, &kit.generator] {
            if !world.in_production(self.owner, kind) {
                let _ = world.put_into_production(self.owner, kind, rules, scripts);
            }
        }
    }

    /// Idle Transports of the opponent go and harvest the nearest geyser with stock.
    fn send_idle_transports_harvesting(&self, world: &mut World) {
        let has_temple = world.structures.iter().any(|s| s.is_temple && s.owner == self.owner);
        if !has_temple {
            return;
        }
        let idle: Vec<usize> = world
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && u.owner == self.owner && u.is_transport && u.carrying.is_none() && u.task == Task::Idle && !u.is_moving())
            .map(|(i, _)| i)
            .collect();
        for u in idle {
            let at = world.units[u].cell();
            let mut geysers: Vec<usize> = (0..world.structures.len()).filter(|&i| world.structures[i].stock > 0).collect();
            geysers.sort_by_key(|&i| Self::distance(world.structures[i].cell, at));
            for g in geysers {
                if world.order_harvest(u, g) {
                    break;
                }
            }
        }
    }

    fn distance(a: Cell, b: Cell) -> i32 {
        (a.x - b.x).abs() + (a.y - b.y).abs()
    }

    /// Cells of the opponent's ground that border the sky: island rims and open ends.
    fn frontier(&self, world: &World) -> Vec<Cell> {
        let mut out = Vec::new();
        for (i, island) in world.islands.iter().enumerate() {
            if world.island_owners.get(i).copied().flatten() != Some(self.owner) {
                continue;
            }
            for c in island.cells() {
                if [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| !world.is_ground(c.offset(dx, dy))) {
                    out.push(c);
                }
            }
        }
        for (&c, &o) in &world.bridge_owners {
            if o == self.owner && world.is_open_end(c) {
                out.push(c);
            }
        }
        out
    }

    /// Place the piece and rotation that brings the bridge closest to the target.
    fn extend_bridge(&mut self, world: &mut World) -> AiMove {
        let frontier = self.frontier(world);
        let current = frontier.iter().map(|&c| Self::distance(c, self.target)).min().unwrap_or(i32::MAX);
        let mut best: Option<(i32, usize, Piece, Cell)> = None;
        for (slot, piece) in self.queue.slots.iter().enumerate() {
            let mut p = piece.clone();
            for _ in 0..4 {
                for &f in &frontier {
                    for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                        let anchor = f.offset(dx, dy);
                        for &(ox, oy) in &p.cells {
                            let origin = anchor.offset(-ox, -oy);
                            let cells = p.cells_at(origin);
                            if world.can_place_piece(&cells).is_err() {
                                continue;
                            }
                            let score = cells.iter().map(|&c| Self::distance(c, self.target)).min().unwrap_or(i32::MAX);
                            if best.as_ref().is_none_or(|b| score < b.0) {
                                best = Some((score, slot, p.clone(), origin));
                            }
                        }
                    }
                }
                p = p.rotated();
            }
        }
        let Some((score, slot, piece, origin)) = best else { return AiMove::Nothing };
        if score >= current && current <= 1 {
            return AiMove::Nothing; // already touching the target
        }
        let cells = piece.cells_at(origin);
        let harden = world.cfg.ticks(world.cfg.bridges.harden_seconds);
        let fresh = world.cfg.bridges.harden_seconds > 0.0 && self.queue.fresh(slot, world.tick, harden);
        if world.place_piece_as(self.owner, &cells, fresh).is_err() {
            return AiMove::Nothing;
        }
        self.queue.refill(slot, world.tick);
        AiMove::Piece { name: piece.name.clone(), at: origin }
    }

    /// Drop a shooter in the sky next to the open end nearest the target.
    /// Drop the shooter at the open end nearest the target; when every site
    /// lacks Energy, drop the generator at the nearest site that has it,
    /// so the line of batteries follows the bridge.
    fn drop_battle_unit(&mut self, world: &mut World, kit: &AiKit) -> AiMove {
        let mut ends: Vec<Cell> = world.bridge_owners.iter().filter(|(c, o)| **o == self.owner && world.is_open_end(**c)).map(|(&c, _)| c).collect();
        ends.sort_by_key(|&c| Self::distance(c, self.target));
        let (kind, rules) = &kit.shooter;
        let mut refused: Vec<DropError> = Vec::new();
        for end in ends.iter().take(3) {
            match self.drop_near(world, *end, kind, rules) {
                Ok(at) => return AiMove::Shooter { kind: kind.clone(), at },
                Err(Some(e)) => refused.push(e),
                Err(None) => {}
            }
        }
        let mut why = refused.first().map(|e| e.to_string());
        if refused.iter().any(|e| matches!(e, DropError::NotEnoughEnergy { .. })) {
            // Chain batteries from the island's edge along the bridge: the
            // site nearest the target that Energy already reaches.
            let (kind, rules) = &kit.generator;
            let mut anchors = self.frontier(world);
            anchors.sort_by_key(|&c| Self::distance(c, self.target));
            let mut generator_why: Option<DropError> = None;
            for anchor in anchors {
                match self.drop_near(world, anchor, kind, rules) {
                    Ok(at) => return AiMove::Generator { kind: kind.clone(), at },
                    Err(e) => generator_why = generator_why.or(e),
                }
            }
            if let Some(e) = generator_why {
                why = Some(format!("{}; no site for a {kind} either: {e}", why.unwrap_or_default()));
            }
        }
        match why {
            Some(why) => AiMove::Refused { kind: kind.clone(), why },
            None => AiMove::Nothing,
        }
    }

    /// Try the sites around an open end nearest the target first. `Err(None)`
    /// means no site fits at all; `Err(Some(e))` the rule that refused the best.
    fn drop_near(&self, world: &mut World, end: Cell, kind: &str, rules: &TypeRules) -> Result<Cell, Option<DropError>> {
        let mut spots = Vec::new();
        for dy in -self.shooter_reach..=self.shooter_reach {
            for dx in -self.shooter_reach..=self.shooter_reach {
                let at = end.offset(dx, dy);
                if world.can_drop(rules, at).is_ok() {
                    spots.push(at);
                }
            }
        }
        spots.sort_by_key(|&c| Self::distance(c, self.target));
        let mut first = None;
        for at in spots {
            match world.drop_structure_for(self.owner, kind, rules, at) {
                Ok(_) => return Ok(at),
                Err(e) => first = first.or(Some(e)),
            }
        }
        Err(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_config;
    use crate::island::IslandMap;
    use crate::script::test_scripts;

    #[test]
    fn opponent_bridges_towards_the_target_and_drops_shooters() {
        let mut cfg = test_config();
        cfg.ai.move_seconds = 10.0 / cfg.sim.tick_hz as f64;
        let mut ai = Ai::new(1, Cell::new(2, 2), &cfg);
        let mut w = World::new(cfg);
        w.push_island(IslandMap::rect(Cell::new(0, 0), 6, 6), 0);
        w.push_island(IslandMap::rect(Cell::new(20, 0), 6, 6), 1);
        w.powers[1] = 5000;
        let scripts = test_scripts();
        let thrower = TypeRules { foot_x: 3, foot_y: 3, creates_island: true, may_drop_on_rim: true, max_hit_points: 400, range: 8, hp_per_sec: 12, damage_per_shot: 12, ..TypeRules::plain() };
        let battery = TypeRules { foot_x: 2, foot_y: 2, creates_island: true, may_drop_on_rim: true, max_hit_points: 200, ..TypeRules::plain() };
        let kit = AiKit { shooter: ("sunarcher".into(), thrower), generator: ("sunbattery".into(), battery) };
        let mut pieces = 0;
        let mut shooters = 0;
        for _ in 0..600 {
            w.step(&scripts);
            match ai.tick(&mut w, &kit, &scripts) {
                Some(AiMove::Piece { .. }) => pieces += 1,
                Some(AiMove::Shooter { .. }) => shooters += 1,
                _ => {}
            }
        }
        assert!(pieces >= 10, "placed {pieces} pieces");
        assert!(shooters >= 1, "dropped {shooters} shooters");
        let closest = w.bridge_owners.keys().map(|c| Ai::distance(*c, Cell::new(2, 2))).min().unwrap();
        assert!(closest < 14, "bridge got within {closest} of the target");
        assert!(w.bridge_owners.values().all(|&o| o == 1));
        assert!(w.structures.iter().any(|s| s.owner == 1 && s.kind == "sunarcher"));
    }

    #[test]
    fn opponent_captures_a_stunned_priest_and_sacrifices_him() {
        let mut cfg = test_config();
        cfg.ai.move_seconds = 1.0 / cfg.sim.tick_hz as f64;
        let mut ai = Ai::new(1, Cell::new(8, 3), &cfg);
        let mut w = World::new(cfg);
        w.push_island(IslandMap::rect(Cell::new(0, 0), 14, 8), 1);
        w.powers[1] = 1000;
        w.energy_enforced = false;
        let altar = TypeRules { foot_x: 3, foot_y: 3, is_altar: true, may_drop_on_rim: true, max_hit_points: 1000, cost: 500, ..TypeRules::plain() };
        let a = w.drop_structure_for(1, "dais", &altar, Cell::new(8, 3)).unwrap();
        let priest = TypeRules { is_unit: true, is_priest: true, max_hit_points: 100, threat: 25, speed: 1.8, ..TypeRules::plain() };
        let p = w.spawn_unit_for(0, "priest", &priest, Cell::new(11, 5)).unwrap();
        let golem = TypeRules { is_unit: true, is_transport: true, max_hit_points: 50, speed: 3.0, ..TypeRules::plain() };
        let g = w.spawn_unit_for(1, "sunwalker", &golem, Cell::new(1, 5)).unwrap();
        let scripts = test_scripts();
        assert!(!w.units[p].stunned);
        w.damage_unit(p, 50);
        w.step(&scripts);
        assert!(w.units[p].stunned, "half health stuns the priest");
        let kit = AiKit { shooter: ("sunarcher".into(), TypeRules::plain()), generator: ("sunbattery".into(), TypeRules::plain()) };
        let (mut captured, mut sacrificed) = (false, false);
        for _ in 0..2000 {
            w.step(&scripts);
            match ai.tick(&mut w, &kit, &scripts) {
                Some(AiMove::Capture { priest, .. }) if priest == p => captured = true,
                Some(AiMove::Sacrifice { altar, .. }) if altar == a => sacrificed = true,
                _ => {}
            }
            if !w.units[p].alive {
                break;
            }
        }
        assert!(captured, "the opponent went for the stunned priest");
        assert!(sacrificed, "and carried him to its finished Altar");
        assert!(!w.units[p].alive, "the priest was sacrificed");
        assert_eq!(w.units[g].carrying, None);
    }

    #[test]
    fn a_flying_transport_takes_a_floating_priest() {
        use crate::unit::Pos;
        let cfg = test_config();
        let ai = Ai::new(1, Cell::new(4, 4), &cfg);
        let mut w = World::new(cfg);
        w.push_island(IslandMap::rect(Cell::new(0, 0), 14, 8), 1);
        let priest = TypeRules { is_unit: true, is_priest: true, max_hit_points: 100, speed: 1.8, ..TypeRules::plain() };
        let p = w.spawn_unit_for(0, "priest", &priest, Cell::new(11, 5)).unwrap();
        w.units[p].floating = true;
        w.units[p].pos = Pos::cell_centre(Cell::new(20, 5), w.subcell());
        let balloon = TypeRules { is_unit: true, is_air: true, is_transport: true, max_hit_points: 150, speed: 4.0, ..TypeRules::plain() };
        let b = w.spawn_unit_for(1, "sunballoon", &balloon, Cell::new(1, 5)).unwrap();
        assert_eq!(ai.capture_a_priest(&mut w), Some(AiMove::Capture { unit: b, priest: p }));
        assert_eq!(w.units[b].task, Task::Capture { priest: p });
    }

    #[test]
    fn opponent_leaves_a_healthy_priest_alone() {
        let mut cfg = test_config();
        cfg.ai.move_seconds = 1.0 / cfg.sim.tick_hz as f64;
        let mut ai = Ai::new(1, Cell::new(4, 4), &cfg);
        let mut w = World::new(cfg);
        w.push_island(IslandMap::rect(Cell::new(0, 0), 14, 8), 1);
        let priest = TypeRules { is_unit: true, is_priest: true, max_hit_points: 100, speed: 1.8, ..TypeRules::plain() };
        let p = w.spawn_unit_for(0, "priest", &priest, Cell::new(11, 5)).unwrap();
        let golem = TypeRules { is_unit: true, is_transport: true, max_hit_points: 50, speed: 3.0, ..TypeRules::plain() };
        w.spawn_unit_for(1, "sunwalker", &golem, Cell::new(1, 5)).unwrap();
        let scripts = test_scripts();
        let kit = AiKit { shooter: ("sunarcher".into(), TypeRules::plain()), generator: ("sunbattery".into(), TypeRules::plain()) };
        for _ in 0..300 {
            w.step(&scripts);
            assert!(!matches!(ai.tick(&mut w, &kit, &scripts), Some(AiMove::Capture { .. })));
        }
        assert!(w.units[p].alive && w.units[p].carrying == None);
    }

    #[test]
    fn opponent_chains_generators_when_its_shooters_need_energy() {
        use crate::rules::EnergyNeed;
        use islefall_data::isle::Theme;
        let mut cfg = test_config();
        cfg.ai.move_seconds = 10.0 / cfg.sim.tick_hz as f64;
        let mut ai = Ai::new(1, Cell::new(2, 2), &cfg);
        let mut w = World::new(cfg);
        w.push_island(IslandMap::rect(Cell::new(0, 0), 6, 6), 0);
        w.push_island(IslandMap::rect(Cell::new(30, 0), 8, 8), 1);
        let scripts = test_scripts();
        let temple = TypeRules { foot_x: 2, foot_y: 2, is_temple: true, produces: Some(Theme::Sun), ..TypeRules::plain() };
        let workshop = TypeRules { foot_x: 2, foot_y: 2, is_workshop: true, theme: Theme::Sun, ..TypeRules::plain() };
        w.drop_structure_for(1, "residence", &temple, Cell::new(32, 2)).unwrap();
        w.drop_structure_for(1, "sunfactory", &workshop, Cell::new(35, 5)).unwrap();
        w.powers[1] = 20_000;
        w.energy_enforced = true;
        let need = Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 1 });
        let thrower = TypeRules { foot_x: 3, foot_y: 3, creates_island: true, may_drop_on_rim: true, max_hit_points: 400, range: 8, hp_per_sec: 12, damage_per_shot: 12, energy: need, cost: 250, ..TypeRules::plain() };
        let battery = TypeRules { foot_x: 2, foot_y: 2, creates_island: true, may_drop_on_rim: true, max_hit_points: 200, produces: Some(Theme::Sun), energy: need, cost: 300, ..TypeRules::plain() };
        let kit = AiKit { shooter: ("sunarcher".into(), thrower), generator: ("sunbattery".into(), battery) };
        let (mut shooters, mut generators) = (0, 0);
        for _ in 0..1500 {
            w.step(&scripts);
            match ai.tick(&mut w, &kit, &scripts) {
                Some(AiMove::Shooter { .. }) => shooters += 1,
                Some(AiMove::Generator { .. }) => generators += 1,
                _ => {}
            }
        }
        assert!(generators >= 1, "a battery went down to feed the line ({generators})");
        assert!(shooters >= 1, "and shooters followed ({shooters})");
        assert!(w.structures.iter().any(|s| s.owner == 1 && s.slots > 0 && s.production.len() == 2), "both types in production");
    }
}

/// The opponents' state as bytes, for a snapshot: their counters and piece
/// queues live outside the world, and a rejoining player needs them too.
pub fn encode_ais(ais: &[Ai]) -> Vec<u8> {
    postcard::to_stdvec(ais).unwrap_or_default()
}

pub fn decode_ais(bytes: &[u8]) -> Result<Vec<Ai>, String> {
    postcard::from_bytes(bytes).map_err(|e| format!("opponents: {e}"))
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::config::test_config;

    #[test]
    fn the_opponents_state_survives_a_snapshot() {
        let cfg = test_config();
        let mut ai = Ai::new(2, Cell::new(9, 7), &cfg);
        ai.queue.refill(1, 40);
        let ais = vec![ai, Ai::new(3, Cell::new(1, 1), &cfg)];
        assert_eq!(decode_ais(&encode_ais(&ais)).unwrap(), ais);
        assert_eq!(decode_ais(&encode_ais(&[])).unwrap(), Vec::new());
    }
}
