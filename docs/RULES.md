# Game rules as read from the data

Islefall derives its rules from the `typeflags` and properties of the
original `.type` files rather than hardcoding per-unit behaviour. This file
records how each flag is interpreted and how sure that reading is. The
implementation lives in `crates/islefall-sim/src/rules.rs` and `world.rs`.

## Map

- The world is a rectangular grid of 16 x 11 pixel cells. Islands are sets
  of cells; bridges are single cells; structures occupy `foot_x` x `foot_y`
  cells with the type's hotspot on the bottom-right cell.
- Ground is any island cell, platform cell or bridge cell. Everything else
  is sky.

## Walking

| Flag | Reading | Confidence |
|------|---------|------------|
| `walkBlocking` | Units cannot enter the footprint. Only `edgefarm` has it. | High |
| `yuckWalk` | Units may cross but avoid it: path cost x5. Factories, trees, ruins, monuments, outposts, vortexes and residences have it. | Medium: it may mean fully blocked. |
| none (emplacements, the altar) | Freely walkable. Priests must reach the altar, and 3x3 emplacements sit on 1-cell bridges that units travel along. | Medium |

Paths are found with A* over eight neighbours; diagonal steps never cut the
corner of a blocked cell. Movement is fixed point (256 steps per cell) at
30 ticks per second, with `speed` read as cells per second.

## Bridges

- A bridge cell can be placed on sky orthogonally adjacent to ground.
- A bridge cell's tile is chosen by which of its four neighbours are ground;
  see the letter table in `FORMATS.md`.
- Where a bridge meets island land, the island's rim cell is overlaid with a
  `bridgeconnector` frame (island north, east, south, west of the bridge).

Not yet modelled: NetStorm's bridge pieces (several cells dropped as one
tetromino-like shape), `cracked` and `hard` bridge states, and bridge
destruction.

## Dropping structures

A structure may be dropped when every footprint cell is ground, no cell is
already covered by a structure, no unit stands on a cell, and, unless the
type has `mayDropOnRim`, no cell is a natural island's edge cell.

| Flag | Reading | Confidence |
|------|---------|------------|
| `dropBlocking` | Nothing may later be dropped onto the footprint. Currently every structure blocks drops, so the flag is recorded but not yet distinguishing. | Medium |
| `createsisland` | Bridge cells under the footprint become island ground (a platform) and the bridge tiles there disappear. Every 3x3 emplacement has it. | High for the effect, medium for the exact extent. |
| `mayDropOnRim` | The footprint may include island rim cells. | Medium |
| `mayDropOnIsle` | Not yet used. Probably: may be dropped on platforms created by other structures. | Low |
| `emplacement`, `factory`, `fence`, `tree`, `vortex`, `dais` | Category markers, no rule attached yet. | - |

## Units

- `walker`, `flyer` and `balloon` mark mobile types. Flyers and balloons are
  not yet special: they walk.
- Facing uses the eight walk animations `A` to `H`: north, north-east, east,
  south-east, south, south-west, west, north-west.

## Open questions

- Whether `yuckWalk` blocks or merely deters.
- What `mayDropOnIsle` and `mayDropOnRim` mean exactly for the home island.
- Hit points, threat, range, cost, mana and the geyser economy are parsed
  but unused.
