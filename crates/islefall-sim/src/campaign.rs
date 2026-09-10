// SPDX-License-Identifier: Apache-2.0
//! The original's campaign scenarios as maps: the geyser and extra
//! islands, bridges and structures of a `.fort` file's world table, and
//! its players' fortresses seated round them on islands of our own
//! making, since the file holds neither a fortress's shape nor its place
//! (see `islefall_data::fort`). The mission text supplies start money and
//! each computer player's technologies.

use std::collections::{BTreeMap, BTreeSet};

use islefall_data::fort::{self, Fort, Item};
use islefall_data::mission::Mission;

use crate::config::Config;
use crate::generate::{blob, footprint, seat, Rng};
use crate::grid::Cell;
use crate::map::{BridgeDef, IslandDef, MapDef, OpponentDef, PlacementDef, UnitDef};

/// What the conversion could not place, for the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub unknown_codes: BTreeMap<u8, usize>,
    pub notes: Vec<String>,
}

fn code_key(code: u8) -> String {
    format!("{code:02x}")
}

/// Convert a scenario. `foot` gives a type's footprint for shaping the
/// fortress islands round their buildings.
pub fn fort_to_map(fortfile: &Fort, mission: Option<&Mission>, cfg: &Config, name: &str, foot: &dyn Fn(&str) -> (i32, i32)) -> (MapDef, Report) {
    let fc = &cfg.fort;
    let mut report = Report::default();
    let mut map = MapDef { name: mission.and_then(|m| m.get("title")).unwrap_or(name).to_string(), start_power: mission.and_then(|m| m.int("mystartmoney")).map(|p| p as i32).unwrap_or(fc.start_power), camera: [0, 0], ..MapDef::default() };

    // Who sits where: the seat the file was saved from is the player, the
    // others become opponents in file order.
    let mut seats: Vec<(usize, u8)> = Vec::new(); // (fortress index, file player number)
    for (k, f) in fortfile.fortresses.iter().enumerate() {
        let number = f.items.iter().filter(|i| i.code == fort::CODE_TEMPLE || fort::CODE_ALTARS.contains(&i.code)).find_map(Item::owner).unwrap_or(k as u8 + 1);
        seats.push((k, number));
    }
    let local = fortfile.fortresses.iter().position(|f| f.items.iter().any(Item::is_local_temple)).unwrap_or(0);
    let mut ours: BTreeMap<u8, u8> = BTreeMap::new();
    let mut order: Vec<usize> = vec![local];
    order.extend((0..fortfile.fortresses.len()).filter(|&k| k != local));
    for (our, &k) in order.iter().enumerate() {
        ours.insert(seats[k].1, our as u8);
    }
    let owner_of = |item: &Item| item.owner().and_then(|p| ours.get(&p).copied()).unwrap_or(0);

    // Islands of the world table, by flood fill over the ground cells.
    let mut ground: BTreeMap<Cell, u8> = BTreeMap::new();
    for (x, y, theme) in fortfile.ground() {
        ground.insert(Cell::new(x, y), theme);
    }
    let mut unvisited: BTreeSet<Cell> = ground.keys().copied().collect();
    let mut island_of: BTreeMap<Cell, usize> = BTreeMap::new();
    while let Some(&start) = unvisited.iter().next() {
        let mut cells = Vec::new();
        let mut stack = vec![start];
        while let Some(c) = stack.pop() {
            if !unvisited.remove(&c) {
                continue;
            }
            cells.push(c);
            stack.extend([c.offset(1, 0), c.offset(-1, 0), c.offset(0, 1), c.offset(0, -1)].into_iter().filter(|n| unvisited.contains(n)));
        }
        let theme_byte = ground[&start] as usize;
        let theme = fc.ground_themes.get(theme_byte).cloned().unwrap_or_else(|| "sun".to_string());
        let (x0, y0) = (cells.iter().map(|c| c.x).min().unwrap_or(0), cells.iter().map(|c| c.y).min().unwrap_or(0));
        let (x1, y1) = (cells.iter().map(|c| c.x).max().unwrap_or(0), cells.iter().map(|c| c.y).max().unwrap_or(0));
        let index = map.islands.len();
        for c in &cells {
            island_of.insert(*c, index);
        }
        map.islands.push(IslandDef { owner: None, theme, origin: [x0, y0], size: [x1 - x0 + 1, y1 - y0 + 1], remove: Vec::new(), cells: cells.iter().map(|c| [c.x, c.y]).collect() });
    }

    // Everything else in the world table.
    let mut bridges: BTreeMap<u8, Vec<[i32; 2]>> = BTreeMap::new();
    for item in &fortfile.items {
        let at = Cell::new(item.x, item.y);
        match item.code {
            fort::CODE_GROUND => {}
            c if fort::CODE_BRIDGES.contains(&c) => bridges.entry(fc.bridge_owner).or_default().push([item.x, item.y]),
            fort::CODE_GEYSER => map.structures.push(PlacementDef { owner: 0, kind: fc.geyser.clone(), at: [item.x, item.y], spell: None, frame: None }),
            _ => {
                let island = island_of.get(&at).copied();
                place_item(item, at, owner_of(item), fc, &mut map, &mut report, island);
            }
        }
    }
    for (owner, cells) in bridges {
        map.bridges.push(BridgeDef { owner, cells });
    }

    // Fortresses round a ring about the world table.
    let world = Cell::new(cfg.generate.world[0], cfg.generate.world[1]);
    let (centre, radius) = match fortfile.bounds() {
        Some((x0, y0, x1, y1)) => {
            let half = ((((x1 - x0) as f32).powi(2) + ((y1 - y0) as f32).powi(2)).sqrt() / 2.0) as i32;
            (Cell::new((x0 + x1) / 2, (y0 + y1) / 2), half + fc.ring_margin)
        }
        None => (Cell::new(world.x / 2, world.y / 2), fc.ring_margin),
    };
    let mut rng = Rng::new(fortfile.items.len() as u64 * 131 + fortfile.fortresses.len() as u64);
    let n = fortfile.fortresses.len();
    for (our, &k) in order.iter().enumerate() {
        let f = &fortfile.fortresses[k];
        let (cw, ch) = f.canvas_size();
        let s = seat(centre, radius, our, n);
        let origin = Cell::new((s.x - cw / 2).clamp(0, (world.x - cw).max(0)), (s.y - ch / 2).clamp(0, (world.y - ch).max(0)));
        let number = seats[k].1;
        let techs = if our == 0 { mission.map(|m| m.techs("mytech")).unwrap_or_default() } else { mission.map(|m| m.ai_techs(number as usize)).unwrap_or_default() };
        let theme = f
            .items
            .iter()
            .find(|i| fort::CODE_ALTARS.contains(&i.code))
            .and_then(|a| fc.altar_themes.get(&code_key(a.code)).cloned())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| {
                if techs.iter().any(|t| t.starts_with("wind")) && !techs.iter().any(|t| t.starts_with("sun") && !t.ends_with("walker")) {
                    "wind".to_string()
                } else if our == 0 {
                    fc.human_theme.clone()
                } else {
                    "sun".to_string()
                }
            });
        let temple_kind = fc.temples.get(&theme).cloned().unwrap_or_else(|| fc.temples.values().next().cloned().unwrap_or_default());
        // Buildings first, so the island can be shaped round them.
        let mut keep = Vec::new();
        let mut placements = Vec::new();
        let mut units = Vec::new();
        for item in &f.items {
            let at = Cell::new(item.x, item.y);
            let kind = if item.code == fort::CODE_TEMPLE {
                Some(temple_kind.clone())
            } else if fort::CODE_ALTARS.contains(&item.code) {
                Some(fc.altar.clone())
            } else if let Some(u) = fc.units.get(&code_key(item.code)) {
                units.push((u.clone(), at));
                keep.push(at);
                None
            } else {
                match fc.codes.get(&code_key(item.code)) {
                    Some(k) if k.is_empty() => None,
                    Some(k) => Some(k.clone()),
                    None => {
                        *report.unknown_codes.entry(item.code).or_default() += 1;
                        None
                    }
                }
            };
            if let Some(kind) = kind {
                keep.extend(footprint(at, foot(&kind)));
                placements.push((kind, at));
            }
        }
        let island = blob(&mut rng, cw, ch, cfg.generate.fill, &keep);
        map.islands.push(IslandDef { owner: Some(our as u8), theme: theme.clone(), origin: [origin.x, origin.y], size: [cw, ch], remove: Vec::new(), cells: island.cells().map(|c| [c.x + origin.x, c.y + origin.y]).collect() });
        for (kind, at) in placements {
            map.structures.push(PlacementDef { owner: our as u8, kind, at: [at.x + origin.x, at.y + origin.y], spell: None, frame: None });
        }
        for (kind, at) in units {
            map.units.push(UnitDef { owner: our as u8, kind, at: [at.x + origin.x, at.y + origin.y], move_to: None });
        }
        if our == 0 {
            map.camera = [origin.x + cw / 2, origin.y + ch / 2];
            map.player_tech = techs;
        } else {
            let shooter = techs.iter().find(|t| t.ends_with("cannon") || t.ends_with("archer")).cloned();
            let generator = techs.iter().find(|t| t.ends_with("battery")).cloned();
            let power = mission.and_then(|m| m.int(&format!("ai{number}startmoney")).or_else(|| m.int("aistartmoney"))).map(|p| p as i32).or(Some(fc.ai_power));
            map.opponents.push(OpponentDef { owner: our as u8, target: None, shooter, generator, knowledge: Vec::new(), tech: techs, power });
        }
    }
    if map.camera == [0, 0] {
        map.camera = [centre.x, centre.y];
    }
    report.notes.push(format!("{} islands from the world table, {} fortresses seated {} cells round {:?}", map.islands.len() - n, n, radius, centre));
    (map, report)
}

/// A world-table structure or unit; an island it stands on becomes its owner's.
fn place_item(item: &Item, at: Cell, owner: u8, fc: &crate::config::FortRules, map: &mut MapDef, report: &mut Report, island: Option<usize>) {
    let key = code_key(item.code);
    if let Some(u) = fc.units.get(&key) {
        map.units.push(UnitDef { owner, kind: u.clone(), at: [at.x, at.y], move_to: None });
        return;
    }
    let kind = if item.code == fort::CODE_TEMPLE {
        fc.temples.values().next().cloned().unwrap_or_default()
    } else if fort::CODE_ALTARS.contains(&item.code) {
        fc.altar.clone()
    } else {
        match fc.codes.get(&key) {
            Some(k) if k.is_empty() => return,
            Some(k) => k.clone(),
            None => {
                *report.unknown_codes.entry(item.code).or_default() += 1;
                return;
            }
        }
    };
    if let (Some(i), Some(_)) = (island, item.owner()) {
        map.islands[i].owner = Some(owner);
    }
    map.structures.push(PlacementDef { owner, kind, at: [at.x, at.y], spell: None, frame: None });
}

#[cfg(test)]
mod tests {
    use super::*;
    use islefall_data::fort::{encode, CODE_GEYSER, CODE_GROUND, CODE_PRIEST, CODE_TEMPLE};

    #[test]
    fn converts_a_scenario_into_islands_fortresses_and_opponents() {
        let cfg = Config::parse(include_str!("../../../data/rules.toml")).unwrap();
        let mut table = vec![Vec::new(); 256];
        for c in 0..3u8 {
            for r in 0..3u8 {
                table[100].push((c << 4 | r, CODE_GROUND, vec![0]));
            }
        }
        table[100].push((0x22, CODE_GEYSER, vec![0]));
        table[100].push((0x55, 0xfd, vec![0x29]));
        // Seat 2 is the file's own player, seat 1 an opponent with a Disc Thrower.
        let mine = vec![vec![(0x7d, 0x7b, vec![2, 1, 0x85, 0])], vec![(0x41, CODE_TEMPLE, vec![0x81, 2, 0]), (0x40, CODE_PRIEST, vec![2])]];
        let theirs = vec![vec![(0x7d, 0x7c, vec![1, 1, 0x85, 0]), (0x33, 0x47, vec![1])], vec![(0x41, CODE_TEMPLE, vec![1, 1, 0]), (0x40, CODE_PRIEST, vec![1])]];
        let f = Fort::parse(&encode(14, "Player", &table, &[theirs, mine])).unwrap();
        let mission = Mission::parse("[Header]\nmyStartMoney = 2000\nmyTech = \"sunArcher\"\nai1Tech = \"rainCannon;rainBattery\"\nai1StartMoney = 500\ntitle = \"Test\"\n");
        let foot = |k: &str| if k == "dais" { (7, 7) } else if k.ends_with("vortex") { (8, 6) } else { (3, 3) };
        let (m, report) = fort_to_map(&f, Some(&mission), &cfg, "test", &foot);
        assert_eq!(m.name, "Test");
        assert_eq!(m.start_power, 2000);
        assert_eq!(m.islands.len(), 3, "a geyser island and two fortresses");
        assert_eq!(m.islands[0].cells.len(), 9);
        assert!(m.structures.iter().any(|s| s.kind == cfg.fort.geyser && s.at == [66, 98]));
        assert_eq!(m.bridges.len(), 1);
        let mine_island = m.islands.iter().find(|i| i.owner == Some(0)).unwrap();
        let theirs_island = m.islands.iter().find(|i| i.owner == Some(1)).unwrap();
        assert_eq!(mine_island.theme, "wind", "the seat's temple (0x81) is a Wind Temple");
        assert_eq!(theirs_island.theme, "rain", "altar code 7c marks the Rain faction");
        assert!(m.units.iter().filter(|u| u.kind == "priest").count() == 2);
        assert_eq!(m.opponents.len(), 1);
        let op = &m.opponents[0];
        assert_eq!((op.owner, op.shooter.as_deref(), op.generator.as_deref(), op.power), (1, Some("raincannon"), Some("rainbattery"), Some(500)));
        assert!(m.structures.iter().any(|s| s.kind == "sunarcher" && s.owner == 1));
        assert!(report.unknown_codes.is_empty(), "{:?}", report.unknown_codes);
        // Every fortress building stands on its island.
        for s in m.structures.iter().filter(|s| s.kind != cfg.fort.geyser) {
            let island = m.islands.iter().find(|i| i.owner == Some(s.owner)).unwrap();
            assert!(island.cells.contains(&s.at), "{} at {:?} is on land", s.kind, s.at);
        }
    }
}
