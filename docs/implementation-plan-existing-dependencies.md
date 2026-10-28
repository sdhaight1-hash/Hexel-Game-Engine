# Unified Planet Implementation Plan — Existing Dependencies Only

Status: implementation plan for `feat/unified-planetary-gameplay`, 2026-10-28.

## Decision

Keep the current Rust + wgpu stack. Do not add crates for this pass. The dependencies already present are sufficient to establish coordinate correctness, stable cell identity, and an end-to-end editable hexel before optimization. The goal is one spherical survival world, not separate flat and globe modes.

## Existing dependencies and intended jobs

| Dependency | Use in the plan | Boundary / caution |
|---|---|---|
| `glam 0.29` | Authoritative planet/player positions with `DVec3`; tangent frames and f64-to-f32 local conversion; local `Vec3` and matrices for rendering. | Never cast planet-centred Earth-radius positions to f32 before rebasing. |
| `hexx 0.20` | Tested axial/cube hex math, neighbor directions, distance, rings and point-to-hex operations for local hex grids, where its API matches. | Does not provide global spherical topology or solve cube-face seams. Inspect existing custom hex math and replace selectively, not blindly. |
| `noise 0.9` | Deterministic terrain/biome signals from seed + stable cell address or normalized planetary direction. | Generated base terrain must be deterministic; edits are sparse overrides. |
| `wgpu 24` | Render camera-relative terrain, hex-prism faces, trees, outlines and other geometry. | GPU vertices stay local to the active origin. GPU buffers are not authoritative world state. |
| `bytemuck 1.21` | Upload explicitly laid-out vertex and uniform data. | Keep global f64 state separate from GPU structs. |
| `winit 0.30` | Input, frame loop and bounded streaming/remesh scheduling. | Avoid full-region generation or uploads blocking a frame. |
| `image 0.25` | Existing PNG texture loading for terrain and vegetation. | Presentation polish follows terrain/data correctness. |
| `tracing`, `tracing-subscriber` | Instrument generation, raycasts, remeshes, rebasing and GPU uploads. | Measure before adding concurrency or replacing algorithms. |
| `pollster 0.4` | Existing GPU initialization path. | Not a terrain-worker solution. |
| `approx 0.5` (dev) | Tolerant assertions for coordinate, seam and geometry regressions. | Test precision and identity, not only whether a scene renders. |

## Phase 1 — Fix world coordinates end to end

**Use:** `glam`, `PlanetaryFrame`, `wgpu`, `bytemuck`, `approx`.

- Promote live player/planet position to f64 (`DVec3`), not just save-file coordinates.
- Create one `PlanetaryFrame` around the player and use it as the only planetary-to-render conversion.
- Rebase global surface geometry, streamed patches, tree roots/vertices, outlines, camera and ray queries before any f32 conversion. Preserve source positions or chunk origins in f64; never infer exact world positions from rounded GPU vertices.
- Keep camera view/projection math in the local frame.
- Add tests: two positions one metre apart at Earth radius remain one metre apart after conversion; local/world round trips meet tolerance; frames stay finite near poles and across wrap boundaries.
- Add tracing counters for active chunks, generated vertices, ray tests, remeshes and upload time.

**Gate:** no planetary path uploads absolute Earth-centred f32 positions; precision tests pass.

## Phase 2 — Make hex identity canonical

**Use:** existing `hexx`, `glam`, `noise`, and the current topology/chunk/voxel modules.

- Audit custom hex coordinate/neighbor code and compare it with the installed `hexx 0.20` API. Use the crate for local hex operations where its conventions match and tests cover them.
- Define a stable address: planetary region/patch + local hex coordinate + radial layer. It must not depend on chunk residency, load order, mesh index or LOD.
- Make generated material/terrain a deterministic function of seed + address. Store only player overrides against that address.
- Define neighbor mapping across patch boundaries and test reverse-neighbor symmetry, deterministic regeneration, all six face orientations, face edges and corners.
- Keep the current geodesic topology as a regional graph only if fine subdivisions remain consistent; otherwise use cube-sphere patches as regions with canonical local hex ownership. Do not commit to either topology until seam tests pass.

**Gate:** an edited cell retains the same identity after unload/reload, regeneration and save/load.

## Phase 3 — Prove one editable hexel loop

**Use:** existing Rust data structures, `hexx`, `glam`, `wgpu`, `bytemuck`, `approx`.

- Build a small neighborhood of discrete hexagonal-prism cells with explicit solid/empty state and vertical layers.
- Derive visible faces from the cell data and hide faces shared by adjacent solid cells.
- Raycast from the local camera to canonical cell addresses. Return address, layer, face, distance and contact position.
- Use that one hit for target outline, mining and placement.
- Make edits dirty the changed cell and neighbors so visible faces and nearby collision update before the next interaction.
- Use the same cell occupancy for nearby collision queries. Do not require a new physics engine to prove this loop.
- Save sparse edits by stable address with explicit versioning and existing save compatibility.

**Gate:** automated tests and a local play test can target, mine, place, collide, save and reload the same cell with consistent results.

## Phase 4 — Connect hexels to the planet stream

**Use:** existing `planetary_chunks`, `planetary_world`, `planetary_voxels`, `noise`, renderer and frame loop.

- Treat cube-sphere patches as streaming/visibility units that request canonical hexel data, not as a second playable terrain model.
- Prioritize generation and collision data by player distance and interaction need; enforce per-frame work/upload budgets.
- Reuse chunk/buffer allocations and remesh only affected neighborhoods.
- Test traversal and edits across all face orientations, edges, corners, equator and high latitudes.
- Keep the active fine-detail set bounded; persist edits sparsely and regenerate untouched cells deterministically.

**Gate:** crossing any patch boundary does not crack, teleport, change cell identity or lose edits.

## Phase 5 — Integrate survival movement and world presentation

**Use:** existing `glam`, `noise`, `image`, `wgpu`, `winit` and gameplay/data modules.

- Put movement, jumping, gravity and camera orientation in the player tangent frame; continuously update the frame as the player moves over the sphere.
- Drive trees, material, water/shoreline and resource placement from the same deterministic samples and cell addresses. Tree roots must respond to surface edits.
- Build a coarse distant spherical surface as a derived view of the same world data; transition to fine editable hexels through a reconciled boundary ring.
- Once geometry and interaction are coherent, tune lighting, sky, biomes, vegetation density, horizon culling and LOD hysteresis.

**Gate:** one spawn flow enters a continuous spherical survival world with human-scale editable hexels and no F3 flat/globe toggle.

## Phase 6 — Measure and validate

**Use:** existing `tracing`, `approx`, unit tests and CI.

- Profile terrain sampling, meshing, raycasting, collision, rebasing and uploads under travel/edit workloads.
- Optimize only the measured bottleneck. This phase intentionally does not add a parallelism or physics crate.
- Test deterministic generation, save migration, edit persistence, seam continuity, ray/edit/collision agreement and Earth-radius precision.
- Run formatting, tests, clippy where supported, Windows release CI, and a real local gameplay pass. CI cannot confirm visual quality.

**Gate:** repeatable tests/builds plus runtime checks at equator, high latitudes, face seams, edited chunks and save/reload.

## Explicit constraints

- No new dependencies in this pass: no Rayon, Rapier, Serde/Postcard, FastNoise Lite, Bevy or engine migration.
- No full-planet one-metre allocation. The planet is finite; fine detail is sparse and streamed.
- A coarse geodesic cell with radial layers is not considered a metre-scale hex-prism surface.
- No parallel flat and globe modes.
- No optimization claims without measurements.

## Acceptance criteria

- A 1 m offset at Earth radius remains 1 m through every world-to-render conversion.
- Render targeting, mining, placement, collision and save/load agree on cell identity.
- Edits update neighboring visible faces and collision promptly.
- Unloading/reloading preserves edits and removed entities.
- Streaming remains bounded and prioritizes nearby playable terrain.
- No flat/globe switch; one world authority from spawn.
- Windows CI and local runtime checks pass.
