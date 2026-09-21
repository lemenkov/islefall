# Generated art

Islefall ships no art: every picture comes from a NetStorm installation.
`crates/islefall-art` is the beginning of art of our own, made from a seed
rather than drawn, for the parts of the picture that are texture more than
drawing. What it makes belongs to the project (CC-BY-4.0, like the maps),
so each piece that passes takes the game a step closer to standing on its
own.

This is an experiment. The original's pictures stay the default until a
generated one survives a side-by-side comparison.

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
four-paned windows of cliff dwellings, lit or dark. Set `generated = true`
in the `[fringe]` section of `rules.toml` to draw every island's underside
this way; `[fringe.rock]` holds the ramp, the depths, the widths of lobes
and teeth, and how many windows there are. Unlike the original's wall
pieces, a generated strip follows the island's real outline run by run,
and a short run carries less rock than a long one.

## Candidates

Ground tiles and rim lips (noise in a palette already); the faces of left
and right cliffs, which the original has no pieces for; bridges; effects
such as smoke, fire and the construction cloud. Units and buildings are
drawn characters, not texture, and are not candidates.

Other ways of making pictures were weighed: shaders (alive on screen, but
hard to keep in the pixel style and outside the mod layer), rendering to a
texture once (much machinery for a few thousand pixels) and vector shapes
(they clash with pixel art). Shaders remain the choice for things that
should move: the construction cloud, barricade beams, Energy rings, ice
and lava.
