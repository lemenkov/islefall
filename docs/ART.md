# Generated art

Islefall ships no art: every picture comes from a NetStorm installation.
`crates/islefall-art` is the beginning of art of our own, made from a seed
rather than drawn, for the parts of the picture that are texture more than
drawing. What it makes belongs to the project (CC-BY-4.0, like the maps),
so each piece that passes takes the game a step closer to standing on its
own.

A generated picture becomes the default only once it has survived a
side-by-side comparison with the original's, and the original's stays a
key away: `F4` switches all generated art off and on in the game, `F3`
the undersides alone. The undersides and the ground have passed and are
the default; the rules can turn either back.

## The rules of the style

A generated picture stands beside hand-made pixel art and must not look
pasted in:

- source-pixel resolution, one texel to a pixel, nothing smoothed and no
  half-clear pixels;
- a short colour ramp per material, snapped to the game's fixed palette
  when the game uses it, with ordered dithering carrying the gradients;
- light from the top left, as in the original's sprites: left flanks lit,
  right edges and lower edges drawn dark;
- the same seed gives the same picture, on every machine.

The crate knows nothing of the engine. A picture is a buffer of RGBA
bytes; the game turns it into a texture, and `artgen` writes it as PNG:

```sh
cargo run -p islefall-art --bin artgen -- underside --width 384 --seed 7 rock.png
```

## What is made so far

**The underside of an island** (`rock::Rock`): a mass of rock hanging from
a run of an island's bottom edge, shallow at the run's ends, breaking into
overlapping, blunt stalactites of many widths and lengths, with the little
four-paned windows of cliff dwellings, lit or dark. `generated` in the
`[fringe]` section of `rules.toml` draws every island's underside this
way (`false` for the original's wall pieces); press `F3` in the game to switch the undersides alone
(`F4` switches all generated art together); `[fringe.rock]` holds the ramp, the depths, the widths of lobes
and teeth, and how many windows there are. Unlike the original's wall
pieces, a generated strip follows the island's real outline run by run,
and a short run carries less rock than a long one.

**The ground of an island** (`ground::Ground`): one picture for the whole
island instead of tiles, so it can have what tiles cannot: drifts of
lighter and darker ground, regions of another material (dry grass and
moss, bare earth and scrub, snow and deep ice, pale and burnt ash), tufts
and clumps, scattered flowers, stones, glints and embers, and cracks
across ice and ash. Its colours and their proportions are taken from the
theme's own filled tiles in your installation at load, so it matches the
original rim tiles it meets; only the extra materials' colours come from
the rules. `generated_ground` in `[fringe]` switches it on or off, `F4`
switches it in the game together with the undersides, and `[fringe.ground.<theme>]` shapes each of the
four themes. The rim tiles stay the original's.

**Fire** (`fire::Flame`, `fire::Smoke`, `fire::Blast`): a flame that
loops (a climbing noise pattern eating into a teardrop, two copies
cross-faded so the last frame runs into the first), a puff of smoke that
swells and thins by ordered dithering rather than by going half-clear,
and an explosion that bursts out, breaks up into smoke and throws sparks.
The game makes three sizes of flame in a few variants at start-up and
stands them on opaque pixels of a damaged structure's own picture, so
the fire is on the building and not on the grass beside it; this is the
default (`generated_fire` in `[effects]`, `F4` with the rest), since what
it replaces was never the original's art but specks of our own. The
explosion replaces the original's fireball and is therefore an
experiment, off until it has been compared (`generated_blast`, `F6`).
`[effects.fire]` holds every setting.

```sh
cargo run -p islefall-art --bin artgen -- flame flame.png
cargo run -p islefall-art --bin artgen -- blast --size 96 blast.png
```

## Candidates

The faces of left and right cliffs, which the original has no pieces for; bridges; missile
impacts and the construction cloud. Units and buildings are
drawn characters, not texture, and are not candidates.

Other ways of making pictures were weighed: shaders (alive on screen, but
hard to keep in the pixel style and outside the mod layer), rendering to a
texture once (much machinery for a few thousand pixels) and vector shapes
(they clash with pixel art). Shaders remain the choice for things that
should move: the construction cloud, barricade beams, Energy rings, ice
and lava.
