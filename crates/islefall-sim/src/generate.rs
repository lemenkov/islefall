// SPDX-License-Identifier: Apache-2.0
//! Islands made up on the spot: a fortress island for a player, and whole
//! skirmish maps of fortresses and geyser islands, the way the original
//! made its battlefields afresh each game. Everything is driven by a seed
//! so the same seed gives the same map on every machine.

use std::collections::BTreeSet;

use rand::{RngExt as _, SeedableRng};
use rand_pcg::Pcg32;

use crate::config::Config;
use crate::grid::Cell;
use crate::island::IslandMap;
use crate::map::{IslandDef, MapDef, OpponentDef, PlacementDef, UnitDef};

/// A seeded generator with a fixed, portable algorithm (PCG-32), so the
/// same seed makes the same map on every machine.
#[derive(Clone, Debug)]
pub struct Rng(Pcg32);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(Pcg32::seed_from_u64(seed))
    }

    /// An integer in `0..n`.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.0.random_range(0..n)
    }

    /// A float in `0..1`.
    pub fn unit(&mut self) -> f32 {
        self.0.random::<f32>()
    }
}

/// An island of 3 x 3-cell pieces, like the original's, on a canvas of
/// `w` x `h` cells with its top-left at (0, 0): an ellipse filling `fill`
/// of the canvas, its edge roughened, the `keep` cells always land, and
/// everything connected to the middle.
pub fn blob(rng: &mut Rng, w: i32, h: i32, fill: f32, keep: &[Cell]) -> IslandMap {
    let (pw, ph) = ((w / 3).max(1), (h / 3).max(1));
    let (cx, cy) = (pw as f32 / 2.0, ph as f32 / 2.0);
    let scale = fill.clamp(0.2, 1.0).sqrt();
    let (rx, ry) = ((cx - 0.5) * scale, (cy - 0.5) * scale);
    let mut land = vec![false; (pw * ph) as usize];
    let at = |i: i32, j: i32| (j * pw + i) as usize;
    for j in 0..ph {
        for i in 0..pw {
            let (dx, dy) = ((i as f32 + 0.5 - cx) / rx.max(0.5), (j as f32 + 0.5 - cy) / ry.max(0.5));
            let d = dx * dx + dy * dy;
            let noise = rng.unit() * 0.7 - 0.35;
            land[at(i, j)] = d + noise <= 1.0;
        }
    }
    // Kept cells and the cells round them, so nothing kept ends on the rim.
    let must: BTreeSet<(i32, i32)> = keep
        .iter()
        .flat_map(|c| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dx| c.offset(dx, dy))))
        .map(|c| (c.x.div_euclid(3), c.y.div_euclid(3)))
        .filter(|&(i, j)| i >= 0 && j >= 0 && i < pw && j < ph)
        .collect();
    // Smooth twice: lonely pieces sink, holes fill.
    for _ in 0..2 {
        let before = land.clone();
        for j in 0..ph {
            for i in 0..pw {
                let mut n = 0;
                for dj in -1..=1 {
                    for di in -1..=1 {
                        if (di != 0 || dj != 0) && i + di >= 0 && j + dj >= 0 && i + di < pw && j + dj < ph && before[at(i + di, j + dj)] {
                            n += 1;
                        }
                    }
                }
                let k = at(i, j);
                if before[k] && n <= 2 {
                    land[k] = false;
                } else if !before[k] && n >= 6 {
                    land[k] = true;
                }
            }
        }
    }
    for &(i, j) in &must {
        land[at(i, j)] = true;
    }
    // Keep what the middle piece reaches; join the kept pieces to it by
    // straight strips if the noise cut them off.
    let centre = (pw / 2, ph / 2);
    land[at(centre.0, centre.1)] = true;
    let reach = |land: &[bool]| -> BTreeSet<(i32, i32)> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![centre];
        while let Some((i, j)) = stack.pop() {
            if i < 0 || j < 0 || i >= pw || j >= ph || !land[at(i, j)] || !seen.insert((i, j)) {
                continue;
            }
            stack.extend([(i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)]);
        }
        seen
    };
    let mut seen = reach(&land);
    for &(i, j) in &must {
        if !seen.contains(&(i, j)) {
            let (mut x, mut y) = (i, j);
            while x != centre.0 {
                land[at(x, y)] = true;
                x += (centre.0 - x).signum();
            }
            while y != centre.1 {
                land[at(x, y)] = true;
                y += (centre.1 - y).signum();
            }
            seen = reach(&land);
        }
    }
    let mut island = IslandMap::new();
    for &(i, j) in &seen {
        for dy in 0..3 {
            for dx in 0..3 {
                island.insert(Cell::new(i * 3 + dx, j * 3 + dy));
            }
        }
    }
    island
}

/// Cells a structure of `foot` size covers with its hotspot at `at`.
pub fn footprint(at: Cell, foot: (i32, i32)) -> Vec<Cell> {
    let mut v = Vec::new();
    for dy in 0..foot.1.max(1) {
        for dx in 0..foot.0.max(1) {
            v.push(Cell::new(at.x - dx, at.y - dy));
        }
    }
    v
}

/// Where a player's seat lies on the ring: the first at the top, the
/// rest clockwise.
pub fn seat(centre: Cell, radius: i32, k: usize, players: usize) -> Cell {
    let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * k as f32 / players.max(1) as f32;
    Cell::new(centre.x + (radius as f32 * angle.cos()).round() as i32, centre.y + (radius as f32 * angle.sin()).round() as i32)
}

/// A whole battlefield: `players` fortresses round a ring, each with its
/// Temple, altar and High Priest, geyser islands scattered between them
/// and a few bare islets. `foot` gives a type's footprint.
pub fn skirmish(cfg: &Config, seed: u64, players: usize, foot: &dyn Fn(&str) -> (i32, i32)) -> MapDef {
    let g = &cfg.generate;
    let mut rng = Rng::new(seed);
    let players = players.clamp(1, cfg.sim.max_players);
    let world = Cell::new(g.world[0], g.world[1]);
    let centre = Cell::new(world.x / 2, world.y / 2);
    let (fw, fh) = (g.fortress[0], g.fortress[1]);
    let mut map = MapDef { name: format!("Skirmish {seed}"), start_power: g.start_power, camera: [centre.x, centre.y], ..MapDef::default() };
    let mut taken: Vec<(i32, i32, i32, i32)> = Vec::new();
    for k in 0..players {
        let theme = g.themes[k % g.themes.len().max(1)].clone();
        let s = seat(centre, g.ring_radius, k, players);
        let origin = Cell::new((s.x - fw / 2).clamp(0, world.x - fw), (s.y - fh / 2).clamp(0, world.y - fh));
        let mid = Cell::new(fw / 2, fh / 2);
        let altar_at = Cell::new(mid.x + 3, mid.y + 3);
        let temple_kind = cfg.fort.temples.get(&theme).cloned().unwrap_or_else(|| cfg.fort.temples.values().next().cloned().unwrap_or_default());
        let tf = foot(&temple_kind);
        let temple_at = Cell::new(mid.x + tf.0 / 2, mid.y + 3 + tf.1);
        let priest_at = Cell::new(mid.x - 6, mid.y);
        let mut keep = footprint(altar_at, foot(&cfg.fort.altar));
        keep.extend(footprint(temple_at, tf));
        keep.push(priest_at);
        let island = blob(&mut rng, fw, fh, g.fill, &keep);
        let cells: Vec<[i32; 2]> = island.cells().map(|c| [c.x + origin.x, c.y + origin.y]).collect();
        map.islands.push(IslandDef { owner: Some(k as u8), theme: theme.clone(), origin: [origin.x, origin.y], size: [fw, fh], remove: Vec::new(), cells });
        map.structures.push(PlacementDef { owner: k as u8, kind: cfg.fort.altar.clone(), at: [origin.x + altar_at.x, origin.y + altar_at.y], spell: None, frame: None });
        map.structures.push(PlacementDef { owner: k as u8, kind: temple_kind, at: [origin.x + temple_at.x, origin.y + temple_at.y], spell: None, frame: None });
        map.units.push(UnitDef { owner: k as u8, kind: cfg.fort.priest.clone(), at: [origin.x + priest_at.x, origin.y + priest_at.y], move_to: None });
        taken.push((origin.x - g.geyser_spacing, origin.y - g.geyser_spacing, origin.x + fw + g.geyser_spacing, origin.y + fh + g.geyser_spacing));
        if k == 0 {
            map.camera = [origin.x + mid.x, origin.y + mid.y];
        } else {
            map.opponents.push(OpponentDef { owner: k as u8, ..OpponentDef::default() });
        }
    }
    // Geyser islands and islets anywhere inside the ring's reach that
    // keeps clear of everything placed so far.
    let place = |rng: &mut Rng, w: i32, h: i32, taken: &mut Vec<(i32, i32, i32, i32)>| -> Option<Cell> {
        let reach = g.ring_radius + fh;
        for _ in 0..200 {
            let x = centre.x - reach + rng.below((2 * reach - w).max(1) as u32) as i32;
            let y = centre.y - reach + rng.below((2 * reach - h).max(1) as u32) as i32;
            if x < 1 || y < 1 || x + w >= world.x - 1 || y + h >= world.y - 1 {
                continue;
            }
            let free = taken.iter().all(|&(x0, y0, x1, y1)| x + w + g.geyser_spacing <= x0 || x >= x1 + g.geyser_spacing || y + h + g.geyser_spacing <= y0 || y >= y1 + g.geyser_spacing);
            if free {
                taken.push((x, y, x + w, y + h));
                return Some(Cell::new(x, y));
            }
        }
        None
    };
    let gf = foot(&cfg.fort.geyser);
    for _ in 0..players * g.geysers_per_player {
        let (w, h) = (gf.0.max(3), gf.1.max(3));
        if let Some(o) = place(&mut rng, w, h, &mut taken) {
            let cells = (0..h).flat_map(|dy| (0..w).map(move |dx| [o.x + dx, o.y + dy])).collect();
            map.islands.push(IslandDef { owner: None, theme: g.geyser_theme.clone(), origin: [o.x, o.y], size: [w, h], remove: Vec::new(), cells });
            map.structures.push(PlacementDef { owner: 0, kind: cfg.fort.geyser.clone(), at: [o.x + w - 1, o.y + h - 1], spell: None, frame: None });
        }
    }
    for _ in 0..g.islets {
        let (w, h) = (g.islet[0], g.islet[1]);
        if let Some(o) = place(&mut rng, w, h, &mut taken) {
            let island = blob(&mut rng, w, h, g.fill, &[]);
            let cells = island.cells().map(|c| [c.x + o.x, c.y + o.y]).collect();
            map.islands.push(IslandDef { owner: None, theme: g.geyser_theme.clone(), origin: [o.x, o.y], size: [w, h], remove: Vec::new(), cells });
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blobs_are_seeded_connected_and_keep_their_cells() {
        let keep = [Cell::new(3, 3), Cell::new(28, 40)];
        let a = blob(&mut Rng::new(7), 32, 48, 0.7, &keep);
        let b = blob(&mut Rng::new(7), 32, 48, 0.7, &keep);
        let c = blob(&mut Rng::new(8), 32, 48, 0.7, &keep);
        assert_eq!(a, b, "the same seed gives the same island");
        assert_ne!(a, c, "another seed another island");
        assert!(keep.iter().all(|k| a.contains(*k)), "kept cells are land");
        assert!(a.len() >= 32 * 48 / 4, "a fair share of the canvas is land: {}", a.len());
        assert!(a.cells().all(|c| c.x >= 0 && c.y >= 0 && c.x < 32 && c.y < 48));
        // Connected: a walk from the middle reaches every cell.
        let mut seen = BTreeSet::new();
        let mut stack = vec![Cell::new(15, 24)];
        while let Some(c) = stack.pop() {
            if !a.contains(c) || !seen.insert(c) {
                continue;
            }
            stack.extend([c.offset(1, 0), c.offset(-1, 0), c.offset(0, 1), c.offset(0, -1)]);
        }
        assert_eq!(seen.len(), a.len(), "every cell is reachable from the middle");
    }

    #[test]
    fn seats_go_round_the_ring_from_the_top() {
        let c = Cell::new(100, 100);
        assert_eq!(seat(c, 40, 0, 2), Cell::new(100, 60));
        assert_eq!(seat(c, 40, 1, 2), Cell::new(100, 140));
        assert_eq!(seat(c, 40, 1, 4), Cell::new(140, 100));
    }

    #[test]
    fn skirmish_maps_seat_every_player_apart() {
        let cfg = Config::parse(include_str!("../../../data/rules.toml")).unwrap();
        let foot = |kind: &str| match kind {
            "dais" => (7, 7),
            "residence" => (4, 4),
            k if k.ends_with("vortex") => (8, 6),
            _ => (3, 3),
        };
        let m = skirmish(&cfg, 42, 3, &foot);
        assert_eq!(m.opponents.len(), 2);
        let forts: Vec<&IslandDef> = m.islands.iter().filter(|i| i.owner.is_some()).collect();
        assert_eq!(forts.len(), 3);
        for (a, b) in [(0, 1), (1, 2), (0, 2)] {
            let (x, y) = (forts[a].origin, forts[b].origin);
            let apart = (x[0] - y[0]).abs() >= forts[a].size[0] || (x[1] - y[1]).abs() >= forts[a].size[1];
            assert!(apart, "fortress canvases do not overlap");
        }
        assert!(m.islands.iter().filter(|i| i.owner.is_none()).count() >= 3 * cfg.generate.geysers_per_player / 2, "geyser islands found room");
        assert_eq!(m.units.len(), 3, "one High Priest each");
        assert_eq!(skirmish(&cfg, 42, 3, &foot), m, "seeded: the same map again");
    }
}
