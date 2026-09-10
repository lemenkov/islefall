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
`[production]` sets the slots per Workshop and the types a Temple provides
without one. `[effects]` shapes the generated fire and smoke on damaged structures. `[spells]` names the prayer Spell, the Obelisk pool and each Spell's effect. `[ai]` names the opponent's shooter and generator and its pace. `[air]` names the base classes, which attacker each base
launches, the respawn wait and strike reach, and per attacker its flight
time and habits (refuelling, hunting Transports, cracking bridges, feeding
on kills).

## rules.rhai

Hooks are plain functions called by the simulation with simple arguments
and must return the documented type. They run in a sandbox without file,
time or random access, so they are deterministic: the same inputs always
give the same result, which matters for replays and lockstep multiplayer.
A hook that fails is logged and the calling rule falls back to a harmless
value (zero damage, no Energy need, no Knowledge). The script is compiled
once at start-up; a syntax error stops the game with the message.

Available hooks:

- `energy_need(level, theme, mana, class)` -> `#{ theme, themed, any }` or `()`
- `fires_straight(name, flags)` -> bool
- `damage_per_shot(hp_per_sec, delay_seconds)` -> int
- `salvage_refund(cost, hp, max_hp, refund_percent)` -> int
- `knowledge_grant(known_bits, all_bits)` -> int or `()`
- `target_priority(threat, distance, is_unit)` -> int, higher wins
- `workshop_can_produce(workshop_theme, type_theme, type_level)` -> bool
- `air_attack(class, use_air_damage, air_range, air_damage, range, hp_per_sec)` -> `#{ air_range, air_damage, ground }` or `()`
- `air_target_priority(distance, is_unit, is_transport, hunts_transports)` -> int or `()` to refuse
- `bridge_hit(state, damage)` -> "crack", "destroy" or "none"
- `construction_seconds(cost, construction_rate, power_per_rate)` -> seconds
- `kill_reward(cost, percent)` -> int
- `sound_gain(edge, zoom, base_db, edge_db, reference_zoom, db_per_halving)` -> decibels (presentation only)

## Sounds

The simulation emits events (a shot fired, a hit, a building finished, a
structure destroyed or salvaged, a move order, a priest picked up or
sacrificed, a crystal taken, a piece placed, a bridge cracking or falling,
an island falling, a unit lost) and the app plays what `[sounds.events]`
names for each. A cue has a `property`, the type's own sound as the `.type`
files name it (`fireSound`, `impactSound`, `buildDoneSound`, `moveSound`,
`pickupSound`), and a `file` used when the type names none; a cue with
neither is silent. Fire and impact sounds mostly sit on projectile types,
which nothing in the data ties to their shooter, so `[sounds.projectiles]`
pairs them by name. Files are looked up case-insensitively in the
installation's `sound/` directory; `[sounds.aliases]` points names the type
files use at the files that actually exist (most differ by a `-500` or
`-1000` suffix on disk). `[sounds.overrides]` maps a file name
to a path of your own relative to the data directory, which is how a mod
replaces a sound; only WAV is decoded unless the app is built with more of
Bevy's decoders. `volume` is the master level, `max_per_frame` caps how
many sounds start at once, and `ISLEFALL_VOLUME=0` silences a run.

Every sound is heard from the camera. `[sounds.attenuation]` sets the
decibels lost at the edge of the view, the zoom at which the camera's
height costs nothing and the decibels lost per halving of the zoom below
it; the `sound_gain` hook turns those into a gain, so a mod can reshape
the curve. Anything outside the view is silent. A file name ending in
`-500` carries a base gain of -5 dB, DirectSound's hundredths of a
decibel, as the original engine read it. Objects whose type names a
`loop_property` sound (`activeSound`: batteries hum, Whirligigs spin)
play it as a loop while in view, nearest the centre first up to
`max_loops`. `[sounds.ambient]` names a bed that never stops and sky
noises played at random intervals; both ignore the camera.
A cue with `variants = true` plays one of the file's numbered siblings in
turn, which is how a Golem answers an order with a different line each
time (`golemMove1` to `5`). `[sounds.footsteps]` lists the walkers that
have true step sounds (the Bulf's `bulfWalk1` to `5`) and how many steps
a walk cycle carries; everything else walks silently.

## The panel

`[sidebar]` builds the panel down the left from the original's `fortgump`
art: which frames are the stone background, the ruler and the Storm
Power box, the crystal beside the figure, the icon size of the build
list, the minimap's height and its colours. Every type's list icon is the
frame its type file flags `gumpframe`.

## Screen text

`[hud]` holds the lines drawn in the corner as templates with `{power}`,
`{knowledge}`, `{techs}`, `{tool}`, `{status}` and `{opponents}` filled
in, plus the victory and defeat lines, so a mod can reword or translate
them.

## Sky

The sky is generated, so no picture is needed for it: `[sky]` sets the
background colour and a list of cloud layers, each a seeded tile of
fractal noise on a torus (it wraps without a seam) with its cloud size,
detail, cover, edge softness, tint, opacity and drift speed, bottom
layer first. Change a seed for a different sky, the cover for a clearer
or heavier one, the tints for another time of day.

## Bringing your own files

Every file the rules name is looked up in the data directory first and
in the installation second, so a data set of your own grows one file at
a time: put a WAV under the data directory and name it in
`[sounds.overrides]`, or a sprite sheet under `sprites/`. Pictures may be GIF or PNG; sounds are WAV unless
the app is built with more of Bevy's decoders.

Sprites are replaced per type. Put `<stem>.toml` and its picture under the
directory `[sprites].dir` names (`sprites/` by default): the index lists
the frames in animation order, each with its `animation` letter (`A` to
`H` are the eight facings clockwise from north, `P` poses), its `rect`
in the picture, its `hotspot` measured from the rect's top-left corner,
and optionally a `shadow` rect and `shadow_hotspot` in the same picture.
A type with a sheet never touches the sprite cache, so a whole data set
of your own is a directory of sheets. To start from the originals:

```sh
cargo run -p islefall-data --bin shpdump -- export "$NETSTORM_DIR" /tmp/sheets sunwalker
```

writes `sunwalker.png` and `sunwalker.toml` in exactly this form; with no
stems it exports every type. Keep exported originals out of the
repository, as with all NetStorm data.

## Maps

Cells are `[x, y]` with y growing downwards. An island without an `owner`
is neutral; its `theme` (sun, thunder, wind, rain) chooses its ground and
the Temple's look on it. An opponent may name its own `shooter` and
`generator` types and the `knowledge` bits it starts with, so each
faction fights with its own weapons. A structure's position is its
hotspot cell, the bottom-right cell of its footprint. The layout is placed
with costs and Energy switched off, then the reserves are set to
`start_power` and the rules apply.

## What is still in code

Facts about NetStorm's file formats (the sprite container order, the RLE
opcodes, the palette file) live in `crates/islefall-data`, because they
describe the files rather than the game. Rendering details such as draw
order and overlay colours live in the Bevy crate.
