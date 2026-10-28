# Hexel Engine Planetary World Specification

Status: **Foundational design contract**  
Target branch: `feat/unified-planetary-gameplay`

This document records the world-scale decisions that new generation, chunk streaming,
rendering, navigation, interaction, and save/load work must share. Treat the values below
as the initial playable profile, not as limits that can never be configured.

## 1. Core world model

Hexel Engine is one continuous, spherical survival world. The planet is approximately
Earth-sized, with a nominal radius of **6,371,000 metres**. The sphere is the global
reference for position and gravity/orientation; it is not a separate game mode.

The player must not switch between a flat world and a globe. The same world data model,
Hexel units, collision, mining, building, terrain generation, inventory interactions, and
save/load semantics apply everywhere. A flat tangent view may be useful for tests or
debugging, but it is not a second gameplay mode.

## 2. Hexel and chunk dimensions

- Horizontal Hexel scale: **1 metre nominal width**.
- Vertical Hexel step: **0.6 metres**.
- Reference player height: **1.8 metres** (three vertical Hexel steps).
- Chunk dimensions: **16 × 16 × 16 cells**.
- Cells per chunk: **4,096**.
- Chunk vertical span: **9.6 metres**.
- Horizontal dimensions are measured in the local axial-hex grid; the exact footprint
  follows the documented Hexel radius and axial-coordinate convention rather than assuming
  a square 16 m footprint.

Chunks remain actual editable 3D Hexel prism volumes. A cube-sphere patch or geodesic
surface cell may be used to locate a chunk on the planet, but neither may replace the
16³ chunk as the authoritative storage/editing unit.

## 3. Initial playable vertical range

Use a **256-metre initial playable vertical range** as the baseline survival profile,
inspired by the practical scale of configurable voxel survival games. This is a design
target for the vertical world budget, not a requirement to store a 256-metre-high column
of allocated chunks everywhere.

The profile must allow:
- terrain and structures above the local sea reference;
- surface water and ocean depth;
- soil, rock, caves, mining, and deeper geology below the local surface;
- later configuration of the vertical budget without changing the meaning of a Hexel,
  chunk coordinates, or save data.

The implementation must define the range's exact lower and upper bounds explicitly.
Do not silently treat “256 m tall” as “256 m above the surface” or “256 m below sea level.”
For the first playable profile, use a 256 m total vertical budget centred on a documented
planetary datum/sea-level reference; tune the split between above- and below-datum space
as generation and gameplay become testable. Do not hard-code a Vintage Story sea-level
ratio as a physical law.

## 4. Sea level and local elevation

Sea level is a global **datum** used by terrain, hydrology, climate, and gameplay—not a
single flat plane cutting through the sphere. At each surface location, local elevation is
measured relative to the datum along the local radial direction.

- Ocean surfaces follow the planet's spherical reference at the sea-level datum.
- Terrain elevation is a radial offset from that reference.
- Underwater depth, cave depth, and mining depth use a consistent signed radial/elevation
  convention.
- The datum and vertical budget must be represented in configuration, not duplicated as
  magic numbers across systems.

## 5. Planet coordinates and precision

Planet-scale positions use double precision for global coordinates. Rendering and local
physics operate in a camera/player-relative tangent frame with single-precision local
coordinates. Rebase mesh vertices and nearby gameplay geometry before converting to
`f32`; do not cast Earth-radius absolute coordinates directly to `f32` and expect
metre-scale Hexels to remain stable.

A chunk placement records a stable surface anchor plus its local chunk address. Tangent
frames define local east/up/north axes; local up follows the radial direction. Moving across
the surface changes the tangent frame continuously while preserving the planet-scale
position model.

## 6. Authoritative storage and streaming

- The authoritative editable unit is the 16³ Hexel chunk.
- Chunk identity must be stable across streaming, edits, save/load, and neighbouring chunk
  boundaries.
- Surface anchors map chunk-local axial coordinates onto the globe.
- Vertical chunk layers extend relative to the local planetary datum/surface convention.
- Terrain generation must be deterministic from world seed and stable planetary/chunk
  coordinates, independent of chunk load order.
- Missing/unloaded neighbours must not create permanent mesh cracks, duplicated cells, or
  inconsistent edits.
- Streaming should load nearby chunks first and bound per-frame generation/meshing work.

The existing sparse radial-column planetary voxel prototype and cube-sphere backdrop mesh
are transitional tools. They must not become a second authoritative terrain representation.

## 7. Required gameplay parity

The same systems must work on the entire globe:
- walk, jump, collide, and orient to local radial gravity;
- mine and place individual Hexels;
- resolve targeting/raycast hits against editable Hexel prisms;
- update neighbouring chunk meshes after edits;
- generate and persist terrain, structures, and edits;
- transition across chunk boundaries and around the planet without a mode switch.

## 8. Implementation order and acceptance criteria

1. **Configuration contract:** centralize planet radius, Hexel dimensions, chunk dimensions,
   vertical budget, and sea-level datum; add tests for the invariants above.
2. **Planet-addressed chunks:** make stable planetary anchors address real 16³ chunks and
   deterministic local cell coordinates.
3. **Playable crust:** generate a continuous visible surface from chunk-backed Hexels, with
   correct local tangent placement and camera-relative precision.
4. **Interaction:** connect raycasts, collision, mining, and placement to the same chunk data.
5. **Persistence:** round-trip chunk contents, planetary anchors, seed, and edits.
6. **Streaming and optimization:** improve LOD, mesh caching, seam handling, and generation
   budgets only after the integrated world can be played.

A milestone is not considered complete merely because it adds coordinate helpers or a
backdrop surface. The player must be able to see and interact with the same chunk-backed
terrain that the world stores.

## 9. Initial profile summary

| Setting | Initial target |
| --- | --- |
| Planet radius | 6,371,000 m |
| Hexel nominal horizontal width | 1 m |
| Hexel vertical step | 0.6 m |
| Reference player height | 1.8 m |
| Chunk dimensions | 16 × 16 × 16 |
| Cells per chunk | 4,096 |
| Chunk vertical span | 9.6 m |
| Playable vertical budget | 256 m total, configurable |
| Sea level | Configurable spherical datum |
| Global position precision | `f64` |
| Camera-relative render precision | `f32` after rebasing |
