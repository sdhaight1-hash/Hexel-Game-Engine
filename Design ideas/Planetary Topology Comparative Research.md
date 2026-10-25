# Planetary Topology Comparative Research for Hexel Engine

**Status:** Research synthesis; initial geodesic topology implementation now exists in `src/topology.rs`  
**Date:** 2026-10-25  
**Scope:** Compare Hexel's current spherical-topology audit with PlanetSmith, public spherical-voxel projects, geodesic/Goldberg grid literature, and alternative planetary representations.  
**Decision status:** No production topology selected. This report informs the topology proof described in `Hexel Engine Architecture Decision.md`.

## Executive summary

The evidence supports one important distinction: **PlanetSmith is a strong gameplay reference for a mostly-hexagonal voxel planet, but the public evidence does not establish that its exact cell graph, pentagon handling, or block-neighbor rules match Hexel's current geodesic-dual audit.**

PlanetSmith Episode 1 describes subdividing an icosahedron and using the dual relationship to create mostly hexagonal surface cells with twelve pentagons. Episode 5 discusses hexagonal block top/bottom texture layout and special block shapes. Episode 10 describes a custom ray traversal: it calculates spherical intersections for vertical layers, walks between neighboring hexagonal regions by intersecting edge planes, maps those coordinates into stored voxel data, and uses triangle tests for smaller/custom blocks. This confirms a real hexel-specific interaction path, but the public videos do not fully document how the twelve pentagonal regions connect to the regular hex cells. The game is not open-source in the sources examined, so its internal implementation cannot be compared line-by-line.

The external implementations show three materially different strategies:

1. **Geodesic/Goldberg surface graph** — mostly hexagons plus twelve pentagons; natural global cell adjacency, but variable-degree cells are fundamental.
2. **Cubed-sphere / six-face grids** — square or cube-like local cells on six connected patches, with face-edge transforms and projection distortion; supports hierarchical patching and LOD.
3. **Moving local charts / dual representations** — maintain a spherical canonical position but render or edit a locally convenient grid, potentially re-centering the chart as the player moves. This avoids a single global planar grid but creates identity, frame-transport, chart-transition, and multiplayer-consistency problems.

A fourth category, **continuous SDF/smooth voxel terrain**, is useful for planetary terrain and LOD engineering but is not directly equivalent to discrete hexagonal-prism cells.

**Implementation direction (updated 2026-10-25):** Start with the geodesic dual cell graph and integrate it into Hexel incrementally. The initial data layer now exists in `src/topology.rs`, with cell IDs, centers, ordered boundaries, reciprocal neighbors, and nearest-cell lookup. Do not wait for another broad comparison study before proceeding. Keep cubed-sphere patches as a fallback if actual prism integration exposes a concrete topology or performance blocker. The geodesic graph still does not, by itself, solve pure hexagonal-prism geometry, underground layers, raycasting, or persistence; those are the next implementation tasks.

## 1. Hexel's starting point: what our repository already says

The existing [Spherical Topology Research for Hexel Engine](Spherical%20Topology%20Research%20for%20Hexel%20Engine.md) recommends a provisional cubed-sphere prototype while preserving a geodesic/Goldberg-style cell graph as a candidate. The [Architecture Decision](Hexel%20Engine%20Architecture%20Decision.md) leaves topology and the meaning of a hexel open pending experiments.

That is the correct status. The current engine uses planar axial hex coordinates, vertical columns, per-column meshes, and raycasting for exposed faces. The separate topology audit constructs and validates a geodesic dual mesh. Those are two different representations today:

- The **gameplay prototype** is a planar axial hex grid with columns.
- The **topology audit** is a spherical polygon graph derived from an icosphere.
- The audit's clean manifold/orientation/coverage diagnostics do not yet prove that a gameplay column can be attached to each surface cell with correct side walls, stable IDs, ray selection, or save/load behavior.

Reported audit results at frequencies 2–4 are valuable geometry evidence, but they do not answer those integration questions.

## 2. PlanetSmith: closest gameplay analogue

Sources:
- [Episode 1 — What if Minecraft was spherical](https://www.youtube.com/watch?v=mLONYQ4qHRA)
- [Episode 5 — Hexagonal Textures](https://www.youtube.com/watch?v=eqU7rPSe0Js)
- [Episode 6 — Mesh Blocks](https://www.youtube.com/watch?v=qW726K0m3uc)
- [Episode 9 — Saving Worlds](https://www.youtube.com/watch?v=lUcDWMNpBCg)
- [Episode 10 — Hexel Raycasting](https://www.youtube.com/watch?v=NrLV9nHWnKE)
- [PlanetSmith official site](https://www.planetsmith.world/)

### Verified from the creator's public explanations

**Surface construction (Episode 1).** The creator first tests a cube and radial gravity, then maps cube points onto a sphere. He rejects that as insufficient for regular block placement and compares regular solids. He subdivides an icosahedron's triangles and explains that the dual construction produces mostly hexagonal regions while retaining twelve pentagons. He explicitly accepts those twelve pentagons as part of the design.

**Hexagonal block geometry (Episode 5).** The discussion is not just about a hexagonal-looking texture: the creator works through the hexagon footprint, seamless edge relationships, top/bottom UV layout versus side textures, and introduces half slabs and other block variants. This is direct evidence that hexagonal block geometry imposes different texture and shape rules than cubes.

**Chunk meshing (Episode 6).** Custom mesh blocks are integrated into chunk meshes rather than naively drawing every object separately, to avoid excessive draw calls. This is a useful performance lesson, though it does not prove that Hexel should use the same meshing strategy.

**Raycasting and cell traversal (Episode 10).** The creator says mesh-triangle-to-block-index lookup can become stale when multiple edits occur before the world updates. His custom raycast instead derives the exact voxel coordinates. The described traversal:
- confines rays to the planet/build limits;
- splits the ray at the point where distance to the sphere centre changes direction;
- calculates intersections with spherical vertical layers;
- walks in the horizontal plane by testing the ray against hexagon edge planes and advancing to neighboring tiles;
- combines those intersections into an ordered list of hexels;
- maps the selected coordinates into voxel storage;
- uses triangle tests for smaller or custom-shaped blocks.

This is a significant architecture lesson: **interaction should resolve stable world/cell identity directly, not depend on a transient render-mesh triangle index.**

### What remains unverified

The public videos do **not** fully specify:
- the exact stored cell-ID format or complete neighbor tables;
- how pentagonal surface regions are represented in block storage;
- whether the pentagons are true five-sided terrain prisms, special transition cells, or handled by a different internal mapping;
- how neighbor directions and block footprints are defined around those exceptions;
- whether the precise underlying surface graph is the same as Hexel's current icosphere-dual generator.

Therefore, PlanetSmith validates the *overall design family*—spherical voxel world, mostly-hexagonal surface, local gravity, chunked rendering, custom cell raycasting—not a drop-in algorithm or proof that our current graph is identical. The game source code was not available in the public sources reviewed.

## 3. Public repository: Planetcraft

Source: [ddupont808/planetcraft](https://github.com/ddupont808/planetcraft)

Planetcraft is a playable Unity/C#/HLSL proof of concept with a reported 12 km planet made of **cubical**, not hexagonal, voxels. Its README says the world is organized as six different grids and mapped to a sphere using a cube-to-sphere mapping. As altitude increases, blocks stretch horizontally; chunks subdivide to reduce distortion.

**What it teaches Hexel**
- Six connected face-local grids are a real alternative to a single global spherical grid.
- Face-local integer coordinates and explicit edge transforms can keep storage and meshing tractable.
- Projection distortion is not eliminated; the project manages it and uses subdivision at higher altitudes.
- This design is not a pure-hex solution. It is relevant if Hexel prioritizes manageable patching/LOD and allows hexels to be local terrain units rather than one uniform global cell graph.

The repository has a small public history and is best treated as a concrete proof of concept, not evidence of a mature general-purpose planetary engine.

## 4. Public repository: metal-voxel-planet

Source: [zfurman56/metal-voxel-planet](https://github.com/zfurman56/metal-voxel-planet)

This Swift/Metal proof of concept takes a deliberately different route. Its README explains that at a distance the planet is rendered as a sphere; near the surface, the world is represented as a conventional flat voxel grid whose terrain is warped in the vertex shader to appear spherical. It transitions between the two representations and acknowledges possible small discontinuities.

**What it teaches Hexel**
- A game can separate the far-field planetary representation from the near-field editable representation.
- If the player only sees a local region at high detail, a local flat grid can be a pragmatic rendering/interaction model.
- This approach trades exact globally consistent spherical cell topology for a representation transition. It may be inappropriate if Hexel requires every cell to have a permanent, globally coherent spherical identity or if players can freely inspect the whole globe while editing.

This is an explicit visual/representation compromise, not a geodesic surface-cell solution.

## 5. Public repository: spherical-voxel-lab

Source: [kzahel/spherical-voxel-lab](https://github.com/kzahel/spherical-voxel-lab)

This is an experimental TypeScript research lab rather than a finished voxel game. Its README and design records compare a canonical spherical world with moving square voxel charts. Its later regional-atlas model avoids requiring one planet-wide integer voxel coordinate. A persistent block is identified by an owner region and local address, conceptually `BlockId(regionId, localX, localY, localZ)`. Each frozen region stores a spherical anchor, local tangent frame, grid phase, local voxel data, and neighbor-transition records. An observer's presentation chart may recenter while the stored block identity stays fixed.

The repository explicitly investigates **holonomy**: transporting local directions around a loop on a curved surface can return with a rotated frame or shifted grid phase. A consistent position alone does not guarantee that neighboring local grids line up. The lab tests different frame-seeding policies, phase residuals, return-to-region behavior, and deterministic ownership.

**What it teaches Hexel**
- Stable identity and a stable visual grid are separate requirements.
- If Hexel uses local grids or overlapping patches, chart transitions must preserve cell ownership and grid phase.
- A region should have a deterministic owner and identity; overlapping presentation/halo cells must not become duplicate simulation cells.
- Multiplayer or different region materialization orders require deterministic frame/transition rules.

**Important limitation:** this lab's local chart is a square-grid research model, not a demonstrated solution for Hexel's hexagonal-prism requirement. It is most useful as a warning and design reference for local-frame continuity and stable identity.

## 6. Technical articles and geometry references

### Red Blob Games — Wraparound hexagon tile maps on a sphere

Source: [Amit Patel, 2016](https://www.redblobgames.com/x/1640-hexagon-tiling-of-sphere/)

This practical exploration explains why a sphere is not a regular planar hex grid with simple wraparound. The standard icosahedron-derived hex-like construction has twelve pentagons. It also highlights pole-crossing orientation and coordinate-system difficulties, and explores regional coordinates, convex map limits, and hiding exceptional regions.

**Use for Hexel:** a clear explanation of why global axial arithmetic cannot be assumed. Hiding pentagons only works when the game can restrict traversal or visibility; it is not suitable for a fully explorable globe where every region is accessible.

### Babylon.js — Geodesic and Goldberg Polyhedra Mathematics

Source: [Babylon.js geodesic/Goldberg math guide](https://doc.babylonjs.com/guidedLearning/workshop/Geodesic_Math)

The guide formalizes the construction of geodesic triangulations from an icosahedron and their dual Goldberg polyhedra. It describes triangular subdivisions, matching vertices across face edges, and mapping isometric triangular-lattice coordinates onto icosahedron faces.

**Use for Hexel:** a technical path for generating and matching the underlying geodesic mesh. It supports the topology audit's broad construction family, but the article is not a complete gameplay specification for terrain prisms, neighbor exceptions, or persistence.

### Ellipsoidal Cube Maps for Accurate Rendering of Planetary-Scale Terrain

Source: [Lambers and Kolb — Ellipsoidal Cube Maps](https://marlam.de/ecm/)

This work focuses on accurate planetary-scale terrain rendering, including hierarchical data and projection/precision issues. It is not a hex-cell algorithm, but it is relevant to cube-face patches and large-world rendering.

**Use for Hexel:** reference for precision, hierarchical patching, and projection-aware terrain rendering if the cubed-sphere candidate advances.

### A New Coordinate System for Constructing Spherical Grid Systems

Source: [Applied Sciences, 2020](https://www.mdpi.com/2076-3417/10/2/655)

This paper is relevant to spherical grid construction and coordinate mapping. It should be evaluated at the method level if we pursue a geodesic grid; a citation alone is not evidence that its specific cell geometry fits Hexel.

### GDVoxelTerrain for Godot

Source: [JorisAR/GDVoxelTerrain](https://github.com/JorisAR/GDVoxelTerrain)

This plugin supports editable planar and spherical terrain using a signed-distance-field (SDF) representation, surface nets, and level of detail. It is not a discrete hexagonal-prism cell graph. It is useful for comparing spherical terrain generation, runtime editing, and LOD if Hexel later considers a smooth terrain layer beneath or alongside hexel structures.

## 7. Comparison for Hexel

| Approach | Global identity | Surface cells | Main advantage | Main cost / mismatch |
|---|---|---|---|---|
| Geodesic / Goldberg graph | Stable cell IDs and explicit adjacency | Mostly hexagons + 12 pentagons in the standard construction | Natural finite surface-cell graph | Exceptional cell degrees; prism/wall geometry still needs design |
| Cubed-sphere patches | Face + local coordinates, with edge transforms | Usually quad or patch cells; hex gameplay grid is separate | Hierarchical subdivision and practical patch LOD | Distortion and cross-face adjacency; not globally all-hex |
| PlanetSmith-style hex planet | Publicly described hexels and custom coordinate raycast; exact data model not public | Mostly hexagonal regions with 12 pentagons acknowledged | Closest demonstrated gameplay precedent | Internal pentagon/neighbor rules remain unverified |
| Planetcraft | Six grids | Cubical voxels | Concrete six-face planetary implementation | Not a hexagonal cell layout |
| Moving local charts / regional atlas | Region ID + local cell ID | Local square grids in the research lab | Local storage/editing with a spherical canonical position | Frame transport, grid phase, region ownership, holonomy; not yet a hex solution |
| Far sphere + near flat grid | Representation-dependent | Flat local voxels near player | Simple local editing and distant planet rendering | Transition discontinuities; no single exact global cell graph |
| SDF / surface-net terrain | Continuous spatial field | Smooth surface, not discrete hex cells | Editable terrain and LOD | Different gameplay/storage model |

## 8. What this means for our current geodesic audit

The audit is still useful and should remain intact. It checks closed edge incidence, manifold-style vertex links, consistent edge orientation, malformed inputs, duplicate/overlapping cells, area, and sampled coverage. Those are good low-level prerequisites for a polygon-cell graph.

It does **not** yet answer:
- Can a gameplay hexel column be attached to each surface cell without gaps or overlapping walls?
- What does a pentagonal cell mean to the player and to the block inventory?
- Are cell IDs stable across generation versions and save/load?
- Can a ray select a cell unambiguously at seams, pentagons, and block-shape boundaries?
- Does movement maintain a stable local frame when crossing cells or patches?
- Can a local edit rebuild only affected chunks while preserving neighboring geometry?
- Does a simple water-transfer field conserve volume using cell areas and shared-edge lengths?

These should be the next tests—not more generic mesh metrics alone.

## 9. Recommended prototype plan

Keep this diagnostic-only until the comparison produces evidence.

### Prototype A: geodesic surface-cell graph
1. Reuse the validated geodesic dual generator.
2. Assign deterministic stable IDs independent of vertex array order.
3. Expose neighbors, shared edges, edge lengths, cell area, center, normal, and cell boundary.
4. Render each cell with a unique ID color; visibly mark pentagons.
5. Attach a minimal column/prism representation to every cell, using an explicit policy for the twelve pentagons rather than silently coercing them into hexagons.
6. Test ray selection, movement frame, local edits, save/load, and reciprocal adjacency.
7. Run a small conservative scalar/water-transfer test.

### Prototype B: cubed-sphere patch graph
1. Generate six face patches with documented cube-to-sphere mapping.
2. Implement face-edge coordinate transforms and stable face/local IDs.
3. Add patch subdivision/LOD only after base seams pass.
4. Render the same cell-ID debug view and implement equivalent ray selection/edit/save-load tests.
5. If the intended gameplay still requires hexel columns, test a local hex layout within patches and measure how patch boundaries are resolved.

### Shared acceptance tests
- All cells reachable; unique and stable IDs.
- Reciprocal adjacency and no missing/duplicate neighbor relationships.
- Shared boundaries match geometrically and have consistent orientation.
- Surface position/normal and local tangent frame stay valid across seams.
- Ray selection returns the expected stable cell ID.
- Edits survive save/load and do not depend on transient mesh indices.
- Deterministic generation and local edits do not rebuild unrelated chunks.
- Cell area and edge-length distributions are measured.
- A water/field test respects area and edge measures and passes conservation checks.
- CPU time, memory, mesh size, edit latency, and seam failures are measured at comparable surface resolution.

### Decision gate

Prefer the geodesic graph if the global cell graph and mostly-hexagonal surface are central to gameplay, and variable-degree cells are acceptable.

Prefer cubed-sphere patches if hierarchical terrain rendering, streaming, and local editable patches dominate, and the global surface does not have to be a regular hex grid.

Investigate local charts only if the game can accept regional coordinate systems and their transition/identity complexity.

Do not commit the save format, public topology API, or production world generation until both primary prototypes pass the same tests.

## Sources

1. [PlanetSmith Episode 1 — What if Minecraft was spherical](https://www.youtube.com/watch?v=mLONYQ4qHRA)
2. [PlanetSmith Episode 5 — Hexagonal Textures](https://www.youtube.com/watch?v=eqU7rPSe0Js)
3. [PlanetSmith Episode 6 — Mesh Blocks](https://www.youtube.com/watch?v=qW726K0m3uc)
4. [PlanetSmith Episode 10 — Hexel Raycasting](https://www.youtube.com/watch?v=NrLV9nHWnKE)
5. [PlanetSmith official site](https://www.planetsmith.world/)
6. [ddupont808/planetcraft](https://github.com/ddupont808/planetcraft)
7. [zfurman56/metal-voxel-planet](https://github.com/zfurman56/metal-voxel-planet)
8. [kzahel/spherical-voxel-lab](https://github.com/kzahel/spherical-voxel-lab)
9. [Red Blob Games — Wraparound hexagon tile maps on a sphere](https://www.redblobgames.com/x/1640-hexagon-tiling-of-sphere/)
10. [Babylon.js — Geodesic and Goldberg Polyhedra Mathematics](https://doc.babylonjs.com/guidedLearning/workshop/Geodesic_Math)
11. [Lambers and Kolb — Ellipsoidal Cube Maps](https://marlam.de/ecm/)
12. [A New Coordinate System for Constructing Spherical Grid Systems](https://www.mdpi.com/2076-3417/10/2/655)
13. [JorisAR/GDVoxelTerrain](https://github.com/JorisAR/GDVoxelTerrain)

**Evidence limits:** PlanetSmith's conclusions above are based on the creator's public devlog explanations and official site, not access to its source code. Other repositories are proof-of-concept or specialized systems and should not be treated as directly comparable benchmarks. No claim is made that any external implementation has the same cell IDs, geometry, neighbor rules, or requirements as Hexel.
