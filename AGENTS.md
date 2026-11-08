<!--
SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
SPDX-License-Identifier: Apache-2.0
-->

# AGENTS.md

Instructions for coding agents working on **Islefall**, a from-scratch Rust
reimplementation of *NetStorm: Islands at War*. Read this file first, then
`README.md` and `docs/ROADMAP.md`.

## What this project is

Islefall ships no NetStorm data. Sprites, palettes, sounds, type definitions
and campaign scenarios are loaded at run time from a NetStorm installation
pointed to by `NETSTORM_DIR`, in the way OpenRA uses its original game's
assets. The repository holds only Islefall's own code, rules, maps,
documentation and generated art.

Never commit NetStorm assets, extracted files, exported campaign maps or
anything else derived from them. `data/maps/campaign/` is intentionally
ignored and must stay that way.

## Golden rules

1. **The simulation is the contract.** `islefall-sim` must stay
   deterministic and free of Bevy/rendering. No wall-clock time, no
   `thread_rng`, no OS-dependent ordering in state changes. Use the seeded
   `Pcg32` already in `World`.
2. **All player actions are `Command`s.** UI code must send commands through
   `World::apply(owner, command, scripts)`; it must not mutate the world
   directly. Replays, undo and network lockstep depend on this.
3. **Keep state snapshottable.** New simulation or AI state must be
   `serde`-serializable and included in `World::snapshot`/`restore` or
   `ai::encode_ais`. Add a round-trip test when adding state.
4. **Keep the layers apart.** `islefall-data` has no engine dependency;
   `islefall-sim` has no Bevy dependency; `islefall-art` knows nothing of the
   engine. Do not leak rendering types into lower crates.
5. **Be data-driven.** Numbers belong in `data/rules.toml`, decisions in
   `data/scripts/rules.rhai`, scenes in `data/maps/*.toml`. Do not hardcode a
   game rule in Rust when an existing rules key or hook can express it.
6. **No `unsafe`.** The codebase currently contains none; keep it that way.
7. **No silent data loss.** Configs use `deny_unknown_fields`. Add fields and
   documentation together so typos fail loudly.

## Repository map

| Path | Purpose |
|------|---------|
| `crates/islefall-data` | Loaders and decoders for NetStorm's SHP, COL, GIF, TARC, `.type`, `.fort` and mission files. No engine dependency. |
| `crates/islefall-sim` | Deterministic simulation: maps, islands, bridges, units, structures, economy, combat, priests, spells, AI, commands, replays, snapshots, campaign conversion. |
| `crates/islefall` | Bevy client: start screen, sidebar/HUD, input, rendering, audio, campaign/briefing UI, lobby, viewer mode. |
| `crates/islefall-net` | Versioned wire protocol and framing. |
| `crates/islefall-server` | TCP lockstep relay server; currently runs no simulation. |
| `crates/islefall-art` | Seed-generated art (ground, rock, fire, explosions, islets) and the `artgen` binary. |
| `data/` | Islefall's rules, Rhai hooks, maps and scripts. |
| `docs/` | Formats, rules, campaign format, modding, networking, art and roadmap. |

Read the document that owns the area before changing it:

- rules and flag interpretations: `docs/RULES.md`
- data directory, rules schema and hooks: `docs/MODDING.md`
- campaign `.fort` and mission text: `docs/FORT.md`
- original file formats: `docs/FORMATS.md`
- lockstep and protocol: `docs/NETWORK.md`
- generated art and style: `docs/ART.md`
- current status and priorities: `docs/ROADMAP.md`

## Build, test and run

Requires Rust 1.85+ and Bevy's Linux dependencies (Fedora:
`alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel`).

```sh
cargo build --release
cargo run --features dynamic_linking     # faster incremental builds
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
reuse lint
```

Without `NETSTORM_DIR`, the installation-dependent tests in
`islefall-data` print a notice and return early. Run them against real data
when touching loaders or campaign conversion:

```sh
NETSTORM_DIR=~/games/NetStorm cargo test --workspace --no-fail-fast
```

The game and tools need the installation. With no direct launch variable the
binary opens the start screen; `ISLEFALL_MAP` (or `ISLEFALL_CAMPAIGN`,
`ISLEFALL_MISSION`, `ISLEFALL_JOIN` or viewer mode) skips it:

```sh
NETSTORM_DIR=~/games/NetStorm ISLEFALL_MAP=demo cargo run --release
NETSTORM_DIR=~/games/NetStorm ISLEFALL_CAMPAIGN=1 cargo run --release
ISLEFALL_START=0 NETSTORM_DIR=~/games/NetStorm cargo run --release
cargo run --release -p islefall-server

cargo run -p islefall-data --bin shpdump -- stats "$NETSTORM_DIR/d/_shapes.shp"
cargo run -p islefall-data --bin tarcdump -- types "$NETSTORM_DIR/netstorm.tarc"
cargo run -p islefall-data --bin fortdump -- campaign "$NETSTORM_DIR/netstorm.tarc"
cargo run -p islefall-art --bin artgen -- ground --width 384 --height 132 --seed 7 /tmp/ground.png
```

`cargo run` opens a window and blocks. For an automated visual check, use the
screenshot variables instead of leaving a window running:

```sh
NETSTORM_DIR=~/games/NetStorm ISLEFALL_MAP=demo ISLEFALL_SCREENSHOT=/tmp/shot.png \
  ISLEFALL_SCREENSHOT_AT=2 cargo run --release
```

Stop any background server or game process before finishing a task.

## Testing expectations

- Every bug fix gets a regression test. Prefer a focused unit test in the
  module that owns the behavior.
- Test names are descriptive sentences, e.g.
  `a_stunned_priests_shield_keeps_him_alive`.
- Sim changes must keep the determinism tests green:
  `the_same_commands_give_the_same_world`,
  `taking_a_command_back_leaves_the_world_as_if_it_was_never_given`,
  `a_snapshot_restores_the_world_and_carries_on_identically`.
- Network message changes require a `PROTOCOL` bump in
  `crates/islefall-net/src/lib.rs` and a round-trip test update.
- If a test needs a real installation, follow the existing pattern: read
  `NETSTORM_DIR`, print a skip notice and return when it is unset.
- Treat decoders as hostile-input code: sizes come from decoded data, raster
  dimensions are capped, and no allocation should trust a header. If fuzzing
  exploratory decode logic, run it under a memory limit.

## Coding conventions

- Rust 2024, workspace-resolved dependencies. Do not hand-edit `Cargo.lock`.
- Formatting is rustfmt default; there is no custom `rustfmt.toml`.
- Keep module-level `//!` docs current. Comments explain *why*, not *what*.
- Errors: `thiserror` in `islefall-data`/`islefall-sim`; user-facing failures
  in the client should end up in `Status` or a `tracing` message, not a panic.
- Use `BTreeMap`/`BTreeSet` where iteration order can affect state. Avoid
  `HashMap` for simulation state.
- No `unwrap`/`expect` on external data or user input. Existing `unwrap`s in
  tests and after proven invariants are fine.
- Add new environment variables to the README table and the module docs in
  `crates/islefall/src/main.rs`.
- When changing a `Command`, map format, rules key or network message, update
  the owning document and any committed fixtures/tests.

## Licensing and file headers

The repository is REUSE-compliant. Every new file needs an SPDX header or a
`REUSE.toml` annotation:

- code, rules, scripts: `Apache-2.0`
- documentation, maps, emblem, generated art: `CC-BY-4.0`
- `.gitignore` and `Cargo.lock`: `CC0-1.0`
- copyright line: `2026 Peter Lemenkov <lemenkov@gmail.com>`

Run `reuse lint` after adding files. Do not copy art, text or data from
NetStorm into the repository. The original installation stays external.

## Current known gaps

Use `docs/ROADMAP.md` as the source of truth; this is only a quick orientation.

- The start screen is a separate Bevy app in `start.rs`; it validates the
  choices and re-executes the program with explicit environment variables.
  Keep the direct-launch variables and `ISLEFALL_START=0` working for scripts.
- The computer opponent is still simple: it bridges, drops shooters and
  generators, and harvests. It does not capture or sacrifice priests, use
  spells or air units, upgrade, or play defensively.
- Campaign scripting still lacks step-triggered lesson pages and several
  header keys (`aiAbility`, `aiCollectors`, `aiGeyserAttachments`,
  `aiMoneyRechargeRate`, `aiNoTemple`, `moreGeysers`, `myProd`, seat
  colours). Changing missions re-execs the process instead of rebuilding the
  world in place.
- Network play is plain TCP with no late join as a new player, no
  server-side simulation and no TLS/QUIC.
- There is no CI, packaging or desktop file yet. Generated art covers ground,
  undersides and fire; units, buildings, rims and bridges still come from the
  original installation.
- Several rules figures are explicitly marked as guesses. Prefer a
  play-test finding and a `rules.toml`/Rhai change over speculative code.

## Definition of done

Before reporting a task complete:

1. `cargo test --workspace --no-fail-fast` passes.
2. `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets`
   pass when those components are available.
3. `reuse lint` passes.
4. Documentation that owns the changed behavior is updated.
5. No NetStorm-derived data, build artifacts, screenshots or background
   processes are left in the tree.
