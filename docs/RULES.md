# Game rules as read from the data

Islefall derives its rules from the `typeflags` and properties of the
original `.type` files, cross-checked against the game's own manual
(`help/GAME.HLP`, a WinHelp file readable after decompiling it with
`helpdeco`). This file records how each flag is interpreted and how sure
that reading is. The numbers live in `data/rules.toml` and the formulas in
`data/scripts/rules.rhai`; see `docs/MODDING.md`. The implementation lives in `crates/islefall-sim/src/rules.rs`,
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

- `walker`, `flyer` and `balloon` mark mobile types; those without `priest`
  are Transports. Flyers and balloons are not yet special: they walk.
- Facing uses the eight walk animations `A` to `H`: north, north-east, east,
  south-east, south, south-west, west, north-west.
- Units have `maxHitPoints` and `threat`; shooters prefer the highest
  threat, which makes the High Priest (25) the favourite target.

## The High Priest

From the manual: bringing a priest's health to the half-way point stuns
him; a stunned priest can be captured by any Transport; a Transport carries
him to the centre of your Altar, secures him and walks away; the sacrifice
grants Knowledge; a priest near his own Temple regenerates.

Islefall: a priest at or below half health is stunned and takes no orders.
A Transport ordered to capture walks next to him and picks him up; the
priest then follows the carrier and is not shot at. Ordered to sacrifice,
the carrier walks to the altar's centre cell, the priest dies and the
player's Knowledge counter rises by one. A priest within 8 cells of an own
Temple heals two points per second and leaves the stun above half health.
The heal range and rate are Islefall's guesses; Knowledge does not yet
unlock anything.

Not yet modelled: the priest floating when his bridge is blown, capture by
aerial Transports, and what each sacrifice actually teaches.

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

## Energy

The manual: Temples and Generators each produce one unit of Energy in a
fixed-range circle; placing a unit needs the right Energy at the site; Sun
Energy means any kind; operation needs none. Examples given: a Level One
Bulf needs one Thunder, a Level Two Whirlibase two Sun, a Level Three
Thunder Cannon two Thunder and one Sun, an Air Ship two Wind and one Sun.

Islefall reads a type's requirement from `mana` when present (`s` any, `w`,
`r`, `t` themed, one unit per letter) and otherwise from `level` and
`theme` for types that have a `class`: a Sun type needs `level` units of
any kind; a themed type needs one of its theme at level one, and its theme
for all but one unit from level two up. Types without a class (trees, the
altar, geysers) need none. Generators (`class` "Source of Energy") radiate
their theme; the Temple (`residence`) radiates Sun. The circle radius, 128
source pixels, is a guess. Only the local player is checked; enemies place
freely for now.

## Combat

From the type properties and the manual:

- A shooting type has `range` (cells), `hpPerSec` (damage per second) and
  `delayBetweenShots` (seconds, 1 when absent); one shot deals
  `hpPerSec x delayBetweenShots`. Sun Cannon: range 22, 80 damage every 5
  seconds. Sun Disc Thrower: range 8, 12 damage every second.
- Cannons fire only straight north, south, east or west (the manual); no
  flag says so, so Islefall applies it to types whose name contains
  "cannon". Everything else fires in any direction.
- Targets are enemies within range, highest `threat` first, nearest second.
  Units count below any structure. Range is measured from the footprint
  edge in Chebyshev distance.
- `maxHitPoints` is the health of structures and units; 0 means the type
  cannot be damaged (terrain and effects).
- A destroyed structure explodes: everything within 2 cells takes 150
  damage, which can chain; cracked bridges within 2 cells are destroyed and
  normal ones crack; then unsupported ground falls. The manual says
  "when shooting units explode, they also damage every other unit nearby";
  the radius and damage figures are Islefall's guesses.
- Salvaging one of your own structures refunds 25% of its cost scaled by
  its remaining health, and shocks bridges like an explosion but damages
  nothing else (the manual).

## Air

The manual: Aerial Transports (Balloon, Air Ship, Cloud Floater) float
over the clouds and travel anywhere, harvesting and capturing like Golems.
Air Attack Bases (Whirlibase, Devil Maker, Man o'War Pool) launch aerial
attackers and make a new one each time theirs is lost; the base's range is
the distance at which it detects an enemy and sends its attacker. A
Whirligig destroys target after target, refuels at its base once a minute,
never targets a Transport and soon crashes if its base is destroyed. A
Dust Devil lasts ten seconds, returns every twenty and cracks the bridges
it crosses. A Man o'War lives a minute, prefers Ground Transports, and
each kill feeds it for another minute. The Vander Tower is a short-range
weapon that fires at airborne units only. Attackers cost no Storm Power
and refund none.

Islefall: `flyer` types are attackers and `balloon` types Aerial
Transports; both fly straight lines, need no ground, never fall and are
hit only by weapons with `airrange`/`airdamage` (Sun Disc Thrower 12
cells, Crossbow 16, Vander Tower 18), whose air shot deals
`airdamage x delayBetweenShots`. The `air_attack` hook reads a type's
class and air properties: "Anti-Air" shoots only upwards, a Shooter with
air damage both ways. Bases (`air.base_classes`) launch the attacker
`air.launches` names for them, since the data does not, after
`air.respawn_seconds`. An attacker hunts the nearest enemy within its
`range` of the base (the `air_target_priority` hook decides preference
and refusals), strikes within `air.strike_range` cells for one shot of its
`hpPerSec`, and lives `life_seconds` from `[air.attackers]`; whether it
refuels, hunts Transports, cracks bridges or feeds on kills is set there
too. Balloons are Battle units: they cost, need Energy and production,
and are placed anywhere.

Not yet modelled: the Cloud Floater's one-in-twenty hit chance, the
floating priest, unit-versus-unit ground combat (no walker shoots).

## Workshops and production

The manual: Workshops build the units of battle; a Level One Workshop has
two production slots; to create a unit other than a Golem or a bridge piece
you put its Knowledge into production at a Workshop; units must be aligned
with the Workshop; a destroyed Workshop loses what it was producing; the
Temple gives the power to create bridges and Golems.

Islefall: types with the `factory` flag are Workshops with
`production.workshop_slots` slots. Placing a Battle unit (any type that
needs Energy) requires it to be in production at one of the owner's
Workshops; Temple-provided types (`production.temple_types`, the Golem)
need an own Temple instead. The `workshop_can_produce` hook decides
alignment: the tutorial's Sun Workshop produces a Wind Generator, so only
units above level one must match the Workshop's theme. Picking a building
tool puts the type into the first able Workshop with a free slot. Build
time (`constructionRate`) and the Storm Power Stream are not modelled.

## The opponent

A first computer player, in `crates/islefall-sim/src/ai.rs`, moves every
few seconds: it lays the piece and rotation from its own random queue that
brings a bridge closest to the player's altar, attaching only to its own
island edges and open bridge ends, and every third move drops a Sun Disc
Thrower in the sky at the open end nearest the target. It pays its own Storm
Power and sends idle Transports to harvest, but needs no Energy yet.

## Ownership

Islands, platforms and bridge cells carry an owner. Following the manual:
a piece must touch the builder's own island edge or open bridge end, and
may also touch other players' ground, which is how you connect to an enemy
island or enemy open end without being able to build off them. A drop on
land needs every footprint cell to be the dropper's; a drop in the sky needs
the touched open end to be the dropper's. Neutral islands and the Outpost
rule for building off them are not modelled yet.

## Knowledge

Types with a `techBit` need that Knowledge before their owner can build
them; types without one are known from the start. Each sacrifice grants the
lowest bit the owner lacks among the shipped types, which is Islefall's
ordering, not the game's: the manual only says the Furies grant Knowledge.

## Open questions

- Whether `yuckWalk` blocks or merely deters.
- What `mayDropOnIsle` means exactly.
- The real bridge piece catalogue.
- Explosion radius and damage; `damageEffect` is always 1 in the data and
  may select a visual effect rather than a damage class.
