// SPDX-License-Identifier: Apache-2.0
//! The whole simulated state and its fixed-rate tick.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::grid::Cell;
use crate::island::IslandMap;
use crate::path::find_path_costed;
use crate::pieces::PieceQueue;
use crate::rules::{AVOID_COST, TypeRules, Walk};
use crate::structure::Structure;
use crate::unit::{Pos, Unit};

/// Simulation ticks per second. Rendering interpolates or samples; the
/// simulation never sees wall-clock time.
pub const TICK_HZ: u32 = 30;
/// Ticks an unsupported bridge cell stays cracked before it crumbles.
pub const CRUMBLE_TICKS: u32 = 4 * TICK_HZ;
/// Bridge pieces offered at once.
pub const PIECE_SLOTS: usize = 4;

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

/// Why a piece could not be placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceError {
    /// A piece cell is not sky.
    NotSky(Cell),
    /// No piece cell touches an island edge or an open bridge end.
    NoAttachment,
}

impl std::fmt::Display for PieceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PieceError::NotSky(c) => write!(f, "{c:?} is not sky"),
            PieceError::NoAttachment => write!(f, "must attach to an island edge or an open bridge end"),
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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct World {
    pub tick: u64,
    /// Natural islands.
    pub islands: Vec<IslandMap>,
    /// Platforms created under `createsisland` structures dropped on bridges.
    pub platforms: IslandMap,
    /// Incremented whenever islands or platforms change.
    pub terrain_version: u64,
    /// Cells carrying a bridge tile and their condition. Bridges are one cell wide.
    pub bridges: BTreeMap<Cell, BridgeState>,
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
}

impl Default for World {
    fn default() -> World {
        World {
            tick: 0,
            islands: Vec::new(),
            platforms: IslandMap::new(),
            terrain_version: 0,
            bridges: BTreeMap::new(),
            bridge_version: 0,
            structures: Vec::new(),
            structure_version: 0,
            units: Vec::new(),
            doomed: BTreeMap::new(),
            queue: PieceQueue::new(PIECE_SLOTS, 0x5eed),
        }
    }
}

impl World {
    pub fn new() -> World {
        World::default()
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
                Walk::Avoid => cost = cost.max(AVOID_COST),
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
        self.can_place_piece(cells)?;
        for &c in cells {
            self.bridges.insert(c, BridgeState::Normal);
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
        for (c, state) in self.bridges.iter_mut() {
            if !reached.contains(c) && !self.doomed.contains_key(c) {
                self.doomed.insert(*c, CRUMBLE_TICKS);
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
        }
        self.terrain_version += 1;
        self.remove_unsupported_things(&reached);
        falling.len()
    }

    fn remove_unsupported_things(&mut self, reached: &BTreeSet<Cell>) {
        let before = self.structures.len();
        self.structures.retain(|s| s.cells().all(|c| reached.contains(&c)));
        if self.structures.len() != before {
            self.structure_version += 1;
        }
        for u in &mut self.units {
            if u.alive && !reached.contains(&u.pos.cell()) {
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
            if self.units.iter().any(|u| u.alive && u.pos.cell() == c) {
                return Err(DropError::UnitInTheWay(c));
            }
        }
        Ok(())
    }

    /// Drop a structure of type `kind`. Sky under a `createsisland` type
    /// becomes platform ground.
    pub fn drop_structure(&mut self, kind: &str, rules: &TypeRules, cell: Cell) -> Result<usize, DropError> {
        self.can_drop(rules, cell)?;
        let mut s = Structure::new(kind, cell, rules.foot_x, rules.foot_y, rules.walk);
        s.drop_blocking = rules.drop_blocking;
        if rules.creates_island {
            let mut changed = false;
            for c in s.cells() {
                if !self.is_land(c) {
                    self.platforms.insert(c);
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
        if !self.is_walkable(cell) {
            return None;
        }
        self.units.push(Unit::new(kind, cell, speed_per_tick(rules.speed)));
        Some(self.units.len() - 1)
    }

    /// Order a unit to walk to `cell` along the cheapest walkable path.
    /// Returns `false` if the cell is unreachable.
    pub fn order_move(&mut self, unit: usize, cell: Cell) -> bool {
        let Some(from) = self.units.get(unit).filter(|u| u.alive).map(|u| u.pos.cell()) else { return false };
        let Some(path) = find_path_costed(from, cell, |c| self.walk_cost(c)) else { return false };
        let u = &mut self.units[unit];
        u.path = path.into_iter().map(Pos::cell_centre).collect();
        if u.path.is_empty() {
            // Already in the cell: walk to its centre so the unit settles.
            u.path.push_back(Pos::cell_centre(cell));
        }
        true
    }

    /// Advance the simulation by one tick.
    pub fn step(&mut self) {
        for u in &mut self.units {
            u.step();
        }
        self.crumble();
        self.tick += 1;
    }
}

/// Convert a type's `speed` property (cells per second) to fixed-point steps per tick.
pub fn speed_per_tick(cells_per_second: f64) -> i32 {
    ((cells_per_second * crate::unit::SUBCELL as f64) / TICK_HZ as f64).round().max(1.0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new();
        w.islands.push(IslandMap::rect(Cell::new(0, 0), 8, 4));
        w.units.push(Unit::new("priest", Cell::new(0, 1), speed_per_tick(1.0)));
        w
    }

    fn rules(foot: i32) -> TypeRules {
        TypeRules { foot_x: foot, foot_y: foot, ..TypeRules::plain() }
    }

    #[test]
    fn orders_only_onto_land() {
        let mut w = world();
        assert!(!w.order_move(0, Cell::new(9, 9)));
        assert!(w.order_move(0, Cell::new(7, 3)));
        for _ in 0..400 {
            w.step();
        }
        assert_eq!(w.units[0].pos.cell(), Cell::new(7, 3));
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
            w.step();
            visited.push(w.units[0].pos.cell());
            if !w.units[0].is_moving() {
                break;
            }
        }
        assert_eq!(w.units[0].pos.cell(), Cell::new(7, 1));
        assert!(visited.iter().all(|&c| w.is_walkable(c)), "never stood on the wall");
        assert!(visited.contains(&Cell::new(4, 0)), "went through the gap");
    }

    #[test]
    fn avoided_structures_are_passable_but_costly() {
        let mut w = world();
        w.structures.push(Structure::new("factory", Cell::new(4, 3), 1, 4, Walk::Avoid));
        assert_eq!(w.walk_cost(Cell::new(4, 2)), Some(AVOID_COST));
        assert!(w.order_move(0, Cell::new(7, 1)), "still reachable");
    }

    #[test]
    fn drop_rules() {
        let mut w = world();
        let r = rules(2);
        assert_eq!(w.can_drop(&r, Cell::new(9, 3)), Err(DropError::NotGround(Cell::new(9, 3))));
        assert_eq!(w.can_drop(&r, Cell::new(1, 1)), Err(DropError::OnRim(Cell::new(0, 1))), "island edge is rim");
        w.units.push(Unit::new("golem", Cell::new(2, 1), 9));
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
            w.step();
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
            w.step();
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
    fn spawns_units_on_walkable_ground_only() {
        let mut w = world();
        let golem = TypeRules { is_unit: true, speed: 1.8, ..TypeRules::plain() };
        assert_eq!(w.spawn_unit("sunwalker", &golem, Cell::new(9, 9)), None);
        assert_eq!(w.spawn_unit("sunwalker", &golem, Cell::new(2, 2)), Some(1));
        assert_eq!(w.units[1].speed, speed_per_tick(1.8));
    }

    #[test]
    fn speed_conversion() {
        assert_eq!(speed_per_tick(1.0), 9);
        assert_eq!(speed_per_tick(0.0), 1);
    }
}
