# Islefall

A real-time strategy game of floating islands, bridges and priests, in the
spirit of *NetStorm: Islands at War* (Titanic Entertainment, 1997). Written
from scratch in Rust on top of [Bevy](https://bevy.org).

Islefall ships **no game data**. It reads sprites, palettes and unit
definitions from a NetStorm installation you already own, in the way OpenRA
and OpenTTD use their original games' assets. See `NOTICE`.

Status: early development. Nothing is playable yet.

## Layout

| Path | What |
|------|------|
| `crates/islefall-data` | Loaders for the original data files (sprite cache, palettes, the `netstorm.tarc` archive and its `.type` unit definitions). No engine dependency. |
| `crates/islefall-sim` | The deterministic simulation: grid, islands, later units and rules. No engine dependency. |
| `crates/islefall` | The game binary, built on Bevy. Currently draws a first island scene and doubles as a sprite viewer. |
| `docs/FORMATS.md` | Reverse-engineered descriptions of the NetStorm file formats. |
| `docs/RULES.md` | How the type flags are read as game rules, with confidence notes. |
| `docs/MODDING.md` | The data directory: `rules.toml`, the Rhai hooks and map files. |
| `data/` | The game's own rules, scripts and maps; nothing from NetStorm. |
| `tools/shp_decode.py` | Python reference decoder used while working out the sprite format. |

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

Rules, scripts and maps are loaded from `data/` (or `ISLEFALL_DATA`); see
`docs/MODDING.md`. By default this shows the `demo` map, whose home island now has a Sun
Workshop; picking a building tool puts that unit into production there: a home island with the altar and the
Temple, a bridge to a battery on its own islet, an enemy island with a
Storm Geyser, a Disc Thrower and the enemy High Priest, whose owner bridges
towards your altar and drops shooters on the way, and two units walking
under the simulation's control. Structures shoot enemies in range; damaged things
show a health bar, destroyed ones explode and crack nearby bridges.
Left-click on a unit selects it, left-click elsewhere sends the selected unit
there along a path around obstacles, left-click on a Storm Geyser sets it
harvesting crystals to the Temple, left-click on a stunned enemy priest sends
the selected golem to capture him, and left-click on your altar while he is
carried sacrifices him for Knowledge. The corner text and the window title show the Storm Power
reserve, Knowledge, the tool in hand and the last thing the game had to
say; sacrifice the enemy High Priest to win; drops cost their type's price and stand as translucent shells
until a stream of Storm Power from your Temple, Workshop or Outpost has
built them, which needs connected ground. Right-click acts with the current tool:
`Q`, `W`, `A`, `S` pick one of the four bridge pieces on offer (`R` rotates
it), `1` to `6` drop a sun cannon, archer, battery, factory, tree or wind
generator (the last needs Knowledge from a sacrifice) and `7` an Outpost
for claiming the neutral island, `U`
places a golem and `I` a balloon, which flies anywhere. `Delete` destroys the bridge cell under the cursor, `C`
cracks it, `H` hardens it, `V` salvages your structure under the cursor, `G` upgrades the Workshop under it; unsupported bridges crack and crumble, taking
whatever stands on them. Drops follow the rules in `docs/RULES.md`, including Energy: the rings
around the Temple and Generators show where units can be placed. Arrow
keys or WASD pan the camera and `-` / `=` zoom.
`ISLEFALL_MODE=viewer` instead
animates one object type at a time: `[` and `]` step through types, `,` and
`.` through its animations. In both modes `Space` pauses and `P` saves a
screenshot; `ISLEFALL_SCREENSHOT=file.png` saves one automatically after
start-up. `ISLEFALL_PALETTE` selects a palette file stem from `d/` (default
`gifcloud`, which is the game's fixed 8-bit palette). Sounds come from the
installation's `sound/` directory through Bevy's built-in audio; the
`[sounds]` section of the rules says what each event plays, how sounds
fade towards the edge of the view and with zoom, which objects hum, and
what the sky sounds like. `ISLEFALL_VOLUME=0` silences a run.

## Inspecting the data

`shpdump` prints statistics for the sprite cache or renders a shape to a PNG
sheet:

```sh
cargo run -p islefall-data --bin shpdump -- stats "$NETSTORM_DIR/d/_shapes.shp"
cargo run -p islefall-data --bin shpdump -- sheet "$NETSTORM_DIR/d/_shapes.shp" 88 walker.png --col "$NETSTORM_DIR/d/SUNCANNON.COL"
cargo run -p islefall-data --bin shpdump -- export "$NETSTORM_DIR" /tmp/sheets sunwalker
```

`export` writes a type's sprites as a PNG plus a TOML frame index, the form
a mod uses to replace them (see `docs/MODDING.md`).

`tarcdump` lists or extracts the text archive and parses all unit definitions:

```sh
cargo run -p islefall-data --bin tarcdump -- types "$NETSTORM_DIR/netstorm.tarc"
cargo run -p islefall-data --bin tarcdump -- cat "$NETSTORM_DIR/netstorm.tarc" sunwalker.type
```

Decoders treat the input as untrusted: sizes come from the decoded data, not
from headers, and every raster is capped. Running exploratory tools under a
memory limit is still a good habit:

```sh
systemd-run --user --scope -p MemoryMax=8G cargo test -p islefall-data
```

Set `NETSTORM_DIR` when running the tests to include the checks against the
real files.

## License

Apache-2.0. See `LICENSE` and `NOTICE`.
