# Modding Islefall

Islefall keeps no game rule in its code. Everything that is a number, a
name, a key or a decision comes from the `data/` directory, which a mod can
copy and edit; point `ISLEFALL_DATA` at the copy to use it.

| File | Holds |
|------|-------|
| `data/rules.toml` | Every parameter: tick rate, grid, walking costs, which type flags mean what, the bridge piece catalogue, economy figures, combat, priest healing, Energy range, the opponent's pacing, and controls (keys, tools, zoom, palette). |
| `data/scripts/rules.rhai` | Decision hooks in [Rhai](https://rhai.rs): Energy requirements from level and theme, which types fire straight, damage per shot, salvage refunds, which Knowledge a sacrifice grants, target priority. |
| `data/maps/*.toml` | Scenes: islands with owner and theme, map-laid bridges, structures, units with a first move order, opponents, starting Storm Power and camera. `ISLEFALL_MAP=name` picks one. |

The original NetStorm data stays where it is; unit statistics (cost, hit
points, range, speed, footprints, flags, frames) are read from its `.type`
files, and the rules file only says how to interpret them.

## rules.toml

The file is validated on load with unknown keys rejected, so a typo fails
fast rather than silently using a default. Comments in the file explain
each value and mark the ones that are guesses rather than figures from the
game's manual.

Changing a flag list under `[flags]` changes which types the simulation
treats as walk-blocking, island-creating, geysers, Temples and so on.
Adding a piece to `[bridges].pieces` puts it into every player's queue.

## rules.rhai

Hooks are plain functions called by the simulation with simple arguments
and must return the documented type. They run in a sandbox without file,
time or random access, so they are deterministic: the same inputs always
give the same result, which matters for replays and lockstep multiplayer.
A hook that fails is logged and the calling rule falls back to a harmless
value (zero damage, no Energy need, no Knowledge). The script is compiled
once at start-up; a syntax error stops the game with the message.

Available hooks:

- `energy_need(level, theme, mana, has_class)` -> `#{ theme, themed, any }` or `()`
- `fires_straight(name, flags)` -> bool
- `damage_per_shot(hp_per_sec, delay_seconds)` -> int
- `salvage_refund(cost, hp, max_hp, refund_percent)` -> int
- `knowledge_grant(known_bits, all_bits)` -> int or `()`
- `target_priority(threat, distance, is_unit)` -> int, higher wins

## Maps

Cells are `[x, y]` with y growing downwards. A structure's position is its
hotspot cell, the bottom-right cell of its footprint. The layout is placed
with costs and Energy switched off, then the reserves are set to
`start_power` and the rules apply.

## What is still in code

Facts about NetStorm's file formats (the sprite container order, the RLE
opcodes, the palette file) live in `crates/islefall-data`, because they
describe the files rather than the game. Rendering details such as draw
order and overlay colours live in the Bevy crate.
