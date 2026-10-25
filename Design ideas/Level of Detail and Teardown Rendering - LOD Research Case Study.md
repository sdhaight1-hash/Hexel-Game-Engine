# Level of Detail (LOD) for Hexel Engine — Research Case Study

**Status:** Research and design guidance; no implementation decision is locked in  
**Date:** 2026-10-21  
**Scope:** LOD for a destructible, editable world made from discrete hexagonal-prism cells (“hexels”), including conventional mesh LOD, point/splat representations, hierarchical voxel data, and lessons from Teardown  
**Repository:** [Hexel-Game-Engine](https://github.com/sdhaight1-hash/Hexel-Game-Engine)

---

## 1. Executive summary

The strongest recommendation for Hexel is to **keep the current exposed-face mesh renderer and establish a measurable, staged LOD path**, rather than immediately replacing rendering with point splats or a sparse voxel octree (SVO).

The proposed first steps are:

1. Keep the world’s exact occupancy and gameplay state authoritative and independent from its rendered representation.
2. Establish a reliable baseline for the current full-detail geometry.
3. Add a conventional simplified/chunk LOD (LOD1) and measure whether it improves frame time, memory use, mesh-build time, and draw-call cost.
4. Add coarser levels and neighbor-aware transitions only when measurements show a need.
5. Treat point/splat rendering, hierarchical occupancy, and voxel-space ray tracing as separate possible techniques—not as synonyms for “far-away LOD”—and prototype them only against a specific measured bottleneck.

Teardown is an important reference because its destructible world uses voxel volumes and a specialized voxel-space ray-tracing renderer. Its public technical material describes multiresolution voxel data used to accelerate ray traversal. **That is not evidence that Teardown simply swaps nearby meshes for distant point splats, nor does it establish that its coarse levels are ordinary camera-distance mesh LODs.** The most transferable ideas are compact world representation, hierarchical summaries, variable precision for different queries, and a clear separation between authoritative world state and derived render data.

No single source proves which renderer is fastest for Hexel. The best choice depends on Hexel’s geometry, edit frequency, visibility, camera range, lighting, hardware targets, and the cost of rebuilding representations.

## 2. Hexel context and constraints

Hexel is a Rust engine using macroquad. The world is intended to consist of discrete hexagonal-prism cells arranged on a three-dimensional hex grid. It must support more than simple height-map terrain: caves, tunnels, arches, overhangs, and potentially floating structures must be representable.

Relevant current-project context at the time of this study:

- World occupancy is represented separately from rendered geometry.
- The world implementation has used sparse occupied-cell storage, dirty-chunk tracking, and cached chunk meshes.
- Mesh generation emits exposed faces and avoids internal faces between neighboring occupied cells.
- Raycasting and cell highlighting are gameplay/editor concerns, not merely rendering details.
- A target of **16 × 16 × 16 logical hex coordinates** has been discussed as a chunk-size benchmark target, not as a proven optimal size.
- The current chunk/culling/outlining code has had differing chunk-size constants in separate places. LOD work should avoid creating more independent definitions of chunk dimensions.
- Edits need to invalidate affected derived data, including neighboring chunks when a face or transition at a boundary depends on the edit.
- Rendering LOD must not silently change the world’s authoritative occupancy, editing result, or collision/raycast answer.

These are project-specific constraints. They are not claims about the internal design of any external game.

## 3. Terminology: techniques that should not be conflated

### 3.1 Mesh LOD

The same surface is represented by meshes at different levels of detail. Nearby chunks use a detailed mesh; distant chunks use simplified meshes. The renderer selects a level based on distance, projected screen size, or another visibility/error metric.

Potential benefits:
- Fewer vertices and triangles for distant geometry.
- Existing rasterization and material pipelines can often be reused.
- Surface rendering remains compatible with conventional depth testing and triangle-based graphics.

Costs and risks:
- Each level takes storage and build time.
- Switching levels can pop unless selection is stable or transitions are blended.
- Adjacent chunks at different levels can form cracks or visible mismatches.
- Dynamic edits may require rebuilding multiple derived levels.
- Simplification must preserve important silhouettes, narrow structures, and the appearance of destructible surfaces.

### 3.2 Chunk simplification / progressive meshes

A chunk’s detailed mesh is simplified or represented in a way that can provide several levels of detail. Lysenko’s “A Level of Detail Method for Blocky Voxels” discusses progressive vertex clustering and POP buffers as a way to produce LOD for blocky voxel terrain.

This is relevant prior art, not a drop-in implementation specification. Hexel’s cells are hexagonal prisms rather than ordinary cubic voxels, so the simplification error metric and boundary rules must be validated for Hexel geometry.

### 3.3 Point clouds and point/splat rendering

A distant surface can be represented by points or splats rather than by all of its triangles. Depending on the method, each sample may carry position, color, normal, radius, or other shading data.

Potential benefits:
- A compact representation can depict dense detail without preserving every original triangle.
- A sample density tied to projected pixel size can avoid spending geometry on details too small to see.
- Aggregation can help with very dense geometry.

Risks and open questions:
- A point representation is not automatically cheaper. Overdraw, sorting, blending, point-size limits, shader work, and coverage gaps can dominate.
- Sparse samples can show holes, shimmer, or unstable coverage while the camera moves.
- Splats may not preserve watertight solid surfaces, exact silhouettes, or the semantic meaning of exposed faces.
- Editing, collision, raycasting, shadows, and material boundaries still need authoritative data or suitable separate representations.
- A point renderer would be a second rendering path with its own data generation, invalidation, and debugging burden.

The Nanite foliage article is an example of aggregating dense geometry at approximately pixel scale. It should be read as a demonstration of a technique for that rendering system and content class, not as proof that custom point splats are the right answer for Hexel.

The TRIPS material is useful background on trilinear point splatting and how point/splat methods differ from Gaussian splatting. It does not establish that a splat renderer is a direct replacement for Hexel’s editable solid-cell surface.

### 3.4 Hierarchical voxel data / SVO / DAG

A hierarchical structure summarizes occupancy or other properties at progressively coarser spatial scales. An octree partitions three-dimensional space into eight children per node; sparse voxel octrees omit empty regions. A directed acyclic graph can merge identical subtrees, reducing storage for repeated patterns.

A hierarchy can help accelerate queries, visibility decisions, streaming, or voxel-space rendering. But a hierarchy is not itself a complete LOD policy or renderer. It introduces construction, update, memory, traversal, and synchronization costs. Destructible worlds make update behavior especially important.

For Hexel, a hierarchy should be justified by a concrete query or rendering bottleneck. It should not be introduced just because an external renderer uses one.

### 3.5 Voxel-space ray tracing

Instead of rasterizing a triangle mesh, a renderer traces rays through a voxel representation. It may use hierarchical occupancy, bit-packed data, coarse levels, or specialized traversal algorithms.

This is the central distinction in the Teardown case study below. It is a different rendering architecture from simply switching between detailed and simplified triangle meshes.

### 3.6 LOD transitions

When neighboring chunks use different resolutions, their borders must remain visually acceptable. Approaches include:
- Restricting which neighboring LOD levels may touch.
- Aligning chunk boundaries and simplified border geometry.
- Generating explicit transition geometry.
- Using skirts or other seam-hiding geometry where appropriate.
- Using screen-space or temporal blending when supported by the representation.

Transvoxel is a well-known transition-cell method for stitching different-resolution volumetric surfaces, especially Marching Cubes-style terrain. It is relevant background, but it is **not directly a ready-made solution for Hexel’s discrete hexagonal-prism cell faces**. Hexel should first investigate aligned boundaries, neighbor-aware LOD selection, and hexel-specific border rules.

---

## 4. Research findings: general voxel LOD

### 4.1 Lysenko — “A Level of Detail Method for Blocky Voxels”

Reference: https://0fps.net/2018/03/03/a-level-of-detail-method-for-blocky-voxels/

This article explores LOD for blocky voxel terrain using progressive vertex clustering and POP buffers. The broad lesson is that a voxel-derived surface can have multiple representations, and simplification can be structured to support changing detail rather than requiring a wholly independent manually authored model for every distance.

**Relevance to Hexel**
- Supports investigating mesh-based LOD before committing to a different renderer.
- Raises the need to manage boundaries and transitions across chunks.
- Provides a useful conceptual starting point for progressive detail.

**Limits**
- It does not directly specify a Hexel-compatible algorithm.
- Cube-based blocky terrain and Hexel’s hexagonal-prism topology differ.
- The method’s performance should not be assumed without a benchmark on Hexel workloads.

### 4.2 Aokana — “A GPU-Driven Voxel Rendering Framework for Open World Games” (2025)

Reference: https://arxiv.org/html/2505.02017v1

The paper describes a GPU-driven voxel-rendering framework for open worlds, including sparse voxel DAGs, LOD, and streaming. Its reported results belong to its framework and test conditions.

**Relevance to Hexel**
- Shows that hierarchy, LOD, and streaming can be designed as connected parts of a large-world rendering architecture.
- Provides research context for GPU-driven approaches and compact voxel structures.
- Helps identify a possible future direction if world scale and query costs eventually justify it.

**Limits**
- The paper’s reported performance is not a prediction for Hexel.
- Its architecture should not be adopted wholesale before Hexel has a baseline and a defined bottleneck.
- A hierarchy may be more useful for traversal/streaming than for directly simplifying Hexel’s rasterized meshes.

### 4.3 Transvoxel

Reference: https://transvoxel.org/

Transvoxel introduces transition cells to stitch voxel terrain surfaces at different resolutions. It is especially associated with smooth volumetric terrain and Marching Cubes-like surfaces.

**Relevance to Hexel**
- Demonstrates that LOD seams are a first-class technical problem.
- Provides ideas and reference material for transition topology.
- Encourages explicit testing of borders between neighboring chunks at different detail levels.

**Limits**
- Hexel has discrete hexagonal-prism cells and exposed-face meshing, not the same surface representation as the method’s usual use cases.
- It should not be integrated by name alone; the topology and assumptions would need to match.
- Initial Hexel experiments should favor aligned chunk boundaries, neighbor-aware LOD selection, and direct tests of hexel-specific seams.

### 4.4 Godot Voxel Tools

Reference: https://github.com/Zylann/godot_voxel

Godot Voxel Tools is an open-source voxel-terrain implementation with chunked terrain and LOD approaches, including smooth terrain LOD that uses Transvoxel-style transitions.

**Relevance to Hexel**
- A practical reference for the engineering shape of a chunked voxel terrain system.
- Illustrates that chunk meshes and multiresolution terrain can coexist in a production-oriented tool.
- Useful for studying implementation boundaries, chunk updates, and transition handling.

**Limits**
- Godot’s terrain representations and engine architecture differ from Hexel.
- Its smooth-terrain transition code is not automatically appropriate for hexagonal-prism surfaces.
- Treat it as implementation reference material, not a requirement to use the same algorithms.

### 4.5 Nanite foliage and pixel-scale aggregation

Reference: https://dev.epicgames.com/documentation/unreal-engine/nanite-foliage?lang=en-US

Epic’s Nanite foliage material discusses representing dense foliage efficiently, including aggregation of detail around pixel scale.

**Relevance to Hexel**
- Supports the general principle that detail smaller than a pixel may be represented in aggregate rather than by fully detailed geometry.
- Suggests that distant representation should be chosen based on projected screen size and perceptual error, not distance alone.
- May be useful if Hexel later contains dense small-scale objects or decorative geometry.

**Limits**
- Nanite is a specialized rendering system with its own data structures and GPU pipeline.
- Foliage has different coverage and silhouette properties from solid, destructible hexel terrain.
- This is not evidence that Hexel should implement a custom point/splat renderer.

### 4.6 TRIPS — Trilinear Point Splatting

Reference: https://www.magnopus.com/blog/trilinear-point-splatting-trips-and-its-advantages-over-gaussian-splatting-and-adop

This overview discusses trilinear point splatting and compares it with Gaussian splatting and related approaches.

**Relevance to Hexel**
- Helps frame point/splat rendering as a distinct representation with its own reconstruction and coverage behavior.
- Encourages evaluation of sample density, projected footprint, visual stability, and overdraw.
- Could inform a future experimental far-distance renderer if a measured bottleneck remains after mesh LOD.

**Limits**
- A general point/splat technique does not automatically preserve solid terrain semantics.
- It should not be assumed to replace collision, editing, or exact ray queries.
- Hexel would need to measure both visual quality and GPU cost, not just point count.

---

## 5. Case study: Teardown

### 5.1 Why Teardown matters

Teardown is highly relevant because its world is destructible and voxel-based, and its developers have published unusually direct technical descriptions of the rendering approach. It provides a valuable comparison for world representation, destruction, and voxel queries.

Primary and supporting references:

1. Dennis Gustafsson, **“From screen space to voxel space”** (2018): https://blog.voxagon.se/2018/10/17/from-screen-space-to-voxel-space.html
2. Dennis Gustafsson, **“The Spraycan”** (2020): https://blog.voxagon.se/2020/12/03/spraycan.html
3. 80.lv, **“Teardown Developer Breaks Down Multiplayer and Voxel Destruction Tech”** (March 17, 2026): https://80.lv/articles/teardown-developer-breaks-down-multiplayer-and-voxel-destruction-tech
4. Graphics Programming Conference 2026, **“Raytracing Voxels in Teardown and Beyond”**: https://www.youtube.com/watch?v=IM1Dr98f3xU

The 80.lv article is secondary reporting. The Voxagon blog posts are first-party technical material from Teardown’s developer. The conference talk features lead rendering engineer Gabe Rundlett and CTO/engine programmer Dennis Gustafsson; the linked video is useful for a deeper explanation of the renderer and its development.

### 5.2 Teardown’s voxel-space rendering is not conventional mesh LOD

In “From screen space to voxel space,” Gustafsson describes moving from conventional screen-space approaches toward tracing rays through voxel space. The post describes techniques including:
- Tracing through voxel data rather than relying exclusively on triangle meshes for the described rendering path.
- Packing occupancy information in a compact, bit-level representation for shadow-related data.
- Using multiple mip levels / coarser voxel levels to accelerate traversal, including levels described at roughly 20 cm and 40 cm scales.
- Using a supercover traversal approach for watertight ray tracing.
- Varying ray accuracy according to the use case.
- Combining screen-space and voxel-space ray-tracing techniques.

These details are useful because they show that “voxel rendering” can be an algorithm built around spatial traversal, not merely a mesh generated from occupied cells.

**Important distinction:** The described coarse levels are part of a multiresolution representation used to accelerate voxel-space ray traversal. The source does **not** establish that these levels are simply conventional camera-distance triangle-mesh LODs, and it does not support describing Teardown as “near voxels, far points.”

### 5.3 Compact material indices and a material palette

In “The Spraycan,” the developer describes using 8-bit voxel material indices with a palette containing material properties such as color, roughness, emissiveness, reflectivity, and physical type.

**Potential lesson for Hexel:** Store compact material identifiers in world cells and keep material definitions/properties in a separate table or palette, if that suits Hexel’s material requirements. This can reduce repeated per-cell property storage and centralize material behavior.

This is a design lesson, not a claim that Hexel already uses this layout or that an 8-bit ID is necessarily sufficient for Hexel’s future material count.

### 5.4 Voxel volumes and destruction

The 2026 80.lv interview describes object geometry stored as voxel volumes and discusses how voxel representation supports destruction without the same constraints as fragmenting traditional meshes. It also describes the renderer marching through voxel volumes along rays for the discussed rendering path.

**Potential lesson for Hexel:** An authoritative cell/voxel representation is a strong foundation for destruction and editing. Derived surfaces can be rebuilt or updated from that data. Keeping the authoritative world state distinct from render caches makes it easier to add alternate representations without making those representations the source of truth.

**Limits:** Teardown is a mature engine with different performance budgets, content, lighting, and rendering architecture. Public descriptions do not reveal every implementation detail. Its architecture is a source of ideas, not a performance guarantee or a mandate to rebuild Hexel as a ray tracer.

### 5.5 What Hexel should take from Teardown

Transferable ideas worth considering:
- **Separate authoritative world data from rendering representations.** The renderer may change without changing what cells exist.
- **Use compact cell/material data where appropriate.** Keep material properties separate from occupancy if this is beneficial for memory and flexibility.
- **Use hierarchy when it accelerates a specific operation.** Coarse occupancy summaries can help queries, traversal, streaming, or visibility—but must be measured.
- **Vary precision by task.** Picking, collision, shadows, and distant visual rendering may not need identical data or accuracy, provided gameplay semantics remain correct.
- **Treat ray traversal and mesh rendering as different architectures.** A voxel-space ray tracer is not a small extension to the current triangle-mesh renderer.
- **Profile before adopting complexity.** Teardown’s techniques are tuned to its engine and target workloads.

Ideas not to assume:
- That Teardown uses far-distance point splats as its main LOD.
- That the described voxel mip levels are equivalent to a camera-distance mesh LOD ladder.
- That a sparse voxel octree, DAG, or ray-tracing renderer is automatically faster for Hexel.
- That Hexel needs to replace its current mesh renderer to benefit from Teardown’s data-architecture lessons.

---

## 6. Recommended Hexel architecture

The recommended architecture has three explicit layers.

### 6.1 Layer A — Authoritative world state

This layer answers what exists and what material or state each cell has. It is the source of truth for:
- Editing and destruction.
- Placement and removal.
- Collision and gameplay queries.
- Exact cell selection / raycasting.
- Saving and loading.
- Any future simulation that depends on occupancy.

Rendering LOD must never delete, merge, or rewrite authoritative cells merely because they are too small to see at a distance.

### 6.2 Layer B — Derived representations and caches

Derived data is built from authoritative state:
- Detailed exposed-face meshes.
- Simplified mesh LODs.
- Optional coarse occupancy summaries or hierarchical nodes.
- Optional distant point/splat samples.
- Optional shadow or lighting acceleration data.

Each derived representation needs explicit ownership, build rules, cache keys, and invalidation behavior. A world edit marks affected representations dirty; a background or incremental builder updates them safely.

A change to one cell can affect its own exposed faces and the faces of neighboring cells. If LOD simplification crosses chunk boundaries or uses neighbor information, invalidation may need to extend to neighboring chunks and all affected LOD levels.

### 6.3 Layer C — Representation selection and renderer

The renderer decides which representation to use for a visible chunk or region. Selection can consider:
- Camera distance and projected size.
- Screen-space geometric error.
- Visibility and occlusion.
- Available cached representations.
- Transition compatibility with neighboring chunks.
- Memory budget and streaming state.

This policy should be independent from the world’s data semantics. It should be possible to switch a chunk from detailed mesh to simplified mesh without changing raycast results or occupancy.

### 6.4 Suggested initial LOD levels

| Level | Initial representation | Intended use | Status |
|---|---|---|---|
| LOD0 | Current full-detail exposed-face mesh | Near camera; editing and silhouette-critical geometry | Existing baseline |
| LOD1 | Simplified or reduced-detail chunk mesh | Medium distance where simplification saves meaningful work | First experiment |
| LOD2 | More aggressively simplified mesh or another measured representation | Far distance | Only after LOD1 is measured |
| Optional | Point/splat or hierarchical voxel representation | Only if conventional LOD leaves a demonstrated bottleneck | Research/prototype, not a commitment |

LOD distances and error thresholds should not be chosen by intuition alone. Begin with conservative settings, test moving-camera stability, and measure both GPU and CPU costs.

### 6.5 Why mesh LOD comes first

Mesh LOD is the least disruptive experiment because Hexel already builds meshes from exposed cell faces. It can reuse much of the current rendering path and preserves conventional depth and surface behavior.

A first version could:
1. Build the existing detailed mesh.
2. Build a simplified representation for selected chunks.
3. Choose between them using a simple, stable projected-size or distance threshold.
4. Add neighbor constraints so adjacent chunks do not create unacceptable cracks.
5. Rebuild derived representations after edits.
6. Compare measured results against LOD0-only rendering.

This does not mean mesh LOD is guaranteed to win. It means it is a lower-risk hypothesis to test before introducing a separate renderer.

---

## 7. Chunk sizing and boundary rules

The discussed **16 × 16 × 16 logical-coordinate chunk** is a benchmark candidate, not a proven optimum. Chunk size affects:
- Number of chunk objects and draw submissions.
- Mesh size and index limits.
- Amount of geometry rebuilt after a local edit.
- Culling granularity.
- LOD selection granularity.
- Neighbor-boundary handling.
- Memory and cache overhead.

Small chunks offer finer culling and smaller rebuild units but create more bookkeeping and potentially more draw calls. Large chunks reduce chunk count but can make edits and mesh rebuilds more expensive and reduce culling precision. LOD can change these trade-offs.

Recommendations:
- Benchmark several chunk sizes using the same representative worlds and camera paths.
- Centralize chunk-dimension definitions; avoid mismatched hardcoded sizes in meshing, culling, highlighting, and LOD.
- Test edits on chunk faces, edges, and corners.
- Test adjacent chunks at different LOD levels.
- Make occupancy, face exposure, and chunk-boundary conventions explicit.
- Preserve full three-dimensional occupancy so caves, arches, overhangs, and floating structures are not reduced to a height map.

---

## 8. LOD transition and correctness requirements

A successful LOD system must not merely look good in a static screenshot. It must remain stable while the camera and world change.

### Required tests

- Move the camera slowly toward and away from an LOD boundary; check for popping, flicker, or rapid level switching.
- Move sideways along a boundary between chunks with different LODs.
- Test every possible neighbor direction relevant to Hexel’s horizontal hex grid, as well as vertical chunk boundaries.
- Place and remove cells near chunk faces, edges, and corners.
- Edit cells that change which faces are exposed.
- Test thin walls, isolated columns, narrow arches, tunnels, and overhangs.
- Verify raycast/picking continues to identify the correct authoritative cell when the renderer displays a coarser LOD.
- Verify collision and gameplay logic do not use simplified visual geometry unless explicitly designed to do so.
- Check shadows, material boundaries, and silhouettes for each representation.
- Confirm stale cached LOD meshes are not displayed after an edit.
- Confirm chunk visibility and culling agree with the actual chunk dimensions.

### Neighbor-aware LOD

As an initial rule, constrain adjacent chunks to differ by no more than one LOD level if that simplifies transition handling. This is a possible policy to test, not a universal requirement. If this constraint causes too many chunks to use higher detail, benchmark the trade-off.

Do not import Transvoxel directly without confirming that its topology matches Hexel’s cell surfaces. Prefer a small, explicit test scene that exposes seam behavior before applying transitions to a large world.

---

## 9. Point/splat LOD: when it might be worth a prototype

A point/splat renderer should remain an optional branch of research, not the default next milestone.

It becomes more interesting if measurements show that:
- Distant detailed meshes still dominate GPU cost after conventional LOD.
- The world contains enough small detail that aggregation is visually acceptable.
- A stable point density and footprint can be chosen based on screen-space coverage.
- The implementation can avoid unacceptable holes, flicker, and overdraw.
- The same authoritative occupancy continues to support editing and gameplay independently.

A fair experiment should compare point/splat rendering with LOD1/LOD2 meshes under the same camera path, world content, resolution, and material conditions.

Measure:
- GPU frame time and CPU frame time.
- Number of submitted vertices/triangles or points.
- Fragment workload/overdraw where available.
- Memory consumed by each representation.
- Representation build and rebuild time.
- Edit-to-visible-update latency.
- Visual stability during camera movement.
- Silhouette and surface coverage quality.
- Performance with shadows and material variation enabled.

Do not judge the experiment only by the number of points compared with the number of triangles. Fewer primitives do not guarantee lower total cost.

---

## 10. Hierarchical occupancy: when it might be worth a prototype

A sparse hierarchy, SVO, or DAG should be considered only after a concrete need appears, such as:
- Very large worlds where sparse spatial queries are expensive.
- Ray traversal through voxel data.
- Visibility or occupancy queries over large regions.
- Streaming or memory pressure that benefits from coarse summaries.
- A future voxel-space lighting or rendering path.

Potential advantages:
- Skip empty space or whole regions in a query.
- Store coarse occupancy summaries for fast decisions.
- Share repeated substructures in suitable data.
- Support multiple spatial scales.

Potential costs:
- More complex updates when cells are edited.
- Additional memory for nodes, links, and summaries.
- Build/rebuild and synchronization work.
- Harder debugging and more complex consistency rules.
- DAG sharing can make local mutation and ownership more complicated.

A simpler coarse occupancy grid or per-chunk summary may be sufficient for an initial experiment. Choose the least complex structure that answers the measured problem.

---

## 11. Benchmark plan

Before changing architecture, capture a reproducible baseline. Use fixed world fixtures and repeatable camera paths so results can be compared after each change.

### 11.1 Test scenes

At minimum, create or retain representative cases:
1. Mostly empty world with a small structure.
2. Broad, exposed terrain with long sight lines.
3. Dense built-up region with many exposed faces.
4. A cave/tunnel/overhang scene.
5. A world with many small independent structures.
6. An edit-heavy scene where cells are repeatedly added and removed.
7. A scene with many chunks visible at once.
8. A scene where many chunks sit near LOD thresholds.

### 11.2 Record

- Average frame time and frame-time percentiles, not just FPS.
- CPU time spent on meshing and LOD building.
- GPU frame time, if measurable with the available tools.
- Number of visible chunks and draw calls.
- Mesh vertex/index counts by LOD.
- Cached mesh memory and total derived-data memory.
- Number of dirty chunks and time to rebuild after edits.
- Time from an edit to the correct visible result.
- Visual errors: cracks, missing faces, silhouette loss, flicker, or stale meshes.

### 11.3 Compare candidates

Compare at least:
- **A:** LOD0 only (baseline).
- **B:** LOD0 + LOD1 simplified meshes.
- **C:** LOD0 + LOD1 + LOD2.
- **D:** Optional point/splat prototype, only if justified.
- **E:** Optional hierarchy-assisted query/render prototype, only if justified.

Keep content, camera path, resolution, material setup, and other conditions the same. Record the hardware and build configuration. Do not compare results from different workloads as if they were equivalent.

### 11.4 Decision rule

Adopt a more complex technique only if it yields a meaningful improvement in the bottleneck that matters, without unacceptable costs in visual quality, edit latency, memory, or implementation complexity.

If frame time is dominated by CPU mesh rebuilds, a distant point renderer may not help. If it is dominated by GPU shading or overdraw, reducing triangle count may not be enough. If the bottleneck is draw-call or culling overhead, chunk organization may matter more than surface simplification.

---

## 12. Suggested implementation sequence

### Phase 0 — Baseline and invariants
- Document current world occupancy, mesh cache, dirty-chunk behavior, raycasting, and culling.
- Ensure one source of truth for chunk dimensions.
- Add repeatable test scenes and performance counters.
- Keep raycast and collision results tied to authoritative occupancy.

### Phase 1 — Mesh LOD experiment
- Add LOD metadata to derived chunk representations, without changing world data.
- Generate LOD1 for a controlled subset of chunks.
- Add conservative selection rules.
- Add tests for boundary edits, seams, silhouettes, and camera movement.
- Benchmark against LOD0-only.

### Phase 2 — Robust transitions and cache invalidation
- Implement the simplest Hexel-appropriate boundary strategy.
- Track which chunk representations become stale after an edit.
- Verify neighbor invalidation and update ordering.
- Test rapid repeated edits and camera motion.

### Phase 3 — Tune and consider a second mesh level
- Try several chunk sizes and LOD thresholds.
- Add LOD2 only if LOD1 leaves a demonstrated opportunity.
- Track memory and build costs as well as frame time.

### Phase 4 — Optional research branches
Choose only a branch connected to a measured problem:
- Point/splat representation for very distant visual detail.
- Coarse occupancy hierarchy for spatial queries, traversal, or streaming.
- Specialized voxel-space ray tracing for a future renderer or lighting feature.

These branches are not required for the first LOD milestone and should not delay a working, benchmarked mesh LOD.

---

## 13. Risks and unresolved questions

- **What currently limits performance?** No LOD architecture should be selected until CPU and GPU costs are measured.
- **What is the target world scale?** A small editable world and a huge open world have different needs.
- **What platforms must be supported?** Advanced GPU-driven methods may not be suitable for every target.
- **How frequently is terrain edited?** High edit rates increase the cost of rebuilding several LODs or hierarchies.
- **How much silhouette error is acceptable?** Hexel cells can create narrow features and strong angular silhouettes.
- **How should materials be aggregated at distance?** A simplified mesh or splat can blur or erase meaningful material boundaries.
- **How should LOD interact with lighting and shadows?** A representation suitable for the main camera may not suit shadow queries.
- **How should dynamic content be treated?** Static terrain, destructible terrain, and moving objects may require different policies.
- **Will a hierarchy serve rendering, queries, streaming, or all three?** Those use cases may want different data layouts and update strategies.
- **Can any point/splat representation remain visually watertight enough?** This needs an actual prototype and camera-motion testing.

The sources provide techniques and case studies, but they do not answer these Hexel-specific questions. Benchmarks must.

---

## 14. Final recommendation

For Hexel, the recommended order is:

1. **Preserve authoritative 3D occupancy and current correct editing/raycast semantics.**
2. **Measure the current chunk-mesh renderer.**
3. **Prototype simplified mesh LOD first.**
4. **Make chunk boundaries, dirty propagation, and neighbor LOD behavior explicit.**
5. **Add more mesh levels only when measurements justify them.**
6. **Treat points/splats as an optional distant-rendering experiment, not an assumed optimization.**
7. **Study Teardown for compact voxel data, hierarchical traversal, variable precision, and separation of world state from derived rendering—but do not assume Hexel should copy its ray-tracing architecture.**
8. **Introduce an SVO/DAG or voxel-space ray tracer only to solve a measured problem.**

This path keeps the engine’s options open. It allows Hexel to learn from voxel LOD research and Teardown without paying the cost of a wholesale renderer rewrite before knowing which part of the current system needs to be faster.

---

## 15. References

All reference links gathered for this study are listed below. Sources are grouped by the part of the research they inform.

### General voxel LOD and rendering

1. Mikola Lysenko — “A Level of Detail Method for Blocky Voxels” (2018).  
   https://0fps.net/2018/03/03/a-level-of-detail-method-for-blocky-voxels/
2. Aokana — “A GPU-Driven Voxel Rendering Framework for Open World Games” (2025).  
   https://arxiv.org/html/2505.02017v1
3. Transvoxel — voxel terrain LOD transition method.  
   https://transvoxel.org/
4. Zylann — Godot Voxel Tools repository.  
   https://github.com/Zylann/godot_voxel
5. Epic Games — “Nanite Foliage” documentation.  
   https://dev.epicgames.com/documentation/unreal-engine/nanite-foliage?lang=en-US
6. Magnopus — “Trilinear Point Splatting (TRIPS) and its Advantages over Gaussian Splatting and ADOP.”  
   https://www.magnopus.com/blog/trilinear-point-splatting-trips-and-its-advantages-over-gaussian-splatting-and-adop

### Teardown and voxel-space rendering

7. Dennis Gustafsson / Voxagon — “From screen space to voxel space” (2018).  
   https://blog.voxagon.se/2018/10/17/from-screen-space-to-voxel-space.html
8. Dennis Gustafsson / Voxagon — “The Spraycan” (2020).  
   https://blog.voxagon.se/2020/12/03/spraycan.html
9. 80.lv — “Teardown Developer Breaks Down Multiplayer and Voxel Destruction Tech” (March 17, 2026).  
   https://80.lv/articles/teardown-developer-breaks-down-multiplayer-and-voxel-destruction-tech
10. Graphics Programming Conference 2026 — “Raytracing Voxels in Teardown and Beyond” (talk with Gabe Rundlett and Dennis Gustafsson).  
    https://www.youtube.com/watch?v=IM1Dr98f3xU

### Hexel project context

11. Hexel Engine repository.  
    https://github.com/sdhaight1-hash/Hexel-Game-Engine
12. Hexel issue #2 — “Goal: Adopt 16×16×16 chunks for 3D hexagonal-prism hexels.”  
    https://github.com/sdhaight1-hash/Hexel-Game-Engine/issues/2
13. Hexel — “Reactive World Architecture - PubSub and Fields.”  
    https://github.com/sdhaight1-hash/Hexel-Game-Engine/blob/main/Design%20ideas/Reactive%20World%20Architecture%20-%20PubSub%20and%20Fields.md
14. Hexel — “Voxel World Data Structures - Vintage Story Case Study.”  
    https://github.com/sdhaight1-hash/Hexel-Game-Engine/blob/main/Design%20ideas/Voxel%20World%20Data%20Structures%20-%20Vintage%20Story%20Case%20Study.md
15. Hexel — “World Data Structures - Hytale Case Study.”  
    https://github.com/sdhaight1-hash/Hexel-Game-Engine/blob/main/Design%20ideas/World%20Data%20Structures%20-%20Hytale%20Case%20Study.md

---

## 16. Evidence and interpretation notes

- This document preserves the research findings, technical distinctions, recommendations, caveats, implementation outline, benchmarking plan, and reference links from the LOD and Teardown study.
- External sources describe their own systems. Any proposed Hexel application is labeled as a possible lesson, recommendation, or experiment rather than a statement about Hexel’s existing implementation.
- Teardown’s coarse voxel levels are described here as acceleration data for voxel-space traversal; they are not re-labelled as ordinary camera-distance mesh LOD.
- No performance numbers from external papers or products are presented as expected Hexel results.
- No code changes are included in this document. It is a research/design note and does not commit Hexel to an implementation.
