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

Every structure's footprint is impassable, and no unit may be placed on
one (`walking.structures_block`): a Golem walks around a cannon, not over
it, and sacrifices from beside the altar. The flags then only refine it:

| Flag | Reading | Confidence |
|------|---------|------------|
| `walkBlocking` | Units cannot enter the footprint even with `structures_block` off. Only `edgefarm` has it. | High |
| `yuckWalk` | With `structures_block` off, units may cross but avoid it: path cost x5. Factories, trees, ruins, monuments, outposts, vortexes and residences have it. | Medium: it may mean fully blocked, which is the default reading now. |
| a `walk_free` flag | Units cross freely; no type in the data has one. | - |

A ground unit standing still blocks its cell (`walking.units_block`): no
unit may be placed there, and a walker about to enter it waits
`wait_seconds`, then finds a way round with standing units treated as
walls. Units on the move pass through each other, so two walkers meeting
on a one-cell bridge never jam; whether the original blocked units is not
known.

Paths are found with A* over eight neighbours; diagonal steps never cut the
corner of a blocked cell. Movement is fixed point (256 steps per cell) at
30 ticks per second, with `speed` read as cells per second.

## What stands still shows

The original's structures move: geysers spout, batteries turn, Workshops
work, the Disc Thrower spins its arm to throw and a cannon rises to fire.
The type files carry it all as frame labels: `A` frames loop for an idle,
and a Workshop's `A`, `B` and `C` are its three levels. Within a label,
one big picture followed by small frames means window lights drawn over
the picture (a Workshop), while frames all of a size are whole pictures
that loop (a battery turning). The Disc Thrower's 32 `P` frames are one
spin of its arm, small pictures drawn over the dome and played once per
throw, after which it rests as the dome alone; a cannon's `L`, `M`, `N`
and `O` frames are its north, east, south and west firing animations (the
letters are from the files' own comments), played once per shot before it
settles back to its default look. Types flagged
`randframe` never animate: they show one of their default frame's group,
chosen per cell unless the map pins one with `frame` (the original saved
the choice with `saveFrame`), so no two trees look alike and a Temple has
one of three layouts (a well in front, a round tower, a hut on the
left). The Temple's other frames are the same layouts on the grey, autumn and
snow grounds of the Thunder, Wind and Rain islands, and it takes the
ground of the island it stands on (`ground_label` and `ground_order`).
A missile's landing shows the original's small blast and a destroyed
structure its great fireball, both from the `anim` container, and a
structure under construction sparkles with `flare` frames now and then
(`[effects]`). A damaged structure burns: below half health flames stand
on its picture, wander and leave smoke, more and larger below a quarter.
The flames and the smoke are Islefall's generated art
(`effects.generated_fire`, shaped by `[effects.fire]`; `false` or `F4`
gives rising specks over the footprint instead), and
`effects.generated_blast` or `F6` swaps the great fireball for a
generated explosion. A map may open on a battlefield: `health` on a
structure is its starting health in per cent. An
Energy source's stars are the original's twinkling `range` frames in the
source's theme colour (`energy.star_type` and `star_labels`). A geyser's 49 idle frames are
three runs of the spout, full, half and low; `[animation].stages` says
so and the run shown follows the stock left. `[animation]` in the rules names the labels and
flags. Turrets keep their bearing: a cannon's direction label is its
firing sequence, played once per shot, and it rests on that direction's
first frame afterwards, so the Thunder Cannon keeps facing its target and
the Ice Cannon keeps its gun out. The Crossbow's hundred frames are a ring
of bearings, five firing frames at each cardinal point with turning frames
between; it swings along the ring only as far as the new target, a quarter
turn in `turn_seconds`, fires on arrival and stays there. The Sun Cannon's
frames are not in play order, so `fire_sequences` spells its shot out: it
rests with the barrel up (its default frame), lowers it towards the
target, fires and raises it again. The `range` map is a shooting range for
checking all this. A shooter stays on the target it is firing at while
that remains in range (the `target_priority` hook's `is_current`), so it
does not swing to every newcomer; and a structure's sprite survives
changes elsewhere in the world, so a turret keeps its bearing when the
opponent drops a building or something is destroyed. An Energy source's reach is shown as the original showed it: a ring
of small stars drifting round it, not a drawn circle.

## Maps

Skirmish maps are made up (`[generate]`): players' fortresses sit on a
ring round the middle of a 256-cell world, the first at the top, each an
island of 3 x 3 pieces like the original's, shaped round its Temple,
altar and High Priest; geyser islands and bare islets lie between. The
original's campaign scenarios load from the archive and are read by
`[fort]`; `docs/FORT.md` has the format and what of it is known.

## Pictures on the grid

A structure's picture has a hotspot (from the sprite cache) that the
engine puts `hotFootRatioX` and `hotFootRatioY` cells in from the
bottom-right corner of the footprint's hotspot cell; types that say
nothing sit on the corner. Read that way, every picture in the data comes
out centred on its footprint: a Generator's narrow picture with ratio
0.5, a Thunder Cannon's wide wings with 0.125, and the geyser's rocks
(0.22) stay inside their little island instead of running to its edge.

Islands have undersides, as the original's did: `fringe` stalactites hang
under the bottom rim cells of a big island (chosen by the rim's `isle`
piece label through `[fringe].pieces`, their hotspot `hang` cells below
the rim: the pieces' hotspots lie four cell rows below their tops, so at 4
the picture starts right under the rim tile), and a single
`islandstalag` rock hangs under every three-by-three island. Both are
drawn behind the ground.

## Bridges

From the manual and the tutorial texts:

- Bridges are dropped as multi-cell pieces from a Production window that
  offers 2, 4 or 6 random pieces (keys Q, W, A, S, Z, X); a used piece is
  replaced by a new random one. The shipped catalogue of shapes is not in
  the data, so `pieces.rs` carries a plausible set.
- A piece must attach to the edge of an island or to the open end of another
  bridge. Islefall reads "open end" as a bridge cell with exactly one
  connection.
- The manual: "Initially, bridges in the Production window appear
  cracked, but if not immediately used, they will quickly harden."
  Islefall: a piece is cracked for `bridges.harden_seconds` after it comes
  on offer (the panel shows it so) and a piece laid before then is laid
  cracked. Cracked cells die to one hit and take their cracked neighbours
  with them, so a bridge thrown out as fast as the window refills is
  fragile; this, not decay, is what keeps the original's bridges in
  check: an attached bridge never falls with time. Computer players are
  bound by it too.
- A bridge with no connection to an island cracks from its own weight and
  then crumbles (the manual: it "will eventually crack, then crumble and
  fall under its own weight"). Islefall: a stretch of bridge that loses
  its support holds sound for `bridges.hold_seconds`, cracks, holds
  cracked for `crumble_seconds`, and then falls section by section from
  the break outwards, `section_cells` cells every `section_seconds`,
  taking whatever stands on each section as it goes. Support that comes
  back in time saves what has not fallen (the cracks stay). Platforms and
  the structures on them fall immediately.
- Destroying a bridge cell destroys adjacent cracked cells too (the manual's
  "shock"). A structure destroyed nearby explodes: bridge cells within
  `combat.explosion_radius` crack, and cracked ones fall (the manual's
  explosions of units cracking bridges; walkers and flyers that die do
  not explode).
- Hardened bridges (`hard` frames, the Bridge Harden spell) do not crack from
  damage but still crumble when unsupported.
- A bridge cell's tile is chosen by which of its four neighbours are ground;
  see the letter table in `FORMATS.md`. Where a bridge meets island land,
  the island's rim cell is overlaid with a `bridgeconnector` frame.

The manual: bridges may not be attached where an Edge Farm grows.
Islefall: a type with an `edge_farm` flag (`edgefarm`, one cell, free, drops
on rims) blocks bridge pieces from attaching to the island cells it
covers, yours and the enemy's alike; a piece whose only landfall is a
farmed cell is refused with a message naming it. It is drawn as the
ploughed version of the island tile it grows on, since its frames mirror
the island set. Bridge ownership is in the Ownership section.

## Dropping structures

The manual: "Units may only be built on islands you own, off of the ends
of friendly bridges, or on Neutral Islands. Buildings, by virtue of their
tremendous weight, can only be constructed on islands", and "place the unit
in the sky, just off the end of a bridge piece. You cannot place units
directly on top of the bridges themselves."

So a `createsisland` type (every 3x3 emplacement) is dropped either wholly on
island ground or wholly in the sky with a footprint cell orthogonally
adjacent to an open bridge end; in the sky it creates its own island under
itself, and that islet goes with it when it is destroyed or salvaged:
walkers on it are lost, and bridges that hung off it alone crack and fall
like any unattached stretch. On screen the islet's picture falls out of
the sky and fades (`fall_seconds`, `fall_gravity_px` and
`fall_fade_share` in `[fringe]`). Everything else needs island ground under every cell. In both cases
no cell may be covered by another structure or a unit, and unless the type
has `mayDropOnRim`, no cell may be a natural island's edge.

| Flag | Reading | Confidence |
|------|---------|------------|
| `createsisland` | May be placed in the sky at a bridge end; the footprint becomes a platform island. | High |
| `dropBlocking` | Nothing may later be dropped onto the footprint. Currently every structure blocks drops, so the flag is recorded but not yet distinguishing. | Medium |
| `mayDropOnRim` | The footprint may include island rim cells. | Medium |
| `mayDropOnIsle` | Not yet used. Probably: may be dropped on platforms created by other structures. | Low |
| `fence` | A barricade post; see Barricades below. | High |
| `emplacement`, `factory`, `tree`, `vortex`, `dais` | Category markers, no rule attached yet. | - |

Ownership, Storm Power costs and the Energy requirement ("the proper Energy
influencing the space") are covered in their own sections below.

## Units

- `walker`, `flyer` and `balloon` mark mobile types; those without `priest`
  are Transports. Flyers and balloons cross the sky in straight lines
  (see Air below).
- Walkers keep to the inside of an island: a rim cell costs
  `walking.rim_cost` on a path, so the edge is stepped on only for a
  bridge or a place on the rim itself (a unit's feet are drawn at the
  bottom of its cell, which on the bottom rim is the cliff's lip).
- Facing uses the eight walk animations `A` to `H`: north, north-east, east,
  south-east, south, south-west, west, north-west.
- Units have `maxHitPoints` and `threat`; shooters prefer the highest
  threat, which makes the High Priest (25) the favourite target.

## The High Priest

From the manual: bringing a priest's health to the half-way point stuns
him; a stunned priest can be captured by any Transport; a Transport carries
him to the centre of your Altar, secures him and walks away; the sacrifice
grants Knowledge; a priest near his own Temple regenerates.

The Altar you place is the `dais` type, a floor without hit points; the
original stands the `altar` type on it, which has 1500. `[bodies]` in the
rules says so, and the placed Altar can be shot down like anything else.
The sacrifice itself is instant on arrival, so there is no "mid-sacrifice"
to interrupt: a carrier whose Altar falls first keeps the priest and
waits for orders.

Islefall: a priest at or below half health is stunned and takes no orders.
The manual has him throw a protective shield round himself that keeps him
from being destroyed: with `priest.stun_shield` a stunned priest takes no
further damage from anything, and the blow that stuns him leaves him at
least one hit point. Shooters go on choosing him all the same, as the
manual says they choose the closest target even when it is invulnerable.
A Transport ordered to capture walks next to him and picks him up; the
priest then follows the carrier and is not shot at. Ordered to sacrifice,
the carrier walks to the altar's centre cell, the priest dies and the
player chooses what the sacrifice teaches (see Knowledge). A priest
within 8 cells of an own Temple heals two points per second and leaves
the stun above half health. The heal range and rate are Islefall's
guesses.

The manual: a priest whose bridge is blown out from under him does not
fall but floats in the clouds until a bridge is rebuilt beneath him, when
he settles onto it; an Aerial Transport can capture a floating enemy
priest. Islefall: a priest over fallen ground gets `floating` instead of
dying, cannot move, lands the moment his cell is ground again, and may be
captured by a `balloon`-flagged Transport whether or not he is stunned.
A Transport that falls releases the priest it carried.

## Storm Power

Storm Power is the currency (the manual). Islefall models:

- A Storm Geyser (`geyser`, flag `geyser`) holds its `cost`, 2000, in
  crystals. Geysers are placed by the map and cost nothing.
- A Storm Crystal (`nugget`) is worth its `cost`, 200, so a Transport carries
  200 per trip. Harvesting takes one second next to the geyser, delivering
  one second next to the Temple; the unit repeats until the geyser is empty,
  when it becomes an `emptygeyser`, and only then stands idle for its next
  order. While it works it keeps moving on the spot and plays its
  `pickupSound` (a Golem's grunt) as it takes the crystal and its
  `dropSound` as it hands it in (`sounds.events.pickup` and `dropped`);
  on the way home the crystal is drawn over it (`hud.carried_crystal`,
  the `nugget` frames, `hud.carried_lift` pixels above its feet). A way it
  cannot find, because someone stands where it was going or a bridge is
  down, is looked for again after `walking.wait_seconds` rather than
  abandoned.
- The Temple is read as the `residence` type (flag `residence`); crystals
  handed in there become Storm Power.
- Dropping a structure costs its `cost` property; bridge pieces and golems
  are free, as in the manual ("the Temple gives you the power to create
  bridges and Golems").

An Aerial Transport (a `balloon` type: Balloon, Air Ship, Cloud Floater)
harvests the same way but flies straight to the geyser and back, needing
no bridge, as the manual says.

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
their theme and ask for `level` units of any Energy, so the first Wind
Generator stands on the Temple's Sun and the next on the first (the
tutorial's Sun Workshop produces a Wind Generator); the Temple
(`residence`) radiates Sun. The circle radius, 128
source pixels, is a guess. Only the local player is checked; enemies place
freely for now.

## Combat

From the type properties and the manual:

- A shooting type has `range` (cells), `hpPerSec` (damage per second) and
  `delayBetweenShots` (seconds); one shot deals `hpPerSec x
  delayBetweenShots`. Sun Cannon: range 22, 80 damage every 5 seconds. Sun
  Disc Thrower: range 8, 12 damage every second. Only the Sun Cannon and
  the Vander Tower carry a delay in the data; for the rest the `shot_delay`
  hook decides, giving the other cannons the Sun Cannon's five seconds
  (Thunder Cannon 200 a shot, Ice Cannon 100) and everything else the
  rules' default second, so cannons are heavy and slow rather than a hail.
- Cannons fire only straight north, south, east or west (the manual); no
  flag says so, so Islefall applies it to types whose name contains
  "cannon". Everything else fires in any direction.
- A shot is a missile in flight: `[projectiles]` names each shooter's
  missile type (the data never links them); its frames spin, or turn to
  its bearing when there are many, or, for a missile listed in
  `cardinal_frames`, loop the run drawn for the flight's direction (the
  Thunder Cannon's bolt has four runs of three), as it lobs from muzzle
  to target at `speed_px`, and a flash marks where it lands.
- Targets are enemies within range, highest `threat` first, nearest second.
  Units count below any structure. Enemy bridge cells are targets too, at
  no threat, so a cannon with nothing else in reach shoots the bridge: a
  shot cracks a sound cell and breaks a cracked one (the `bridge_hit`
  hook, read from the manual's rule for explosions), and a hardened cell
  shrugs it off. Range is measured from the footprint
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

## Barricades

The manual: the Sun Barricade repels enemy fire; an acidic barrier is
created by aligning two Acid Barricade posts along a straight horizontal
or vertical line and dissolves every enemy unit that comes between the
posts; the Arc Spires form barricades (Damage 50). Islefall: two
complete posts of one `fence` type and owner on the same row or column,
within the type's `range` of each other, raise a barrier over the cells
strictly between them, drawn as a beam in the type's colour
(`[fences] colours`). What the barrier does is `[fences] effects` by
type: `shield` stops any enemy shot whose line would cross it, so the
shooter finds no target there until a post falls; `dissolve` kills an
enemy walker standing between the posts; `arc` deals the type's
`hpPerSec` to one every second. Flyers pass over acid and arcs. A post's
`range` and `hpPerSec` belong to its barrier: posts never shoot. Which
effect belongs to which barricade is Islefall's reading of the manual.

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
`hpPerSec`, and flies for its type's own `lifeSpan` seconds (Whirligig
75, Dust Devil 30, Man o'War 60; `life_seconds` in `[air.attackers]`
overrides, `air.default_life_seconds` serves a type that names none).
The manual's ten seconds for the Dust Devil are not the data's: at its
speed of 3 they cover 30 cells of its base's range of 45. Whether it
refuels, hunts Transports, cracks bridges or feeds on kills is set there
too. Balloons are Battle units: they cost, need Energy and production,
and are placed anywhere.

The manual: the vapour-like Cloud Floater is hard to hit, only one shot in
twenty connects. Islefall: `[combat.hit_chance]` gives the share of shots
that connect with a unit of each type (the shipped rules say one in ten
for `rainballoon`, since its type file notes both its hit points and the
chance to hit were doubled after the manual was written); the roll comes
from dice seeded by `combat.dice_seed`, so every machine in a network game
sees the same misses. A missed shot still flies and lands.

No unit shoots on the ground: the manual gives every Ground Transport
(Golem, Sail Skater, Crystal Crab, Bulf) and the High Priest a range and
damage of "n/a", and the data carries no weapon for them, so walkers only
carry, capture and cast. Aerial attackers are the only units that strike.

## Construction and the Stream of Power

The manual: once a unit is placed it must receive Storm Power before it
becomes active; a stream is emitted from the Workshop (or Temple, or
Outpost) and zigzags to the unit, and only then does the unit respond to
its surroundings. The stream is what prevents building on islands or
bridges not connected to the Home Island. Destroying an enemy unit or
building rewards a quarter of its Storm Power value (adjustable by the
Battlemaster); attackers that cost nothing return nothing.

Islefall: a structure placed in play (not by the map, and never a geyser)
starts as a shell with `construction.start_health_percent` of its health
(a quarter, a guess; with a single hit point, whatever had felled a
structure felled its replacement with the next shot). Each tick, every player's streams
reach the ground connected by land or bridge to one of their complete
Temples, Workshops or Outposts; a shell any of whose cells is reached
gains a tick of build and that tick's share of the remaining health, so
that damage taken while building stays taken, and stands
once `construction_seconds` (from cost, `constructionRate` and
`construction.power_per_rate`, a guess) have passed. A shell shoots
nothing, produces no Energy, launches nothing, builds nothing and takes
no crystals. The stream's path is not drawn. `kill_reward` grants the
killer `economy.kill_reward_percent` of the victim's cost; a Golem costs
nothing and rewards nothing.

## Workshops and production

The manual: Workshops build the units of battle; a Level One Workshop has
two production slots; to create a unit other than a Golem or a bridge piece
you put its Knowledge into production at a Workshop; units must be aligned
with the Workshop; a destroyed Workshop loses what it was producing; the
Temple gives the power to create bridges and Golems.

The manual: Level One Workshops have two production slots, Level Two
three and Level Three four; a Workshop may be upgraded twice, for a cost.
Islefall: types with the `factory` flag are Workshops with
`production.workshop_slots[level]` slots; `upgrade_workshop` raises the
level for `upgrade_cost_percent` of the Workshop's own price (a guess). Placing a Battle unit (any type that
needs Energy) requires it to be in production at one of the owner's
Workshops; Temple-provided types (`production.temple_types`, the Golem)
need an own Temple instead. The `workshop_can_produce` hook decides
alignment: the tutorial's Sun Workshop produces a Wind Generator, so only
units above level one must match the Workshop's theme. Picking a building
tool puts the type into the first able Workshop with a free slot.

The manual's Unit Rate: once a unit is placed it leaves the Production
window and comes back after a while, slow, medium or fast. Islefall keeps
the type on the panel but takes it off offer: `production.unit_rate`
picks an entry of `refresh_rates` (seconds per hundred Storm Power of the
unit's price), and the `refresh_seconds` hook turns that and
`refresh_min_seconds` into the wait, so a Sun Cannon at 400 comes back in
eight seconds at medium and a free Golem in two (the figures are guesses;
the manual gives none). The panel shows the seconds in place of the
price; every player, the computer ones included, waits the same way.
Build time is the shell's construction below.

A structure under construction is drawn as the original drew it: its
own outline filled with grey cloud (the `mcloud` type's first frame,
`animation.construction_cloud`), the real picture coming through from
the bottom right in `animation.construction_stages` steps as the stream
builds it, with the sparkles on top. How the original ordered the reveal
is not known; the diagonal with a ragged front is Islefall's.

## The opponent

A computer player follows the same rules as you: its drops cost Storm
Power and need Knowledge, Energy at the site and a Workshop with the type
in production; its structures are built by streams. Every `ai.move_seconds`
it sends idle Transports harvesting, keeps its shooter and generator in
production at any Workshop it owns, and either drops a shooter at the open
end nearest its target (every `shooter_every`th move), drops a generator
at the nearest site with Energy when every shooter site lacked it, or lays
the bridge piece from its own queue that brings it closest to its target.
An opponent without a Workshop only bridges and harvests. A map may give
an opponent its own `shooter` and `generator`, so a Wind opponent drops
Crossbows and Wind Generators.

## Ownership

Islands, platforms and bridge cells carry an owner. Following the manual:
a piece must touch the builder's own island edge or open bridge end, and
may also touch other players' ground, which is how you connect to an enemy
island or enemy open end without being able to build off them. A drop on
land needs every footprint cell to be the dropper's; a drop in the sky needs
the touched open end to be the dropper's.

Neutral islands (the manual: the archipelago between opponents, with
Geysers and Obelisks) have no owner. Anyone may build units and buildings
on one and connect bridges to it, but nobody may build bridges off it
until they have erected an Outpost there. An Outpost gives its builder the
island: bridges may leave it, no enemy may build on it, and Transports may
bring crystals to the Outpost instead of the Temple. Destroy the Outpost
and the island is neutral again. An island holding an enemy Temple or
Outpost takes no construction from you. In Islefall a map island without
an `owner` is neutral; `production.outpost_types` names the Outpost. Not
modelled: two Outposts raised at once cancelling each other (there is no
construction time), Obelisks, and the Outpost's Storm Power stream.

## Spells and Obelisks

The manual: Spells are contained in Obelisks, found on neutral islands; a
Transport that touches an Obelisk learns its Spell, keeps it until
destroyed and casts it as often as Storm Power allows; a cast halts the
Transport for a couple of seconds and never affects the caster; a High
Priest may pray for Devastation at a lower cost. Point Blast, Devastation
and Decimation damage everything within short, medium and long range;
Heal restores every unit in range and lifts Paralysis and Invisibility;
Invisibility hides units from targeting and capture; Paralysis stops
movement, shooting and casting; Bridge Harden makes bridges
indestructible; Treason hands every unit in range but a High Priest to
the caster.

Islefall: the `buried` type is the Obelisk and `bomb` types are Spells,
each with its `range`, `cost`, `casttime`, `praytime` and `effecttime`
from the type file. A map names an Obelisk's Spell or leaves it to a
seeded draw from `spells.pool`. Reading needs a Transport next to the
Obelisk; casting (`X`) pays the cost at once and lands after the cast
time on everything within the range in cells; praying (`Y`) takes the
prayer time. `[spells.effects]` maps each Spell to its effect and, for
damage, a guessed amount.

The higher Spells, whose figures the handbook does not give: Bombardment
(`bombmeteor`), Thunder Strike (`bomblightingzap`) and Thunderstorm
(`bomblightingwave`) deal guessed damage scaled by their price, and the
game scatters the `effect` picture named for them (meteors, lightning)
over the cells they reach; Graviton (`bombgraviton`) is read as a pull on
what flies, damaging only air units (a guess; `air_only`). The Summons
conjure a base's attackers beside the caster: Hydra, Hydra Wave and Hydra
Flood (`bombimano`, `bombiimano`, `bombiiimano`) Man o'Wars and
Whirlwind, Twister and Vortex (`bombtwister`, `bombiitwister`,
`bombiiitwister`) Dust Devils, as many as the Spell's `spawns` property
says (`unit` and `count` in `[spells.effects]` override); with no base to
refuel at they fall when their flight time is up. Which creature
each Summons conjures is Islefall's reading of the names (`bombImano` to
`bombIIIMano`), not stated by the data. Not modelled: the Spell icon in
the original's overlay style.

## Winning

The manual: to win a multiplayer game you must capture the enemy High
Priests and Sacrifice them. Islefall: every owner who fields a High Priest
is a player; a player whose last High Priest is sacrificed is out, and
when one player remains of two or more, that player has won. The game
keeps running afterwards. Nothing else ends a game: losing the Temple or
altar does not.

## Island walls and islets

The fringe sheet holds, besides the plain rock, wall pieces with windows,
grilles and red doors with lamps, each flagged `lit` or `unlit`: dwellings
in the cliff. Islefall gives `fringe.dwelling_share` of an island's wall
pieces a dwelling, always the same ones, lit on an island somebody owns
and dark on a neutral one; what lit them in the original is not known.

The islet a unit makes for itself is one picture in the original, the
`island` type, with a large emblem in its owner's colour (eight of them,
and a plain ninth), over the `islandstalag` underside with a matching
apron. Islefall draws a three-by-three islet that way, frame
`fringe.player_frames[owner]`, a geyser's with the plain one, and tiles
any other platform ground from the island set as before.

## Island ground

The island tile set holds, besides the numbered filled and rim pieces,
36 more filled textures per theme numbered 0, which its comments call the
core pieces of the island terrain scrambler. Islefall can draw filled
cells at least `fringe.core_depth` cells from the island's edge with them
(laid in grid order, `core_columns` wide, or shuffled at 0), keeping the
numbered filled tiles nearer the rim. It is off by default: how the
original laid them is not known, and on some themes they come out as
coarse speckle beside the plain tiles.

## Knowledge

Types with a `techBit` need that Knowledge before their owner can build
them; types without one are known from the start.

The manual's multiplayer rules: a sacrifice lets the player choose the
Knowledge of one of the available Level One units, or upgrade the Altar;
after an upgrade the next sacrifice offers Level Two units, and after
another Level Three, so the advanced units cost two or three priests.
The campaign decides the Knowledge for you. Islefall: every sacrifice
leaves a choice pending for its owner (`pending_knowledge`); the
`Learn` command spends it on a type whose `level` the owner's Altar
level allows and whose bit the owner lacks, `UpgradeAltar` on raising the
Altar up to `knowledge.altar_levels`. The game shows the offer in a panel
until you choose; computer players, and the player when
`knowledge.auto_choose` is set, choose through the `knowledge_choice`
hook, which by default learns everything on offer before raising the
Altar. The Altar level is per player, not per building, so a rebuilt
Altar keeps it.

Rank (the manual): once a player knows every unit, the next sacrifice
returns the island to Level One and raises the Rank, and each Rank adds
25% to the hits and damage of Battle units. Islefall: a sacrifice with
nothing left to offer (every bit known, the Altar at the top) clears the
owner's Knowledge, returns the Altar to level one and raises the Rank;
Battle units (types that need Energy) placed from then on get
`knowledge.rank_bonus_percent` more hit points and damage per Rank.
Units already standing keep their figures. As the manual says other
players can see your level printed on your island, each player's name,
Altar level and Rank stand over their Altar (`hud.island_labels`).

## Open questions

The facts about the original that neither the data nor the manual
settles are listed in `ROADMAP.md`, with what comes next.
