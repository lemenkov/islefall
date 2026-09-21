# Roadmap

Where Islefall stands and where it goes next. The rules a feature follows
are in `RULES.md`; how to change them is in `MODDING.md`; the network
design is in `NETWORK.md`. Dates are when the work landed.

## What works (September 2026)

**Data.** Every original file the game needs is read at run time from a
NetStorm installation: the sprite cache and palettes, the `netstorm.tarc`
archive with its type definitions, the island tile set, the bridge
tiles, the sounds, and the campaign scenarios (`.fort`). Nothing from the
game is in the repository.

**Rules.** The simulation is deterministic and data-driven: numbers in
`data/rules.toml`, decisions in Rhai hooks, every file overridable from
a data directory. Modelled after the manual: islands, bridge pieces with
attachment, cracking, hardening and crumbling, Edge Farms; ownership,
neutral islands and Outposts; Storm Power, geysers and harvesting by
walkers and flyers; Energy from Temples and Generators; Workshops,
production, the Unit Rate and the stream that builds shells; shooters,
cannons, turrets, air attack bases and their attackers, barricades, the
Cloud Floater's dodge; priests, stunning, capture, sacrifice, Knowledge
by choice, Altar levels and Rank; Obelisks and all the Spells, Summons
included; computer opponents that bridge, drop and harvest.

**Game.** Bevy renders the original's art with its undersides, turrets,
projectiles, effects, sounds and sky; the sidebar follows the original's
Production window; the controls follow the original's pick-up and drop.
Snapshots save and load; replays record every command.

**Network.** Lockstep over a relay server with hash checks, a lobby, and
rejoining a running game under the same name.

**Campaign.** The chapters and missions are read from the archive; a
mission loads its scenario with the money and Knowledge its header
grants, opens on its briefing, turns its pages, judges the end by the
High Priests and offers the next mission, remembering what is done.

**Project.** Apache-2.0 code, CC-BY-4.0 content, REUSE-compliant, an
emblem, and over a hundred tests across the crates.

## Next

In the order that changes the game most.

1. **Play-test against the original.** Sit down with both games and list
   what feels different: pacing, reach, damage, the AI's habits. Most of
   the figures marked "guess" in `RULES.md` (spell damage, heal range,
   explosion radius, upgrade cost, refresh times) can only be set this
   way. Every finding becomes a rules change, not a code change.
2. **The campaign's scripting.** The flow is in; what the missions'
   texts ask of the engine is not: lessons whose pages turn when the
   player has done the step, the timed sections (`[@120]`), goals other
   than the enemy priests (`Ai2PriestSaved`, `Ai3TempleDead`), allies
   (`myAllyList`), the opponents' habits (`aiAbility`,
   `aiTimeBetweenMoves`, `aiCollectors`), `techAllowed`, `denySalvage`
   and `moreGeysers`. A mission also restarts the game; loading one in
   place needs the world torn down and built again.
3. **A start screen.** Map choice, name, server address and the campaign
   are all environment variables today. A menu in the game is the first
   thing a newcomer meets.
4. **Late joining.** A player may only come back under a name that
   left. Joining a running game as a new player needs a free slot, a
   home island and Storm Power for them.
5. **Packaging.** A desktop file with the emblem (Wayland shows no
   window icon without one), a release build workflow, and continuous
   integration running the tests, `reuse lint`, formatting and clippy.
6. **Art of our own.** `crates/islefall-art` generates an island's
   underside from a seed (`fringe.generated`, off by default; see
   `ART.md`; `F3` switches in the game). If it passes beside the
   original, cliff sides, bridges and effects may follow; the ground
   stays the original's pixel art.
7. **The remaining art.** The island undersides' left and right edge
   pieces are unused; the Spell icon in the original's overlay style;
   the original's Storm Power stream animation from Workshop to shell.

## Open questions about the original

Facts we could not settle from the data or the manual. A play-test or
a reader who remembers the game can answer them.

- Whether `yuckWalk` blocks walking or merely deters it.
- What `mayDropOnIsle` means exactly.
- The real bridge piece catalogue: Islefall makes its pieces up.
- Explosion radius and damage; `damageEffect` is always 1 in the data
  and may select a visual effect rather than a damage class.
- What each sacrifice taught in the campaign, mission by mission.
- Which creature each Summons conjured, and the damage of Bombardment,
  Thunder Strike and Thunderstorm.
- Whether the Sun Barricade's shield stops shots outright or absorbs
  them, and whether a post could be shot through it.
- The Unit Rate's actual refresh times at slow, medium and fast.

## How to propose something

Open an issue naming the manual passage or the behaviour of the original
that Islefall gets wrong, or the feature that is missing. Rules changes
go in `data/rules.toml` or `data/scripts/rules.rhai` with a note in
`RULES.md` saying what the manual states and what is a guess.
