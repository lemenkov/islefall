// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Things placed on the map that occupy a footprint of cells.

use serde::{Deserialize, Serialize};
use islefall_data::isle::Theme;

use crate::grid::Cell;
use crate::rules::Walk;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Structure {
    /// Type file stem, e.g. `dais` or `treetwo`.
    pub kind: String,
    /// The cell holding the hotspot: the bottom-right cell of the footprint.
    pub cell: Cell,
    /// Footprint size in cells (`foot_x`, `foot_y` in the type file).
    pub foot_x: i32,
    pub foot_y: i32,
    /// How the footprint affects walking.
    pub walk: Walk,
    /// Whether nothing may be dropped onto the footprint.
    pub drop_blocking: bool,
    /// Storm crystals left, for geysers.
    pub stock: i32,
    /// Whether units deliver crystals here.
    pub is_temple: bool,
    /// An Outpost: takes in crystals and holds its island.
    pub is_outpost: bool,
    pub owner: u8,
    /// Hit points left; `max_hp` 0 means it cannot be damaged.
    pub hp: i32,
    pub max_hp: i32,
    /// Storm Power it was bought for, refunded in part when salvaged.
    pub cost: i32,
    /// Weapon, if the type shoots.
    pub weapon: Option<Weapon>,
    /// Ticks until the weapon may fire again.
    pub cooldown: u32,
    pub threat: i32,
    pub is_altar: bool,
    /// Energy this structure radiates, for Generators and Temples.
    pub produces: Option<Theme>,
    /// Workshop production slots: type stems in production, at most `slots`.
    pub production: Vec<String>,
    pub slots: usize,
    pub theme: Theme,
    /// The aerial attacker type this base launches, if it is one.
    pub launches: Option<String>,
    /// Index of the attacker in the air, if alive.
    pub flyer: Option<usize>,
    /// Ticks until the base launches again.
    pub respawn: u32,
    /// Ticks of stream still needed before the structure stands; 0 when built.
    pub building: u32,
    /// Ticks the whole build takes, for progress.
    pub build_ticks: u32,
    /// Workshop level, from one.
    pub level: u8,
    /// An Obelisk and the Spell it holds.
    pub is_obelisk: bool,
    pub spell: Option<String>,
    /// Ticks of paralysis left: no shooting.
    pub paralysed: u32,
    /// Where the last shot went, for turning the turret.
    pub aim: Option<Cell>,
    /// The variant frame the map pinned, as the original saved with `saveFrame`.
    pub variant: Option<u32>,
    /// A number that stays with the structure while it stands, unlike its
    /// index; 0 until the world takes it in.
    pub id: u32,
    /// What it shot at last, kept while that stays a valid target.
    pub target: Option<Aim>,
}

/// What a shooter is trained on, by identities that survive removals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aim {
    Structure(u32),
    Unit(usize),
    Bridge(Cell),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weapon {
    pub range: i32,
    pub damage: i32,
    /// Ticks between shots.
    pub delay: u32,
    pub cardinal_only: bool,
    /// Whether ground targets can be hit at all (the Vander Tower cannot).
    pub ground: bool,
    /// Reach and damage per shot against flyers; 0 when they cannot be hit.
    pub air_range: i32,
    pub air_damage: i32,
}

impl Structure {
    /// Whether the stream has finished building the structure.
    pub fn complete(&self) -> bool {
        self.building == 0
    }

    /// Build progress from 0 to 1.
    pub fn progress(&self) -> f32 {
        if self.build_ticks == 0 { 1.0 } else { 1.0 - self.building as f32 / self.build_ticks as f32 }
    }

    pub fn new(kind: impl Into<String>, cell: Cell, foot_x: i32, foot_y: i32, walk: Walk) -> Structure {
        Structure {
            kind: kind.into(),
            cell,
            foot_x: foot_x.max(1),
            foot_y: foot_y.max(1),
            walk,
            drop_blocking: false,
            stock: 0,
            is_temple: false,
            is_outpost: false,
            owner: 0,
            hp: 0,
            max_hp: 0,
            cost: 0,
            weapon: None,
            cooldown: 0,
            threat: 0,
            is_altar: false,
            produces: None,
            production: Vec::new(),
            slots: 0,
            theme: Theme::Sun,
            launches: None,
            flyer: None,
            respawn: 0,
            building: 0,
            build_ticks: 0,
            level: 1,
            is_obelisk: false,
            spell: None,
            paralysed: 0,
            aim: None,
            variant: None,
            id: 0,
            target: None,
        }
    }

    /// Centre of the footprint, in cells (rounded towards the hotspot).
    pub fn centre(&self) -> Cell {
        self.cell.offset(-(self.foot_x - 1) / 2, -(self.foot_y - 1) / 2)
    }

    /// Whether `cell` shares a row or column with the footprint: a straight line of fire.
    pub fn in_line(&self, cell: Cell) -> bool {
        let dx = self.cell.x - cell.x;
        let dy = self.cell.y - cell.y;
        (0..self.foot_x).contains(&dx) || (0..self.foot_y).contains(&dy)
    }

    /// Chebyshev distance from the footprint to `cell`, 0 when covered.
    pub fn distance_to(&self, cell: Cell) -> i32 {
        let dx = if cell.x > self.cell.x { cell.x - self.cell.x } else { (self.cell.x - self.foot_x + 1 - cell.x).max(0) };
        let dy = if cell.y > self.cell.y { cell.y - self.cell.y } else { (self.cell.y - self.foot_y + 1 - cell.y).max(0) };
        dx.max(dy)
    }

    /// Cells orthogonally adjacent to the footprint, where a unit can stand to work on it.
    pub fn adjacent_cells(&self) -> Vec<Cell> {
        let mut out = Vec::new();
        for c in self.cells() {
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let n = c.offset(dx, dy);
                if !self.covers(n) && !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out
    }

    pub fn is_adjacent(&self, cell: Cell) -> bool {
        !self.covers(cell) && [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| self.covers(cell.offset(dx, dy)))
    }

    pub fn blocks_walking(&self) -> bool {
        self.walk == Walk::Blocked
    }

    /// Every cell of the footprint. The hotspot cell is the bottom-right one.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.foot_y).flat_map(move |dy| (0..self.foot_x).map(move |dx| self.cell.offset(-dx, -dy)))
    }

    pub fn covers(&self, cell: Cell) -> bool {
        let dx = self.cell.x - cell.x;
        let dy = self.cell.y - cell.y;
        (0..self.foot_x).contains(&dx) && (0..self.foot_y).contains(&dy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_cells() {
        let s = Structure::new("dais", Cell::new(9, 7), 3, 2, Walk::Free);
        assert_eq!(s.owner, 0);
        let cells: Vec<Cell> = s.cells().collect();
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&Cell::new(9, 7)));
        assert!(cells.contains(&Cell::new(7, 6)));
        assert!(!cells.contains(&Cell::new(6, 6)));
        assert!(s.covers(Cell::new(8, 7)));
        assert!(!s.covers(Cell::new(10, 7)));
        assert!(!s.covers(Cell::new(9, 5)));
        assert_eq!(s.adjacent_cells().len(), 10);
        assert!(s.is_adjacent(Cell::new(10, 7)));
        assert!(!s.is_adjacent(Cell::new(10, 8)), "diagonal does not count");
        assert_eq!(s.distance_to(Cell::new(9, 7)), 0);
        assert_eq!(s.distance_to(Cell::new(12, 7)), 3);
        assert_eq!(s.distance_to(Cell::new(5, 3)), 3);
        assert!(s.in_line(Cell::new(20, 6)) && s.in_line(Cell::new(8, 0)) && !s.in_line(Cell::new(20, 0)));
        assert_eq!(s.centre(), Cell::new(8, 7));
    }
}
