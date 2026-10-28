# Planetary Hexel Engineering Research

Status: research synthesis for `feat/unified-planetary-gameplay`, 2026-10-28.

## Executive decision

Do not replace Hexel's renderer with a different engine, and do not bolt a second "planet mode" onto the flat game. Keep Rust + wgpu and make the planetary surface the single world authority.

The best approach is a hybrid of proven open-source techniques:

1. **CPU authoritative positions in f64, render positions relative to a nearby origin in f32.** This is the common floating-origin solution in planet renderers and is already anticipated by `src/planetary_frame.rs`, but it is not consistently applied in `src/renderer.rs`.
2. **Sparse, deterministic, addressable hexel data at human scale.** Store only touched/loaded fine cells and derive untouched cells from deterministic generation. Do not allocate a one-metre grid for the entire Earth.
3. **Streaming patches are a transport/visibility mechanism, not a second terrain model.** A single stable cell address and terrain query must feed rendering, raycast, collision, mining/placement, vegetation, and save/load.
4. **Separate planetary coverage from local gameplay detail.** Use a coarse spherical base for distant silhouette and climate/biome colour, with locally refined patches for playable detail. Local refinement must use the same authoritative surface-cell addresses and edits; it cannot be an unrelated height mesh.
5. **Incremental integration with regression tests.** First fix coordinate correctness, then unify cell identity and local hexel geometry, then collision and streaming/seams, then presentation and LOD.

## What the code currently tells us

- `src/planetary_frame.rs` already provides a double-precision tangent frame and a tested `to_local_f32` conversion. This is a sound primitive to integrate.
- `src/planetary_chunks.rs` keeps generated patch positions as `DVec3` until it returns the mesh. The renderer then casts positions to `Vec3/f32` before GPU upload, so the precision guarantee is lost at the call site.
- The renderer's global surface mesh also casts planet-centred coordinates (radius about 6,371,000 m) directly into `Vertex.position: [f32; 3]`. The same absolute-f32 assumption appears in streamed terrain, tree vertices, cell refresh, outlines, and ray-triangle queries.
- The current `PlanetaryWorld::new(seed, 16, ...)` topology is a coarse global geodesic partition. `PlanetVoxelVolume` stores radial layers per coarse surface-cell ID; a one-metre layer above a large coarse cell is not a one-metre hexagonal prism surface grid.
- Streaming patches are cube-sphere square grids. They are useful for bounded terrain generation and LOD, but cannot silently become a different identity system from the hexel cells used by gameplay.
- Save format version 2 stores player coordinates as f64, but live camera/player state remains f32. Persistence precision alone does not solve runtime precision.

These are architectural mismatches, not problems solved by increasing mesh resolution or changing colours.

## Codebases worth borrowing from

### 1. Earth-radius chunked planet renderer — origin shifting, quadtree LOD, frame budgets

**Repository:** [cuberact/godot-cuberact-planet-chunked-lod](https://github.com/cuberact/godot-cuberact-planet-chunked-lod)

- [Implementation deep dive in README](https://github.com/cuberact/godot-cuberact-planet-chunked-lod#implementation-deep-dive)
- [scripts directory](https://github.com/cuberact/godot-cuberact-planet-chunked-lod/tree/main/scripts)
- [shaders directory](https://github.com/cuberact/godot-cuberact-planet-chunked-lod/tree/main/shaders)

**Relevant implementation:** six-face cube-to-sphere projection, quadtree split/merge, hysteresis to avoid LOD thrashing, per-frame mesh-build budgets, frustum/horizon culling, skirts to conceal LOD boundaries, pooled chunks, and camera origin shifting. Its README explicitly keeps planet objects attached to the shifted planet transform and notes that distant absolute f32 coordinates jitter.

**Adopt:** LOD hysteresis, bounded per-frame work, chunk pooling, horizon/frustum culling, and the principle of keeping every renderable in the same origin-rebase scheme.

**Do not copy wholesale:** it is Godot/GDScript and a square-patch renderer. Its terrain is not a destructible hex-prism world and its camera/scene graph assumptions do not map directly to wgpu.

### 2. Editable spherical voxel terrain — authoritative density, octree, meshing and colliders

**Repository:** [JorisAR/GDVoxelTerrain](https://github.com/JorisAR/GDVoxelTerrain)

- [Source tree](https://github.com/JorisAR/GDVoxelTerrain/tree/main/src)
- [Voxel terrain implementation](https://github.com/JorisAR/GDVoxelTerrain/tree/main/src/voxel_terrain)
- [SDF primitives and composition](https://github.com/JorisAR/GDVoxelTerrain/tree/main/src/sdf)
- [Project README and spherical demo](https://github.com/JorisAR/GDVoxelTerrain)

The project documents a sparse octree SDF, custom Surface Nets meshing, runtime terrain edits, spherical and planar terrain, LOD, and generated collision meshes. Its recent history also describes a persistent worker, pooled chunks, and a time budget for collider uploads.

**Adopt:** a strict data → mesh/collision pipeline; dirty-region remeshing; persistent worker ownership; prioritizing nearby colliders; bounded main-thread upload budgets; and regression tests for chunk boundaries.

**Do not copy the terrain representation directly:** an SDF/smooth isosurface is not the desired discrete hexagonal-prism terrain. Use it as a reference for ownership, chunk scheduling and collision lifecycle, not as Hexel's cell model. This is a separate engine extension (C++/Godot), not a Rust dependency.

### 3. Rust hex-coordinate algorithms — local grid math only

**Repository:** [ManevilleF/hexx](https://github.com/ManevilleF/hexx)

- [README and supported operations](https://github.com/ManevilleF/hexx)
- [Rust source](https://github.com/ManevilleF/hexx/tree/main/src)
- [Examples](https://github.com/ManevilleF/hexx/tree/main/examples)

This is an actively maintained Rust library for hex coordinates and grid operations.

**Adopt selectively:** tested axial/cube coordinate conversion, neighbor directions, distance, rings, spirals, and point-to-hex math for the *local tangent hex lattice*. It can reduce custom math bugs if its API and license fit the crate's needs.

**Important limit:** planar axial coordinates alone do not define a globally continuous sphere. The planetary address layer still needs a stable spherical patch/cell address and explicit cross-patch neighbor mapping. Do not assign one giant flat axial grid to the whole globe.

### 4. Procedural Earth-size planets and floating camera origin

**Repository:** [jakubnei/procedural-planets-generator](https://github.com/jakubnei/procedural-planets-generator)

- [README with implementation notes](https://github.com/jakubnei/procedural-planets-generator)

This older custom C# engine documents Earth-size terrain generated from a direction-based height function, chunk LOD, skirts, GPU-generated mesh/normal/biome maps, and a floating camera origin. It is useful as a compact conceptual reference, but its age and custom engine make it a weaker source than the actively maintained projects above.

**Adopt:** direction-based deterministic planet sampling and keeping global terrain definition independent of camera position. Review any copied code and license before reuse.

### 5. Rust/wgpu origin-relative simulation example

**Repository:** [kraken-space-program/kraken-space-program](https://github.com/kraken-space-program/kraken-space-program)

- [Repository and coordinate-system notes](https://github.com/kraken-space-program/kraken-space-program)

The README describes a Rust + wgpu coordinate split with f64 simulation positions and a single f64-to-f32 render-sync stage.

**Adopt:** one explicit conversion boundary between authoritative world coordinates and render coordinates. This is closer to Hexel's technology stack than the Godot and Unity references, though it is not itself a finished planetary hexel implementation.

### 6. Established hex ray traversal

- [Amanatides & Woo: A Fast Voxel Traversal Algorithm (1987)](http://www.cse.yorku.ca/~amana/research/grid.pdf) is the standard reference for stepping a ray through a regular grid.
- For the local hex grid, adapt traversal to the three edge directions of a triangular/hexagonal lattice or use a tested point-to-hex walk over neighbors. Do not raycast by iterating every coarse global cell's rendered triangles.
- Keep the hit result as a stable authoritative cell address plus face/normal and distance; use the same result for target outline, mining, placement, and collision diagnostics.

## Proposed Hexel architecture

### A. Authoritative coordinate and render boundary

- Store player position, planet-centred cell centres, terrain sample positions, tree roots, and persistent locations as `DVec3` or stable integer/geodesic addresses plus local offsets.
- Each frame, construct one `PlanetaryFrame` at the player. Subtract/project in f64, then cast the small east/up/north result to f32.
- The camera render position becomes local origin (or a small local offset), and the view matrix uses the local tangent orientation. The GPU never receives Earth-centred positions.
- Do not fix only streamed patches: global mesh, tree geometry, target outline, ray queries, collision shapes and any debug geometry must follow the same rule.
- Keep source mesh positions in f64 or store chunk-local f32 vertices plus an f64 chunk origin. Never attempt to recover exact global positions from already-rounded absolute f32 vertices.

### B. One cell address, multiple derived representations

Each surface hexel needs a stable address independent of load order and chunk residency. Its generated properties should be a pure function of seed + address, then player edits override that base.

A practical representation is:
- global region/patch address (integer cube-sphere or geodesic face + hierarchical path);
- local hex coordinate inside that patch;
- radial layer / vertical block coordinate;
- deterministic generated block/material;
- sparse edit overrides and persistent entity state keyed by the same address.

The planet can have billions/trillions of possible cells without allocating them. Load the active neighborhood, cache it, and persist only overrides/state. Neighbor lookup must cross patch boundaries without changing identity.

**Topology decision to test:** retain the existing geodesic hex/pentagon topology as the global regional graph, and add hierarchical fine hexel subdivisions beneath a region; alternatively, use cube-sphere patches as region addresses and define a canonical tangent-plane hex lattice within each patch. Prototype both on one patch plus all edge/corner seams before committing to either. The first option preserves geodesic adjacency but needs careful subdivision rules; the second fits current streaming but needs canonical seam ownership and cross-face neighbor rules. A square heightfield patch is not the final terrain representation.

### C. Render from the same authoritative cell data

- Generate local hex-prism surfaces from cell material and neighbor state.
- Cull hidden faces between adjacent solid cells; regenerate only the changed cell and its neighbors.
- For distant untouched regions, use a coarse derived surface/biome representation. Near the player, replace/refine it with detailed cells and reconcile boundary rings. Distant LOD is an approximation of the same world, not a second editable world.
- Use one canonical terrain query for terrain height, normal, material, collision surface, and plant placement.
- Keep vegetation instances keyed by stable IDs and attach them to the same surface frame/cell data.

### D. Raycast, collision and edits

- Raycast from camera-local coordinates against nearby loaded cell geometry or traverse candidate cell addresses first, then test only those cell faces.
- The hit contains `cell_address`, `layer`, `face_direction`, `distance`, and contact position.
- Mining changes the authoritative cell state; mark the cell, neighboring face visibility, collision, and affected render chunk dirty. Placement uses the adjacent address derived from the hit face.
- Build collision only for the nearby interaction radius and prioritize chunks closest to the player. Reuse mesh/collision buffers where possible.
- Save sparse overrides keyed by stable addresses, not by transient GPU indices or a global coarse-cell ID that will change when topology resolution changes.

### E. Streaming and performance

- Keep the existing bounded queue idea, but replace fixed arbitrary scan budgets with a priority queue by distance, visibility, and whether the chunk is needed for collision.
- Add hysteresis to chunk LOD; avoid repeated split/merge when the camera hovers at a threshold.
- Use overlap/padding or skirts only for the coarse LOD boundary; detailed adjacent hexel chunks should share exact canonical boundary vertices.
- Add explicit seam tests for all six cube faces, twelve cube-face edges, corners, and all hex neighbors.
- Do not generate all Earth terrain. Generate deterministically on demand and retain only a bounded working set plus sparse persistent changes.

## High-value implementation order

1. **Precision regression:** implement a render-origin boundary and unit tests proving metre-scale separations survive at Earth radius. Convert all world geometry and ray queries consistently; no absolute planet-centred f32 vertices.
2. **Canonical cell identity:** specify the stable hierarchical address and cross-boundary neighbor function. Test generation determinism and save/load identity before large renderer rewrites.
3. **One-cell vertical slice:** render one real hexel prism and neighbors; raycast/select it; mine it; place it; collide against it; save, reload, and verify the same address/state. All operations must query the same data.
4. **Neighborhood meshing:** chunk the canonical hexel data; face culling; dirty remesh; bounded collision generation; seam tests.
5. **Integrate existing gameplay:** player movement/gravity in the tangent frame, trees, water/shorelines, resources, inventory, and persistence.
6. **Planet-scale presentation:** coarse distant surface + local fine cells, atmosphere/sky, lighting, biome transitions, horizon/frustum culling, LOD hysteresis and frame budgets.
7. **Validation:** Windows release build, unit tests, clippy/fmt, and a runtime test matrix (equator, high latitude, cube-face seams, edited chunks, reload, and long-distance traversal).

## Acceptance tests that should block "done"

- A 1 m offset at Earth radius remains 1 m after every world-to-render conversion.
- Walking across any patch/face boundary does not teleport, rotate controls unexpectedly, crack the surface, or lose edits.
- The same cell address is returned by render targeting, mining, placement, collision, and save/load.
- Editing a cell updates visible faces and collision before the next interaction.
- Unloading/reloading a chunk preserves mined blocks, placed blocks, and removed trees.
- The active terrain/collision working set is bounded and prioritized near the player.
- No flat/globe toggle: one spawn flow and one world authority.
- Windows CI passes, and a real local gameplay pass confirms the visual result. CI success alone is not visual acceptance.

## Source and license caution

Use these repositories to learn and verify algorithms. Before copying implementation code, check the exact file-level license and preserve notices/attribution. Prefer reimplementing small algorithms from their documented behavior when that is cleaner than importing an entire engine-specific subsystem.
