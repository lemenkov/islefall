// SPDX-License-Identifier: Apache-2.0
//! The whole simulated state and its fixed-rate tick.

use std::collections::BTreeSet;

use crate::grid::Cell;
use crate::island::IslandMap;
use crate::path::find_path_costed;
use crate::rules::{AVOID_COST, TypeRules, Walk};
use crate::structure::Structure;
use crate::unit::{Pos, Unit};

/// Simulation ticks per second. Rendering interpolates or samples; the
/// simulation never sees wall-clock time.
pub const TICK_HZ: u32 = 30;

/// Why a drop was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropError {
    /// A footprint cell is sky.
    NotGround(Cell),
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
            DropError::Occupied(c) => write!(f, "{c:?} is occupied"),
            DropError::OnRim(c) => write!(f, "{c:?} is an island rim"),
            DropError::UnitInTheWay(c) => write!(f, "a unit stands on {c:?}"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct World {
    pub tick: u64,
    /// Natural islands.
    pub islands: Vec<IslandMap>,
    /// Platforms created under `createsisland` structures dropped on bridges.
    pub platforms: IslandMap,
    /// Incremented whenever islands or platforms change.
    pub terrain_version: u64,
    /// Cells carrying a bridge tile. Bridges are one cell wide.
    pub bridges: BTreeSet<Cell>,
    /// Incremented whenever `bridges` changes, so renderers can refresh.
    pub bridge_version: u64,
    pub structures: Vec<Structure>,
    /// Incremented whenever `structures` changes.
    pub structure_version: u64,
    pub units: Vec<Unit>,
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
        self.bridges.contains(&cell)
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
        self.bridges.insert(cell);
        self.bridge_version += 1;
        true
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
    pub fn can_drop(&self, rules: &TypeRules, cell: Cell) -> Result<(), DropError> {
        let probe = Structure::new("", cell, rules.foot_x, rules.foot_y, Walk::Free);
        for c in probe.cells() {
            if !self.is_ground(c) {
                return Err(DropError::NotGround(c));
            }
            if self.structure_at(c).is_some() {
                return Err(DropError::Occupied(c));
            }
            if !rules.may_drop_on_rim && self.is_rim(c) {
                return Err(DropError::OnRim(c));
            }
            if self.units.iter().any(|u| u.pos.cell() == c) {
                return Err(DropError::UnitInTheWay(c));
            }
        }
        Ok(())
    }

    /// Drop a structure of type `kind`. Bridge cells under a `createsisland`
    /// type become platform ground.
    pub fn drop_structure(&mut self, kind: &str, rules: &TypeRules, cell: Cell) -> Result<usize, DropError> {
        self.can_drop(rules, cell)?;
        let mut s = Structure::new(kind, cell, rules.foot_x, rules.foot_y, rules.walk);
        s.drop_blocking = rules.drop_blocking;
        if rules.creates_island {
            // Bridge cells under the footprint become island ground.
            let mut changed = false;
            for c in s.cells() {
                if !self.is_land(c) {
                    self.platforms.insert(c);
                    changed = true;
                }
                if self.bridges.remove(&c) {
                    self.bridge_version += 1;
                }
            }
            if changed {
                self.terrain_version += 1;
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
        let Some(from) = self.units.get(unit).map(|u| u.pos.cell()) else { return false };
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
    fn emplacement_on_bridge_creates_platform() {
        let mut w = world();
        for x in 8..14 {
            assert!(w.place_bridge(Cell::new(x, 1)));
        }
        let cannon = TypeRules { foot_x: 3, foot_y: 3, creates_island: true, ..TypeRules::plain() };
        // Footprint (9..11, -1..1): the middle row is bridge, the others sky.
        assert_eq!(w.can_drop(&cannon, Cell::new(11, 1)), Err(DropError::NotGround(Cell::new(11, 0))));
        for x in 9..12 {
            for y in [0, 2] {
                assert!(w.place_bridge(Cell::new(x, y)), "{x},{y}");
            }
        }
        w.drop_structure("suncannon", &cannon, Cell::new(11, 2)).unwrap();
        assert_eq!(w.platforms.len(), 9);
        assert!(w.is_land(Cell::new(10, 1)));
        assert!(!w.is_bridge(Cell::new(10, 1)), "bridge under the platform is gone");
        assert!(w.is_bridge(Cell::new(8, 1)) && w.is_bridge(Cell::new(12, 1)), "bridge continues on both sides");
        assert_eq!(w.terrain_version, 1);
        assert_eq!(w.bridge_connections(Cell::new(12, 1)) & 8, 8, "bridge still connects to the platform");
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
