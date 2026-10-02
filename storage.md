# Storage

How the simulation represents the world in memory. This is the concrete form of
the data-driven and determinism rules described in [backend.md](backend.md); it
is the file to read before writing any tick logic.

At the target load of ~50 000 entities, this design keeps the whole world in
about 12 MB and leaves the 4 ms tick budget dominated by belt and machine
simulation rather than by storage indirection.

## Scope

This document covers everything needed to take raw ore out of the ground and
turn it into a running automated factory: miners, furnaces, assemblers, belts,
inserters, power distribution, and research.

Resource types in scope: **metal, copper, stone, coal**. Water is also in scope,
but as terrain rather than a resource — see
[Water is a flag, not a resource](#water-is-a-flag-not-a-resource).

## The three tiers

Four different kinds of data are lumped together by the question "where should
I store this?", but they want opposite designs. Split them by *who mutates them*
and *how often*.

| Data | Mutated by | Count | Pattern |
|---|---|---|---|
| Ore type and amount per tile | Map gen, mining | millions of tiles | Columnar arrays per chunk |
| Water presence per tile | Map gen, pumping | millions of tiles | Bit in the chunk flag array |
| Belts, machines, inserters, trees | Build/remove, tick | ~50 000 | Typed ID pools |
| "What occupies tile T?" | Build/remove only | one per tile | Spatial index |
| Items and fluids | Tick | variable | Packed values inside entities |

The first tier is read-mostly and wants cache residency. The second is mutated
constantly and wants dense iteration. The third exists purely to answer
point-lookup queries. Treating them uniformly is the main mistake to avoid.

## Coordinates and chunking

The world is a single surface — `y` is vertical but not a navigable z-level.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct TilePos { pub x: i32, pub y: i32 }

impl TilePos {
    /// Packed key for hashing and indexing. i32 casts to u32 are
    /// lossless and wrap correctly for negative coordinates, so this
    /// is a bijection over the whole coordinate space.
    #[inline]
    pub fn key(self) -> u64 {
        ((self.x as u32 as u64) << 32) | (self.y as u32 as u64)
    }
}

pub const CHUNK_SIZE: i32 = 32;
pub const CHUNK_AREA: usize = 32 * 32;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkPos { pub x: i32, pub y: i32 }

impl From<TilePos> for ChunkPos {
    fn from(p: TilePos) -> Self {
        ChunkPos { x: p.x.div_euclid(CHUNK_SIZE), y: p.y.div_euclid(CHUNK_SIZE) }
    }
}

#[inline]
pub fn chunk_origin(c: ChunkPos) -> TilePos {
    TilePos { x: c.x * CHUNK_SIZE, y: c.y * CHUNK_SIZE }
}
```

`div_euclid` rather than `/`, so chunks tile correctly across negative
coordinates. A map whose origin sits at `(0, 0)` will otherwise have a chunk
boundary at zero that does not align with the grid.

## Tier 1 — Terrain

Terrain holds only data that is **neither derivable from entities nor expressible
as an entity**. Ore fits: it is passive, per-tile, has no behavior beyond being
mined, and exists in the millions. Ground type and elevation were considered and
cut for now — nothing in the scope above needs them yet, and empty arrays are
cheaper to add later than to maintain.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Resource { Metal, Copper, Stone, Coal }

impl Resource {
    pub const COUNT: usize = 4;
    pub fn from_index(i: u8) -> Option<Self> {
        use Resource::*;
        Some(match i { 0 => Metal, 1 => Copper, 2 => Stone, 3 => Coal, _ => return None })
    }
}

/// A newtype rather than a `#[repr(u16)] enum` because flags combine with `|`
/// and test with `&`, which an enum cannot express without an external crate.
/// `sim` deliberately has no dependencies, so the combinators live here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(transparent)]
pub struct TileFlag(pub u16);

impl TileFlag {
    pub const NONE: TileFlag = TileFlag(0);

    /// Tile is water. Present or absent, never a quantity — see "Water is a
    /// flag, not a resource" below.
    pub const WATER: TileFlag = TileFlag(1 << 0);

    /// Reserved; blocks entity placement. Not used in current scope.
    pub const CLIFF: TileFlag = TileFlag(1 << 1);

    #[inline] pub fn is_set(self, flag: TileFlag) -> bool { self.0 & flag.0 != 0 }
    #[inline] pub fn set(&mut self, flag: TileFlag) { self.0 |= flag.0 }
    #[inline] pub fn clear(&mut self, flag: TileFlag) { self.0 &= !flag.0 }
}
```

```rust
pub struct TerrainChunk {
    pub resource:   Box<[u8;  CHUNK_AREA]>,  // Resource index, 0 = none
    pub ore_amount: Box<[u16; CHUNK_AREA]>,  // depletes as mined, 0 when exhausted
    pub flags:      Box<[u16; CHUNK_AREA]>,  // TileFlag bits; WATER is live
}
```

Design notes:

- **Columnar, not interleaved.** One array per field, so "scan every tile with
  coal" walks a contiguous 1 KB array instead of striding across an
  `Option<Resource>` per tile. Columnar is what makes the common query fast.
- **Fixed-size arrays, not `Vec`.** `Box<[u8; CHUNK_AREA]>` has no length field,
  no capacity field, and no reallocation. `CHUNK_AREA` is a compile-time constant
  so bounds checks fold away. Terrain arrays are allocated once at chunk load and
  never resized.
- **`u8` resource index, not an enum field.** Storing the index rather than the
  enum keeps the array a flat byte plane for generation and serialization, and
  leaves room for mod-supplied resources without widening every tile.
- **`u16` ore amount.** Measured in mining units; base ore patches are ~500, and
  a `u8` would quantize small patches too coarsely to mine smoothly. `u16` gives
  comfortable headroom without wasting a byte more.

### Water is a flag, not a resource

Water is terrain state, not a mined quantity, and the storage reflects that
completely.

A water tile has `TileFlag::WATER` set, `resource == 0`, and `ore_amount == 0`.
It is **not** a fifth `Resource` variant, and it has no `ore_amount` — a lake has
no mining progress to track. Three consequences fall out of this:

- **Mining a water tile is not a partial operation.** A mining drill cannot mine
  water at all; it must be an offshore pump. There is no "how far along is this
  lake" question to answer, because there is no amount to decrement.
- **Extraction clears the flag.** An offshore pump yields a water item and then
  calls `flags[i].clear(TileFlag::WATER)`. One bit flip, no arithmetic, no
  half-mined shoreline states to render.
- **Tile cost is 2 bytes.** Water costs one bit of an array that already exists,
  where modelling it as a resource would have cost a full `u16` amount on every
  tile in the world to store a value that is only ever `0` or nonzero.

This also means the water flag participates in chunk generation as a terrain
pass, and the `flags` array earns its place in the layout — it is no longer a
sparse placeholder.

A tile is passable when it holds no entity and is not water. Do **not** cache a
`BLOCKED` bit in `flags`: passability is derived, and a second copy would
desync from the occupancy index when an entity dies.

### Chunk registry

```rust
pub struct World {
    pub chunks: HashMap<ChunkPos, Box<TerrainChunk>>,
    // ...
}
```

Chunks are generated on a worker thread and swapped in at the AOI boundary.
Chunk generation must not run inside a tick — see the streaming notes in
[backend.md](backend.md).

## Tier 2 — Entity pools

Entities are stored in **one dense `Vec` per entity type**, not in a
general-purpose ECS.

This is the most important structural decision in the document, so it is worth
being explicit about why. A component ECS earns its complexity when entities are
composed from many optional parts and queried in many different ways. Here the
entity types are few and fixed, the queries are few, and the dominant operation
is "tick every belt" — a linear walk over one contiguous array. An ECS would
route that walk through archetypes and bit-flag tables to arrive at the same
memory, while adding a moving part between the tick phases and the data.

A pool also removes an entire class of bug. Because each pool has its own ID
type, an inserter can only ever hold a belt ID or a machine ID, never a tree ID.
The compiler rejects the mistake instead of the world silently losing items.

```rust
pub struct Pool<T> {
    items: Vec<T>,
    free: Vec<u32>,  // LIFO free list of reusable indices
}

impl<T> Pool<T> {
    /// Allocate, reusing a free index if one exists. This does not zero the
    /// slot; every constructor must write every field.
    pub fn alloc(&mut self, value: T) -> u32 { /* ... */ }

    /// Release. Returns the value so callers can drop stored references.
    pub fn dealloc(&mut self, index: u32) -> Option<T> { /* ... */ }

    #[inline]
    pub fn get(&self, index: u32) -> Option<&T> { self.items.get(index as usize) }

    #[inline]
    pub fn get_mut(&mut self, index: u32) -> Option<&mut T> {
        self.items.get_mut(index as usize)
    }

    #[inline]
    pub fn len(&self) -> u32 { self.items.len() as u32 }

    /// Ascending-index order. This *is* the deterministic iteration order
    /// required by backend.md — there is no query planner to keep in sync.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &T)> { /* ... */ }
}
```

Pool indices are dense and start at zero, so a pool index is a valid save-file
reference on its own. Remove an entity with `swap_remove` and the moved entity
takes over the vacated index; nothing else stores that index as identity, so
this stays sound, and iteration order remains deterministic.

### IDs and cross-references

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BeltId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct MachineId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct InserterId(pub u32);
```

Every pool gets its own newtype ID. These are four lines of code per entity type
and they buy compile-time proof that references are well-typed — an inserter's
`source` field is `Option<BeltId>` and cannot be assigned a `MachineId`.

For the occasional entity that genuinely holds one of several kinds, use a
`u32` pool index plus a small `EntityKind` tag:

```rust
pub enum AnyId { Belt(BeltId), Machine(MachineId), Inserter(InserterId) }
```

Reach for this only at the boundaries — occupancy queries, save files, and the
network protocol. It should not appear inside tick logic.

### World

```rust
pub struct World {
    pub chunks:   HashMap<ChunkPos, Box<TerrainChunk>>,
    pub occupied: OccupancyIndex,

    pub belts:     Pool<TransportBelt>,
    pub machines:  Pool<Machine>,
    pub inserters: Pool<Inserter>,
    pub players:   Pool<Player>,
}
```

## Tier 3 — The occupancy index

Every spatial question — "can a 3×3 assembler fit here", "what is this inserter
grabbing from", "which entity did the player click" — funnels through one
structure:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Occupant {
    pub kind: EntityKind,
    pub index: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EntityKind { Belt, Machine, Inserter, Player }

pub struct OccupancyIndex {
    by_tile: FxHashMap<u64, Occupant>,
}
```

A hash map keyed by packed tile position, rather than a dense grid. At 50 000
entities the dense alternative saves perhaps 2 MB and a few nanoseconds per
lookup while requiring chunk-unload remapping logic that is genuinely difficult
to get right. The hash map is the right trade at this scale; the trait exists so
the swap stays a one-file change if the target moves up an order of magnitude.

```rust
pub trait OccupancyIndex {
    fn get(&self, pos: TilePos) -> Option<Occupant>;
    fn set(&mut self, pos: TilePos, occ: Occupant);
    fn remove(&mut self, pos: TilePos);
    fn is_free(&self, pos: TilePos, size: (i32, i32)) -> bool;
}
```

Key points:

- **Multi-tile entities register every covered tile.** A 3×3 assembler writes its
  index into nine slots, so a click anywhere on it resolves to the machine.
- **`get` does not check whether the occupant is still alive.** Remove from the
  index inside `dealloc` wrappers, never in tick code.
- **The occupancy index holds entities only, never terrain.** A water tile is
  empty in this index. "Is this tile usable" is `occupancy.get(pos).is_none()`
  *and* `!flags[pos].is_set(TileFlag::WATER)` — two independent questions, both
  answered from their own source of truth.

## The entity catalog

Everything required to go from untouched ground to a self-sustaining factory.
Grouped by role, roughly in the order a new player encounters it.

### Belts and transport

| Entity | Role |
|---|---|
| **Transport belt** | 2 tiles/s. The backbone. Moves items 4 tiles/s diagonally. |
| **Splitter** | 2×2. Diverges or merges belt lanes. |
| **Underground belt** | Buries a belt run under other belts. Entrance and exit are separate entities, so a long line can span many chunks without surface routing. |

### Handling

| Entity | Role |
|---|---|
| **Inserter** | Moves 1 item per swing between belt, machine, or ground. The critical entity: it is the only bridge between transport and processing. |
| **Fast inserter** | 2.5 items/s. Unlocks high-throughput machines. |
| **Long-handed inserter** | 2 tiles reach. Spans a gap without tiling belts. |

### Mining

| Entity | Role |
|---|---|
| **Burner mining drill** | 3×3, no electricity, burns coal directly. The starting point. |
| **Electric mining drill** | 3×3, faster, costs power. Requires research. |

### Smelting

| Entity | Role |
|---|---|
| **Stone furnace** | 2×2, no electricity, burns coal. Produces metal plate, copper plate, brick, stone brick. |
| **Electric furnace** | 2×2, smelts twice as fast, costs power, and unlocks steel. |
| **Steel furnace** | 2×2, steel only, no power. |

### Crafting and assembly

| Entity | Role |
|---|---|
| **Assembler** | 3×3, consumes power. The core production entity — gear wheels, circuits, engines. |
| **Chemical plant** | 3×3. Sulfur, plastics, explosives. Requires oil, not in the current resource set. |

### Power

| Entity | Role |
|---|---|
| **Burner generator** | 1×1, burns coal for electricity. The bootstrap power source — it needs neither water nor a boiler, so it unlocks before offshore pumping is researched. |
| **Steam engine** | 2×2, converts steam to power. |
| **Boiler** | 2×2, boils water with coal. |
| **Offshore pump** | 1×1, extracts water into an item and **clears `TileFlag::WATER`** on the tile. Consumes the flag rather than decrementing an amount. No electricity needed. |
| **Solar panel** | 3×3, free power in daylight. |
| **Accumulator** | 2×2, stores surplus charge for night. |
| **Small electric pole** | Supplies nearby buildings. |


### Research and player

| Entity | Role |
|---|---|
| **Lab** | 3×3, consumes science packs, produces research progress. |
| **Player** | Movable, carries inventory, builds and mines. |

### Items

Items are data, not entities — a stack is a pair of integers.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stack {
    pub item:  u16,  // index into the item registry
    pub count: u16,
}
```

Four bytes, `Copy`, and trivially comparable. Keeping items as registry indices
rather than Rust enums is what allows content to be extended from data files —
`content/items.toml` defines the registry, and `u16` leaves 65 000 slots.

The distinction that matters: an **item** is a resource in the world, while an
**entity** is a placed object with behavior. A stack of copper plate is an item;
a stone furnace holding a stack is an entity containing one.

## Belt storage

Belts are 60–80% of all entities at steady state and get their own
representation rather than sharing the generic item path. A belt's identity is
its lanes and its connections, not an inventory.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TransportBelt {
    pub pos:   TilePos,      // anchors the belt; direction is stored packed
    pub flags: u8,           // bit 0-1 direction, bit 2 curve, bit 3 side
    pub next:  u32,          // downstream belt index, u32::MAX if terminated
    pub prev:  u32,          // upstream belt index, u32::MAX if unconnected
}
```

Direction, curve, and side pack into a single byte because three `u8` fields
would spend eight bytes on three values that never change together. Item
transport state lives in a parallel structure keyed by belt index, holding two
`[Stack; 4]` lane buffers.

```rust
pub struct BeltLanes {
    /// Indexed by belt index. Each belt owns two 4-slot halves: the half it
    /// accepts into and the half it moves out of.
    pub halves: Vec<[Stack; 8]>,
}
```

Belts are modeled as carrying items forward with the "next" pointer giving O(1)
transport direction, rather than the alternative of scanning all belts each tick
and computing direction from geometry. `prev` is what makes side-loading and
merging tractable.

Belt lanes are the one place item counts exceed what a `u16` naturally wants;
use `u32` for lane occupancy when belt compression is implemented so a full lane
does not overflow a stack count.

## Machine storage

```rust
#[derive(Clone, Copy, Debug)]
pub struct Machine {
    pub kind:     u8,      // index into the machine prototype registry
    pub pos:      TilePos,
    pub size:     u8,      // footprint edge length: 1, 2, or 3
    pub recipe:   u16,     // index into the recipe registry, u16::MAX if idle
    pub progress: u32,     // ticks accumulated toward the current recipe
    pub in_buf:   [Stack; 4],
    pub out_buf:  [Stack; 4],
    pub power:    u32,     // last-computed satisfaction ratio, 0..=65535
}
```

Machines are **state machines, not entity subclasses**. One struct, a `kind` that
selects a prototype, and a `recipe` index into the recipe registry. Adding a new
machine is a data-file edit, not a new Rust type and not a new pool.

`progress` in ticks rather than seconds keeps the tick deterministic and avoids
accumulating floating-point error across frames. `power` stores the last
computed satisfaction ratio, so the power phase runs once per tick and the
machine phase reads the result instead of re-querying the grid.

## Inserter storage

```rust
#[derive(Clone, Copy, Debug)]
pub struct Inserter {
    pub pos:     TilePos,
    pub dir:     u8,
    pub source:  u32,        // Occupant index of the pickup side
    pub target:  u32,
    pub held:    Option<Stack>,  // item currently in the grab
    pub progress: u32,       // 0..=SWING_TICKS
}
```

Inserters cache their `source` and `target` occupant indices at construction time
and refresh on rebuild, rather than re-querying the occupancy index every tick.
Two occupancy lookups per inserter per tick is affordable, but caching turns it
into zero and makes the swing logic straightforward.

## Memory budget

At the 50 000-entity target, per-entity costs:

| Structure | Bytes each | Notes |
|---|---|---|
| Belt | ~16 | Dense; the dominant count. |
| Machine | ~40 | In/out buffers dominate the size. |
| Inserter | ~24 | Two cached indices plus held stack. |
| Occupancy entry | ~8 | Plus hashmap overhead, roughly 1.5× amortized. |
| Terrain tile | 4 | `u8` resource + `u16` amount + `u16` flags. Water adds no per-tile cost. |

A loaded view of, say, 40×40 chunks is 51 200 tiles, which is about 200 KB of
terrain regardless of how many entities sit on it. Terrain is not the problem to
optimize; belt iteration is. Water in particular costs **zero extra memory** — it
shares the `flags` array that already exists, so a fully submerged map is the
same 4 bytes per tile as a barren one.

The practical consequence: at this scale there is no reason to bit-pack entity
structs or use a dense occupancy grid. Both are premature. Reach for them only
after a profiler points at a specific structure, and treat any such change as a
determinism review, since both affect iteration and index layout.

## Save format

Save files store **pool arrays in index order** plus a chunk map, with
structural versioning for migrations.

```rust
pub struct SaveFile {
    pub version: u32,
    pub tick:    u64,
    pub rng:     RngState,     // PCG32 state, so the world resumes identically
    pub chunks:  HashMap<ChunkPos, ChunkData>,  // resource, ore_amount, flags
    pub pools:   PoolData,     // each pool's Vec, serialized in index order
    pub occupancy: OccupancyData,
}
```

Three properties are worth preserving deliberately:

- **RNG state is part of the save.** Replaying from a load must continue the
  same random sequence, or determinism is broken across save/load boundaries.
- **Pools serialize in index order**, matching in-memory iteration order. A
  load is therefore a straight read into the same layout, and a post-load state
  hash matches the pre-save one — which is what makes the golden tests in
  [backend.md](backend.md) meaningful.
- **Chunk arrays serialize columnar, not interleaved.** The on-disk form
  matches the in-memory form, so loading is a bulk read with no per-tile
  shuffling. `flags` is written as a bit plane — 128 bytes per chunk rather than
  2 KB — which is worth doing because water is the only live flag today and the
  bit plane is measurably smaller on every chunk that contains any.

## Invariants

These are the properties the storage layer must hold. They belong in property
tests, listed alongside the simulation's own invariants:

1. **Every index is in bounds.** A live entity's pool index is always less than
   `pool.len()`. Swap-removal invalidates no stored index because indices are
   dense and nothing caches them across a removal except the occupancy index,
   which is updated in the same operation.
2. **At most one occupant per tile.** Enforced by the occupancy index being
   `set`/`remove`, never `set`-over-`set`. Multi-tile entities register all
   covered tiles and clear all of them together.
3. **No dangling cross-references.** Every `source`, `target`, `next`, and
   `prev` field either holds a valid live index or a sentinel. Removing an entity
   clears every reference to it in the same tick it is removed, so there is no
   frame where a reference points at a freed slot.
4. **Entity count never exceeds capacity assumptions.** Adding an entity type
   or increasing a footprint size is a versioned migration, not a runtime
   resize.
5. **Iteration is deterministic.** Ascending pool index, always. Never hash-map
   iteration order, never `HashMap` keys, never a sort by a non-total order.
6. **Water and ore never overlap on a tile.** A tile is water XOR
   it carries a resource with `ore_amount > 0` — never both. Because water is a
   flag and ore is an amount, this is a cheap assertion to write and a cheap bug
   to make, since generation sets both from the same pass.
7. **Extracting water clears the flag exactly once.** An offshore pump clears
   `TileFlag::WATER` in the same tick it emits a water item, so the total count
   of water items ever produced equals the number of tiles that have had the flag
   cleared. Any discrepancy means a pump ran twice on a drained tile.