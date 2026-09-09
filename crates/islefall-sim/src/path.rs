// SPDX-License-Identifier: Apache-2.0
//! Grid path search: A* over eight neighbours with integer costs.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::grid::Cell;

/// Cost of a straight step; diagonals cost [`DIAGONAL`].
const STRAIGHT: u32 = 10;
const DIAGONAL: u32 = 14;
/// Give up after this many expansions so a bad request cannot stall a tick.
const MAX_EXPANSIONS: usize = 200_000;

const NEIGHBOURS: [(i32, i32); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

fn heuristic(a: Cell, b: Cell) -> u32 {
    let dx = (a.x - b.x).unsigned_abs();
    let dy = (a.y - b.y).unsigned_abs();
    let (lo, hi) = (dx.min(dy), dx.max(dy));
    DIAGONAL * lo + STRAIGHT * (hi - lo)
}

/// Shortest path from `from` to `to` over cells where `walkable` holds,
/// excluding `from` and including `to`. Diagonal moves may not cut the
/// corner of a blocked cell. `None` if unreachable.
pub fn find_path(from: Cell, to: Cell, walkable: impl Fn(Cell) -> bool) -> Option<Vec<Cell>> {
    if from == to {
        return Some(Vec::new());
    }
    if !walkable(to) {
        return None;
    }
    let mut open = BinaryHeap::new();
    let mut best: HashMap<Cell, u32> = HashMap::new();
    let mut came_from: HashMap<Cell, Cell> = HashMap::new();
    best.insert(from, 0);
    open.push(Reverse((heuristic(from, to), 0u32, from.x, from.y)));
    let mut expansions = 0;
    while let Some(Reverse((_, g, x, y))) = open.pop() {
        let cur = Cell::new(x, y);
        if cur == to {
            let mut path = vec![to];
            let mut c = to;
            while let Some(&p) = came_from.get(&c) {
                if p == from {
                    break;
                }
                path.push(p);
                c = p;
            }
            path.reverse();
            return Some(path);
        }
        if g > best.get(&cur).copied().unwrap_or(u32::MAX) {
            continue; // stale entry
        }
        expansions += 1;
        if expansions > MAX_EXPANSIONS {
            return None;
        }
        for (dx, dy) in NEIGHBOURS {
            let next = cur.offset(dx, dy);
            if !walkable(next) {
                continue;
            }
            if dx != 0 && dy != 0 && !(walkable(cur.offset(dx, 0)) && walkable(cur.offset(0, dy))) {
                continue; // no corner cutting
            }
            let step = if dx != 0 && dy != 0 { DIAGONAL } else { STRAIGHT };
            let ng = g + step;
            if ng < best.get(&next).copied().unwrap_or(u32::MAX) {
                best.insert(next, ng);
                came_from.insert(next, cur);
                open.push(Reverse((ng + heuristic(next, to), ng, next.x, next.y)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_field(c: Cell) -> bool {
        (0..10).contains(&c.x) && (0..10).contains(&c.y)
    }

    #[test]
    fn straight_and_diagonal() {
        let p = find_path(Cell::new(0, 0), Cell::new(3, 0), open_field).unwrap();
        assert_eq!(p, vec![Cell::new(1, 0), Cell::new(2, 0), Cell::new(3, 0)]);
        let p = find_path(Cell::new(0, 0), Cell::new(3, 3), open_field).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p.last(), Some(&Cell::new(3, 3)));
        assert_eq!(find_path(Cell::new(2, 2), Cell::new(2, 2), open_field), Some(Vec::new()));
    }

    #[test]
    fn routes_around_a_wall_without_cutting_corners() {
        // Wall at x=5 from y=0..8, gap at y=9.
        let walkable = |c: Cell| open_field(c) && !(c.x == 5 && c.y < 9);
        let p = find_path(Cell::new(2, 2), Cell::new(8, 2), walkable).unwrap();
        assert!(p.iter().all(|&c| walkable(c)));
        assert!(p.iter().any(|&c| c.y == 9), "must use the gap");
        // Every diagonal step must have both orthogonal neighbours free.
        let mut prev = Cell::new(2, 2);
        for &c in &p {
            let (dx, dy) = (c.x - prev.x, c.y - prev.y);
            assert!(dx.abs() <= 1 && dy.abs() <= 1);
            if dx != 0 && dy != 0 {
                assert!(walkable(prev.offset(dx, 0)) && walkable(prev.offset(0, dy)));
            }
            prev = c;
        }
    }

    #[test]
    fn unreachable() {
        let walkable = |c: Cell| open_field(c) && c.x != 5;
        assert_eq!(find_path(Cell::new(1, 1), Cell::new(8, 8), walkable), None);
        assert_eq!(find_path(Cell::new(1, 1), Cell::new(50, 50), walkable), None);
    }
}
