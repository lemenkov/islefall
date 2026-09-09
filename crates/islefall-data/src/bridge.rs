// SPDX-License-Identifier: Apache-2.0
//! The bridge tile set (`bridge.type`).
//!
//! Bridges are one cell wide. A cell's tile is chosen by which of its four
//! neighbours it connects to, giving 15 shapes: the letters `A` to `O` in
//! the type file. Each letter has a normal frame (number 1, sometimes 2),
//! `cracked` frames (11, 12) and one `hard` frame (20). `P00` is the help
//! picture. The letter assignment below was read from the rail positions
//! in the sprites: a rail runs along every side that is not connected.

use crate::typefile::TypeDef;

/// Connection bits of a bridge cell.
pub const NORTH: u8 = 1;
pub const EAST: u8 = 2;
pub const SOUTH: u8 = 4;
pub const WEST: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Condition {
    Normal,
    Cracked,
    Hard,
}

/// Animation label for a connection mask (any of the 16 values).
/// An unconnected cell falls back to the cross so it still draws.
pub fn label(mask: u8) -> &'static str {
    match mask & 0xF {
        m if m == NORTH | EAST | SOUTH | WEST => "A",
        m if m == NORTH | EAST | SOUTH => "B",
        m if m == EAST | SOUTH | WEST => "C",
        m if m == NORTH | SOUTH | WEST => "D",
        m if m == NORTH | EAST | WEST => "E",
        m if m == EAST | SOUTH => "F",
        m if m == SOUTH | WEST => "G",
        m if m == NORTH | WEST => "H",
        m if m == NORTH | EAST => "I",
        m if m == NORTH | SOUTH => "J",
        m if m == EAST | WEST => "K",
        m if m == SOUTH => "L",
        m if m == WEST => "M",
        m if m == NORTH => "N",
        m if m == EAST => "O",
        _ => "A",
    }
}

/// Frame indices (into `def.frames`) for a connection mask and condition.
/// Several frames mean variations; pick one per cell.
pub fn frames(def: &TypeDef, mask: u8, condition: Condition) -> Vec<usize> {
    let want = |n: u32| match condition {
        Condition::Normal => (1..=9).contains(&n),
        Condition::Cracked => (11..=19).contains(&n),
        Condition::Hard => n == 20,
    };
    let l = label(mask);
    def.frames.iter().enumerate().filter(|(_, f)| f.animation.eq_ignore_ascii_case(l) && want(f.number)).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_distinct() {
        let mut seen = std::collections::BTreeSet::new();
        for m in 1..16u8 {
            assert!(seen.insert(label(m)), "mask {m} reuses {}", label(m));
        }
        assert_eq!(label(0), "A");
        assert_eq!(label(NORTH | SOUTH), "J");
    }

    #[test]
    fn real_bridge_if_available() {
        let Ok(dir) = std::env::var("NETSTORM_DIR") else {
            eprintln!("NETSTORM_DIR not set; skipping real-file test");
            return;
        };
        let inst = crate::Installation::load(dir).unwrap();
        let def = inst.type_def("bridge").unwrap();
        for m in 1..16u8 {
            for c in [Condition::Normal, Condition::Cracked, Condition::Hard] {
                assert!(!frames(def, m, c).is_empty(), "mask {m} {c:?} has no frame");
            }
        }
        assert_eq!(frames(def, EAST | WEST, Condition::Normal).len(), 2);
        assert_eq!(frames(def, NORTH | EAST | SOUTH | WEST, Condition::Hard).len(), 1);
    }
}
