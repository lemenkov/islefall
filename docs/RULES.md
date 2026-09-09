# Game rules as read from the data

Islefall derives its rules from the `typeflags` and properties of the
original `.type` files, cross-checked against the game's own manual
(`help/GAME.HLP`, a WinHelp file readable after decompiling it with
`helpdeco`). This file records how each flag is interpreted and how sure
that reading is. The implementation lives in `crates/islefall-sim/src/rules.rs`,
`pieces.rs` and `world.rs`.

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

From the manual and the tutorial texts:

- Bridges are dropped as multi-cell pieces from a Production window that
  offers 2, 4 or 6 random pieces (keys Q, W, A, S, Z, X); a used piece is
  replaced by a new random one. The shipped catalogue of shapes is not in
  the data, so `pieces.rs` carries a plausible set.
- A piece must attach to the edge of an island or to the open end of another
  bridge. Islefall reads "open end" as a bridge cell with exactly one
  connection.
- A bridge with no connection to an island cracks from its own weight and
  then crumbles. Islefall cracks unsupported cells at once and drops them
  four seconds later; platforms and the structures on them fall immediately.
- Destroying a bridge cell destroys adjacent cracked cells too (the manual's
  "shock"). Explosions of units nearby crack un-cracked bridges; not modelled
  yet since there is no combat.
- Hardened bridges (`hard` frames, the Bridge Harden spell) do not crack from
  damage but still crumble when unsupported.
- A bridge cell's tile is chosen by which of its four neighbours are ground;
  see the letter table in `FORMATS.md`. Where a bridge meets island land,
  the island's rim cell is overlaid with a `bridgeconnector` frame.

Not yet modelled: an Edge Farm blocking bridges off an island edge, bridge
ownership (you may connect to enemy open ends but not build off them), and
a floating priest waiting for a bridge to be rebuilt under him.

## Dropping structures

The manual: "Units may only be built on islands you own, off of the ends
of friendly bridges, or on Neutral Islands. Buildings, by virtue of their
tremendous weight, can only be constructed on islands", and "place the unit
in the sky, just off the end of a bridge piece. You cannot place units
directly on top of the bridges themselves."

So a `createsisland` type (every 3x3 emplacement) is dropped either wholly on
island ground or wholly in the sky with a footprint cell orthogonally
adjacent to an open bridge end; in the sky it creates its own island under
itself. Everything else needs island ground under every cell. In both cases
no cell may be covered by another structure or a unit, and unless the type
has `mayDropOnRim`, no cell may be a natural island's edge.

| Flag | Reading | Confidence |
|------|---------|------------|
| `createsisland` | May be placed in the sky at a bridge end; the footprint becomes a platform island. | High |
| `dropBlocking` | Nothing may later be dropped onto the footprint. Currently every structure blocks drops, so the flag is recorded but not yet distinguishing. | Medium |
| `mayDropOnRim` | The footprint may include island rim cells. | Medium |
| `mayDropOnIsle` | Not yet used. Probably: may be dropped on platforms created by other structures. | Low |
| `emplacement`, `factory`, `fence`, `tree`, `vortex`, `dais` | Category markers, no rule attached yet. | - |

Ownership, Storm Power costs and the Energy requirement ("the proper Energy
influencing the space") are not modelled yet.

## Units

- `walker`, `flyer` and `balloon` mark mobile types. Flyers and balloons are
  not yet special: they walk.
- Facing uses the eight walk animations `A` to `H`: north, north-east, east,
  south-east, south, south-west, west, north-west.

## Storm Power

Storm Power is the currency (the manual). Islefall models:

- A Storm Geyser (`geyser`, flag `geyser`) holds its `cost`, 2000, in
  crystals. Geysers are placed by the map and cost nothing.
- A Storm Crystal (`nugget`) is worth its `cost`, 200, so a Transport carries
  200 per trip. Harvesting takes one second next to the geyser, delivering
  one second next to the Temple; the unit repeats until the geyser is empty,
  when it becomes an `emptygeyser`.
- The Temple is read as the `residence` type (flag `residence`); crystals
  handed in there become Storm Power.
- Dropping a structure costs its `cost` property; bridge pieces and golems
  are free, as in the manual ("the Temple gives you the power to create
  bridges and Golems").

Not yet modelled: harvesting by aerial Transports without bridges, the 25%
refund for destroying enemy units, Energy requirements from Temples and
Generators, Workshops putting Knowledge into production, and capturing and
sacrificing enemy High Priests.

## Open questions

- Whether `yuckWalk` blocks or merely deters.
- What `mayDropOnIsle` means exactly.
- The real bridge piece catalogue.
