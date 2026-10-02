# Project

A clone of Factorio: a factory automation game with tile-based terrain, belts,
inserters, machines, and a research tree.

The project is split into two halves that communicate over the network:

- **[backend.md](backend.md)** — authoritative simulation server, written in Rust.
- **[frontend.md](frontend.md)** — rendering, input, and UI client.

Plus one cross-cutting design doc:

- **[storage.md](storage.md)** — the in-memory data model: terrain, entity pools,
  and the spatial index both halves depend on.

## Quick start

You need a Rust toolchain (edition 2021; developed against 1.99) and Node 20 or
newer. Nothing else: the backend has no third-party crates, and the frontend has
no runtime dependencies.

Run the headless simulation:

```sh
cd backend
cargo run -p factorio-server -- 7 2 600
```

Run the web client in a second terminal:

```sh
cd frontend
npm install
npm run dev
```

Then open <http://localhost:5173>. The client boots its own demo world, then
connects to the backend automatically: once the handshake lands it adopts the
server's seed and goes live. While no server is reachable it keeps running the
demo world, so the page is never blank. Start the backend in listener mode so
there is something to connect to:

```sh
cd backend
cargo run -p factorio-server -- --serve
```

It listens on `ws://127.0.0.1:9000/ws`, which is exactly what the client dials.

There is also a convenience script that starts both halves together:

```sh
./run.sh                # listener + dev server, Ctrl-C stops both
./run.sh --batch        # batch backend + dev server
./run.sh --help         # all options
```

With the listener running, the browser client connects automatically on load and
goes live.

Each half is run from its own directory on purpose: Cargo builds the backend
workspace from `backend/`, and Vite treats `frontend/` as its root. The
[Running it](#running-it) section below covers both in detail.

## Architecture at a glance

```
┌───────────────────────────────────────────────┐
│ frontend (TypeScript, WebGL2)                 │
│  • renders world, entities, GUI               │
│  • predicts locally, reconciles on correction │
└───────────────┬───────────────────────────────┘
                │ WebSocket, binary protocol
┌───────────────┴───────────────────────────────┐
│ backend (Rust)                                │
│  • fixed-tick simulation, headless            │
│  • persistence, chunk streaming, anti-cheat   │
└───────────────────────────────────────────────┘
```

## Design principles

**The backend is the only source of truth.** The frontend never decides what
happens. It may *predict* — moving a ghost belt, pre-flighting an insert — but
every prediction is speculative and is discarded the moment the server's
authoritative state arrives.

**Tick-based determinism.** The simulation advances in discrete ticks (60 Hz by
default). Entity behaviour is pure: `(state, tick, inputs) -> state'`. No wall
clock, no floating-point accumulation across ticks, no hash-map iteration order
leaking into results. Given the same seed and the same input stream, the
backend produces byte-identical state. This is what makes replays, lockstep
testing, and server migration possible.

**Everything is data, not code.** Machines, items, recipes, and entities are
defined in data files (see `backend.md`), not as compiled-in logic. A new
machine should not require a recompile.

**Bands, not threads, for content.** Machines move items in discrete bands
(`band_1`, `band_2`, …) rather than physical simulation. This is far cheaper,
perfectly deterministic, and matches the genre's feel.

**Streaming world.** The map is divided into chunks generated on demand and
released when unobserved, so a 1000×1000 map costs no more memory than a
100×100 one.

## Scope

In scope for the initial playable build:

- Map generation with four resources: metal, copper, stone, and coal.
- Belts, underground belts, splitters, inserters, filters.
- Mining, smelting, crafting, assembling.
- Electric network: generators, boilers, poles, power distribution.
- Pipes and fluid handling.
- Player character with movement, mining, building, and crafting.
- Enemies that attack the factory.
- Research tree with technology unlocks.
- Save/load, multiplayer, and server-side event log for replays.

Explicitly out of scope: full mod support (data-file architecture is designed to
allow it, but the mod runtime is not part of this build), vehicles, and
space-age content.

## Getting started

Read [storage.md](storage.md) first, then [backend.md](backend.md) and
[frontend.md](frontend.md). The data model comes first because both halves are
generated against it, and because a headless simulation that runs correctly is
the foundation everything else needs. Implementation notes for the client live in
[frontend/README.md](frontend/README.md); the crate-by-crate backend layout, the
wire protocol and the determinism check are in [Running it](#running-it) below.

## Running it

The commands below all assume you have already changed into the directory named
in the section heading.

The backend is a Cargo workspace in `backend/`. It is std-only — there are no
third-party crates, so `cargo build` needs nothing but a Rust toolchain
(edition 2021; developed against 1.99).

```sh
cd backend

cargo build              # debug build
cargo build --release    # optimized (opt-level 3 + LTO)
cargo test --workspace   # 98 tests across the 7 crates
```

### The headless server

`crates/server` produces a `factorio-server` binary. It takes three positional
arguments — seed, chunk view radius, and tick count — and defaults to `1 2 1200`:

```sh
cargo run -p factorio-server -- 7 2 600
```

```
factorio-server seed=7 view_radius=2 ticks=600
tick=600 hash=540c084eda419bf9 power=0/0 chunks=25 events=1
final tick=600 hash=540c084eda419bf9 belts=6 machines=2 entities=13
wrote save to save.fsv
```

It runs entirely in-process: it applies a short scripted set of placement
intents, advances the simulation for the requested number of ticks, streams
chunks around the player, prints the final state hash, and writes `save.fsv` to
the **current working directory** when it exits. Run it somewhere disposable if
you do not want that file:

```sh
mkdir -p /tmp/factorio-run && cd /tmp/factorio-run
/path/to/backend/target/release/factorio-server 7 2 600
```

#### The listener

Pass `--serve` to run a WebSocket listener instead of the batch simulation. It
takes an optional address (default `127.0.0.1:9000`), seed and view radius, and
runs at a fixed 60 Hz until interrupted:

```sh
cargo run -p factorio-server -- --serve                       # 127.0.0.1:9000, seed 7
cargo run -p factorio-server -- --serve 0.0.0.0:9000 8 3
```

```
factorio-server listening on ws://127.0.0.1:9000 seed=7
serve: client 1 connected from 127.0.0.1:52344 (1 total)
```

Clients use the binary protocol from `crates/proto` over WebSocket frames. The
handshake and framing are hand-written in `crates/net` (`ws.rs`) because the
backend has no third-party crates. A client sends `Hello`, `Intent` and `Ping`
messages; the server answers with `Hello` plus a replay of every existing
entity, then broadcasts a `Snapshot` every 6 ticks and an `Event` for each
simulation event. The browser client in `frontend/` speaks this protocol and
connects automatically on load (press `C` to toggle).

### The web client

`frontend/` is a Vite + TypeScript project with no runtime dependencies, using
raw WebGL2 for rendering:

```sh
cd frontend

npm install
npm run dev        # http://localhost:5173
npm test           # 274 tests
npm run build      # typecheck, then bundle into dist/
```

The client boots a demo world — real terrain generation on a fixed seed, a small
factory layout, and a 60 Hz tick loop — and only talks to a server when you press
`C`. On connect it adopts the server's seed, rebuilds terrain from it, and follows
the authoritative tick. See
[frontend/README.md](frontend/README.md) for the controls, the module layout, and
how the protocol, terrain, and content fixtures are regenerated from the backend
so the two halves cannot drift apart.

### Determinism check

The state hash printed at the end is a fingerprint of the whole simulation, so
the same seed and input stream must reproduce it exactly:

```sh
cargo run -q -p factorio-server -- 7 2 600 | grep final   # hash=540c084eda419bf9
cargo run -q -p factorio-server -- 7 2 600 | grep final   # identical
cargo run -q -p factorio-server -- 8 2 600 | grep final   # different
```

This is the quickest end-to-end check that nothing in the tick loop has picked
up a wall clock, a floating-point accumulation, or a hash-map iteration order.

### Crate layout

| Crate | Role |
| --- | --- |
| `crates/content` | TOML parser and the compiled prototype registry (items, machines, belts, inserters, recipes, technologies) |
| `crates/sim` | the deterministic simulation: terrain, entity pools, belts, inserters, machines, fluids, power, research, and the fixed phase order in `tick.rs` |
| `crates/world` | chunk streaming around a focus point, releasing empty chunks |
| `crates/proto` | binary message encoding for client/server intent and snapshots |
| `crates/net` | framed transport, sessions, and a hand-written WebSocket handshake/frame codec (`ws.rs`) |
| `crates/persistence` | save capture, serialization, restore, and migration validation |
| `crates/server` | server wiring: intent validation, ticking, events, save integration, and the `--serve` listener |

Content lives in `crates/content/content/*.toml` and is compiled into the binary
with `include_str!`, so the server does not read anything from disk at startup
and produces the same world regardless of where it is launched from.

### Tests

```sh
cargo test -p factorio-sim         # simulation: terrain, pools, belts, power, research
cargo test -p factorio-content     # TOML parsing and prototype ordering
cargo test -p factorio-persistence # save round-trip and migration checks
cargo test --workspace             # everything
```

`cargo clippy` and `cargo fmt` are not installed in this environment; install
them with `rustup component add clippy rustfmt` if you want them.