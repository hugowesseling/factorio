# Frontend

The client: renders the world, reads input, and predicts movement locally so
the game feels responsive despite the round trip.

Reads authoritative state from the backend described in [backend.md](backend.md).

## Stack

- **TypeScript**, strict mode, no framework — a game client is a render loop and
  a scene graph, not a document.
- **WebGL2** (via a thin wrapper, no engine) for the world and entity rendering.
- **HTML overlay + CSS** for UI. GUI panels are DOM; they are easier to lay out,
  to make accessible, and to style than anything drawn into a canvas.
- **Vite** for dev server and bundling.
- No game engine. Bevy or Three would work, but the whole world is
  tile-and-sprite rendering, which is a few thousand draw calls of instanced
  quads — well within reach of raw WebGL, and an engine's scene graph would only
  be in the way.

Runs in the browser. A desktop build is the same code with Electron around it,
later.

## Structure

```
frontend/src/
├── main.ts               # bootstrap: canvas, GL context, main loop
├── net/
│   ├── connection.ts     # WebSocket lifecycle, reconnect, backoff
│   ├── protocol.ts       # mirrors backend/proto — generated, do not hand-edit
│   ├── snapshot.ts       # snapshot + delta application
│   └── interest.ts       # which chunks to request, AOI radius
├── state/
│   ├── world.ts          # authoritative store, read-only to the renderer
│   ├── predict.ts        # local prediction, prediction reconciliation
│   └── stores.ts         # small observable stores for UI
├── render/
│   ├── renderer.ts       # frame graph, passes
│   ├── terrain.ts        # chunk meshes, chunk caching
│   ├── entities.ts       # instanced sprites + entity animation
│   ├── belts.ts          # belt item rendering
│   ├── effects.ts        # particles, smoke, explosions
│   └── camera.ts         # pan, zoom, follow
├── input/
│   ├── keyboard.ts
│   ├── mouse.ts          # click, drag-place, drag-line
│   └── hotbar.ts
├── ui/
│   ├── inventory.ts      # player + container inventories
│   ├── crafting.ts
│   ├── research.ts       # technology tree view
│   ├── minimap.ts
│   ├── tooltip.ts        # hover cards for entities and items
│   └── alerts.ts
└── util/
```

## Rendering

WebGL2 with instanced rendering throughout. Each entity type gets one draw call
for every instance currently on screen; tiles are batched per chunk into a
single mesh.

- **Terrain** — one mesh per visible chunk, uploaded once on arrival, cached,
  evicted when out of range. Water tiles come from the terrain `flags` bit and
  render as part of the chunk mesh rather than as entities, so a lake costs
  nothing to draw beyond the quads it already needs.
- **Entities** — instanced quads sampling from a texture atlas, with a
  per-instance tint and animation frame offset.
- **Belts** — items on belts are separate instances positioned along the belt's
  lane, so they interpolate smoothly rather than snapping between tiles.
- **Lighting** — a single fullscreen pass blending the static lightmap from the
  terrain chunk with dynamic light entities. Cheap, and the reason the game can
  look good at night without a deferred renderer.
- **Post** — optional bloom pass, off by default on integrated GPUs.

The render loop is decoupled from the network and the simulation. It renders
whatever the latest snapshot says at display refresh rate, interpolating between
the two most recent server ticks so motion is smooth even though the server
runs at 60 Hz on a 144 Hz display.

## Prediction and reconciliation

Round trips make raw input feel terrible, so the client predicts — but
prediction is confined to a small set of cases where the server's answer is
obvious:

- **Player movement.** The client simulates locally and sends the input, not the
  position. When the server's authoritative position arrives, it is compared to
  the predicted one; on mismatch the client snaps and replays unacknowledged
  input. Small corrections are ignored or smoothed over a few frames.
- **Building ghosts.** Dragging out a line of belts shows a preview with
  auto-rotate at corners. Purely visual; nothing is sent until release.

Everything else — machines crafting, belts moving, enemies attacking — is
rendered exactly as the server reports it, with no prediction. The rule is that
the client may predict *itself* and never *the world*.

## Interface

Faithful to the genre: grid-snapped placement, drag-to-place-lines, right-click
to pick, rotate with `R`, copy and paste a selection with `Ctrl+C` / `Ctrl+V`,
hotbar bound to number keys, and a character panel for the whole inventory tree.

Belt direction is the main usability problem in this genre, so the UI is
opinionated about it: belts preview with arrows while dragging, and connections
snap to a valid direction when they are unambiguous.

Water is impassable. Placement previews are rejected over water tiles, and
clicking water selects nothing rather than selecting whatever is beneath it.

## Assets and performance

- A texture atlas packed at build time; all sprites are UV-indexed.
- Target 60 fps at 1080p on integrated graphics.
- Budgets enforced in dev builds: max frame time, draw calls, and chunk cache
  size are logged and warned on when exceeded.

## Testing

- **Vitest** for pure logic: prediction reconciliation, AOI computation, hotbar
  and inventory operations, protocol decoding.
- **Playwright** against a mock server that replays a recorded session, to test
  rendering and input without a running backend.
- Protocol types are generated from the backend's `proto/` definitions, so a
  breaking change in either direction fails the build rather than surfacing as a
  runtime desync.