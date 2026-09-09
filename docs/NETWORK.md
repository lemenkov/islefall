# Network play

Islefall is built for deterministic lockstep, the model NetStorm itself
used. This page records the design and what exists so far.

## The simulation is the contract

`islefall-sim` runs at a fixed tick rate on integer arithmetic, takes no
wall-clock time, seeds its own randomness from the rules, and changes only
through `World::apply(owner, command)`. The opponent AI works from the
world state and a seeded queue, so it runs identically everywhere. Every
machine that starts from the same map and rules and applies the same
commands in the same order at the same ticks ends with the same world;
`World::hash()` is the digest that proves it.

Commands are the `Command` enum in `crates/islefall-sim/src/command.rs`:
move, place a piece (by queue slot and rotation, never by cells, so the
queue stays in step), drop, place a unit, produce, harvest, capture,
sacrifice, read, cast, pray, salvage, upgrade, and the Battlemaster's
bridge tools. A refused command is part of the record and is refused the
same way on every machine.

## Replays

`ISLEFALL_RECORD=game.toml` writes every command the local player gives,
with its tick; `ISLEFALL_REPLAY=game.toml` plays such a file back into a
fresh game on the same map. The rules' `hash_every_seconds` logs the
world's hash at intervals; two runs of one replay must log the same
hashes, and a replayed command being refused means the game has drifted
from the recording. This is the determinism guard everything below rests
on, and it is also how bugs get reported: a replay file reproduces them.

## The design for playing over a network

- **Lockstep with a relay server that also runs the game.** Clients send
  commands to the server; the server stamps them into numbered turns (a
  turn being a few ticks) and broadcasts each turn's list; a client
  advances only once it holds the turn. No state crosses the wire in
  play. The server runs the same simulation alongside, which gives it
  snapshots for reconnect and late join, hash checks to catch a desync
  at the turn it happened, and a lobby that knows the score.
- **Transport**: one reliable, ordered, encrypted connection from each
  client to the server. QUIC (the `quinn` crate) is the first choice:
  TLS built in, reliable streams, survives address changes. Plain TCP
  with length-prefixed frames is the simpler fallback; a WebSocket
  variant later would allow a browser client. Players never need port
  forwarding.
- **Protocol**: one versioned serde enum, serialised with `postcard`.
  Client to server: hello (protocol version, name, session token, and a
  hash of rules, scripts and map so everyone plays the same data), lobby
  actions, commands. Server to client: welcome or reject, lobby state,
  one message per turn, a snapshot on join, a desync notice.
- **AI opponents** run inside every client's simulation as now, which is
  free and deterministic.

## What exists

- `crates/islefall-net`: the messages and the framing (a little-endian
  length and postcard bytes), blocking for the client and async for the
  server behind the `tokio` feature, and the data hash clients present.
- `crates/islefall-server`: a relay over TCP. Clients say hello with
  their name, map and data hash; the first player's data is the
  standard and anyone differing is refused. Once `min_players` have
  connected and said ready the game starts; every `turn_ticks` ticks the
  server sends the commands it received as one turn. Clients send their
  world hash every `hash_every_turns`, and the server prints agreement
  or broadcasts a desync with everyone's hashes. It runs no simulation
  yet, so a client that drops cannot rejoin.
- The app joins with `ISLEFALL_JOIN=host:port` and `ISLEFALL_NAME`,
  sends every command to the server instead of applying it, and moves
  its world only by received turns; the player number comes from the
  server. The opponent AIs run inside every client identically.

## Order of work

1. Commands, replays and the world hash (done).
2. Snapshots: serde on the world, for saving, loading and sending (done: `World::snapshot` and `World::restore`, binary through postcard; `F5` and `F9` in the app).
3. A protocol crate shared by client and server (done).
4. A server binary without Bevy: lobby, turn relay, hash checks, a TOML
   config (done); snapshots for rejoin (to do, needs the server to run
   the simulation or fetch a snapshot from a client).
5. Client integration: connect from the command line, lockstep loop
   driving the simulation from received turns (done).
6. Later: TLS or QUIC, a lobby screen, rejoin, browser transport,
   matchmaking, persistence.
