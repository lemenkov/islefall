// SPDX-License-Identifier: Apache-2.0
//! The whole simulated state and its fixed-rate tick.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use islefall_data::isle::Theme;

use crate::config::Config;
use crate::grid::Cell;
use crate::island::IslandMap;
use crate::path::find_path_costed;
use crate::pieces::PieceQueue;
use crate::rules::{TypeRules, Walk};
use crate::script::Scripts;
use crate::structure::{Structure, Weapon};
use crate::unit::{Pos, Task, Unit};

/// Condition of a bridge cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BridgeState {
    #[default]
    Normal,
    /// Damaged; collapses when a neighbouring bridge is destroyed.
    Cracked,
    /// Hardened by the Bridge Harden spell; cannot crack.
    Hard,
}

/// What a shot was aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Structure(usize),
    Unit(usize),
}

/// Why a piece could not be placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceError {
    /// A piece cell is not sky.
    NotSky(Cell),
    /// No piece cell touches an island edge or an open bridge end.
    NoAttachment,
    /// The piece touches only ground belonging to someone else.
    NotOwnGround,
}

impl std::fmt::Display for PieceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PieceError::NotSky(c) => write!(f, "{c:?} is not sky"),
            PieceError::NoAttachment => write!(f, "must attach to an island edge or an open bridge end"),
            PieceError::NotOwnGround => write!(f, "may only be built off your own island or your own bridge end"),
        }
    }
}

/// Why a drop was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropError {
    /// A footprint cell is sky and the type cannot make its own island.
    NotGround(Cell),
    /// A `createsisland` type in the sky must sit against an open bridge end.
    NotAtBridgeEnd,
    /// A `createsisland` type is either wholly on land or wholly in the sky.
    Straddling(Cell),
    /// A footprint cell is covered by a structure that refuses drops, or by any structure.
    Occupied(Cell),
    /// A footprint cell is an island rim and the type may not sit on rims.
    OnRim(Cell),
    /// A unit stands on a footprint cell.
    UnitInTheWay(Cell),
    /// The player cannot afford the type.
    NotEnoughPower { cost: i32, have: i32 },
    /// The owner lacks the Knowledge for the type.
    UnknownTech(u8),
    /// A drop must touch the dropper's own ground or open bridge end.
    NotOwnGround,
    /// Not enough Energy of the right kind reaches the site.
    NotEnoughEnergy { need: crate::rules::EnergyNeed, themed: u8, any: u8 },
}

impl std::fmt::Display for DropError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DropError::NotGround(c) => write!(f, "{c:?} is not ground"),
            DropError::NotAtBridgeEnd => write!(f, "must be placed in the sky against an open bridge end"),
            DropError::Straddling(c) => write!(f, "{c:?}: footprint must be all land or all sky"),
            DropError::Occupied(c) => write!(f, "{c:?} is occupied"),
            DropError::OnRim(c) => write!(f, "{c:?} is an island rim"),
            DropError::UnitInTheWay(c) => write!(f, "a unit stands on {c:?}"),
            DropError::NotEnoughPower { cost, have } => write!(f, "costs {cost} Storm Power, have {have}"),
            DropError::UnknownTech(b) => write!(f, "needs Knowledge {b}"),
            DropError::NotOwnGround => write!(f, "must be built on your own island or off your own bridge end"),
            DropError::NotEnoughEnergy { need, themed, any } => write!(
                f,
                "needs {} {:?} and {} any Energy here; {themed} {:?} and {any} other reach the site",
                need.themed, need.theme, need.any, need.theme
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct World {
    /// The rules this world runs under.
    pub cfg: Config,
    pub tick: u64,
    /// Natural islands.
    pub islands: Vec<IslandMap>,
    /// Owner of each natural island, parallel to `islands`.
    pub island_owners: Vec<u8>,
    /// Platforms created under `createsisland` structures dropped on bridges.
    pub platforms: IslandMap,
    /// Owner of each platform cell.
    pub platform_owners: BTreeMap<Cell, u8>,
    /// Incremented whenever islands or platforms change.
    pub terrain_version: u64,
    /// Cells carrying a bridge tile and their condition. Bridges are one cell wide.
    pub bridges: BTreeMap<Cell, BridgeState>,
    /// Who built each bridge cell.
    pub bridge_owners: BTreeMap<Cell, u8>,
    /// Incremented whenever `bridges` changes, so renderers can refresh.
    pub bridge_version: u64,
    pub structures: Vec<Structure>,
    /// Incremented whenever `structures` changes.
    pub structure_version: u64,
    pub units: Vec<Unit>,
    /// Bridge cells that lost their support, with ticks left before they crumble.
    pub doomed: BTreeMap<Cell, u32>,
    /// The pieces on offer.
    pub queue: PieceQueue,
    /// Storm Power reserve per owner (index = owner).
    pub powers: Vec<i32>,
    /// Knowledge bits (`techBit`) each owner holds.
    pub known_tech: Vec<BTreeSet<u8>>,
    /// Shots fired during the last tick, for effects and logging.
    pub last_shots: Vec<(usize, Target)>,
    /// Knowledge granted to the player by sacrifices.
    pub knowledge: u32,
    /// Owners whose sacrifice has not been turned into a tech bit yet.
    pub pending_knowledge: Vec<u8>,
    /// Whether the player's drops need Energy; off while a map is laid out.
    pub energy_enforced: bool,
}

impl World {
    /// A world running under `cfg`.
    pub fn new(cfg: Config) -> World {
        let players = cfg.sim.max_players;
        let queue = PieceQueue::from_defs(&cfg.bridges.pieces, cfg.bridges.piece_slots, cfg.bridges.queue_seed);
        World {
            cfg,
            tick: 0,
            islands: Vec::new(),
            island_owners: Vec::new(),
            platforms: IslandMap::new(),
            platform_owners: BTreeMap::new(),
            terrain_version: 0,
            bridges: BTreeMap::new(),
            bridge_owners: BTreeMap::new(),
            bridge_version: 0,
            structures: Vec::new(),
            structure_version: 0,
            units: Vec::new(),
            doomed: BTreeMap::new(),
            queue,
            powers: vec![0; players],
            known_tech: vec![BTreeSet::new(); players],
            last_shots: Vec::new(),
            knowledge: 0,
            pending_knowledge: Vec::new(),
            energy_enforced: true,
        }
    }

    /// Fixed-point steps per cell.
    pub fn subcell(&self) -> i32 {
        self.cfg.walking.subcell
    }

    fn slot(&self, owner: u8) -> usize {
        owner as usize % self.cfg.sim.max_players
    }

    /// Storm Power of the local player.
    pub fn storm_power(&self) -> i32 {
        self.powers[0]
    }

    /// Owner of the ground at `cell`: a natural island, a platform or a bridge.
    pub fn ground_owner(&self, cell: Cell) -> Option<u8> {
        if let Some(i) = self.islands.iter().position(|i| i.contains(cell)) {
            return Some(self.island_owners.get(i).copied().unwrap_or(0));
        }
        if let Some(&o) = self.platform_owners.get(&cell) {
            return Some(o);
        }
        self.bridge_owners.get(&cell).copied()
    }

    /// Add a natural island with its owner.
    pub fn push_island(&mut self, island: IslandMap, owner: u8) {
        self.islands.push(island);
        self.island_owners.push(owner);
        self.terrain_version += 1;
    }

    /// Whether an island or a platform covers `cell`.
    pub fn is_land(&self, cell: Cell) -> bool {
        self.platforms.contains(cell) || self.islands.iter().any(|i| i.contains(cell))
    }

    /// Whether `cell` is a natural island's edge cell (not a platform).
    pub fn is_rim(&self, cell: Cell) -> bool {
        self.islands.iter().any(|i| i.contains(cell) && i.piece_at(cell) != Some(islefall_data::isle::Piece::Filled))
    }

    pub fn is_bridge(&self, cell: Cell) -> bool {
        self.bridges.contains_key(&cell)
    }

    pub fn bridge_state(&self, cell: Cell) -> Option<BridgeState> {
        self.bridges.get(&cell).copied()
    }

    /// A bridge cell with exactly one connection: a torn end that pieces may attach to.
    pub fn is_open_end(&self, cell: Cell) -> bool {
        self.is_bridge(cell) && self.bridge_connections(cell).count_ones() == 1
    }

    /// Ground of any kind: island, platform or bridge.
    pub fn is_ground(&self, cell: Cell) -> bool {
        self.is_land(cell) || self.is_bridge(cell)
    }

    pub fn structure_at(&self, cell: Cell) -> Option<&Structure> {
        self.structures.iter().find(|s| s.covers(cell))
    }

    /// Cost multiplier for entering `cell`, `None` if it cannot be entered.
    pub fn walk_cost(&self, cell: Cell) -> Option<u32> {
        if !self.is_ground(cell) {
            return None;
        }
        let mut cost = 1;
        for s in self.structures.iter().filter(|s| s.covers(cell)) {
            match s.walk {
                Walk::Blocked => return None,
                Walk::Avoid => cost = cost.max(self.cfg.walking.avoid_cost),
                Walk::Free => {}
            }
        }
        Some(cost)
    }

    pub fn is_walkable(&self, cell: Cell) -> bool {
        self.walk_cost(cell).is_some()
    }

    /// A bridge cell may be placed on empty sky next to existing ground.
    /// Diagonal neighbours do not count: bridges connect orthogonally.
    pub fn can_place_bridge(&self, cell: Cell) -> bool {
        !self.is_ground(cell) && [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| self.is_ground(cell.offset(dx, dy)))
    }

    pub fn place_bridge(&mut self, cell: Cell) -> bool {
        if !self.can_place_bridge(cell) {
            return false;
        }
        self.bridges.insert(cell, BridgeState::Normal);
        self.bridge_owners.insert(cell, 0);
        self.bridge_version += 1;
        true
    }

    /// Check a whole piece: every cell must be sky, and at least one cell must
    /// touch island land or an open bridge end (the tutorial's rule).
    pub fn can_place_piece(&self, cells: &[Cell]) -> Result<(), PieceError> {
        for &c in cells {
            if self.is_ground(c) {
                return Err(PieceError::NotSky(c));
            }
        }
        let attached = cells.iter().any(|&c| {
            [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| {
                let n = c.offset(dx, dy);
                self.is_land(n) || self.is_open_end(n)
            })
        });
        if attached { Ok(()) } else { Err(PieceError::NoAttachment) }
    }

    pub fn place_piece(&mut self, cells: &[Cell]) -> Result<(), PieceError> {
        self.place_piece_for(0, cells)
    }

    /// Bridge cells placed by the map, owned by nobody in particular.
    pub fn place_map_bridge(&mut self, owner: u8, cell: Cell) -> bool {
        if !self.place_bridge(cell) {
            return false;
        }
        self.bridge_owners.insert(cell, owner);
        true
    }

    /// The tutorial's rule plus ownership: a piece must touch the owner's own
    /// island edge or open bridge end; it may also touch other players' ground.
    pub fn can_place_piece_for(&self, owner: u8, cells: &[Cell]) -> Result<(), PieceError> {
        self.can_place_piece(cells)?;
        let own = cells.iter().any(|&c| {
            [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| {
                let n = c.offset(dx, dy);
                (self.is_land(n) || self.is_open_end(n)) && self.ground_owner(n) == Some(owner)
            })
        });
        if own { Ok(()) } else { Err(PieceError::NotOwnGround) }
    }

    pub fn place_piece_for(&mut self, owner: u8, cells: &[Cell]) -> Result<(), PieceError> {
        self.can_place_piece_for(owner, cells)?;
        for &c in cells {
            self.bridges.insert(c, BridgeState::Normal);
            self.bridge_owners.insert(c, owner);
        }
        self.bridge_version += 1;
        Ok(())
    }

    /// Crack a bridge cell; hardened cells resist.
    pub fn crack_bridge(&mut self, cell: Cell) -> bool {
        match self.bridges.get_mut(&cell) {
            Some(s @ BridgeState::Normal) => {
                *s = BridgeState::Cracked;
                self.bridge_version += 1;
                true
            }
            _ => false,
        }
    }

    pub fn harden_bridge(&mut self, cell: Cell) -> bool {
        match self.bridges.get_mut(&cell) {
            Some(s) if *s != BridgeState::Hard => {
                *s = BridgeState::Hard;
                self.bridge_version += 1;
                true
            }
            _ => false,
        }
    }

    /// Destroy a bridge cell. The shock collapses adjacent cracked cells,
    /// recursively, and then everything left without a connection to a
    /// natural island falls away.
    pub fn destroy_bridge(&mut self, cell: Cell) -> bool {
        if self.bridges.remove(&cell).is_none() {
            return false;
        }
        let mut shock = VecDeque::from([cell]);
        while let Some(c) = shock.pop_front() {
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let n = c.offset(dx, dy);
                if self.bridges.get(&n) == Some(&BridgeState::Cracked) {
                    self.bridges.remove(&n);
                    shock.push_back(n);
                }
            }
        }
        self.prune_bridge_owners();
        self.bridge_version += 1;
        self.settle();
        true
    }

    fn touches_land_static(islands: &[IslandMap], platforms: &IslandMap, c: Cell) -> bool {
        [(0, -1), (1, 0), (0, 1), (-1, 0)]
            .iter()
            .any(|&(dx, dy)| platforms.contains(c.offset(dx, dy)) || islands.iter().any(|i| i.contains(c.offset(dx, dy))))
    }

    /// Cells of ground connected, through ground, to a natural island.
    fn supported(&self) -> BTreeSet<Cell> {
        let mut reached = BTreeSet::new();
        let mut frontier: Vec<Cell> = self.islands.iter().flat_map(|i| i.cells()).collect();
        reached.extend(frontier.iter().copied());
        while let Some(c) = frontier.pop() {
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let n = c.offset(dx, dy);
                if self.is_ground(n) && reached.insert(n) {
                    frontier.push(n);
                }
            }
        }
        reached
    }

    /// Find bridge cells with no connection to a natural island: they crack
    /// and start crumbling. Platforms with no support fall at once, with the
    /// structures and units on them. Returns how many cells fell now.
    pub fn settle(&mut self) -> usize {
        let reached = self.supported();
        let mut changed = false;
        let crumble = self.cfg.ticks(self.cfg.bridges.crumble_seconds);
        for (c, state) in self.bridges.iter_mut() {
            if !reached.contains(c) && !self.doomed.contains_key(c) {
                self.doomed.insert(*c, crumble);
                if *state != BridgeState::Hard {
                    *state = BridgeState::Cracked;
                }
                changed = true;
            }
        }
        if changed {
            self.bridge_version += 1;
        }
        let falling: Vec<Cell> = self.platforms.cells().filter(|c| !reached.contains(c)).collect();
        if falling.is_empty() {
            return 0;
        }
        for c in &falling {
            self.platforms.remove(*c);
            self.platform_owners.remove(c);
        }
        self.terrain_version += 1;
        self.remove_unsupported_things(&reached);
        falling.len()
    }

    /// Forget owners of cells that no longer carry a bridge.
    fn prune_bridge_owners(&mut self) {
        let bridges = &self.bridges;
        self.bridge_owners.retain(|c, _| bridges.contains_key(c));
    }

    fn remove_unsupported_things(&mut self, reached: &BTreeSet<Cell>) {
        let before = self.structures.len();
        self.structures.retain(|s| s.cells().all(|c| reached.contains(&c)));
        if self.structures.len() != before {
            self.structure_version += 1;
        }
        for u in &mut self.units {
            if u.alive && !reached.contains(&u.cell()) {
                u.alive = false;
                u.path.clear();
            }
        }
    }

    /// Advance crumbling: doomed cells count down and fall, taking units with them.
    fn crumble(&mut self) {
        let mut fell = Vec::new();
        for (c, t) in self.doomed.iter_mut() {
            *t = t.saturating_sub(1);
            if *t == 0 {
                fell.push(*c);
            }
        }
        if fell.is_empty() {
            return;
        }
        for c in &fell {
            self.doomed.remove(c);
            self.bridges.remove(c);
        }
        self.prune_bridge_owners();
        self.bridge_version += 1;
        let reached = self.supported();
        self.remove_unsupported_things(&reached);
        self.settle();
    }

    /// Connection mask of a bridge cell (north 1, east 2, south 4, west 8):
    /// a side connects when the neighbour is bridge or island.
    pub fn bridge_connections(&self, cell: Cell) -> u8 {
        let mut m = 0;
        for (bit, dx, dy) in [(1u8, 0, -1), (2, 1, 0), (4, 0, 1), (8, -1, 0)] {
            if self.is_ground(cell.offset(dx, dy)) {
                m |= bit;
            }
        }
        m
    }

    /// Check whether a structure of `rules` may be dropped with its hotspot at `cell`.
    ///
    /// Buildings need island ground under every cell. A `createsisland` type
    /// may instead be placed wholly in the sky against an open bridge end,
    /// where it will make its own island (the manual's "just off the end of
    /// a bridge piece"). Nothing is ever dropped onto bridge cells.
    pub fn can_drop(&self, rules: &TypeRules, cell: Cell) -> Result<(), DropError> {
        let probe = Structure::new("", cell, rules.foot_x, rules.foot_y, Walk::Free);
        let cells: Vec<Cell> = probe.cells().collect();
        let on_land = cells.iter().filter(|&&c| self.is_land(c)).count();
        if rules.creates_island && on_land == 0 {
            for &c in &cells {
                if self.is_bridge(c) {
                    return Err(DropError::NotGround(c));
                }
            }
            let touches_end = cells.iter().any(|&c| {
                [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| self.is_open_end(c.offset(dx, dy)))
            });
            if !touches_end {
                return Err(DropError::NotAtBridgeEnd);
            }
        } else if rules.creates_island && on_land != cells.len() {
            let c = cells.iter().find(|&&c| !self.is_land(c)).copied().unwrap_or(cell);
            return Err(DropError::Straddling(c));
        }
        for &c in &cells {
            if !rules.creates_island && !self.is_land(c) {
                return Err(DropError::NotGround(c));
            }
            if self.structure_at(c).is_some() {
                return Err(DropError::Occupied(c));
            }
            if !rules.may_drop_on_rim && self.is_rim(c) {
                return Err(DropError::OnRim(c));
            }
            if self.units.iter().any(|u| u.alive && u.cell() == c) {
                return Err(DropError::UnitInTheWay(c));
            }
        }
        Ok(())
    }

    /// Drop a structure of type `kind`. Sky under a `createsisland` type
    /// becomes platform ground.
    pub fn drop_structure(&mut self, kind: &str, rules: &TypeRules, cell: Cell) -> Result<usize, DropError> {
        self.drop_structure_for(0, kind, rules, cell)
    }

    /// Energy sources of `owner` whose circle covers `cell`, counted by theme.
    /// Returns `(themed matches of `theme`, all sources)`.
    pub fn energy_at(&self, owner: u8, cell: Cell, theme: Theme) -> (u8, u8) {
        let g = &self.cfg.grid;
        let (cx, cy) = cell.centre_px(g);
        let r = self.cfg.energy.range_px;
        let mut themed = 0u8;
        let mut all = 0u8;
        for s in &self.structures {
            let Some(t) = s.produces else { continue };
            if s.owner != owner {
                continue;
            }
            let (sx, sy) = s.centre().centre_px(g);
            let (dx, dy) = (sx - cx, sy - cy);
            if dx * dx + dy * dy <= r * r {
                all = all.saturating_add(1);
                if t == theme {
                    themed = themed.saturating_add(1);
                }
            }
        }
        (themed, all)
    }

    /// Whether `owner` has the Energy to place `rules` with its hotspot at `cell`.
    pub fn check_energy(&self, owner: u8, rules: &TypeRules, cell: Cell) -> Result<(), DropError> {
        let Some(need) = rules.energy else { return Ok(()) };
        let probe = Structure::new("", cell, rules.foot_x, rules.foot_y, Walk::Free);
        let (themed, all) = self.energy_at(owner, probe.centre(), need.theme);
        let themed_ok = need.theme == Theme::Sun || themed >= need.themed;
        if themed_ok && all >= need.total() {
            Ok(())
        } else {
            Err(DropError::NotEnoughEnergy { need, themed, any: all.saturating_sub(themed.min(need.themed)) })
        }
    }

    /// Ownership for drops: on land, every footprint cell must be the owner's;
    /// in the sky, the touched open end must be the owner's.
    pub fn check_ownership(&self, owner: u8, rules: &TypeRules, cell: Cell) -> Result<(), DropError> {
        let probe = Structure::new("", cell, rules.foot_x, rules.foot_y, Walk::Free);
        let cells: Vec<Cell> = probe.cells().collect();
        if cells.iter().any(|&c| self.is_land(c)) {
            if cells.iter().all(|&c| self.ground_owner(c) == Some(owner)) { Ok(()) } else { Err(DropError::NotOwnGround) }
        } else {
            let own_end = cells.iter().any(|&c| {
                [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| {
                    let n = c.offset(dx, dy);
                    self.is_open_end(n) && self.bridge_owners.get(&n) == Some(&owner)
                })
            });
            if own_end { Ok(()) } else { Err(DropError::NotOwnGround) }
        }
    }

    /// Drop for a given owner. Geysers and map layout aside, the owner pays
    /// the cost, needs the Knowledge, and (for the local player) the Energy.
    pub fn drop_structure_for(&mut self, owner: u8, kind: &str, rules: &TypeRules, cell: Cell) -> Result<usize, DropError> {
        self.can_drop(rules, cell)?;
        if self.energy_enforced {
            self.check_ownership(owner, rules, cell)?;
            if let Some(bit) = rules.tech_bit {
                if !self.known_tech[self.slot(owner)].contains(&bit) {
                    return Err(DropError::UnknownTech(bit));
                }
            }
            if owner == 0 {
                self.check_energy(owner, rules, cell)?;
            }
        }
        // Geysers are placed by the map, not bought; everything else costs its type's price.
        let slot = self.slot(owner);
        let price = if rules.is_geyser { 0 } else { rules.cost };
        if price > self.powers[slot] {
            return Err(DropError::NotEnoughPower { cost: price, have: self.powers[slot] });
        }
        self.powers[slot] -= price;
        let mut s = Structure::new(kind, cell, rules.foot_x, rules.foot_y, rules.walk);
        s.drop_blocking = rules.drop_blocking;
        s.stock = if rules.is_geyser && self.cfg.economy.geyser_stock_from_cost { rules.cost } else { 0 };
        s.is_temple = rules.is_temple;
        s.owner = owner;
        s.hp = rules.max_hit_points;
        s.max_hp = rules.max_hit_points;
        s.cost = rules.cost;
        s.threat = rules.threat;
        s.is_altar = rules.is_altar;
        s.produces = rules.produces;
        if rules.range > 0 && rules.hp_per_sec > 0 {
            s.weapon = Some(Weapon {
                range: rules.range,
                damage: rules.damage_per_shot,
                delay: self.cfg.ticks(rules.delay_between_shots),
                cardinal_only: rules.cardinal_only,
            });
        }
        if rules.creates_island {
            let mut changed = false;
            for c in s.cells() {
                if !self.is_land(c) {
                    self.platforms.insert(c);
                    self.platform_owners.insert(c, owner);
                    changed = true;
                }
            }
            if changed {
                self.terrain_version += 1;
                // The new island now supports whatever touches it.
                self.doomed.retain(|c, _| !Self::touches_land_static(&self.islands, &self.platforms, *c));
            }
        }
        self.structures.push(s);
        self.structure_version += 1;
        Ok(self.structures.len() - 1)
    }

    /// Add a unit of type `kind` at `cell`. Returns its index.
    pub fn spawn_unit(&mut self, kind: &str, rules: &TypeRules, cell: Cell) -> Option<usize> {
        self.spawn_unit_for(0, kind, rules, cell)
    }

    pub fn spawn_unit_for(&mut self, owner: u8, kind: &str, rules: &TypeRules, cell: Cell) -> Option<usize> {
        if !self.is_walkable(cell) {
            return None;
        }
        let mut u = Unit::new(kind, cell, speed_per_tick(&self.cfg, rules.speed), self.subcell());
        u.owner = owner;
        u.hp = rules.max_hit_points.max(1);
        u.max_hp = u.hp;
        u.threat = rules.threat;
        u.is_priest = rules.is_priest;
        u.is_transport = rules.is_transport;
        self.units.push(u);
        Some(self.units.len() - 1)
    }

    /// Grant a Knowledge bit to an owner.
    pub fn grant_tech(&mut self, owner: u8, bit: u8) -> bool {
        let slot = self.slot(owner);
        self.known_tech[slot].insert(bit)
    }

    pub fn knows_tech(&self, owner: u8, bit: u8) -> bool {
        self.known_tech[self.slot(owner)].contains(&bit)
    }

    /// Order a Transport to capture a stunned enemy priest.
    pub fn order_capture(&mut self, unit: usize, priest: usize) -> bool {
        let (Some(u), Some(p)) = (self.units.get(unit), self.units.get(priest)) else { return false };
        if !u.alive || !u.is_transport || u.carrying.is_some() || u.stunned {
            return false;
        }
        if !p.alive || !p.is_priest || !p.stunned || p.owner == u.owner || p.carried_by.is_some() {
            return false;
        }
        let target = p.cell();
        if !self.walk_next_to(unit, target) {
            return false;
        }
        self.units[unit].task = Task::Capture { priest };
        true
    }

    /// Order a Transport carrying a priest to sacrifice him on the altar at `altar`.
    pub fn order_sacrifice(&mut self, unit: usize, altar: usize) -> bool {
        let (Some(u), Some(a)) = (self.units.get(unit), self.structures.get(altar)) else { return false };
        if !u.alive || u.carrying.is_none() || !a.is_altar || a.owner != u.owner {
            return false;
        }
        let centre = a.centre();
        if !self.order_move(unit, centre) {
            return false;
        }
        self.units[unit].task = Task::Sacrifice { altar };
        true
    }

    /// Path a unit onto `target` or, failing that, to a walkable cell next to it.
    fn walk_next_to(&mut self, unit: usize, target: Cell) -> bool {
        let Some(from) = self.units.get(unit).map(|u| u.cell()) else { return false };
        if from == target || [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| from.offset(dx, dy) == target) {
            self.units[unit].path.clear();
            return true;
        }
        let mut targets: Vec<Cell> = [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().map(|&(dx, dy)| target.offset(dx, dy)).filter(|&c| self.is_walkable(c)).collect();
        targets.sort_by_key(|c| (c.x - from.x).abs() + (c.y - from.y).abs());
        for t in targets {
            if self.order_move(unit, t) {
                return true;
            }
        }
        false
    }

    /// Stun and regeneration for priests, and carried units following their carrier.
    fn run_priests(&mut self) {
        let temples: Vec<(u8, Cell)> = self.structures.iter().filter(|s| s.is_temple).map(|s| (s.owner, s.centre())).collect();
        let heal_range = self.cfg.priest.temple_heal_range;
        let heal_ticks = (self.cfg.sim.tick_hz / self.cfg.priest.heal_per_second.max(1)).max(1) as u64;
        for i in 0..self.units.len() {
            let u = &self.units[i];
            if !u.alive {
                continue;
            }
            if let Some(c) = u.carried_by {
                let pos = self.units[c].pos;
                self.units[i].pos = pos;
                continue;
            }
            if !u.is_priest {
                continue;
            }
            let at = u.cell();
            let near_temple = temples.iter().any(|&(o, c)| o == u.owner && (c.x - at.x).abs() <= heal_range && (c.y - at.y).abs() <= heal_range);
            if near_temple && u.hp < u.max_hp && self.tick % heal_ticks == 0 {
                self.units[i].hp += 1;
            }
            let u = &mut self.units[i];
            u.stunned = u.hp * 2 <= u.max_hp;
            if u.stunned {
                u.path.clear();
                u.task = Task::Idle;
            }
        }
    }

    /// Salvage one of the player's structures: part of its cost comes back,
    /// scaled by its remaining health, and nearby bridges take the shock of
    /// its removal but nothing else is damaged.
    pub fn salvage(&mut self, index: usize, scripts: &Scripts) -> Option<i32> {
        self.salvage_for(0, index, scripts)
    }

    pub fn salvage_for(&mut self, owner: u8, index: usize, scripts: &Scripts) -> Option<i32> {
        let s = self.structures.get(index)?;
        if s.owner != owner {
            return None;
        }
        let refund = scripts
            .salvage_refund(s.cost as i64, s.hp.max(0) as i64, s.max_hp as i64, self.cfg.economy.salvage_refund_percent as i64)
            .unwrap_or(0);
        let slot = self.slot(owner);
        self.powers[slot] += refund;
        let centre = s.centre();
        self.remove_structure(index);
        self.shock_bridges(centre);
        self.settle();
        Some(refund)
    }

    fn remove_structure(&mut self, index: usize) {
        self.structures.remove(index);
        self.structure_version += 1;
        for u in &mut self.units {
            if let Task::Harvest { geyser, temple, .. } = u.task {
                if geyser == index || temple == index {
                    u.task = Task::Idle;
                } else if geyser > index || temple > index {
                    u.task = Task::Harvest {
                        geyser: if geyser > index { geyser - 1 } else { geyser },
                        temple: if temple > index { temple - 1 } else { temple },
                        carrying: match u.task {
                            Task::Harvest { carrying, .. } => carrying,
                            _ => 0,
                        },
                        work: 0,
                    };
                }
            }
        }
    }

    /// Bridges near an explosion: cracked cells are destroyed, others crack.
    fn shock_bridges(&mut self, centre: Cell) {
        let mut destroyed = Vec::new();
        let mut changed = false;
        let radius = self.cfg.combat.explosion_radius;
        for (c, state) in self.bridges.iter_mut() {
            if (c.x - centre.x).abs() > radius || (c.y - centre.y).abs() > radius {
                continue;
            }
            match *state {
                BridgeState::Cracked => destroyed.push(*c),
                BridgeState::Normal => {
                    *state = BridgeState::Cracked;
                    changed = true;
                }
                BridgeState::Hard => {}
            }
        }
        for c in destroyed {
            self.bridges.remove(&c);
            changed = true;
        }
        if changed {
            self.prune_bridge_owners();
            self.bridge_version += 1;
        }
    }

    /// A structure is destroyed: it explodes, damaging everything nearby
    /// (which may chain) and shocking bridges, then unsupported ground falls.
    pub fn destroy_structure(&mut self, index: usize) {
        if index >= self.structures.len() {
            return;
        }
        let centre = self.structures[index].centre();
        let (radius, damage) = (self.cfg.combat.explosion_radius, self.cfg.combat.explosion_damage);
        self.remove_structure(index);
        self.shock_bridges(centre);
        let near = |c: Cell| (c.x - centre.x).abs() <= radius && (c.y - centre.y).abs() <= radius;
        let victims: Vec<usize> = (0..self.structures.len()).filter(|&i| self.structures[i].distance_to(centre) <= radius).collect();
        for i in victims.into_iter().rev() {
            self.damage_structure(i, damage);
        }
        for i in 0..self.units.len() {
            if self.units[i].alive && near(self.units[i].cell()) {
                self.damage_unit(i, damage);
            }
        }
        self.settle();
    }

    pub fn damage_structure(&mut self, index: usize, amount: i32) {
        let Some(s) = self.structures.get_mut(index) else { return };
        if s.max_hp == 0 {
            return;
        }
        s.hp -= amount;
        if s.hp <= 0 {
            self.destroy_structure(index);
        }
    }

    pub fn damage_unit(&mut self, index: usize, amount: i32) {
        let Some(u) = self.units.get_mut(index) else { return };
        if !u.alive {
            return;
        }
        u.hp -= amount;
        if u.hp <= 0 {
            u.alive = false;
            u.path.clear();
            u.task = Task::Idle;
            if let Some(p) = u.carrying.take() {
                if let Some(priest) = self.units.get_mut(p) {
                    priest.carried_by = None;
                }
            }
        }
    }

    /// Every armed structure picks the highest-threat enemy in range and
    /// fires when its cooldown allows. Cannons only fire along their row or column.
    fn run_combat(&mut self, scripts: &Scripts) {
        let mut shots: Vec<(usize, Target)> = Vec::new();
        for (i, s) in self.structures.iter().enumerate() {
            let Some(w) = s.weapon else { continue };
            if s.cooldown > 0 {
                continue;
            }
            let in_range = |c: Cell| s.distance_to(c) <= w.range && (!w.cardinal_only || s.in_line(c));
            let mut best: Option<(i64, Target)> = None;
            for (j, t) in self.structures.iter().enumerate() {
                if t.owner == s.owner || t.max_hp == 0 || !in_range(t.centre()) {
                    continue;
                }
                let key = scripts.target_priority(t.threat as i64, s.distance_to(t.centre()) as i64, false).unwrap_or(0);
                if best.as_ref().is_none_or(|b| b.0 < key) {
                    best = Some((key, Target::Structure(j)));
                }
            }
            for (j, u) in self.units.iter().enumerate() {
                if !u.alive || u.owner == s.owner || u.carried_by.is_some() || !in_range(u.cell()) {
                    continue;
                }
                let key = scripts.target_priority(u.threat as i64, s.distance_to(u.cell()) as i64, true).unwrap_or(0);
                if best.as_ref().is_none_or(|b| b.0 < key) {
                    best = Some((key, Target::Unit(j)));
                }
            }
            if let Some((_, target)) = best {
                shots.push((i, target));
            }
        }
        // Resolve in reverse index order so removals do not shift pending shooters.
        for (i, target) in shots.into_iter().rev() {
            let Some(w) = self.structures.get(i).and_then(|s| s.weapon) else { continue };
            self.structures[i].cooldown = w.delay;
            self.last_shots.push((i, target));
            match target {
                Target::Structure(j) => self.damage_structure(j, w.damage),
                Target::Unit(j) => self.damage_unit(j, w.damage),
            }
        }
        for s in &mut self.structures {
            s.cooldown = s.cooldown.saturating_sub(1);
        }
    }

    /// Order a unit to harvest from the geyser at `geyser` and deliver to the
    /// nearest temple. Fails if there is no temple, the geyser is empty, or the
    /// geyser cannot be reached.
    pub fn order_harvest(&mut self, unit: usize, geyser: usize) -> bool {
        let Some(g) = self.structures.get(geyser) else { return false };
        if g.stock <= 0 {
            return false;
        }
        let Some(u) = self.units.get(unit).filter(|u| u.alive) else { return false };
        let from = u.cell();
        let owner = u.owner;
        let Some(temple) = self
            .structures
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_temple && s.owner == owner)
            .min_by_key(|(_, s)| (s.cell.x - from.x).abs() + (s.cell.y - from.y).abs())
            .map(|(i, _)| i)
        else {
            return false;
        };
        if !self.walk_to_adjacent(unit, geyser) {
            return false;
        }
        self.units[unit].task = Task::Harvest { geyser, temple, carrying: 0, work: 0 };
        true
    }

    /// Path a unit to the nearest reachable cell next to a structure.
    fn walk_to_adjacent(&mut self, unit: usize, structure: usize) -> bool {
        let Some(from) = self.units.get(unit).map(|u| u.cell()) else { return false };
        let Some(s) = self.structures.get(structure) else { return false };
        if s.is_adjacent(from) {
            self.units[unit].path.clear();
            return true;
        }
        let mut targets: Vec<Cell> = s.adjacent_cells().into_iter().filter(|&c| self.is_walkable(c)).collect();
        targets.sort_by_key(|c| (c.x - from.x).abs() + (c.y - from.y).abs());
        for t in targets {
            if self.order_move(unit, t) {
                return true;
            }
        }
        false
    }

    /// Advance standing tasks for units that are not walking.
    fn run_tasks(&mut self) {
        for i in 0..self.units.len() {
            let u = &self.units[i];
            if !u.alive || u.is_moving() {
                continue;
            }
            let at = u.cell();
            match u.task {
                Task::Capture { priest } => {
                    let Some(p) = self.units.get(priest) else {
                        self.units[i].task = Task::Idle;
                        continue;
                    };
                    if !p.alive || !p.stunned || p.carried_by.is_some() {
                        self.units[i].task = Task::Idle;
                        continue;
                    }
                    let pc = p.cell();
                    let adjacent = pc == at || [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| at.offset(dx, dy) == pc);
                    if adjacent {
                        self.units[priest].carried_by = Some(i);
                        self.units[priest].task = Task::Idle;
                        self.units[i].carrying = Some(priest);
                        self.units[i].task = Task::Idle;
                    } else if !self.walk_next_to(i, pc) {
                        self.units[i].task = Task::Idle;
                    }
                    continue;
                }
                Task::Sacrifice { altar } => {
                    let owner = u.owner;
                    let (Some(centre), Some(p)) = (self.structures.get(altar).map(|a| a.centre()), u.carrying) else {
                        self.units[i].task = Task::Idle;
                        continue;
                    };
                    if centre == at {
                        self.units[p].alive = false;
                        self.units[p].carried_by = None;
                        self.units[i].carrying = None;
                        self.units[i].task = Task::Idle;
                        if owner == 0 {
                            self.knowledge += 1;
                        }
                        self.pending_knowledge.push(owner);
                    } else if !self.order_move(i, centre) {
                        self.units[i].task = Task::Idle;
                    }
                    continue;
                }
                _ => {}
            }
            let Task::Harvest { geyser, temple, carrying, work } = u.task else { continue };
            let (Some(g), Some(t)) = (self.structures.get(geyser), self.structures.get(temple)) else {
                self.units[i].task = Task::Idle;
                continue;
            };
            if carrying == 0 {
                if g.stock <= 0 {
                    self.units[i].task = Task::Idle;
                } else if g.is_adjacent(at) {
                    if work + 1 < self.cfg.ticks(self.cfg.economy.work_seconds) {
                        self.units[i].task = Task::Harvest { geyser, temple, carrying, work: work + 1 };
                    } else {
                        let take = self.cfg.economy.nugget_power.min(g.stock);
                        self.structures[geyser].stock -= take;
                        if self.structures[geyser].stock == 0 {
                            self.structures[geyser].kind = "emptygeyser".into();
                            self.structure_version += 1;
                        }
                        self.units[i].task = Task::Harvest { geyser, temple, carrying: take, work: 0 };
                        if !self.walk_to_adjacent(i, temple) {
                            self.units[i].task = Task::Idle;
                        }
                    }
                } else if !self.walk_to_adjacent(i, geyser) {
                    self.units[i].task = Task::Idle;
                }
            } else if t.is_adjacent(at) {
                if work + 1 < self.cfg.ticks(self.cfg.economy.work_seconds) {
                    self.units[i].task = Task::Harvest { geyser, temple, carrying, work: work + 1 };
                } else {
                    let slot = self.slot(self.units[i].owner);
                    self.powers[slot] += carrying;
                    let more = self.structures[geyser].stock > 0;
                    self.units[i].task = if more { Task::Harvest { geyser, temple, carrying: 0, work: 0 } } else { Task::Idle };
                    if more && !self.walk_to_adjacent(i, geyser) {
                        self.units[i].task = Task::Idle;
                    }
                }
            } else if !self.walk_to_adjacent(i, temple) {
                self.units[i].task = Task::Idle;
            }
        }
    }

    /// Order a unit to walk to `cell` along the cheapest walkable path.
    /// Returns `false` if the cell is unreachable.
    pub fn order_move(&mut self, unit: usize, cell: Cell) -> bool {
        let Some(from) = self.units.get(unit).filter(|u| u.alive && !u.stunned && u.carried_by.is_none()).map(|u| u.cell()) else { return false };
        let Some(path) = find_path_costed(from, cell, |c| self.walk_cost(c)) else { return false };
        let sub = self.subcell();
        let u = &mut self.units[unit];
        u.path = path.into_iter().map(|c| Pos::cell_centre(c, sub)).collect();
        if u.path.is_empty() {
            // Already in the cell: walk to its centre so the unit settles.
            u.path.push_back(Pos::cell_centre(cell, sub));
        }
        true
    }

    /// A player's move order: cancels any standing task first.
    pub fn command_move(&mut self, unit: usize, cell: Cell) -> bool {
        if let Some(u) = self.units.get_mut(unit) {
            u.task = Task::Idle;
        }
        self.order_move(unit, cell)
    }

    /// Advance the simulation by one tick.
    pub fn step(&mut self, scripts: &Scripts) {
        for u in &mut self.units {
            u.step();
        }
        self.run_priests();
        self.run_tasks();
        self.last_shots.clear();
        self.run_combat(scripts);
        self.crumble();
        self.tick += 1;
    }
}

/// Convert a type's `speed` property (cells per second) to fixed-point steps per tick.
pub fn speed_per_tick(cfg: &Config, cells_per_second: f64) -> i32 {
    ((cells_per_second * cfg.walking.subcell as f64) / cfg.sim.tick_hz as f64).round().max(1.0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_config;
    use crate::script::test_scripts;

    fn world() -> World {
        let mut w = World::new(test_config());
        w.islands.push(IslandMap::rect(Cell::new(0, 0), 8, 4));
        let speed = speed_per_tick(&w.cfg, 1.0);
        let sub = w.subcell();
        w.units.push(Unit::new("priest", Cell::new(0, 1), speed, sub));
        w
    }

    const CRUMBLE_TICKS: u32 = 120;
    const NUGGET_POWER: i32 = 200;
    const HEAL_TICKS: u32 = 15;
    const PIECE_SLOTS: usize = 4;

    fn rules(foot: i32) -> TypeRules {
        TypeRules { foot_x: foot, foot_y: foot, ..TypeRules::plain() }
    }

    #[test]
    fn orders_only_onto_land() {
        let mut w = world();
        assert!(!w.order_move(0, Cell::new(9, 9)));
        assert!(w.order_move(0, Cell::new(7, 3)));
        for _ in 0..400 {
            w.step(&test_scripts());
        }
        assert_eq!(w.units[0].cell(), Cell::new(7, 3));
        assert!(!w.units[0].is_moving());
        assert_eq!(w.tick, 400);
    }

    #[test]
    fn walks_around_a_blocking_structure() {
        let mut w = world();
        // A 1x4 wall at x=4 covering the whole island height.
        w.structures.push(Structure::new("wall", Cell::new(4, 3), 1, 4, Walk::Blocked));
        assert!(!w.is_walkable(Cell::new(4, 2)));
        assert!(!w.order_move(0, Cell::new(7, 1)), "wall seals the island");
        w.structures[0].foot_y = 3; // open a gap at y=0
        assert!(w.order_move(0, Cell::new(7, 1)));
        let mut visited = Vec::new();
        for _ in 0..600 {
            w.step(&test_scripts());
            visited.push(w.units[0].cell());
            if !w.units[0].is_moving() {
                break;
            }
        }
        assert_eq!(w.units[0].cell(), Cell::new(7, 1));
        assert!(visited.iter().all(|&c| w.is_walkable(c)), "never stood on the wall");
        assert!(visited.contains(&Cell::new(4, 0)), "went through the gap");
    }

    #[test]
    fn avoided_structures_are_passable_but_costly() {
        let mut w = world();
        w.structures.push(Structure::new("factory", Cell::new(4, 3), 1, 4, Walk::Avoid));
        assert_eq!(w.walk_cost(Cell::new(4, 2)), Some(w.cfg.walking.avoid_cost));
        assert!(w.order_move(0, Cell::new(7, 1)), "still reachable");
    }

    #[test]
    fn drop_rules() {
        let mut w = world();
        let r = rules(2);
        assert_eq!(w.can_drop(&r, Cell::new(9, 3)), Err(DropError::NotGround(Cell::new(9, 3))));
        assert_eq!(w.can_drop(&r, Cell::new(1, 1)), Err(DropError::OnRim(Cell::new(0, 1))), "island edge is rim");
        w.units.push(Unit::new("golem", Cell::new(2, 1), 9, 256));
        assert_eq!(w.can_drop(&r, Cell::new(3, 2)), Err(DropError::UnitInTheWay(Cell::new(2, 1))));
        w.units.pop();
        assert_eq!(w.can_drop(&r, Cell::new(7, 3)), Err(DropError::OnRim(Cell::new(7, 3))), "rectangle edge is rim");
        let rim_ok = TypeRules { may_drop_on_rim: true, ..rules(2) };
        assert!(w.can_drop(&rim_ok, Cell::new(7, 3)).is_ok());
        // Interior 2x2 at (3,2) covers (2..3, 1..2): all filled cells.
        let i = w.drop_structure("thing", &r, Cell::new(3, 2)).unwrap();
        assert_eq!(w.structures[i].cells().count(), 4);
        assert_eq!(w.structure_version, 1);
        assert_eq!(w.can_drop(&r, Cell::new(3, 2)), Err(DropError::Occupied(Cell::new(3, 2))));
    }

    #[test]
    fn bridges_attach_to_ground_and_connect() {
        let mut w = world();
        assert!(!w.place_bridge(Cell::new(3, 1)), "already land");
        assert!(!w.place_bridge(Cell::new(9, 1)), "not adjacent to ground");
        assert!(w.place_bridge(Cell::new(8, 1)), "east of the island edge");
        assert!(w.place_bridge(Cell::new(9, 1)));
        assert!(w.place_bridge(Cell::new(9, 0)));
        assert_eq!(w.bridge_version, 3);
        assert_eq!(w.bridge_connections(Cell::new(8, 1)), 2 | 8, "east to bridge, west to island");
        assert_eq!(w.bridge_connections(Cell::new(9, 1)), 1 | 8);
        assert_eq!(w.bridge_connections(Cell::new(9, 0)), 4);
        assert!(w.is_walkable(Cell::new(9, 0)));
        assert!(w.order_move(0, Cell::new(9, 0)), "units walk over bridges");
    }

    #[test]
    fn pieces_attach_to_island_edges_and_open_ends_only() {
        let mut w = world();
        let three_east = [Cell::new(8, 1), Cell::new(9, 1), Cell::new(10, 1)];
        assert_eq!(w.can_place_piece(&[Cell::new(7, 1), Cell::new(8, 1)]), Err(PieceError::NotSky(Cell::new(7, 1))));
        assert_eq!(w.can_place_piece(&[Cell::new(10, 1), Cell::new(11, 1)]), Err(PieceError::NoAttachment));
        w.place_piece(&three_east).unwrap();
        assert!(w.is_open_end(Cell::new(10, 1)));
        assert!(!w.is_open_end(Cell::new(9, 1)), "connected on both sides");
        // Attaching to the side of a bridge cell is not allowed, only to an open end or island edge.
        assert_eq!(w.can_place_piece(&[Cell::new(9, 0)]), Err(PieceError::NoAttachment));
        assert!(w.place_piece(&[Cell::new(11, 1), Cell::new(11, 0)]).is_ok());
        assert_eq!(w.bridges.len(), 5);
    }

    #[test]
    fn destruction_cascades_through_cracked_cells_and_drops_the_rest() {
        let mut w = world();
        for x in 8..14 {
            assert!(w.place_bridge(Cell::new(x, 1)));
        }
        let golem = TypeRules { is_unit: true, ..TypeRules::plain() };
        let far = w.spawn_unit("sunwalker", &golem, Cell::new(13, 1)).unwrap();
        assert!(w.crack_bridge(Cell::new(10, 1)));
        assert!(w.harden_bridge(Cell::new(11, 1)));
        assert!(!w.crack_bridge(Cell::new(11, 1)), "hard cells do not crack");
        assert!(w.destroy_bridge(Cell::new(9, 1)));
        assert!(!w.is_bridge(Cell::new(10, 1)), "cracked neighbour collapsed with the shock");
        assert_eq!(w.bridge_state(Cell::new(11, 1)), Some(BridgeState::Hard), "hard cells do not crack");
        assert_eq!(w.bridge_state(Cell::new(12, 1)), Some(BridgeState::Cracked), "unsupported cells crack first");
        assert!(w.is_bridge(Cell::new(8, 1)), "still attached to the island");
        assert!(w.units[far].alive, "the golem stands on a cracked bridge for now");
        for _ in 0..CRUMBLE_TICKS + 1 {
            w.step(&test_scripts());
        }
        assert!(!w.is_bridge(Cell::new(11, 1)), "hard but unsupported: it crumbled");
        assert!(!w.is_bridge(Cell::new(13, 1)));
        assert!(!w.units[far].alive, "the golem fell with the bridge");
        assert!(!w.order_move(far, Cell::new(1, 1)), "dead units take no orders");
        assert!(w.doomed.is_empty());
    }

    #[test]
    fn emplacements_are_dropped_in_the_sky_off_a_bridge_end() {
        let mut w = world();
        for x in 8..10 {
            assert!(w.place_bridge(Cell::new(x, 1)));
        }
        let cannon = TypeRules { foot_x: 3, foot_y: 3, creates_island: true, may_drop_on_rim: true, ..TypeRules::plain() };
        // Footprint (10..12, 0..2) in the sky, touching the open end at (9,1).
        assert_eq!(w.can_drop(&cannon, Cell::new(12, 2)), Ok(()));
        // Overlapping the bridge itself is refused, as is floating away from any end.
        assert_eq!(w.can_drop(&cannon, Cell::new(11, 2)), Err(DropError::NotGround(Cell::new(9, 1))));
        assert_eq!(w.can_drop(&cannon, Cell::new(16, 2)), Err(DropError::NotAtBridgeEnd));
        // On the island it is fine too, but not half on and half off.
        assert_eq!(w.can_drop(&cannon, Cell::new(3, 2)), Ok(()));
        assert_eq!(w.can_drop(&cannon, Cell::new(8, 2)), Err(DropError::Straddling(Cell::new(8, 2))));
        w.drop_structure("suncannon", &cannon, Cell::new(12, 2)).unwrap();
        assert_eq!(w.platforms.len(), 9);
        assert!(w.is_land(Cell::new(10, 1)));
        assert!(!w.is_open_end(Cell::new(9, 1)), "the bridge now leads onto the new island");
        // Blow the bridge: the platform and cannon fall at once, the stub crumbles later.
        assert!(w.destroy_bridge(Cell::new(8, 1)));
        assert!(w.platforms.is_empty(), "platform fell");
        assert!(w.structures.is_empty(), "cannon fell with it");
        assert_eq!(w.bridge_state(Cell::new(9, 1)), Some(BridgeState::Cracked));
        for _ in 0..CRUMBLE_TICKS + 1 {
            w.step(&test_scripts());
        }
        assert!(w.bridges.is_empty());
    }

    #[test]
    fn buildings_need_island_ground() {
        let mut w = world();
        let tree = TypeRules { foot_x: 2, foot_y: 2, ..TypeRules::plain() };
        for x in 8..10 {
            w.place_bridge(Cell::new(x, 1));
        }
        assert_eq!(w.can_drop(&tree, Cell::new(9, 2)), Err(DropError::NotGround(Cell::new(9, 2))));
        assert_eq!(w.can_drop(&tree, Cell::new(3, 2)), Ok(()));
    }

    #[test]
    fn piece_queue_refills_after_use() {
        let mut w = world();
        let first = w.queue.slots[0].clone();
        let cells = first.cells_at(Cell::new(8, 1));
        if w.can_place_piece(&cells).is_ok() {
            w.place_piece(&cells).unwrap();
        }
        w.queue.refill(0);
        assert_eq!(w.queue.slots.len(), PIECE_SLOTS);
    }

    #[test]
    fn golems_harvest_a_geyser_into_the_temple() {
        let mut w = world();
        w.islands.push(IslandMap::rect(Cell::new(10, 0), 6, 6));
        for x in 8..10 {
            w.place_bridge(Cell::new(x, 1));
        }
        let geyser = TypeRules { foot_x: 3, foot_y: 3, cost: 500, is_geyser: true, may_drop_on_rim: true, ..TypeRules::plain() };
        let temple = TypeRules { foot_x: 2, foot_y: 2, is_temple: true, may_drop_on_rim: true, cost: 100, ..TypeRules::plain() };
        let g = w.drop_structure("geyser", &geyser, Cell::new(14, 4)).unwrap();
        assert_eq!(w.powers[0], 0, "geysers are free");
        assert_eq!(w.drop_structure("residence", &temple, Cell::new(2, 2)), Err(DropError::NotEnoughPower { cost: 100, have: 0 }));
        w.powers[0] = 100;
        let t = w.drop_structure("residence", &temple, Cell::new(2, 2)).unwrap();
        assert_eq!(w.powers[0], 0);
        let golem = TypeRules { is_unit: true, speed: 3.0, ..TypeRules::plain() };
        let u = w.spawn_unit("sunwalker", &golem, Cell::new(4, 1)).unwrap();
        assert!(w.order_harvest(u, g));
        assert!(matches!(w.units[u].task, Task::Harvest { .. }));
        for _ in 0..6000 {
            w.step(&test_scripts());
            if w.units[u].task == Task::Idle {
                break;
            }
        }
        assert_eq!(w.units[u].task, Task::Idle, "finished after the geyser ran dry");
        assert_eq!(w.powers[0], 500, "all crystals delivered in trips of at most {NUGGET_POWER}");
        assert_eq!(w.structures[g].stock, 0);
        assert_eq!(w.structures[g].kind, "emptygeyser");
        assert!(!w.order_harvest(u, g), "nothing left to harvest");
        let _ = t;
    }

    fn cannon() -> TypeRules {
        TypeRules {
            foot_x: 3,
            foot_y: 3,
            creates_island: true,
            may_drop_on_rim: true,
            max_hit_points: 600,
            range: 6,
            hp_per_sec: 16,
            delay_between_shots: 5.0,
            damage_per_shot: 80,
            cardinal_only: true,
            threat: 5,
            cost: 400,
            ..TypeRules::plain()
        }
    }

    #[test]
    fn cannons_fire_straight_at_enemies_in_range() {
        let mut w = world();
        w.islands.push(IslandMap::rect(Cell::new(0, 4), 12, 8));
        w.powers[0] = 1000;
        let mine = w.drop_structure("suncannon", &cannon(), Cell::new(3, 6)).unwrap();
        assert_eq!(w.powers[0], 600);
        // Enemy cannon in the same rows, 4 cells east: both in range and in line.
        w.island_owners = vec![0, 0];
        w.energy_enforced = false; // the enemy has no ground of its own here
        w.powers[1] = 1000;
        let theirs = w.drop_structure_for(1, "suncannon", &cannon(), Cell::new(9, 6)).unwrap();
        assert_eq!(w.powers[0], 600, "enemies do not spend our power");
        w.step(&test_scripts());
        assert_eq!(w.structures[mine].hp, 520, "took one 80-point shot");
        assert_eq!(w.structures[theirs].hp, 520);
        assert_eq!(w.last_shots.len(), 2, "both cannons fired this tick");
        w.step(&test_scripts());
        assert_eq!(w.last_shots.len(), 0, "a tick without shots logs none");
        for _ in 0..148 {
            w.step(&test_scripts());
        }
        assert_eq!(w.structures[mine].hp, 520, "cooldown of five seconds");
        w.step(&test_scripts());
        assert_eq!(w.structures[mine].hp, 440);
        // A unit diagonal to the enemy cannon is out of its line of fire.
        let golem = TypeRules { is_unit: true, max_hit_points: 50, ..TypeRules::plain() };
        let u = w.spawn_unit("sunwalker", &golem, Cell::new(5, 10)).unwrap();
        for _ in 0..200 {
            w.step(&test_scripts());
        }
        assert!(w.units[u].alive, "diagonal target never shot");
    }

    #[test]
    fn destroyed_structures_explode_and_shock_bridges() {
        let mut w = world();
        for x in 8..14 {
            w.place_bridge(Cell::new(x, 1));
        }
        let battery = TypeRules { foot_x: 3, foot_y: 3, creates_island: true, max_hit_points: 100, cost: 300, ..TypeRules::plain() };
        w.powers[0] = 300;
        // In the sky off the open end at (13,1): footprint (14..16, 0..2).
        let b = w.drop_structure("sunbattery", &battery, Cell::new(16, 2)).unwrap_or_else(|e| panic!("{e}"));
        w.crack_bridge(Cell::new(13, 1));
        w.damage_structure(b, 100);
        assert!(w.structures.is_empty(), "battery destroyed");
        assert!(!w.is_bridge(Cell::new(13, 1)), "cracked bridge in the blast was destroyed");
        assert_eq!(w.bridge_state(Cell::new(12, 1)), Some(BridgeState::Normal), "outside the blast");
        assert!(w.platforms.is_empty(), "its platform fell once the bridge was gone");
    }

    #[test]
    fn salvage_refunds_a_quarter_scaled_by_health() {
        let mut w = world();
        w.powers[0] = 1000;
        let r = TypeRules { foot_x: 2, foot_y: 2, cost: 400, max_hit_points: 100, ..TypeRules::plain() };
        let i = w.drop_structure("thing", &r, Cell::new(3, 2)).unwrap();
        assert_eq!(w.powers[0], 600);
        assert_eq!(w.salvage(i, &test_scripts()), Some(100));
        assert_eq!(w.powers[0], 700);
        let i = w.drop_structure("thing", &r, Cell::new(3, 2)).unwrap();
        w.damage_structure(i, 50);
        assert_eq!(w.salvage(i, &test_scripts()), Some(50), "half health, half refund");
        w.energy_enforced = false;
        w.powers[1] = 1000;
        let e = w.drop_structure_for(1, "thing", &r, Cell::new(3, 2)).unwrap();
        assert_eq!(w.salvage(e, &test_scripts()), None, "cannot salvage enemy property");
    }

    #[test]
    fn stunned_priest_is_captured_and_sacrificed() {
        let mut w = world();
        w.islands.push(IslandMap::rect(Cell::new(0, 4), 12, 8));
        w.powers[0] = 1000;
        let altar = TypeRules { foot_x: 3, foot_y: 3, is_altar: true, may_drop_on_rim: true, cost: 500, ..TypeRules::plain() };
        let a = w.drop_structure("dais", &altar, Cell::new(5, 9)).unwrap();
        let priest = TypeRules { is_unit: true, is_priest: true, max_hit_points: 100, threat: 25, speed: 1.8, ..TypeRules::plain() };
        let golem = TypeRules { is_unit: true, is_transport: true, max_hit_points: 50, speed: 3.0, ..TypeRules::plain() };
        let p = w.spawn_unit_for(1, "priest", &priest, Cell::new(9, 7)).unwrap();
        w.island_owners = vec![0, 0];
        let g = w.spawn_unit("sunwalker", &golem, Cell::new(1, 6)).unwrap();
        assert!(!w.order_capture(g, p), "a healthy priest cannot be captured");
        w.damage_unit(p, 50);
        w.step(&test_scripts());
        assert!(w.units[p].stunned);
        assert!(!w.order_move(p, Cell::new(9, 9)), "stunned priests cannot move");
        assert!(w.order_capture(g, p));
        for _ in 0..400 {
            w.step(&test_scripts());
            if w.units[g].carrying.is_some() {
                break;
            }
        }
        assert_eq!(w.units[g].carrying, Some(p));
        assert_eq!(w.units[p].carried_by, Some(g));
        assert!(w.order_sacrifice(g, a));
        for _ in 0..400 {
            w.step(&test_scripts());
            if w.units[g].task == Task::Idle && w.units[g].carrying.is_none() {
                break;
            }
        }
        assert!(!w.units[p].alive, "sacrificed");
        assert_eq!(w.knowledge, 1);
        assert_eq!(w.units[g].cell(), w.structures[a].centre());
    }

    #[test]
    fn temple_heals_a_priest_out_of_stun() {
        let mut w = world();
        let temple = TypeRules { foot_x: 2, foot_y: 2, is_temple: true, may_drop_on_rim: true, ..TypeRules::plain() };
        w.drop_structure("residence", &temple, Cell::new(2, 2)).unwrap();
        let priest = TypeRules { is_unit: true, is_priest: true, max_hit_points: 100, ..TypeRules::plain() };
        let p = w.spawn_unit("priest", &priest, Cell::new(4, 1)).unwrap();
        w.damage_unit(p, 60);
        w.step(&test_scripts());
        assert!(w.units[p].stunned);
        for _ in 0..(HEAL_TICKS * 12) {
            w.step(&test_scripts());
        }
        assert!(!w.units[p].stunned, "healed above half by the temple");
        assert!(w.units[p].hp > 50);
    }

    #[test]
    fn placement_needs_energy_within_range() {
        use crate::rules::EnergyNeed;
        let mut w = world();
        w.islands.push(IslandMap::rect(Cell::new(0, 4), 40, 8));
        w.powers[0] = 5000;
        let temple = TypeRules { foot_x: 2, foot_y: 2, is_temple: true, may_drop_on_rim: true, produces: Some(Theme::Sun), ..TypeRules::plain() };
        w.drop_structure("residence", &temple, Cell::new(2, 2)).unwrap();
        let thrower = TypeRules { foot_x: 1, foot_y: 1, energy: Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 1 }), ..TypeRules::plain() };
        assert!(w.drop_structure("sunarcher", &thrower, Cell::new(5, 6)).is_ok(), "next to the temple");
        // 128 px is 8 cells east; (2,2) centre to (30,6) is far outside.
        assert!(matches!(w.drop_structure("sunarcher", &thrower, Cell::new(30, 6)), Err(DropError::NotEnoughEnergy { .. })));
        // A Wind Generator there extends the reach, but wind units need wind.
        let generator = TypeRules { foot_x: 1, foot_y: 1, produces: Some(Theme::Wind), energy: Some(EnergyNeed { theme: Theme::Sun, themed: 0, any: 1 }), ..TypeRules::plain() };
        assert!(w.drop_structure("windbattery", &generator, Cell::new(8, 6)).is_ok());
        assert!(w.drop_structure("sunarcher", &thrower, Cell::new(14, 6)).is_ok(), "fed by the generator's Sun-compatible energy");
        let windarcher = TypeRules { foot_x: 1, foot_y: 1, energy: Some(EnergyNeed { theme: Theme::Wind, themed: 1, any: 1 }), ..TypeRules::plain() };
        assert!(w.drop_structure("windarcher", &windarcher, Cell::new(6, 7)).is_ok(), "temple plus generator overlap");
        assert!(matches!(w.drop_structure("windarcher", &windarcher, Cell::new(14, 7)), Err(DropError::NotEnoughEnergy { .. })), "only the generator reaches");
        // Other owners' sources do not count; enemies need their own ground.
        assert_eq!(w.energy_at(1, Cell::new(3, 3), Theme::Sun), (0, 0));
        assert_eq!(w.drop_structure_for(1, "sunarcher", &thrower, Cell::new(36, 6)), Err(DropError::NotOwnGround));
    }

    #[test]
    fn ownership_and_knowledge_gate_building() {
        let mut w = World::new(test_config());
        w.push_island(IslandMap::rect(Cell::new(0, 0), 8, 6), 0);
        w.push_island(IslandMap::rect(Cell::new(20, 0), 8, 6), 1);
        w.powers = vec![1000, 1000, 0, 0];
        let piece = [Cell::new(8, 2), Cell::new(9, 2)];
        assert_eq!(w.can_place_piece_for(1, &piece), Err(PieceError::NotOwnGround), "off our island, not theirs");
        assert!(w.place_piece_for(0, &piece).is_ok());
        assert_eq!(w.place_piece_for(1, &[Cell::new(10, 2)]), Err(PieceError::NotOwnGround), "off our open end");
        // The enemy builds west from its own island and may connect to our open end.
        for x in (10..20).rev() {
            assert!(w.place_piece_for(1, &[Cell::new(x, 2)]).is_ok(), "{x}");
        }
        assert_eq!(w.ground_owner(Cell::new(10, 2)), Some(1));
        let thrower = TypeRules { foot_x: 1, foot_y: 1, cost: 250, ..TypeRules::plain() };
        assert_eq!(w.drop_structure_for(1, "sunarcher", &thrower, Cell::new(2, 2)), Err(DropError::NotOwnGround));
        assert!(w.drop_structure_for(1, "sunarcher", &thrower, Cell::new(22, 2)).is_ok());
        assert_eq!(w.powers[1], 750, "the opponent pays too");
        let gated = TypeRules { foot_x: 1, foot_y: 1, tech_bit: Some(4), ..TypeRules::plain() };
        assert_eq!(w.drop_structure("windbattery", &gated, Cell::new(3, 3)), Err(DropError::UnknownTech(4)));
        assert!(w.grant_tech(0, 4));
        assert!(w.drop_structure("windbattery", &gated, Cell::new(3, 3)).is_ok());
    }

    #[test]
    fn spawns_units_on_walkable_ground_only() {
        let mut w = world();
        let golem = TypeRules { is_unit: true, speed: 1.8, ..TypeRules::plain() };
        assert_eq!(w.spawn_unit("sunwalker", &golem, Cell::new(9, 9)), None);
        assert_eq!(w.spawn_unit("sunwalker", &golem, Cell::new(2, 2)), Some(1));
        assert_eq!(w.units[1].speed, speed_per_tick(&w.cfg, 1.8));
    }

    #[test]
    fn speed_conversion() {
        let c = test_config();
        assert_eq!(speed_per_tick(&c, 1.0), 9);
        assert_eq!(speed_per_tick(&c, 0.0), 1);
    }
}
