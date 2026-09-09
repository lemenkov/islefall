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
| `crates/islefall-data` | Loaders for the original data files (sprite cache, palettes). No engine dependency. |
| `crates/islefall` | The game binary, built on Bevy. Currently a sprite viewer. |
| `docs/FORMATS.md` | Reverse-engineered descriptions of the NetStorm file formats. |
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

Point `NETSTORM_DIR` at the directory that contains the game's `d/` folder:

```sh
NETSTORM_DIR=~/games/NetStorm cargo run --release
```

The viewer animates one shape from the sprite cache. `[` and `]` step through
shapes, `Space` pauses.

## Inspecting the data

`shpdump` prints statistics for the sprite cache or renders a shape to a PNG
sheet:

```sh
cargo run -p islefall-data --bin shpdump -- stats "$NETSTORM_DIR/d/_shapes.shp"
cargo run -p islefall-data --bin shpdump -- sheet "$NETSTORM_DIR/d/_shapes.shp" 88 walker.png --col "$NETSTORM_DIR/d/SUNCANNON.COL"
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
