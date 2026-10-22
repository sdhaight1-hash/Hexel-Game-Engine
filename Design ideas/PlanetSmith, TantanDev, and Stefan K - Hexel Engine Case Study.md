# Three Video Sources Case Study: PlanetSmith, TantanDev, and Stefan K — Lessons for Hexel Engine

**Status:** Design research; no engine code changes  
**Repository area:** `Design ideas`  
**Purpose:** Extract useful design and architecture lessons from three supplied video sources, with special attention to whether Hexel should target a finite, Earth-sized spherical world instead of an infinite flat world.

## Executive recommendation

**Treat a finite spherical world as a serious long-term direction for Hexel, but do not commit the prototype to a spherical coordinate system until we have written down and tested the world-coordinate abstraction.** The PlanetSmith series is the closest conceptual match: spherical voxel planets, hexagonal blocks, planet-scale procedural generation, climate/biomes, saving, raycasting, physics and optimization. TantanDev provides useful engineering examples in Rust and Bevy around meshing, LOD, biome generation, procedural voxel shapes and performance measurement. Stefan K's playlist appears to emphasize iterative game development: self-building voxel worlds, exploration incentives, revising restrictive world design, and gradually turning an engine into a game.

The combined lesson is not “copy these games.” It is: **keep authoritative world data independent from how it is projected, partitioned, rendered, simulated, and interacted with.** This gives Hexel room to start with a small test map while keeping a path open to a globe.

The three supplied sources are:
1. [PlanetSmith Dev Log playlist — Incandescent Games](https://www.youtube.com/playlist?list=PLXoFLvxt_ISlLLIPHJJUXSV3tr-D61ThP)
2. [TantanDev videos](https://www.youtube.com/@Tantandev/videos)
3. [Gamedev playlist — Stefan K](https://www.youtube.com/playlist?list=PLnNjjV2GwMnOyr2O8D_j9_T2IhjUEI36z)

> **Evidence note:** Public playlist pages and indexed video descriptions were available, but the research environment did not reliably expose full transcripts for every video. The analysis below distinguishes verified public descriptions/titles from recommendations inferred for Hexel. It does not claim access to source code or hidden implementation details.

---

## 1. PlanetSmith: the closest match to Hexel's identity

### What the source covers

The playlist is titled *PlanetSmith Dev Log* and lists 25 videos. Its early sequence includes:
- [What if Minecraft was spherical — Episode 1](https://www.youtube.com/watch?v=mLONYQ4qHRA)
- [Volumetric Atmosphere & Tools — Episode 2](https://www.youtube.com/watch?v=lVuML9bjt5Y)
- [Earth-like world generation — Episode 3](https://www.youtube.com/watch?v=0uXbvKKwpws)
- [Creating the first Biomes — Episode 4](https://www.youtube.com/watch?v=FRPGY-Ovdjs)
- [Hexagonal Textures — Episode 5](https://www.youtube.com/watch?v=eqU7rPSe0Js)
- [Mesh Blocks — Episode 6](https://www.youtube.com/watch?v=qW726K0m3uc)
- [Smart Blocks — Episode 7](https://www.youtube.com/watch?v=wzsLSS0idiU)
- [Lighting a Voxel Planet — Episode 8](https://www.youtube.com/watch?v=8ShIIDSIgBg)
- [Saving Worlds — Episode 9](https://www.youtube.com/watch?v=lUcDWMNpBCg)
- [Hexel Raycasting — Episode 10](https://www.youtube.com/watch?v=NrLV9nHWnKE)
- [Adding Explosions — Episode 11](https://www.youtube.com/watch?v=feiMoHuWtQQ)
- [Optimising voxel data for potato PCs — Episode 12](https://www.youtube.com/watch?v=qVwJtvY0qvs)

The official [PlanetSmith site](https://www.planetsmith.world/) describes a game built around fully spherical, procedurally generated voxel planets, hexagonal building blocks, climate and biomes, a volumetric atmosphere, and travelling between worlds. Its [Steam page](https://store.steampowered.com/app/2539340/PlanetSmith/) describes its hex-based building approach as intended to follow planetary curvature.

### Why a globe changes the design

An Earth-sized globe is not simply an infinite flat map with the edges bent around. It changes the assumptions behind coordinates, neighbours, movement, terrain generation, lighting, climate, and persistence.

**Potential advantages for Hexel**
- **A coherent world with meaningful scale:** the player can learn a finite place, circle a planet, and understand where they are. “More world” comes from variety and connected systems, not endless travel alone.
- **A natural global-to-local simulation hierarchy:** global latitude/longitude-like fields can describe climate and large-scale water flow, while local hexels represent editable terrain and structures.
- **Better geographic identity:** poles, equator, mountain ranges, watersheds, prevailing climate patterns and continents can create meaningful regional differences.
- **Finite-world simulation:** a bounded globe gives the water, nutrients, materials and ecosystems explicit inventories and destinations. Conservation and long-running environmental consequences become easier to define conceptually, even if computationally they remain challenging.
- **A strong design identity:** hexagonal terrain wrapped around a sphere is a specific and recognizable premise, not just another infinite voxel sandbox.

**Costs and risks**
- A single flat axial hex grid cannot wrap a sphere without distortion or special topology. Hexagonal tiling on a sphere requires exceptional cells, seams, a different surface partition, or a hierarchical patch system.
- Gravity and “up” must be local to the planet surface, not a constant global vector.
- Movement, raycasting, terrain edits and physics must behave correctly across patch boundaries and around the poles.
- A true planet needs multiple levels of detail and a strategy for far-side visibility, horizon culling, precision, and streaming.
- Climate and hydrology should respect spherical distances, elevation, flow networks and global boundary conditions. A naïve flat noise field will not automatically produce credible Earth-like geography.
- A planet-sized world can still be empty and uninteresting. Finite size is not a substitute for traversal, ecology, resources, and player goals.

### Implications for our current Hexel prototype

The current prototype uses axial hex coordinates, vertical columns, per-column meshes and raycasting to identify editable hexels. That is a good local prototype, but those concepts currently imply a mostly flat grid. The globe question should be addressed before the coordinate assumptions spread through every subsystem.

Recommended conceptual separation:

1. **World identity / topology:** which places exist and which places are neighbours.
2. **Surface mapping:** how a surface location maps to 3D position and local tangent directions.
3. **Authoritative terrain:** column heights, block/material data, edits and save state.
4. **Derived representations:** meshes, collision surfaces, lighting, LOD and debug overlays.
5. **Simulation fields:** temperature, moisture, water, nutrients, biome suitability and other environmental variables.
6. **Interaction queries:** surface raycasts, placement/removal, movement and selection, expressed against world topology rather than renderer-specific meshes.

This separation would allow the prototype to use a small flat patch while we test the interfaces, then compare a globe patch implementation without rewriting all game logic.

### Important design fork: what exactly is a “hexel” on a globe?

Do not assume a planar regular hexagon can be copied unchanged onto a sphere. Possible paths include:

- **Cubed-sphere or other multi-patch surface:** split the globe into mapped regions, each with a local grid. This gives manageable patches but requires careful cross-patch neighbour mapping and may distort cells.
- **Geodesic/polygonal sphere:** build a globe from mostly hexagonal cells plus exceptional non-hexagonal cells. This offers a more direct spherical cell graph, but neighbourhood and geometry logic is more complex.
- **Hexagonal columns on a curved local surface:** preserve the existing local column concept in small patches, but adapt orientation and geometry to the sphere. This may be easier to prototype, though seams and global consistency still need design.

These are options to test, not a claim about PlanetSmith's internal implementation.

### Recommendation

Prototype the smallest meaningful spherical experiment before adding a huge amount of gameplay:
- A low-resolution globe with labelled cells and neighbour links.
- Player movement with local gravity and a stable “up” vector.
- Raycast and edit one surface cell on every side of the globe.
- Cross-patch/pole traversal tests.
- Save/load a terrain edit and verify its identity after reload.
- A simple latitude-dependent temperature field as a first global field.

If this prototype makes world interaction brittle or forces too many special cases, learn that while the engine is still small.

---

## 2. TantanDev: engineering practices for a performant, editable voxel world

### What the source covers

Tantan describes a Rust-based voxel project using Bevy. Public channel listings and indexed descriptions include:
- [Voxel Game Development Is Hard](https://www.youtube.com/watch?v=Lv6WEFGzqNQ): a broad overview of a voxel game and its systems.
- [Pushing the rendering limits — Rust Voxel Engine](https://www.gamesinprogress.com/indie-game-developers/tantan/pushing-the-rendering-limits-rust-voxel-engine): describes PBR, shadows, fog, LOD, far-distance heightmap approximation and a skybox.
- [Blazingly Fast Greedy Mesher — Voxel Engine Optimizations](https://www.gamesinprogress.com/indie-game-developers/tantan/blazingly-fast-greedy-mesher-voxel-engine-optimizations): describes a greedy mesher implemented in Rust/Bevy using bitwise operations.
- [How to Implement Worley Biomes in Voxel Games with Rust](https://www.gamesinprogress.com/indie-game-developers/tantan/how-to-implement-worley-biomes-in-voxel-games-with-rust): covers Worley noise, biome classification and blending, and integration into a voxel engine.
- [Procedual Voxel Tree Generation](https://www.youtube.com/watch?v=xQLVYnD43vI): describes procedural trees using space colonization.
- [Making procedural trees better in my Voxel Game](https://www.youtube.com/watch?v=sCeIH_wYdy0): indexed descriptions discuss a reusable voxel-shape library, live-reload workflow and benchmarking a voxelization improvement.

A secondary description of the tree-generation work reports that the newer voxelization approach reduced generation time for a large tree from roughly 30 ms to roughly 300 microseconds by iterating over branch bounds rather than testing every point in the full bounding volume. Treat this as a reported result for that workload, not a guarantee for Hexel. See [the indexed write-up](https://daily.dev/posts/making-procedural-trees-better-in-my-voxel-game-suzbbaebf).

### Lessons for Hexel

**1. Measure the actual bottleneck before optimizing.**  
A mesher, world-data lookup, terrain generator, lighting pass, collision query and simulation update have different cost profiles. Add repeatable benchmarks for each before pursuing clever low-level techniques.

**2. Use LOD as a family of representations.**  
Near terrain may need editable detailed geometry; distant terrain can use simplified meshes, height approximations, or other representations. The authoritative world should not become the LOD mesh itself. This aligns with the existing Hexel LOD research case study.

**3. Greedy meshing is a candidate, not a premature requirement.**  
Our current renderer emits exposed faces and builds one mesh per column. That is a straightforward correctness baseline. First test it against representative worlds, then measure vertex count, mesh-build time, draw calls and frame time. Greedy meshing is most useful where adjacent faces can be merged without losing required material, lighting or edit detail.

**4. Procedural content should have bounded work.**  
The tree example illustrates a general algorithmic lesson: calculate the region a feature can affect, and iterate only over that region. Avoid scanning a large bounding volume for every branch or feature when local bounds can reduce work.

**5. Make procedural generation inspectable and reproducible.**  
Seeded generation, tunable parameters, a runtime inspector and benchmark fixtures make it possible to distinguish algorithm improvements from random variation. Keep generated data stable enough that a terrain bug can be reproduced.

**6. Biome classification and blending are separate operations.**  
A hard biome label can create abrupt boundaries. Hexel can derive biome suitability from temperature, moisture, elevation, soil and other fields, then blend appearance or transition rules separately. A biome label should not itself mean a species is present or that a region contains a particular amount of water.

**7. Separate reusable procedural shapes from game-specific meanings.**  
A reusable generator can output abstract material/voxel identifiers, while Hexel maps those identifiers to its own terrain and simulation rules. This reduces coupling between a tree-shape algorithm and the game-specific plant, resource, or material system.

**8. Be cautious with dependencies and architecture borrowed from another engine.**  
Tantan's examples use Rust and Bevy; Hexel currently uses Rust and macroquad. The algorithms and profiling practices can transfer, but Bevy APIs, scheduling, ECS patterns and rendering integration are not drop-in replacements.

### Recommended Hexel experiments inspired by TantanDev

- Benchmark current per-column mesh generation on fixed maps.
- Record mesh build time, visible face count, vertices/indices, memory use and frame time.
- Compare current exposed-face meshing against a small greedy-meshing prototype on the same terrain.
- Add a deterministic seeded world-generation test fixture.
- Build a standalone procedural tree/shape prototype whose output can be placed into the world.
- Add a biome preview overlay based on independent environmental fields, not just random colours.
- Only add far-distance LOD once the prototype has a measured distance/rendering problem.

---

## 3. Stefan K: iteration, world generation and making exploration matter

### What the source covers

The supplied playlist is titled *Gamedev*, by Stefan K, and lists 37 videos. Its recent entries include:
- [I Made My Voxel World Build Itself](https://www.youtube.com/watch?v=c-OxkqD-8QA)
- [I Finally Gave My Voxel World a Reason to Explore](https://www.youtube.com/watch?v=6jU5iNVQI_A)
- [My Voxel World Was Too Restrictive, So I Rebuilt It](https://www.youtube.com/watch?v=d2bRgwqMxro)
- [I Finally Unleashed My RPG Creator](https://www.youtube.com/watch?v=e8kKrfD4yX0)
- [I Finally Added Combat to My RPG Creator](https://www.youtube.com/watch?v=i-y1R7zY1Vg)
- [One Scene, So Many Possibilities — VoxRPGCreator](https://www.youtube.com/watch?v=6xtJ5yIh-FQ)
- [My Voxel Survival Game Finally Feels Real](https://www.youtube.com/watch?v=FwFC4dm0HV0)

The titles alone do not establish the exact mechanics or code in each video. The practical design questions they suggest are still useful: how can a world generate itself, what makes exploration worthwhile, how do you know when world constraints are too restrictive, and how can engine capabilities be turned into playable experiences?

### Lessons for Hexel

**1. World generation should create reasons to go somewhere.**  
Terrain variety is not enough. Regions should differ in resources, environmental conditions, access routes, hazards, ecological relationships, and opportunities for construction or discovery. Our field-based simulation ideas can help make these differences causal rather than decorative.

**2. Treat “the world builds itself” as a layered process.**  
World generation can be split into stages: topology and elevation, climate and water, biome suitability, resources and flora/fauna, then points of interest and authored adjustments. Each stage should have explicit inputs and outputs so that a change to elevation or water can invalidate dependent results without regenerating everything.

**3. Player freedom and understandable rules must be balanced.**  
A world can feel restrictive when the player cannot express an idea, but unlimited freedom can also erase meaningful constraints. Hexel should make its rules consistent and visible: explain why a terrain edit changes water flow, why a biome supports certain plants, or why a building is unstable, rather than hiding arbitrary prohibitions.

**4. The engine should make prototypes cheap to build.**  
A general-purpose engine feature is valuable when it enables a small playable experiment quickly. Prefer a few complete vertical slices over many disconnected systems: edit terrain, observe the result, save/load, and verify that the world remains coherent.

**5. Distinguish engine capabilities from game design.**  
The engine can provide editable terrain, generation, fields, pathfinding, lighting and persistence. It should not hard-code every exploration objective, combat rule or progression loop. Keep gameplay systems able to use engine services without making those services depend on one game mode.

### Proposed exploration test for a finite globe

A small prototype region should answer:
- Is there a visible reason to travel beyond the starting area?
- Do environmental differences change what the player can do?
- Can the player make a meaningful terrain edit and observe downstream effects?
- Does a return trip feel like revisiting a known place rather than a procedurally unrelated patch?
- Does the globe's finite scale improve orientation and discovery, or does it make travel feel too compressed?

These are design tests, not assumptions about Stefan K's implementation.

---

## 4. Synthesis: how the three sources fit together

| Design question | Strongest source | Hexel takeaway |
|---|---|---|
| Should the world be spherical and finite? | PlanetSmith | Prototype globe topology early; don't bury flat-grid assumptions in core APIs. |
| How do we keep voxel rendering performant? | TantanDev | Benchmark first; preserve a simple correctness baseline; add LOD/greedy meshing when measurements justify them. |
| How should biomes and procedural terrain work? | TantanDev + PlanetSmith | Use seeded generation and separate environmental fields, classification, blending and actual ecology. |
| How do we make a world worth exploring? | Stefan K + PlanetSmith | Build distinct regions and meaningful consequences, not merely more terrain. |
| How do we keep the code adaptable? | All three, as interpreted for Hexel | Separate authoritative state from meshes, LOD, rendering, procedural caches and gameplay rules. |
| How do we avoid endless scope creep? | Stefan K, as a project-process lesson | Ship small end-to-end experiments and measure whether each adds a useful player-facing capability. |

## 5. Proposed architecture direction

This is a proposal for Hexel, not a claim about any source's internal implementation.

```text
World Topology / Surface Mapping
             |
             v
Authoritative Terrain + Materials
             |
     +-------+--------+
     |       |        |
     v       v        v
  Rendering Physics  Interaction
  Mesh/LOD  Collision Raycast/edit
     |
     v
Visual lighting / atmosphere / debug views

Environmental Fields
  temperature, moisture, water, nutrients, geology
             |
             v
Derived suitability / biome classification
             |
             v
Ecological state and world processes
```

Important boundary: fields and terrain state are authoritative simulation inputs; meshes and LOD are derived visual representations. A render mesh should never be the sole source of truth for a terrain edit, a water inventory, or an ecosystem state.

For a globe, topology should own neighbour relationships. Code that needs adjacent cells should ask the topology layer instead of assuming axial coordinates always have the same six neighbours everywhere. Rendering can then use local patches while the simulation uses stable cell identities and graph connections.

## 6. Recommended staged plan

### Stage 1 — Document the world model
- Define what uniquely identifies a world cell and a vertical layer.
- Define neighbour lookup independently of rendering.
- Decide which coordinate operations are flat-grid-specific.
- Keep current behaviour passing tests.

### Stage 2 — Build a tiny globe proof of concept
- Low cell count; no full planet content.
- Correct surface orientation, movement and raycasting.
- Cross-boundary adjacency and edit/save/load tests.
- Compare at least one surface topology option before selecting a long-term direction.

### Stage 3 — Measure rendering limits on the current flat prototype
- Fixed terrain fixtures and repeatable benchmark runs.
- Mesh build cost, vertices/indices, draw calls, frame time and memory.
- Test greedy meshing and a simple distant representation independently.

### Stage 4 — Make procedural generation field-driven
- Deterministic seed.
- Elevation/geology before water and climate.
- Temperature/moisture and biome suitability next.
- Resources, flora/fauna and points of interest after that.
- Visualize each field with debug overlays.

### Stage 5 — Add one end-to-end exploration scenario
For example, a small island or globe sector where a ridge blocks or redirects water, creating a wet valley and a dry slope with different ecological opportunities. Let the player alter terrain, then check that the world consequences are explainable, reproducible and persistent.

## 7. Acceptance criteria for decisions

A globe-first direction should only be selected if the prototype can demonstrate:
- Stable cell identity and neighbour lookup across the whole surface.
- Correct raycasting and edits near topology boundaries.
- Consistent local gravity and surface orientation.
- Save/load that preserves edits regardless of camera or patch location.
- A plausible plan for LOD and far-side rendering.
- A path to finite water and field simulation without special-case logic spreading through every system.

A rendering optimization should only be adopted if it improves a measured bottleneck while preserving visible correctness. A world-generation layer should be accepted only if it is deterministic under a fixed seed, inspectable, and testable in isolation.

## 8. Source list and evidence limits

### Primary video sources
- [PlanetSmith Dev Log playlist](https://www.youtube.com/playlist?list=PLXoFLvxt_ISlLLIPHJJUXSV3tr-D61ThP)
- [TantanDev channel](https://www.youtube.com/@Tantandev/videos)
- [Stefan K Gamedev playlist](https://www.youtube.com/playlist?list=PLnNjjV2GwMnOyr2O8D_j9_T2IhjUEI36z)

### Supporting sources
- [PlanetSmith official website](https://www.planetsmith.world/) — describes spherical voxel planets, climate/biomes, hexagonal blocks and atmosphere.
- [PlanetSmith on Steam](https://store.steampowered.com/app/2539340/PlanetSmith/) — public game description and development context.
- [PlanetSmith Steam community/news](https://steamcommunity.com/app/2539340/allnews/) — dated devlog announcements, including the hexel raycasting episode.
- [Tantan's public developer overview and video index](https://www.gamesinprogress.com/indie-game-developers/tantan) — indexed list of voxel rendering, biome, tree-generation and optimization videos.
- [Tantan's procedural tree-generation write-up](https://www.gamesinprogress.com/indie-game-developers/tantan/procedual-voxel-tree-generation-rust-game) — project description and tools.
- [Tantan's Worley biome tutorial](https://www.gamesinprogress.com/indie-game-developers/tantan/how-to-implement-worley-biomes-in-voxel-games-with-rust) — indexed description of biome classification, blending and integration.
- [Tantan's greedy mesher write-up](https://www.gamesinprogress.com/indie-game-developers/tantan/blazingly-fast-greedy-mesher-voxel-engine-optimizations) — indexed description of a Rust/Bevy meshing optimization.
- [Stefan K Gamedev playlist](https://www.youtube.com/playlist?list=PLnNjjV2GwMnOyr2O8D_j9_T2IhjUEI36z) — listed video titles and project progression.

### Caveats
- Video titles and descriptions are useful evidence of subject matter but do not establish all technical details or implementation choices.
- The performance figures quoted from third-party summaries are workload-specific and should not be treated as expected Hexel performance.
- No code was copied from these sources. The recommendations are design interpretations intended to guide experiments in our own Rust/macroquad project.
- This case study is research only. It does not change the engine's current flat-grid implementation or decide the globe topology for us.

## Bottom line

**Hexel should explore the globe option early, before flat-grid assumptions become expensive to undo.** PlanetSmith makes a finite spherical world feel like a coherent identity for a hexel-based engine; TantanDev offers concrete inspiration for performance measurement, LOD, procedural biomes and shape generation; Stefan K's project progression reinforces the importance of turning technical capabilities into a world with reasons to explore.

The next best step is not to build an Earth-sized world. It is to prove that Hexel can represent, navigate, edit, save and query a tiny spherical world cleanly — while keeping terrain state, simulation fields and rendered meshes separate.
