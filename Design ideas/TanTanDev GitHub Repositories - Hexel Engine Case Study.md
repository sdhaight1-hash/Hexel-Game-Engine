# TanTanDev's GitHub Projects — Case Study for Hexel Engine

**Purpose:** Extract practical engineering lessons from TanTanDev's public Rust repositories that could inform Hexel Engine's rendering, world generation, biomes, vegetation, and simulation systems.

**Research date:** 2026-10-22  
**Scope:** Primary-source repository pages and README material. This is a design case study, not a claim that the projects were cloned, built, benchmarked locally, or audited line by line.

## Executive summary

TanTanDev's public repositories are valuable because they isolate several systems that Hexel will eventually need:

1. **Binary greedy meshing** — a performance-oriented alternative to emitting every visible voxel face.
2. **A first voxel engine** — an intentionally unoptimized, end-to-end example of endless voxel generation in Rust with wgpu.
3. **Worley-based biome classification** — a reusable noise/classification library with debug visualization and serialization support.
4. **Procedural vegetation** — the `shrubbery` crate, which has evolved into a separate, reusable vegetation-generation project.
5. **3D cellular automata** — a configurable simulation example with a GUI.

The main lesson is architectural: develop difficult systems as small, measurable modules, then integrate them into the engine through explicit interfaces. Do not copy a Bevy or wgpu implementation directly into the current macroquad prototype, and do not let planar-grid assumptions dictate the eventual spherical world.

## Repositories examined

| Project | Primary source | Main relevance |
|---|---|---|
| Binary greedy mesher demo | https://github.com/TanTanDev/binary_greedy_mesher_demo | Terrain mesh generation and benchmarking |
| First voxel engine | https://github.com/TanTanDev/first_voxel_engine | End-to-end voxel-engine learning example |
| Worley biomes | https://github.com/TanTanDev/worley_biomes | Procedural region/biome classification |
| Shrubbery | https://github.com/TanTanDev/shrubbery | Procedural vegetation generation |
| 3D cellular automata | https://github.com/TanTanDev/3d_celluar_automata | Rule-based spatial simulation |

Profile and project list: https://github.com/TanTanDev

---

## 1. Binary greedy mesher demo

Repository: https://github.com/TanTanDev/binary_greedy_mesher_demo

### What the project documents

The README describes a voxel-engine demo written in Rust using Bevy. Its focus is a binary greedy mesher. It includes Criterion benchmarks comparing a simple culled mesher against the binary greedy implementation, although the README notes that only those two benchmarks are enabled. It cites the talk *Greedy Meshing Voxels Fast* and the reference repository https://github.com/cgerikj/binary-greedy-meshing.

The latest repository activity visible during this research was April 2024. Treat it as a focused algorithm example rather than assume it represents a current production engine.

### Why this matters

Hexel currently builds exposed faces for its hexel columns. Face culling avoids drawing faces hidden by neighbouring terrain, but it does not necessarily combine adjacent coplanar faces into larger primitives. Greedy meshing attempts to merge compatible adjacent faces, reducing vertex/index count and potentially draw and upload costs.

Binary greedy approaches represent occupancy and face masks as bit fields so many visibility decisions can be processed with word-level operations. The details depend on grid layout, dimensions, material boundaries, and the meshing implementation.

### What Hexel should take from it

- **Benchmark before adopting.** Create representative terrain fixtures and compare the current mesher with a candidate implementation using mesh-build time, vertices/indices, allocation volume, and edit latency.
- **Keep correctness tests.** Confirm there are no internal faces, holes, winding errors, or missing surfaces at column/chunk boundaries.
- **Make the mesh a derived representation.** Authoritative terrain data must remain separate from generated vertices and indices so a mesh can be rebuilt after edits or topology changes.
- **Use an adapter, not an engine rewrite.** The repository uses Bevy; Hexel currently uses macroquad. Reuse the algorithmic ideas only after verifying that the data model and dependencies fit.
- **Respect geometry.** Binary greedy meshing commonly targets axis-aligned voxel faces. Hexagonal prism faces and a future spherical surface have different adjacency and face-merging constraints. A rectangular-grid implementation cannot be transplanted unchanged.

### Suggested Hexel experiment

Build a benchmark harness that feeds identical fixed terrain patterns into the current mesher and an experimental mesher: a flat plateau, noisy terrain, isolated columns, long uniform walls, and a dense block. Keep output geometry equivalent and validate surface coverage before comparing speed.

**Recommendation:** High-value research target, but not the first prerequisite for spherical topology. First define stable world/cell identity and meshing boundaries; optimize the derived renderer behind that interface.

---

## 2. First voxel engine

Repository: https://github.com/TanTanDev/first_voxel_engine

### What the project documents

The README calls this the author's first voxel engine, written in Rust with wgpu-rs as its rendering backend. It offers endless voxel generation and one grass/cave world-generation mode, explicitly described as unoptimized. The repository preserves an initial learning state and points to the related video: https://youtu.be/96ht7rd3Y5I. The latest commit visible during this research was May 2021.

### Why this matters

The project is useful as a small historical example of connecting world generation to rendering in Rust. Its own warning that the implementation is unoptimized is important: a prototype can prove an end-to-end loop before performance architecture is settled.

### Lessons for Hexel

- Keep a small, runnable baseline that proves terrain generation and rendering together.
- Separate proof-of-concept code from performance claims.
- Prefer small test worlds and visible debug output while world data structures are changing.
- Study the sequence and boundaries of the implementation, not just the rendered result.
- Avoid treating an older wgpu project as a direct dependency template for a macroquad engine.

### Spherical-world implications

An endless flat world is not the target architecture Hexel has now chosen. The transferable lesson is how to prove a complete slice—not its coordinate system. Hexel's corresponding slice should be a tiny globe: stable cell IDs, neighbours, surface positions, ray selection, an edit, and save/load. World topology should not be hardwired to the rendering mesh.

**Recommendation:** Useful for understanding the shape of a minimal voxel engine; lower priority than the mesher demo and spherical-topology prototype.

---

## 3. Worley biomes

Repository: https://github.com/TanTanDev/worley_biomes

### What the project documents

The README describes a Worley-noise implementation with k-nearest sampling and biome classification. It includes an optional Bevy debug plugin that visualizes the noise as a texture and supports live tweaking. It supports optional `serde` and `bevy` features and references a further-developed noise library: https://github.com/TanTanDev/bracket-fast-noise.

The README lists Bevy 0.17 support for Worley Biomes 0.2.0 and Bevy 0.16 for 0.1.0. The visible latest commit was December 2025. It states that the author had not done comparisons against other noise libraries, so performance should not be assumed to be superior.

### Why this matters

Worley noise partitions space around feature points. With nearest-site information, a generator can assign region identities and use distance or neighbouring sites to shape boundaries. This can produce broad coherent regions that are easier to reason about than selecting a biome independently at each cell.

A biome classifier is not a complete climate simulation. It answers a classification question from inputs; it does not, by itself, simulate finite water, temperature transport, soil moisture, or ecological succession.

### Lessons for Hexel

- Treat biome classification as a distinct module, with input environmental fields and an output classification.
- Build a debug visualization early. A noise/biome map makes scale, boundaries, and parameter mistakes much easier to spot than inspecting the world only at ground level.
- Make seeds and settings reproducible; support serialization when the settings must be saved or shared.
- Keep biome identity separate from actual plant populations and terrain material. Suitability and realized ecology are different layers.
- Benchmark any noise implementation under Hexel's real query patterns instead of relying on a repository's language or marketing.

### Critical adaptation for a spherical world

Do not simply sample a 2D noise map using latitude and longitude and assume it will be seamless. Longitude wraps, the poles converge, and the sampling density is distorted. A seam-safe first experiment is to evaluate a 3D noise function at points on the unit sphere (or use a projection designed for spherical sampling), then feed the resulting fields into the classifier. Test continuity across every patch seam and at the poles/corners of the chosen spherical topology.

For the long-term finite-hydrology direction, use biome classification as a consumer of climate, moisture, elevation, soil, and geology fields. It should not become the source of truth for those fields.

**Recommendation:** Very relevant once Hexel's spherical cell/field API exists. Prototype the debug visualizer and spherical sampling early, but avoid coupling the biome library to rendering or topology.

---

## 4. Shrubbery — procedural vegetation

Repository: https://github.com/TanTanDev/shrubbery

### What the project documents

The repository is a standalone Rust vegetation-generation project with examples, benchmarks, tests, and voxel-game integration imagery. Its README and recent history show a space-colonization-inspired growth approach and a changing API. The repository had active updates in August 2026, including a Bevy 0.19 branch merge and significant growth-API naming changes. Check the current README and changelog before using a specific API example.

Repository: https://github.com/TanTanDev/shrubbery  
Changelog: https://github.com/TanTanDev/shrubbery/blob/main/CHANGELOG.md

### Why this matters

Procedural vegetation has two separate challenges:
1. **Biological shape generation:** deciding where branches grow and how the structure responds to attractors, direction, or growth rules.
2. **World integration:** deciding whether a tree can grow at a cell, how it is represented in the world, how it responds to water/light/soil, and how its geometry is rendered and updated.

A reusable generator can handle much of the first challenge, while Hexel's field and world-state systems should own the second.

### Lessons for Hexel

- Keep vegetation growth rules independent of the renderer.
- Use deterministic seeds or saved growth state where reproducible worlds and stable save/load are required.
- Treat a generated tree as structured output that can be converted to a rendering representation; do not let generated vertices become the authoritative tree state.
- Expose growth parameters and visualization tools so the result can be debugged and tuned.
- Pin dependency/API versions for any experiment. The repository's active development means old examples may no longer match the current interface.
- Review the license and dependency compatibility before copying or integrating code.

### Spherical-world implications

Tree growth direction should use the local surface normal and a tangent frame, not a universal global +Y direction. A tree placed on a globe should grow outward relative to the local ground frame, while branches and gravity-like effects can be evaluated in the appropriate local/world frame. Terrain slope, soil moisture, climate, and biome suitability should constrain growth; a biome label alone should not guarantee that a tree exists.

**Recommendation:** A later-stage integration candidate after stable surface normals, local orientation, environmental fields, and vegetation persistence exist.

---

## 5. 3D cellular automata

Repository: https://github.com/TanTanDev/3d_celluar_automata

### What the project documents

The repository describes a 3D cellular-automata implementation in Rust and Bevy with a graphical interface for configuring the simulation. It links to a project video: https://youtu.be/63qlEpO73C4. The project has contributions from TanTanDev and leddoo. The visible latest commit was March 2022, and it uses an older Bevy-era dependency set, so it is best treated as a learning example rather than a drop-in dependency.

### Why this matters

Cellular automata update each cell based on local state and neighbouring cells. They are a useful model for some local rule systems, but not every simulation should be forced into an automaton. Hydrology, for example, needs explicit water quantities and fluxes if Hexel is to conserve finite water.

### Lessons for Hexel

- A configurable simulation with a GUI makes rules easier to experiment with than hard-coded constants.
- Visualize cell state and provide basic metrics so a simulation can be inspected.
- Make update order and neighbourhood rules explicit.
- Keep simulation state separate from rendering state.
- Add invariants and tests for any rules intended to preserve quantities.

### Spherical-world implications

Neighbour lookup must come from the topology graph. Never assume every cell has the same six planar neighbours when the topology contains patch seams or exceptional cells. A generic neighbour iteration API makes the simulation portable between the current flat prototype and candidate spherical topologies.

For finite water, use cellular rules only where their conservation and stability properties are proven. A cell-to-cell flux model with cell area and shared-edge geometry is a more direct foundation for hydrology.

**Recommendation:** Useful for experiments and debug-UI inspiration; not the architecture to copy wholesale for the field/hydrology system.

---

## Cross-project design principles

### A. Prototype subsystems in isolation, then integrate

These repositories isolate meshing, noise/classification, vegetation, and cellular rules. Hexel should similarly give each subsystem a small executable/testable experiment and a narrow interface. That reduces the risk of mixing unrelated bugs across rendering, topology, and simulation.

### B. Rendering is derived data

Authoritative world state should define cells, terrain edits, field values, and vegetation state. Meshes, debug textures, and render instances are derived and rebuildable. This principle is especially important when the spherical topology, patching, and LOD system evolve.

### C. Debug visualizations are engineering tools

Worley Biomes' noise texture and the cellular automata GUI illustrate why visualization is not just polish. Hexel should plan overlays for cell IDs, neighbours, biome classifications, field values, patch seams, normals, and LOD state.

### D. Keep performance claims evidence-based

Use repeatable benchmarks and equivalent output when comparing meshers or noise implementations. Track build time, runtime cost, memory, and edit latency. A fast algorithm on a regular cube/voxel grid may not be the right algorithm for hexagonal surface cells.

### E. Stable identity precedes scale

Before full planetary simulation, define stable cell identity and neighbour queries independent of render meshes and LOD. Future field systems should consume topology-provided neighbours, cell areas, and shared-edge data.

---

## Recommended roadmap for Hexel

### Phase 1 — Spherical topology proof

- Implement a tiny topology interface with stable cell IDs, neighbours, surface positions, and normals.
- Compare cubed-sphere patches against a geodesic hex/pentagon graph using the same test harness.
- Add ray-to-cell selection, a terrain edit, and save/load.
- Add a debug view of IDs, normals, neighbours, and seams.

### Phase 2 — Meshing benchmark

- Preserve the current macroquad exposed-face mesher as a baseline.
- Create reproducible fixtures and count faces/vertices/indices and mesh-build time.
- Evaluate face merging suitable for Hexel's geometry; do not assume the axis-aligned binary greedy algorithm directly supports hexel prisms or spherical cells.
- Choose the simplest approach that meets measured targets.

### Phase 3 — Spherical fields and biomes

- Define a field API that queries cell/environment values without depending on render meshes.
- Implement a spherical seam-safe noise experiment and a debug map.
- Classify biomes from field inputs, not as a replacement for the fields themselves.
- Test continuity across patch boundaries and at exceptional regions.

### Phase 4 — Vegetation and local simulation

- Add a deterministic vegetation prototype using local surface frames.
- Store plant state independently from generated meshes.
- Add one small local simulation that iterates topology-provided neighbours.
- Before hydrology, define cell areas, shared-edge measures, and tests for water conservation.

## What not to copy directly

- Do not port Bevy systems or wgpu code wholesale into macroquad.
- Do not make current planar axial coordinates the universal identity model for spherical terrain.
- Do not use latitude/longitude as an unexamined 2D noise domain.
- Do not treat biome classification as climate simulation or as actual ecology.
- Do not assume cellular automata conserve mass or energy.
- Do not adopt a meshing algorithm without equivalent-output benchmarks on Hexel's geometry.
- Do not pin Hexel's API to a rapidly changing third-party API without checking versions and license compatibility.

## Source list

1. TanTanDev GitHub profile: https://github.com/TanTanDev
2. Binary greedy mesher demo: https://github.com/TanTanDev/binary_greedy_mesher_demo
3. Binary greedy meshing reference: https://github.com/cgerikj/binary-greedy-meshing
4. *Greedy Meshing Voxels Fast*, Handmade Seattle 2022: https://youtu.be/4xs66m1Of4A
5. First voxel engine: https://github.com/TanTanDev/first_voxel_engine
6. First voxel engine video: https://youtu.be/96ht7rd3Y5I
7. Worley biomes: https://github.com/TanTanDev/worley_biomes
8. TanTanDev's bracket-fast-noise: https://github.com/TanTanDev/bracket-fast-noise
9. Shrubbery: https://github.com/TanTanDev/shrubbery
10. Shrubbery changelog: https://github.com/TanTanDev/shrubbery/blob/main/CHANGELOG.md
11. 3D cellular automata: https://github.com/TanTanDev/3d_celluar_automata
12. Cellular automata video: https://youtu.be/63qlEpO73C4

## Final recommendation

Treat TanTanDev's repositories as a collection of focused engineering references, not a single engine to copy. For Hexel's chosen Earth-sized spherical direction, prioritize **stable spherical topology and debug tooling first**, use the mesher demo to design fair performance experiments, then adapt Worley classification and vegetation around spherical surface frames and environmental fields. The long-term differentiator should be the integration of these systems into a coherent, editable, finite spherical world.
