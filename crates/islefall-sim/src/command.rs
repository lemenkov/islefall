// SPDX-License-Identifier: Apache-2.0
//! Everything a player can do, as data. The app, a replay file and (in
//! time) the network all speak commands; `World::apply` is the one door
//! into the simulation, so the same commands in the same order give the
//! same game everywhere.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::grid::Cell;
use crate::pieces::PieceQueue;
use crate::script::Scripts;
use crate::unit::Task;
use crate::world::World;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    /// Walk or fly a unit to a cell, dropping any task.
    Move { unit: usize, to: Cell },
    /// Lay the piece in queue slot `slot`, turned `rotations` quarter turns, with its origin at `at`.
    PlacePiece { slot: usize, rotations: u8, at: Cell },
    /// Drop a structure of type `kind` with its hotspot at `at`.
    Drop { kind: String, at: Cell },
    /// Place a unit of type `kind` at `at`.
    PlaceUnit { kind: String, at: Cell },
    /// Put a type into production at a Workshop.
    Produce { kind: String },
    Harvest { unit: usize, geyser: usize },
    Capture { unit: usize, priest: usize },
    Sacrifice { unit: usize, altar: usize },
    Read { unit: usize, obelisk: usize },
    Cast { unit: usize },
    Pray { unit: usize },
    Salvage { structure: usize },
    Upgrade { structure: usize },
    /// Battlemaster tools: crack, harden or destroy a bridge cell outright.
    CrackBridge { at: Cell },
    HardenBridge { at: Cell },
    DestroyBridge { at: Cell },
}

/// What a command did, for the status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub message: String,
    /// The unit a `PlaceUnit` created.
    pub unit: Option<usize>,
}

impl World {
    /// The piece queue of a player; every player draws from their own.
    pub fn queue_for(&mut self, owner: u8) -> &mut PieceQueue {
        if owner == 0 {
            return &mut self.queue;
        }
        let cfg = &self.cfg;
        self.queues
            .entry(owner)
            .or_insert_with(|| PieceQueue::from_defs(&cfg.bridges.pieces, cfg.bridges.piece_slots, cfg.bridges.queue_seed.wrapping_add(owner as u64)))
    }

    fn own_unit(&self, owner: u8, unit: usize) -> Result<(), String> {
        match self.units.get(unit) {
            Some(u) if u.owner == owner && u.alive => Ok(()),
            Some(_) => Err(format!("unit {unit} is not yours")),
            None => Err(format!("no unit {unit}")),
        }
    }

    /// Carry out a player's command. Failure leaves the world as it was
    /// and says why; the same command fails the same way on every machine.
    pub fn apply(&mut self, owner: u8, cmd: &Command, scripts: &Scripts) -> Result<Applied, String> {
        let done = |message: String| Ok(Applied { message, unit: None });
        match cmd {
            Command::Move { unit, to } => {
                self.own_unit(owner, *unit)?;
                if self.command_move(*unit, *to) { done(format!("unit {unit} ordered to {to:?}")) } else { Err(format!("{to:?} is unreachable for unit {unit}")) }
            }
            Command::PlacePiece { slot, rotations, at } => {
                let queue = self.queue_for(owner);
                let Some(piece) = queue.slots.get(*slot).cloned() else { return Err(format!("no piece in slot {slot}")) };
                let mut piece = piece;
                for _ in 0..(*rotations % 4) {
                    piece = piece.rotated();
                }
                let cells = piece.cells_at(*at);
                self.place_piece_for(owner, &cells).map_err(|e| format!("cannot place {}: {e}", piece.name))?;
                self.queue_for(owner).refill(*slot);
                done(format!("{} piece placed at {at:?}", piece.name))
            }
            Command::Drop { kind, at } => {
                let rules = self.types.get(kind).cloned().ok_or_else(|| format!("unknown type {kind}"))?;
                self.drop_structure_for(owner, kind, &rules, *at).map_err(|e| format!("cannot drop {kind}: {e}"))?;
                done(format!("{kind} placed; a stream will build it"))
            }
            Command::PlaceUnit { kind, at } => {
                let rules = self.types.get(kind).cloned().ok_or_else(|| format!("unknown type {kind}"))?;
                let i = self.place_unit_for(owner, kind, &rules, *at).map_err(|e| format!("cannot place {kind}: {e}"))?;
                Ok(Applied { message: format!("{kind} placed as unit {i}"), unit: Some(i) })
            }
            Command::Produce { kind } => {
                let rules = self.types.get(kind).cloned().ok_or_else(|| format!("unknown type {kind}"))?;
                let ws = self.put_into_production(owner, kind, &rules, scripts).map_err(|e| format!("{kind}: {e}"))?;
                let s = &self.structures[ws];
                done(format!("{kind} in production at {} ({} of {} slots used)", s.kind, s.production.len(), s.slots))
            }
            Command::Harvest { unit, geyser } => {
                self.own_unit(owner, *unit)?;
                if self.order_harvest(*unit, *geyser) { done(format!("unit {unit} harvesting geyser {geyser}")) } else { Err(format!("unit {unit} cannot harvest geyser {geyser}: needs a Temple or Outpost and a way there")) }
            }
            Command::Capture { unit, priest } => {
                self.own_unit(owner, *unit)?;
                if self.order_capture(*unit, *priest) { done(format!("unit {unit} sent to capture the priest")) } else { Err(format!("unit {unit} cannot capture: needs a Transport and a stunned or floating priest")) }
            }
            Command::Sacrifice { unit, altar } => {
                self.own_unit(owner, *unit)?;
                if self.order_sacrifice(*unit, *altar) { done(format!("unit {unit} carries the priest to the altar")) } else { Err(format!("unit {unit} carries no priest, or that is not your altar")) }
            }
            Command::Read { unit, obelisk } => {
                self.own_unit(owner, *unit)?;
                if self.order_read(*unit, *obelisk) { done(format!("unit {unit} goes to read the Obelisk")) } else { Err(format!("unit {unit} cannot read it: needs a free Transport and an Obelisk with a Spell")) }
            }
            Command::Cast { unit } => {
                self.own_unit(owner, *unit)?;
                let spell = self.units[*unit].spell.clone();
                self.order_cast(*unit).map_err(|e| match spell {
                    None => format!("unit {unit} knows no Spell; read an Obelisk or pray"),
                    Some(_) => format!("unit {unit} cannot cast: {e}"),
                })?;
                done(format!("unit {unit} casts {}", self.units[*unit].spell.as_deref().unwrap_or("?")))
            }
            Command::Pray { unit } => {
                self.own_unit(owner, *unit)?;
                if self.order_pray(*unit) { done(format!("unit {unit} prays")) } else { Err(format!("unit {unit} cannot pray: needs a free High Priest without a Spell")) }
            }
            Command::Salvage { structure } => {
                let kind = self.structures.get(*structure).map(|s| s.kind.clone()).ok_or_else(|| format!("no structure {structure}"))?;
                match self.salvage_for(owner, *structure, scripts) {
                    Some(refund) => done(format!("salvaged {kind} for {refund} Storm Power")),
                    None => Err(format!("cannot salvage {kind}")),
                }
            }
            Command::Upgrade { structure } => {
                let kind = self.structures.get(*structure).map(|s| s.kind.clone()).ok_or_else(|| format!("no structure {structure}"))?;
                let level = self.upgrade_workshop(owner, *structure).map_err(|e| format!("cannot upgrade {kind}: {e}"))?;
                done(format!("{kind} upgraded to level {level}: {} slots", self.structures[*structure].slots))
            }
            Command::CrackBridge { at } => {
                if self.crack_bridge(*at) { done(format!("cracked {at:?}")) } else { Err(format!("nothing to crack at {at:?}")) }
            }
            Command::HardenBridge { at } => {
                if self.harden_bridge(*at) { done(format!("hardened {at:?}")) } else { Err(format!("nothing to harden at {at:?}")) }
            }
            Command::DestroyBridge { at } => {
                let before = self.bridges.len();
                if self.destroy_bridge(*at) { done(format!("destroyed {at:?}; {} bridge cells fell", before.saturating_sub(1 + self.bridges.len()))) } else { Err(format!("no bridge at {at:?}")) }
            }
        }
    }

    /// A digest of the state that matters: two worlds that ran the same
    /// commands must agree on it, and one that drifted will not.
    pub fn hash(&self) -> u64 {
        let mut h = Bytes::new();
        h.u64(self.tick);
        for p in &self.powers {
            h.i32(*p);
        }
        h.u64(self.knowledge as u64);
        for k in &self.known_tech {
            for b in k {
                h.u64(*b as u64);
            }
        }
        for (i, o) in self.island_owners.iter().enumerate() {
            h.u64(i as u64);
            h.u64(o.map(|o| o as u64 + 1).unwrap_or(0));
        }
        for (c, s) in &self.bridges {
            h.cell(*c);
            h.u64(*s as u64);
        }
        for c in self.platforms.cells() {
            h.cell(c);
        }
        for s in &self.structures {
            h.str(&s.kind);
            h.cell(s.cell);
            h.i32(s.hp);
            h.u64(s.owner as u64);
            h.u64(s.building as u64);
            h.u64(s.cooldown as u64);
            h.u64(s.production.len() as u64);
        }
        for u in &self.units {
            h.str(&u.kind);
            h.i32(u.pos.x);
            h.i32(u.pos.y);
            h.i32(u.hp);
            h.u64(u.owner as u64);
            h.u64(u.alive as u64);
            h.u64(u.stunned as u64 | (u.floating as u64) << 1 | (u.is_moving() as u64) << 2);
            h.u64(matches!(u.task, Task::Idle) as u64);
            h.str(u.spell.as_deref().unwrap_or(""));
        }
        xxhash_rust::xxh3::xxh3_64(&h.0)
    }
}

impl World {
    /// The whole world as bytes, for saving, loading and handing to a
    /// client that joins or returns: `restore` gives it back exactly.
    pub fn snapshot(&self) -> Vec<u8> {
        postcard::to_stdvec(self).unwrap_or_default()
    }

    pub fn restore(bytes: &[u8]) -> Result<World, String> {
        postcard::from_bytes(bytes).map_err(|e| format!("snapshot: {e}"))
    }
}

/// The state that matters, laid out as bytes for the hash.
struct Bytes(Vec<u8>);

impl Bytes {
    fn new() -> Bytes {
        Bytes(Vec::with_capacity(4096))
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.u64(v as u32 as u64);
    }
    fn cell(&mut self, c: Cell) {
        self.i32(c.x);
        self.i32(c.y);
    }
    fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.0.extend_from_slice(s.as_bytes());
    }
}

/// A game as the commands that made it: the map and, per tick, who did what.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    pub map: String,
    #[serde(default)]
    pub commands: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub tick: u64,
    pub owner: u8,
    pub command: Command,
    /// Whether the world took the command when it was given; a replay
    /// that decides otherwise has drifted.
    #[serde(default = "yes")]
    pub accepted: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("replay: {0}")]
    Parse(String),
}

impl Replay {
    pub fn parse(text: &str) -> Result<Replay, ReplayError> {
        toml::from_str(text).map_err(|e| ReplayError::Parse(e.to_string()))
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Replay, ReplayError> {
        Replay::parse(&std::fs::read_to_string(path).map_err(ReplayError::Io)?)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ReplayError> {
        std::fs::write(path, self.to_toml()).map_err(ReplayError::Io)
    }

    pub fn record(&mut self, tick: u64, owner: u8, command: Command, accepted: bool) {
        self.commands.push(Entry { tick, owner, command, accepted });
    }

    /// The commands due at `tick`, in the order they were given.
    pub fn due(&self, tick: u64) -> impl Iterator<Item = &Entry> {
        self.commands.iter().filter(move |e| e.tick == tick)
    }

    /// Apply everything due at the world's current tick; returns how many
    /// commands came out differently from the record (a refusal recorded
    /// as such is not a drift).
    pub fn play_tick(&self, world: &mut World, scripts: &Scripts) -> usize {
        let tick = world.tick;
        let mut drifted = 0;
        for e in self.due(tick) {
            if world.apply(e.owner, &e.command, scripts).is_ok() != e.accepted {
                drifted += 1;
            }
        }
        drifted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_config;
    use crate::island::IslandMap;
    use crate::rules::TypeRules;
    use crate::script::test_scripts;
    use islefall_data::isle::Theme;

    fn arena() -> World {
        let mut w = World::new(test_config());
        w.push_island(IslandMap::rect(Cell::new(0, 0), 10, 6), 0);
        w.push_island(IslandMap::rect(Cell::new(20, 0), 6, 6), 1);
        w.register_type("residence", TypeRules { foot_x: 2, foot_y: 2, is_temple: true, produces: Some(Theme::Sun), may_drop_on_rim: true, ..TypeRules::plain() });
        w.register_type("sunwalker", TypeRules { is_unit: true, is_transport: true, max_hit_points: 50, speed: 3.0, ..TypeRules::plain() });
        w.register_type("treetwo", TypeRules { foot_x: 1, foot_y: 1, max_hit_points: 100, cost: 100, build_seconds: 1.0, ..TypeRules::plain() });
        let temple = w.types["residence"].clone();
        w.drop_structure("residence", &temple, Cell::new(2, 2)).unwrap();
        w.powers = vec![2000; 4];
        w.energy_enforced = true;
        w
    }

    fn script() -> Replay {
        let mut r = Replay { map: "arena".into(), commands: Vec::new() };
        r.record(0, 0, Command::PlaceUnit { kind: "sunwalker".into(), at: Cell::new(4, 1) }, true);
        r.record(1, 0, Command::Move { unit: 0, to: Cell::new(8, 4) }, true);
        r.record(2, 0, Command::PlacePiece { slot: 0, rotations: 1, at: Cell::new(10, 2) }, true);
        r.record(3, 0, Command::Drop { kind: "treetwo".into(), at: Cell::new(6, 3) }, true);
        r.record(4, 1, Command::Move { unit: 0, to: Cell::new(1, 1) }, true);
        r.record(5, 0, Command::CrackBridge { at: Cell::new(10, 2) }, true);
        r
    }

    #[test]
    fn the_same_commands_give_the_same_world() {
        let scripts = test_scripts();
        let replay = script();
        let run = |replay: &Replay| {
            let mut w = arena();
            let mut refused = 0;
            for _ in 0..120 {
                refused += replay.play_tick(&mut w, &scripts);
                w.step(&scripts);
            }
            (w.hash(), refused, w)
        };
        let (a, drift_a, wa) = run(&replay);
        let (b, drift_b, _) = run(&replay);
        assert_eq!(a, b, "two runs of one replay agree");
        assert_eq!(drift_a, drift_b);
        assert_eq!(drift_a, 1, "the enemy's order for my unit was refused though recorded as taken");
        assert_eq!(wa.units[0].cell(), Cell::new(8, 4), "the golem walked");
        assert!(wa.structures.iter().any(|s| s.kind == "treetwo" && s.complete()), "the tree was built by the stream");
        let mut other = script();
        other.commands[1].command = Command::Move { unit: 0, to: Cell::new(7, 4) };
        assert_ne!(run(&other).0, a, "a different command, a different world");
        let text = replay.to_toml();
        assert_eq!(Replay::parse(&text).unwrap(), replay, "replays round-trip through TOML");
    }

    #[test]
    fn a_snapshot_restores_the_world_and_carries_on_identically() {
        let scripts = test_scripts();
        let replay = script();
        let mut w = arena();
        for _ in 0..40 {
            replay.play_tick(&mut w, &scripts);
            w.step(&scripts);
        }
        let bytes = w.snapshot();
        let mut back = World::restore(&bytes).unwrap();
        assert_eq!(back.hash(), w.hash(), "restored as it was");
        assert_eq!(back, w);
        for _ in 0..80 {
            replay.play_tick(&mut w, &scripts);
            w.step(&scripts);
            replay.play_tick(&mut back, &scripts);
            back.step(&scripts);
        }
        assert_eq!(back.hash(), w.hash(), "and it goes on the same way");
        assert!(bytes.len() < 64 * 1024, "a small world is a small snapshot ({} bytes)", bytes.len());
    }
}
