# The original's campaign scenarios

NetStorm shipped no map files for ordinary battles: the game made each
battlefield up (geyser islands "appear at the opening of each battle, and
more will be generated throughout"). The campaign's 32 missions, though,
are fixed starting positions stored as `*.fort` files inside
`netstorm.tarc`, next to the `*.english` mission texts that name them
(`loadFort = "CaptureThePriest"`) and carry each player's start money and
technologies.

Islefall reads these at run time from the installation and never copies
them: `ISLEFALL_MAP=capturethepriest` loads the tutorial straight from the
archive, and `ISLEFALL_MAP_EXPORT=file.toml` writes what was loaded as one
of our own map files, for looking at or tuning. Keep such exports out of
the repository (`data/maps/campaign/` is ignored for that purpose); they
are the original's data in another shape.

## The format, as far as it is read

Nothing documents `.fort`; the layout below was found by comparing the 32
files with their mission texts. `fortdump list` and `fortdump show <name>`
print what the parser sees.

A file is `F`, a byte, the format version (8 to 16), four bytes, a `u16`
name length and the name, then a stream of chunks, each a little-endian
`u16` length (counting itself) and its payload. In order:

- a chunk of pointers and counts saved as they sat in memory;
- two empty chunks;
- the **world table**: a kind byte, then 256 records `c` (0x63), a `u16`
  item count and the items. Records are the 16 x 16-cell blocks of a
  256 x 256-cell world in row-major order. An item is a cell byte
  (`column << 4 | row` inside the block), a type code and a payload whose
  length depends on the code. Here live the geyser islands (nine ground
  cells 0x9d and a geyser 0x7a at the bottom-right), extra islands, bridge
  cells (0xfb to 0xfe, the payload a tile mask) and any structure on them;
- a chunk of twenty six-byte objects: tag 3 objects and, one per player
  fortress, tag 9;
- a large chunk that is the same in every file (a registry with the type
  names showing through, not decoded);
- a tech table, `count` then (code, value, 1, 0xff) entries, three empty
  chunks, and one chunk per object: a bare kind byte for tag 3, and for a
  **fortress** the player's structures as `c` records over a canvas of
  16 x 16 blocks two blocks wide, top-left block first, trailing empty
  blocks left out.

Payload lengths: 3 for the Temple (0x9e: a byte with the top bit set for
the seat the file was saved from, the player number, 0), 4 for altars
(0x7b to 0x7d: the player number and `01 85 00`), 3 for Workshops (0x55
to 0x58: level, player, 0), 2 for 0x4f, 0x5c, 0x6f, 0x81, 0x83, 0x85,
0x87, 0x89, 0x8c, 0x8d, 0x8e, 0x92 and 0x9c, otherwise 1. Two files
(`breakingthrough`, `runforit`) have one chunk that does not parse to its
length and is skipped.

What the file does **not** hold: a fortress's ground shape and its place
in the world. The original drew every fortress on the same island and put
it where the player's seat was. Islefall seats the fortresses on a ring
round the world table's islands (`[fort].ring_margin`), the file's own
player at the top, and makes each an island of 3 x 3 pieces shaped round
its buildings (`[generate]`). So the buildings stand as the designer left
them, on ground of our own.

## Type codes

Codes are numbers of the original's type registry, which the files do
not spell out. `[fort].codes` and `[fort].units` in `rules.toml` name
them; the names were read off the missions themselves (the six Sun Disc
Throwers of Tactical Combat are 0x47, its Sun Cannons 0x80, Raw Power's
Sun Barricade posts 0x88, its Whirlibases 0x48, the Wind Generators of
Bow Rushing 0x4b, ...) and the less-attested ones are guesses to correct
when a scenario looks wrong. A player's theme is read from the altar code
(0x7c Rain, 0x7d Thunder) or the mission's tech list, the seat player's
from `[fort].human_theme` (the tutorials say a Wind Temple). The Temple
type per theme is `[fort].temples`; the data has no Sun vortex, so the
Sun's is the Residence.

Where the original stacked buildings (its Temple rose from the altar,
Capture The Priest's Workshop sits on its Temple), the map loader nudges
a building to the nearest free cell and says so in the log.
