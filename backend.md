# Backend

The authoritative game server, written in Rust. It owns the world, advances it
one tick at a time, and tells clients what happened. It runs headless — the
backend has no idea a screen exists.

Consumed by the frontend as described in [frontend.md](frontend.md).

## Responsibilities

- Run the fixed-tick simulation (60 Hz).
- Generate and stream map chunks.
- Validate every client action and reject cheating (the client sends *intent*,
  never results).
- Persist and load games.
- Broadcast state deltas to connected clients.

## Crate layout

```
backend/
├── Cargo.toml            # workspace
├── crates/
│   ├── sim/              # the simulation core: pure, deterministic, no I/O
│   │   ├── tick.rs       #   tick loop and phase ordering
│   │   ├── entity.rs     #   entity pools + typed IDs (see storage.md)
│   │   ├── grid.rs       #   occupancy index, spatial queries, collision
│   │   ├── belt.rs       #   belt lanes and item transport
│   │   ├── machine.rs    #   crafting/processing state machines
│   │   ├── power.rs      #   electric network satisfaction solver
│   │   ├── fluid.rs      #   pipes, tanks, fluid networks
│   │   ├── combat.rs     #   enemies, projectiles, damage
│   │   └── player.rs     #   player character
│   ├── content/          # data-driven definitions, loaded at runtime
│   │   ├── entities.toml
│   │   ├── items.toml
│   │   ├── recipes.toml
│   │   └── technology.toml
│   ├── world/            # map generation and chunk streaming
│   ├── proto/            # serialization types shared with the frontend
│   ├── net/              # transport, sessions, delta encoding
│   ├── persistence/      # save files, snapshots, migrations
│   └── server/           # binary entry point, config, wiring
└── benches/              # tick-rate and tick-budget benchmarks
```

The dependency arrow only ever points inward: `server` depends on `sim`,
never the reverse. `sim` depends on nothing from this project except
`content`'s data structures. This is what keeps the simulation testable and
portable.

The concrete in-memory data model — terrain layout, entity pools, the occupancy
index, and the belt/machine/inserter representations — is documented
separately in [storage.md](storage.md).

## Determinism

Determinism is the hard requirement. It means:

- **No wall clock.** Ticks advance by counting iterations. Any real-time value
  enters the simulation only as a player input.
- **No unseeded randomness.** An explicit PRNG (PCG32) in world state; all
  random draws go through it.
- **Fixed-width, ordered iteration.** `u32`/`i32`/`f32` only — never `usize`
  (pointer width) and never `f64` in tick-advancing math. Entity IDs are dense
  indices, and iteration is always by ascending ID, never by hash-map order.
- **No interior mutability across ticks.** Parallelism uses rayon over
  *independent* spans (e.g. separate electric networks, separate chunks); each
  span merges deterministically into one output buffer.

A debug build asserts determinism by running the same input stream twice in one
process and comparing state hashes every N ticks.

## The tick

Each tick runs ordered phases. Ordering is part of the design, not an
implementation detail — moving power after inserters changes behavior.

1. **Input drain** — apply queued commands from the network layer.
2. **Movement** — players, enemies, projectiles.
3. **Belts** — advance items, handle compression and side insertions.
4. **Inserters** — swing, pick up, drop.
5. **Machines** — progress recipes, consume inputs, produce outputs.
6. **Fluids** — move fluid through pipes and across networks.
7. **Power** — recompute demands, then satisfy them in priority order.
8. **Combat** — apply damage, resolve deaths, spawn drops.
9. **Tick events** — update counters, tick timers, emit events for the net layer.

`tick.rs` owns this list as a single ordered enum so the ordering is greppable
and cannot drift.

## Data-driven content

Machines, items, recipes, and technologies are TOML files compiled into a
registry at startup. A machine is a component set plus a recipe graph, not a
`struct` with hand-written `tick` code:

```toml
# content/entities.toml
[entity.assembler_1]
name = "Assembler"
size = [3, 3]
components = ["machine", "power_consumer", "collision"]
crafting_categories = ["crafting"]
energy_usage = 90_000          # watts
max_power = 900_000
pollution = 4
```

The `components` list is **classification, not an ECS component set**. Each entry
maps to a field or behavior group in the corresponding pool struct, and the
system passes in `tick.rs` skip an entity whose prototype does not declare that
component. There is no component storage, no archetype, and no query planner:
entities live in dense per-type pools (see [storage.md](storage.md)), and a
machine is one struct with a `kind` that selects a prototype.

This keeps the entity count high without the code count growing to match, and it
means a content update is a file edit, not a release.

## Networking model

Clients connect over WebSocket and exchange binary messages (compact bincode or a
hand-rolled varint format; see `proto/`).

Three message classes:

- **Client → server: intent.** "I want to place an assembler at (412, -88)."
  The server validates reachability, ownership, collision, and player
  inventory, then applies it. The client never sends resulting state.
- **Server → client: snapshots and deltas.** Newly-connected clients receive a
  chunk snapshot; connected clients receive per-tick deltas for the chunks they
  can see, interest-managed by an AOI (area of interest) grid.
- **Server → client: events.** Transient things — an item was mined, a
  research completed, an alert fired. Events are reliable and ordered;
  deltas are not acknowledged and periodically re-based by a fresh snapshot so
  a dropped delta cannot corrupt state permanently.

Ticks are not globally synchronized across clients. Each client tracks its own
render interpolation clock and the server's tick number, so a client on a slow
link shows the world slightly behind rather than stalling.

## Persistence

The world is stored as chunk files, each holding entity data in the same
deterministic ID order used in memory. Autosave runs on a background thread
from a consistent snapshot: the sim hands off an immutable copy of state, never
lets the writer see a half-updated tick.

## Performance targets

- Tick budget: **4 ms** at 60 Hz for a target load of ~50 000 entities.
- Server allocates nothing per tick in steady state — all buffers are pooled.
- Chunk generation: under 5 ms per chunk, off the tick thread, streamed in a
  ring around each player.

## Testing

- **Golden tests** — run a scripted input stream and assert a state hash, so
  any behavioral regression fails loudly.
- **Property tests** — assert invariants: no item is ever created or destroyed,
  no item exists in two places, power is conserved, no entity overlaps.
- **Headless soak tests** — build a real factory from a script and assert it
  keeps running at tick rate.