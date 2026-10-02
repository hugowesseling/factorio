# Factorio frontend

WebGL2 client for the Rust backend in `../backend`. Strict TypeScript, no
framework, no runtime dependencies — only Vite, TypeScript and Vitest as dev
dependencies.

## Run it

```sh
npm install
npm run dev        # http://localhost:5173
```

Other scripts:

```sh
npm run typecheck  # tsc --noEmit, strict
npm test           # vitest, single run
npm run test:watch # vitest in watch mode
npm run build      # typecheck then vite build into dist/
npm run preview    # serve the production build
```

Add `?seed=1234` to the URL to generate a different world.

## What you can do without a server

The backend has no network listener yet, so the client boots into a demo world:
the real terrain generator on a fixed seed, a small factory layout, and a fixed
60 Hz tick that moves items along the belts. Movement is predicted locally, and
power, tick and FPS readouts come from the demo tick.

| Input | Action |
| --- | --- |
| `WASD` / arrows | move (predicted locally) |
| `1`–`0` | select hotbar slot |
| `F` / `V` | cycle build item |
| `R` / `Space` | rotate |
| `Q` | mine the tile under the cursor |
| Right click | remove |
| `G` | toggle the tile grid |
| `C` | connect or disconnect |
| Middle drag, or shift + left drag | pan |
| Wheel | zoom |
| `Esc` | clear the selection |

Pressing `C` attempts a WebSocket connection to `ws://127.0.0.1:9000/ws`. While
it is down the client stays in demo mode; intents are not queued to be replayed
as world state, because placement and mining are only ever decided by the
server.

## Layout

```
src/
  content.ts          item and machine definitions, generated from the backend
  main.ts             wiring: fixed 60 Hz simulation step, then render
  content.json        generated manifest, do not edit by hand
  input/controls.ts   keyboard and pointer state
  net/
    protocol.ts       wire codec, byte for byte with crates/proto
    connection.ts     WebSocket lifecycle, reconnect with backoff
    interest.ts       which chunks the client wants resident
    snapshot.ts       applies server messages, interpolation clock
  render/
    camera.ts         tile/screen conversion and the orthographic matrix
    mesh.ts           vertex data, pure functions
    renderer.ts       WebGL2 setup and the draw loop
  sim/
    rng.ts            Pcg32, BigInt so 64 bits stay exact
    terrain.ts        chunk generation, matches crates/sim/src/terrain.rs
    demo.ts           offline world and tick loop
  state/
    world.ts          authoritative view of the world
    predict.ts        local movement prediction and reconciliation
    stores.ts         tiny observable store for the UI
  ui/
    hud.ts            tick, power, position, connection, event log
    hotbar.ts         ten build slots
    panels.ts         inventory, tooltip, minimap
    dom.ts            small DOM helpers
test/
  vectors.json        generated fixtures: protocol bytes, RNG, terrain, content
```

## Parity with the backend

Two things are generated from the backend rather than copied by hand, so they
cannot drift:

```sh
cd ../backend
cargo run -p factorio-server --example frontend_vectors > ../frontend/test/vectors.json
cargo run -p factorio-server --example frontend_vectors -- content > ../frontend/src/generated/content.json
```

`test/vectors.json` pins the wire encoding, the Pcg32 sequence and whole terrain
chunks. `test/protocol.test.ts` and `test/terrain.test.ts` assert the client
matches it exactly, so any change to the backend's encoding or terrain code
fails the frontend tests until both sides agree again.

Item names, machine footprints and power limits come from
`crates/content/content/*.toml` via the second command, and
`test/content.test.ts` checks the committed manifest still matches the backend.

## Notes

- Simulation is integer-only and iterates in fixed order, matching the backend's
  determinism rules. No wall-clock time or floating point accumulation feeds
  into game state.
- The 64-bit seed mixing in `sim/rng.ts` uses `BigInt`. Plain numbers cannot hold
  the chunk keys the backend uses.
- `EventCode` in `net/snapshot.ts` is a client-side convention: the wire format
  carries three event arguments, which cannot losslessly describe every backend
  event (`EntityPlaced` and `TileMined` need four values). Event text shown in
  the HUD, and the `y` coordinate applied on placement, are therefore provisional
  until the backend publishes a real event schema.
- The renderer is untested against a real GPU here; geometry, projection and
  colour selection are covered by unit tests, and everything else is verified by
  `npm run build` and `npm run typecheck`.