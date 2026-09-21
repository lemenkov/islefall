<img src="assets/islefall_256.png" alt="Islefall" width="128" align="right">

# Islefall

A real-time strategy game of floating islands, bridges and priests, in the
spirit of *NetStorm: Islands at War* (Titanic Entertainment, 1997). Written
from scratch in Rust on top of [Bevy](https://bevy.org).

Islefall ships **no game data**. It reads sprites, palettes and unit
definitions from a NetStorm installation you already own, in the way OpenRA
and OpenTTD use their original games' assets. See `NOTICE`. Some of the
picture is Islefall's own: the islands' ground and undersides are
generated (see Generated art).

Status: early development. The demo map is playable against three
computer players, the original's campaign scenarios load, and players
can meet over a network with a lobby, and the campaign can be played
mission after mission. There is no start screen yet; `docs/ROADMAP.md`
says what works and what comes next.

## Building

Requires Rust 1.85 or newer and Bevy's Linux dependencies (on Fedora:
`alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel`).

```sh
cargo build --release
```

The first Bevy build takes several minutes. For faster iteration during
development:

```sh
cargo run --features dynamic_linking
```

## Running

Point `NETSTORM_DIR` at the directory that contains the game's `d/` folder
and `netstorm.tarc`:

```sh
NETSTORM_DIR=~/games/NetStorm cargo run --release
```

This opens the `demo` map. Everything else is chosen through environment
variables:

| Variable | Effect |
|----------|--------|
| `NETSTORM_DIR` | The NetStorm installation (required). |
| `ISLEFALL_DATA` | Rules, scripts and maps directory (default `data/`); see `docs/MODDING.md`. |
| `ISLEFALL_MAP` | A map from `maps/` (`demo`, `range`); a name that is no file loads that campaign scenario from the archive (`capturethepriest`, `bridgethegap`, ...; see `docs/FORT.md`); `random` or `random:<seed>` makes a battlefield up. |
| `ISLEFALL_CAMPAIGN` | `1`: go on with the campaign at the first mission not done yet (see The campaign); `0` or empty leaves it off. It overrides `ISLEFALL_MAP`. |
| `ISLEFALL_MISSION` | Play that mission (`tutorial1`, `thewarbegins`, ...); `fortdump campaign` lists them. |
| `ISLEFALL_DIFFICULTY` | `easy`, `normal` (the default) or `hard`, for the campaign's later chapters. |
| `ISLEFALL_POWER` | Start with this much Storm Power, whatever the map or mission grants. |
| `ISLEFALL_PLAYERS` | How many players a random map is made for. |
| `ISLEFALL_MAP_EXPORT` | Writes whatever map was loaded as a map file of our own (`file.toml`). |
| `ISLEFALL_MODE` | `viewer` browses the sprites instead of playing (see below). |
| `ISLEFALL_CAMERA` | Start looking at cell `x,y`. |
| `ISLEFALL_ZOOM` | Starting zoom (1 is source pixels, smaller is further away). |
| `ISLEFALL_SCREENSHOT` | Save a screenshot to this file after start-up, after `ISLEFALL_SCREENSHOT_AT` seconds. |
| `ISLEFALL_PALETTE` | Palette file stem from `d/` (default `gifcloud`, the game's fixed 8-bit palette). |
| `ISLEFALL_VOLUME` | `0` silences a run. |
| `ISLEFALL_RECORD` / `ISLEFALL_REPLAY` | Record every command to a file, or play such a file back. |
| `ISLEFALL_JOIN` / `ISLEFALL_NAME` | Join a game server as a named player (see Network play). |

Sounds come from the installation's `sound/` directory through Bevy's
built-in audio; the `[sounds]` section of the rules says what each event
plays, how sounds fade towards the edge of the view and with zoom, which
objects hum, and what the sky sounds like. The sky itself is generated
from seeded noise.

## The demo map

All four factions are on it. You hold the Sun home island in the north-west
with the altar, the Temple, a Sun Workshop, a Disc Thrower, a Cannon and a
Stone Tower, plus a bridge to a battery on its own islet and a spur to a
neutral Storm Geyser island, where your Golem sets off to harvest. A Wind
island lies to the east, Rain and Thunder islands far to the south, each
with its Workshop, Temple, shooters, Generator, tower, barricade, air base,
walker and High Priest.

The three computer players bridge towards your altar and drop shooters on
the way. The islands are close enough for the cannons to reach their
neighbours, so the fight is on from the start, though your home island
lies just outside every reach. Sacrifice the enemy High Priest to win.

## The campaign

The original's campaign is read from your installation: five chapters,
from the six lessons of Early Missions to Complete Victory.

```sh
ISLEFALL_CAMPAIGN=1 NETSTORM_DIR=~/games/NetStorm cargo run --release
```

starts the first mission you have not finished; `ISLEFALL_MISSION=tutorial1`
plays a particular one. A mission with an enemy in it opens on its
briefing, the world standing still behind it until you start (a lesson's
world never waits: you read and build at once): the page's own buttons turn its pages and
start play, and lessons get a `Next page` button, since the original
turned those pages itself as you got on. `F1` brings the last page back,
`F2` lists every chapter and mission (click one to play it). When the
enemy High Priests are sacrificed, or yours is, the mission says so and
offers the next one or another try; a lesson, which has no enemy, is
finished when you reach its last page. Finished missions are remembered in
`~/.local/state/islefall/campaign-done.txt`. Starting another mission
restarts the game on it.

## Playing

The controls follow the original: take something in hand, left-click to
put it down, and the hand is empty again. With an empty hand, a left-click
selects or gives orders.

**Taking things in hand**

| Key | Takes |
|-----|-------|
| `Q` `W` `A` `S` (or a click on the slot) | One of the four bridge pieces on offer. |
| `R` or right-click | Turns the piece in hand; the panel shows it turned. |
| `1` to `6` | Sun Cannon, Disc Thrower, Generator, Workshop, tree, Wind Generator (the last needs Knowledge from a sacrifice). |
| `7` / `8` | An Outpost, for claiming the neutral island; an Edge Farm, which stops bridges attaching to the edge cell it is ploughed on. |
| `U` / `I` | A Golem, or a Balloon that flies anywhere. |
| `Escape` | Puts the thing in hand back. |

A bridge piece must touch the edge of your own island or the open end of
your bridge. A piece new in the window is still cracked and hardens there
in a few seconds: lay it at once and it goes down cracked, and a cracked
bridge falls to a single hit together with its cracked neighbours. Buildings cost their type's price and stand as translucent
shells until a stream of Storm Power from your Temple, Workshop or Outpost
has built them, which needs connected ground. Picking a building tool puts
that unit into production at your Workshop. Drops follow the rules in
`docs/RULES.md`, including Energy: the rings around the Temple and
Generators show where units can be placed.

**Orders with an empty hand (left-click)**

| On | Does |
|----|------|
| One of your units | Selects it; a pulsing ring round its feet shows which. |
| Open ground | Sends the selected unit there along a path around obstacles. |
| A Storm Geyser | Sets the unit harvesting crystals to the Temple until the geyser is empty; a Balloon needs no bridge to get there. |
| A stunned enemy priest | Sends the selected Golem to capture him. |
| Your altar, while he is carried | Sacrifices him; a panel then offers the Knowledge of a unit, or an Altar upgrade that unlocks the next level. |
| An Obelisk | Sends the unit to learn the Spell there. |

**Other keys**

| Key | Does |
|-----|------|
| `X` / `Y` | Casts the unit's Spell; sets a High Priest praying for his own. |
| `Delete` / `C` / `H` | Destroys, cracks or hardens the bridge cell under the cursor. |
| `V` / `G` | Salvages your structure under the cursor; upgrades the Workshop under it. |
| Arrow keys | Pan the camera. |
| `=` / `-`, numpad `+` / `-`, mouse wheel | Zoom in and out; the wheel zooms towards the cursor. |
| Minimap click | Looks there. |
| `Space` / `P` | Pauses; saves a screenshot. |
| `Ctrl+W` / `Ctrl+Q` | Leaves the game (so do `Alt+F4` and the window's close button). |
| `F5` / `F9` | Saves the world as a snapshot; loads it back. |
| `F4` / `F3` | The islands' ground and undersides are Islefall's generated art by default; `F4` switches both to the original's pixel art and back, `F3` the undersides alone (see `docs/ART.md`). |
| `Ctrl+Z` | Takes your latest command back: the world returns to the moment before it and runs forward again without it. Against the computer, in lessons and in the campaign only, never over a network; press again to take back the one before, up to twenty. |

The panel on the left shows the Storm Power, the piece queue, the build
list and the minimap. The corner text and the window title show the
reserve, Knowledge, the tool in hand and the last thing the game had to
say. Structures shoot enemies in range; damaged things show a health bar,
destroyed ones explode and crack nearby bridges, and unsupported bridges
crumble, taking whatever stands on them.

## Generated art

Not every picture comes from the original any more. The islands' ground
and their rocky undersides are made by Islefall itself from a seed, in the
manner of the pixel art around them: source-pixel resolution, a short
colour ramp, light from the top left, nothing smoothed.

- **The ground** is one picture per island rather than tiles, so it has
  what tiles cannot: drifts of lighter and darker ground, regions of
  another material, tufts, scattered details and cracks. Each of the four
  themes has its own character: grass with moss, dry drifts and flowers;
  ash with pale and burnt patches, cracks and embers; wind-blown ground
  with bare earth, scrub and stones; ice with snow, deep blue ice, fine
  cracks and glints. Its colours and their proportions are taken from
  that theme's own tiles in your installation, so it meets the original
  rim tiles without a seam.
- **The undersides** are a mass of rock hanging from each run of an
  island's bottom edge, breaking into blunt stalactites, with the lit
  windows of cliff dwellings. Unlike the original's wall pieces they
  follow the island's real outline, and a short edge carries less rock
  than a long one.

The original's pixel art is a key away: `F4` switches the ground and the
undersides to it and back while you play, and `F3` switches the
undersides alone. To start with the original's art, set `generated` and
`generated_ground` to `false` in the `[fringe]` section of
`data/rules.toml`; `[fringe.rock]` and `[fringe.ground.<theme>]` there hold
every setting of the two generators. Rims, bridges, buildings and units
are still the original's.

The generators live in `crates/islefall-art`, which knows nothing of the
engine, and `artgen` writes their pictures as PNG:

```sh
cargo run -p islefall-art --bin artgen -- ground --width 384 --height 132 --seed 7 ground.png
cargo run -p islefall-art --bin artgen -- underside --width 384 --seed 7 rock.png
```

`docs/ART.md` has the rules of the style, the ways of making pictures
that were weighed, and what may follow.

## Viewer mode

`ISLEFALL_MODE=viewer` animates one object type at a time: `[` and `]`
step through types, `,` and `.` through its animations. `Space` and `P`
work as in the game.

## Network play

Run the server and join it from each machine with the same data directory:

```sh
cargo run --release -p islefall-server            # listens on 0.0.0.0:7777
ISLEFALL_JOIN=host:7777 ISLEFALL_NAME=Peter cargo run --release
```

A lobby lists who has joined; press `Enter` when you are ready
(`ISLEFALL_READY=1` says so at once, for scripts). The game starts once
every player has connected and is ready (two by default; a
`server.toml` argument sets `bind`, `turn_ticks`, `min_players`,
`max_players` and `hash_every_turns`). Clients report world hashes and the
server tells everyone if they ever disagree. A player who drops can come
back under the same name and is handed the world by another player.
`RUST_LOG=debug` makes the server log every hash check and connection. `docs/NETWORK.md` explains
how the command recording underpins this.

## Inspecting the data

`shpdump` prints statistics for the sprite cache or renders a shape to a PNG
sheet; `export` writes a type's sprites as a PNG plus a TOML frame index,
the form a mod uses to replace them (see `docs/MODDING.md`):

```sh
cargo run -p islefall-data --bin shpdump -- stats "$NETSTORM_DIR/d/_shapes.shp"
cargo run -p islefall-data --bin shpdump -- sheet "$NETSTORM_DIR/d/_shapes.shp" 88 walker.png --col "$NETSTORM_DIR/d/SUNCANNON.COL"
cargo run -p islefall-data --bin shpdump -- export "$NETSTORM_DIR" /tmp/sheets sunwalker
```

`tarcdump` lists or extracts the text archive and parses all unit
definitions; `fortdump` lists the campaign scenarios and draws one:

```sh
cargo run -p islefall-data --bin tarcdump -- types "$NETSTORM_DIR/netstorm.tarc"
cargo run -p islefall-data --bin tarcdump -- cat "$NETSTORM_DIR/netstorm.tarc" sunwalker.type
cargo run -p islefall-data --bin fortdump -- list "$NETSTORM_DIR/netstorm.tarc"
```

Decoders treat the input as untrusted: sizes come from the decoded data, not
from headers, and every raster is capped. Running exploratory tools under a
memory limit is still a good habit:

```sh
systemd-run --user --scope -p MemoryMax=8G cargo test -p islefall-data
```

Set `NETSTORM_DIR` when running the tests to include the checks against the
real files.

## Layout

| Path | What |
|------|------|
| `crates/islefall-data` | Loaders for the original data files: sprite cache, palettes, the `netstorm.tarc` archive, its `.type` unit definitions and `.fort` scenarios. No engine dependency. |
| `crates/islefall-sim` | The deterministic simulation: grid, islands, units, rules, commands, replays and map generation. No engine dependency. |
| `crates/islefall` | The game binary, built on Bevy; doubles as a sprite viewer. |
| `crates/islefall-net`, `crates/islefall-server` | The wire protocol and the relay server. |
| `crates/islefall-art` | Generated art: pictures made from a seed (an island's ground and its rocky underside), and `artgen` to write them as PNG. No engine dependency. |
| `data/` | The game's own rules, scripts and maps; nothing from NetStorm. |
| `assets/` | The emblem. |
| `docs/FORMATS.md` | Reverse-engineered descriptions of the NetStorm file formats. |
| `docs/FORT.md` | The campaign scenario format. |
| `docs/RULES.md` | How the type flags are read as game rules, with confidence notes. |
| `docs/MODDING.md` | The data directory: `rules.toml`, the Rhai hooks and map files. |
| `docs/NETWORK.md` | Lockstep play over the relay. |
| `docs/ART.md` | Generated art: the rules of the style, what is made so far, and how to switch it on. |
| `docs/ROADMAP.md` | What works, what comes next, and the open questions about the original. |
| `tools/shp_decode.py` | Python reference decoder used while working out the sprite format. |

## License

Apache-2.0 for the code, the rules and the scripts; CC-BY-4.0 for the
maps, the documentation and the emblem; the ignore list and `Cargo.lock`
are public domain (CC0-1.0). See `LICENSE` and `NOTICE`.
The repository follows the
[REUSE](https://reuse.software) specification: every file names its
copyright holder and license in an SPDX header or in `REUSE.toml`, the
license text lives in `LICENSES/`, and `reuse lint` passes.
