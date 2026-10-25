# Spherical Topology Research for Hexel Engine

**Status:** Research and architecture recommendation  
**Scope:** World topology and coordinate architecture for a finite spherical, potentially Earth-sized Hexel world  
**Repository area:** `Design ideas`  
**Related documents:** [PlanetSmith, TantanDev, and Stefan K case study](PlanetSmith,%20TantanDev,%20and%20Stefan%20K%20-%20Hexel%20Engine%20Case%20Study.md); [Field-Based World Simulation and Finite Hydrology](Field-Based%20World%20Simulation%20and%20Finite%20Hydrology.md); [LOD and Teardown Rendering Research](Level%20of%20Detail%20and%20Teardown%20Rendering%20-%20LOD%20Research%20Case%20Study.md)

## Executive recommendation

**Prototype a cubed-sphere surface divided into six face patches, with a quadtree or other hierarchical subdivision per face, while keeping authoritative cell identity and neighbour lookup behind a topology interface. Do not replace the current axial hex grid yet.**

For Hexel, this is the most practical first hypothesis to test because:
- Cube-face patches give the world a finite global layout and a natural basis for hierarchical LOD.
- Each patch can be worked on locally without requiring one enormous global planar grid.
- Cross-face adjacency can be implemented explicitly and tested.
- A quad-based patch system is compatible with conventional terrain mesh generation and planetary LOD literature.
- It avoids forcing a true all-hex geodesic tiling into the first prototype.

The trade-off is important: **a cubed sphere is not a perfect global hexagonal grid.** If the identity of the game depends on every surface cell being hexagonal, we must compare a geodesic/Goldberg-style cell graph with twelve pentagons and many hexagons. If hexels are instead the editable terrain unit inside local regions, cube-face patches may be the more practical engineering choice.

The recommendation is provisional. A small prototype should compare the alternatives before we commit to save formats, public APIs, or game-wide coordinate assumptions.

## 1. The actual design problem

The current prototype uses axial `HexCoord` coordinates on a pointy-top planar grid, vertical column heights, one mesh per column, and raycasting to select an exposed hexel face. This is a sound starting point for local terrain. It is not, by itself, a spherical topology.

A globe changes several foundational assumptions:

- **No single planar regular hex grid can cover a sphere without exceptions or distortion.** A familiar geodesic hexagon/pentagon tiling has twelve pentagons among the hexagons. Red Blob Games explains this constraint and explores several imperfect ways to flatten or navigate such a map. [Red Blob Games: Wraparound hexagon tile maps on a sphere](https://www.redblobgames.com/x/1640-hexagon-tiling-of-sphere/)
- A surface location needs a position and a local tangent frame; “up” depends on the planet surface.
- Six neighbours are not a universal rule if the topology includes pentagons, patch boundaries, or mixed-resolution regions.
- Distances and areas must account for curvature if they feed climate, water flow, movement, or resource calculations.
- Surface edits, raycasts, collision, saving, and simulation must agree on stable cell identity even as meshes are rebuilt or LOD changes.
- Planetary scale introduces camera-relative precision, horizon culling, patch seams, and multi-resolution rendering concerns. The planetary-terrain work on Ellipsoidal Cube Maps explicitly addresses hierarchical storage, projection distortion, and numerical precision. [Lambers and Kolb: Ellipsoidal Cube Maps for Accurate Rendering of Planetary-Scale Terrain Data](https://marlam.de/ecm/)

The most important architecture decision is therefore not “which sphere mesh looks best?” It is **what uniquely identifies a place, and which subsystem owns the rules for finding its neighbours?**

## 2. Topology options

### Option A — Cubed sphere with six face patches

A cube's six faces are mapped onto a sphere. Each face is a 2D patch, often subdivided into a quadtree for different levels of detail.

**Strengths**
- Familiar 2D indexing within each face.
- Natural hierarchy for patch subdivision, streaming, culling, and LOD.
- Well-established planetary rendering approach. A recent comparative procedural-planet study describes quadtree LOD, mesh seam stitching, and floating-point precision as core engineering challenges. [Comparative Analysis of Procedural Planet Generators (2025 preprint)](https://arxiv.org/html/2510.24764v1)
- Cross-face neighbours can be resolved with a table of face-edge transforms.
- Fits an incremental prototype: start with six low-resolution faces, then subdivide.

**Costs**
- The six faces meet at seams and corners; orientation and neighbour indexing must be correct.
- Cube-to-sphere projections distort cell area and shape to varying degrees. Choose a projection deliberately and measure the distortion.
- A square patch grid does not make the entire planet a regular hex grid.
- LOD transitions across patch edges need crack prevention and consistent sampling.
- Face-local coordinates are implementation details and should not be mistaken for globally meaningful geography.

**Hexel fit:** Best first engineering prototype if the priority is a finite editable globe, scalable rendering, and room for local hexel terrain patches.

### Option B — Geodesic / Goldberg-style hexagon-pentagon cell graph

Build a surface from an icosahedron-derived geodesic or Goldberg-style partition. The familiar soccer-ball family has twelve pentagons and a large number of hexagons; other resolutions and constructions are possible. Red Blob Games and the geodesic-grid literature describe the unavoidable exceptional cells when covering a sphere with hexagon-like regions. [Red Blob Games](https://www.redblobgames.com/x/1640-hexagon-tiling-of-sphere/) · [A New Coordinate System for Constructing Spherical Grid Systems](https://www.mdpi.com/2076-3417/10/2/655)

**Strengths**
- A globally connected surface-cell graph is the natural authoritative representation.
- Most cells can be hexagonal, which may best match Hexel's visual identity.
- Neighbour relationships are explicit graph edges rather than inferred from planar axial coordinates.
- Useful for simulation fields, finite inventories, pathfinding, and regional analysis.
- No rectangular-map poles or wraparound hacks.

**Costs**
- Twelve or more exceptional cells are not an optional bug to hide; the topology needs to support variable degree.
- A regular hexagonal grid's simple axial arithmetic no longer works globally.
- Subdivision, cell shape, area, and edge lengths may vary; simulation must account for those differences.
- Editing a cell as a stack of vertical hexel blocks still needs a rule for how those blocks follow the local surface and connect across edges.
- A graph of surface cells is not automatically a good high-detail volumetric terrain representation.

**Hexel fit:** Strong candidate if “hexel” must mean a hexagonal surface cell everywhere except the unavoidable pentagons. Likely more topology work up front, but a compelling world model for simulation-heavy gameplay.

### Option C — Six or twenty triangular patches with local grids

Partition a sphere into triangular regions based on an icosahedron, then map a local grid into each region.

**Strengths**
- Local coordinate systems can be simple.
- Triangular patch layout distributes regions around the sphere and can be useful for hierarchical construction.
- Patch identity plus local coordinates makes stable storage possible.

**Costs**
- Patch-boundary transforms are still required.
- Distances, directions, and cells may distort under projection.
- A local hex grid must be clipped or stitched across patch boundaries.
- It can combine the complexity of patch management with the complexity of hex-cell exceptions.

**Hexel fit:** Worth a small comparison only if it provides a clear advantage over cube faces or a geodesic cell graph. Do not select it just because an icosahedron sounds more spherical.

### Option D — Latitude/longitude grid

Use latitude and longitude as world coordinates and derive 3D position from them.

**Strengths**
- Familiar for geographic data and climate maps.
- Easy to reason about at the conceptual level.
- Convenient for fields expressed as latitude-dependent functions.

**Costs**
- Longitude lines converge at the poles; cell width and area change strongly with latitude.
- Pole singularities make neighbour logic and movement awkward.
- A regular longitude/latitude grid is not a uniform hex grid.
- Distances and area-weighted simulation require corrections.

**Hexel fit:** Good as a geographic coordinate or debug display, not recommended as the primary discrete topology for editable hexel terrain.

### Option E — One large flat hex map with wraparound

Keep axial coordinates and wrap selected coordinate ranges to imitate a globe.

**Strengths**
- Reuses much of the current planar-grid code.
- Cheap to prototype for a deliberately stylized map.

**Costs**
- A sphere's polar geometry cannot be faithfully represented by simply wrapping a rectangle or parallelogram.
- Distances and neighbourhoods become misleading near the poles and boundaries.
- Artificial teleport seams and camera orientation problems can spread into movement and simulation.
- It risks looking like a globe while behaving like a distorted flat map.

**Hexel fit:** Not recommended for a physically coherent, freely traversable globe. Could be acceptable for a deliberately abstract strategy-map layer.

## 3. Comparison matrix

| Criterion | Cubed-sphere patches | Geodesic hex/pentagon graph | Triangular patches | Lat/long grid | Wrapped axial map |
|---|---|---|---|---|---|
| Finite globe | Excellent | Excellent | Excellent | Excellent | Approximate |
| Global cell graph | Must be implemented across patches | Natural | Must be stitched | Awkward near poles | Artificial |
| Uniform hex cells | No | Mostly; 12 pentagons in common construction | Not inherently | No | Only locally |
| Hierarchical LOD | Excellent fit | Possible, but graph subdivision is more involved | Good | Possible | Poor fit |
| Surface cell area consistency | Projection-dependent | Construction-dependent | Projection-dependent | Poor near poles | Distorted |
| Local axial-grid reuse | Inside patches only | Limited | Inside patches only | No | High |
| Cross-boundary complexity | Face-edge transforms | Graph edges | Patch transforms | Pole special cases | Wrap special cases |
| Finite hydrology/simulation | Good with area/edge metadata | Very good graph model | Good with metadata | Needs area corrections | Weak near seams |
| First prototype risk | Moderate | Moderate-high | Moderate-high | Low initially, high later | Low initially, high later |

This is a qualitative engineering comparison, not a benchmark. Actual memory use, performance, and distortion depend on the chosen resolution and algorithms.

## 4. Recommended architecture: topology is authoritative, coordinates are adapters

Hexel should avoid exposing planar axial arithmetic as the universal definition of the world. Keep the current `HexCoord` implementation for the current flat prototype, but introduce conceptual boundaries before adding more subsystems.

### Proposed layers

1. **WorldTopology** — identifies valid surface locations and provides neighbours, edge relationships, distance/path queries, and cell geometry metadata.
2. **SurfaceMapping** — maps a topology location and terrain height to a 3D point, surface normal, and local tangent frame.
3. **TerrainState** — authoritative editable terrain, materials, vertical layers or terrain columns, and modification history.
4. **Patch/Chunk Layout** — groups cells for storage, streaming, mesh building, and LOD. It must not define permanent identity by itself.
5. **Derived Representations** — render meshes, collision meshes, lighting caches, LOD meshes, debug overlays.
6. **World Fields** — water, temperature, moisture, nutrients, geology, and other environmental values attached to stable cells or explicitly defined regions.
7. **Interaction** — raycasting, selection, placement/removal, and movement queries through topology and surface mapping rather than mesh ownership.

### Conceptual Rust API (illustrative only)

```rust
// Illustrative architecture, not drop-in code.
trait WorldTopology {
    type CellId: Copy + Eq + std::hash::Hash;

    fn neighbors(&self, cell: Self::CellId) -> Vec<Self::CellId>;
    fn cell_area(&self, cell: Self::CellId) -> f64;
    fn cell_center(&self, cell: Self::CellId) -> [f64; 3];
}

trait SurfaceMapping<CellId> {
    fn position_and_frame(
        &self,
        cell: CellId,
        local_u: f32,
        local_v: f32,
        height: f32,
    ) -> SurfaceFrame;
}

struct SurfaceFrame {
    position: [f64; 3],
    normal: [f32; 3],
    tangent_u: [f32; 3],
    tangent_v: [f32; 3],
}
```

The exact types should follow the chosen implementation; this snippet expresses the separation, not a final API. Avoid allocating a new `Vec` for every neighbour query in the eventual hot path; fixed-size neighbour buffers or iterators may be better. The topology must also expose edge lengths, orientation/transport between local frames, and cell boundaries if hydrology, movement, or polygonal meshing needs them.

### Stable identity and save data

A cell ID should survive:
- camera movement,
- mesh rebuilds,
- changes in LOD,
- unloading and reloading patches,
- application restart,
- deterministic regeneration of unmodified terrain.

A possible identity could encode topology family/version, face or base-cell ID, subdivision level, and local cell coordinates. The save format should include a version and seed and store only edits/state that differ from deterministic generated defaults where practical.

Do not use floating-point world positions as the sole permanent cell identity. Position-to-cell mapping is a query, not a stable ID scheme.

## 5. Coordinate systems and precision

Use different coordinate systems for different jobs instead of forcing one coordinate type to do everything.

- **Topology ID:** stable integer identity for cells/regions.
- **Patch coordinates:** small integer/local coordinates for mesh generation and data lookup.
- **Geographic coordinates:** latitude/longitude or equivalent only when useful for climate, user-facing mapping, or external data.
- **Planet-space position:** high-precision position relative to a planet origin for global calculations.
- **Camera-relative render position:** local floating-point coordinates near the camera for rendering.
- **Local surface frame:** normal and two tangent directions used for movement, construction orientation, and local geometry.

If Hexel eventually models a physically Earth-sized planet, a radius on the order of 6.37 million metres is a possible scale reference, but it does not force the game to use metres or Earth gravity. Choose a coherent game scale and keep global coordinates separate from camera-relative values. Planetary terrain research highlights numerical precision as a real issue for large worlds. [Ellipsoidal Cube Maps](https://marlam.de/ecm/) · [Comparative Analysis of Procedural Planet Generators](https://arxiv.org/html/2510.24764v1)

For player movement, define local gravity as a direction toward the planet centre (or toward a more general gravity field later). The camera's up vector should follow the local surface frame smoothly; it must not assume global +Y is always “up.” Test orientation while moving across patch seams and around both poles.

## 6. Hexel columns on a curved surface

The current terrain stores a number of stacked unit-height hexels at each axial coordinate. On a globe, we need to decide whether terrain layers are:

- **Radial layers:** each layer moves outward from the planet centre, so the local “vertical” is approximately radial.
- **Surface-normal layers:** each layer follows the normal of the underlying surface mapping.
- **Locally extruded polygonal prisms:** each surface cell has a polygonal footprint extruded along its local normal, with explicit rules for neighbour-face connections.

These models differ on steep terrain and at cell boundaries. They affect how vertical walls, caves, overhangs, destruction, and water are represented. A height-field globe is simpler than a fully volumetric planet but cannot represent arbitrary caves or overhangs. A fully volumetric spherical shell is more expressive and significantly more expensive.

**Recommended first scope:** use a surface terrain height field plus a small number of editable local layers. Do not build a complete planet-scale volumetric voxel shell until the prototype proves the coordinate and interaction model.

## 7. Rendering and LOD implications

A globe is not practical at uniform resolution if the player sees both nearby editable terrain and distant continents. The system should have a hierarchy of representations:

- Low-resolution whole-planet base surface.
- Medium-resolution patches for visible regions.
- High-resolution patches near the player and edited locations.
- Optional far-distance simplified terrain or height representation.
- Separate detail overlays for vegetation, structures, and simulation visualization.

The 0fps voxel LOD article explains that distant voxel terrain can be simplified, but transitions between different LODs need seam handling or continuous morphing to avoid cracks. [A level of detail method for blocky voxels](https://0fps.net/2018/03/03/a-level-of-detail-method-for-blocky-voxels/). Planetary rendering additionally needs patch-edge stitching and precision-aware coordinate calculations. [Comparative Analysis of Procedural Planet Generators](https://arxiv.org/html/2510.24764v1).

For Hexel:
- Keep terrain state authoritative; regenerate meshes from it.
- Ensure edits invalidate affected patches and neighbours, not the entire planet.
- Use stable cell identity across LODs.
- Add seam tests at all six cube-face boundaries if the cubed-sphere option is selected.
- Measure mesh build time, vertex count, draw calls, memory, and frame time before adopting complex meshing.
- Treat LOD as a rendering representation, not as a different simulation truth.

## 8. Implications for finite hydrology and ecological fields

A globe can be a strong foundation for the planned finite-water and ecology systems, but spherical topology alone does not solve simulation.

Every cell or edge may need:
- surface area,
- shared edge length,
- edge orientation or direction,
- elevation/potential,
- a stable neighbour relationship,
- a way to account for distance and flux across the edge.

Water transfer should be defined across shared edges, not by assuming all six axial neighbours have identical physical spacing. Field values such as water depth should be distinguished from conserved water volume; if cell areas differ, equal depths in two cells do not imply equal volumes.

Climate and biome fields can be layered:
1. Global controls such as latitude, axial tilt/season, and broad atmospheric circulation (later scope).
2. Local conditions such as elevation, exposure, geology, moisture, and distance from coasts.
3. Derived suitability for biome/species.
4. Actual stateful water, soil, plant, animal, and pollution inventories.

A cubed-sphere grid needs area/edge metadata and a flux scheme that respects the patch mapping. A geodesic cell graph makes the neighbour graph natural but still requires edge lengths, areas, elevation and a conservation-aware solver. Neither topology automatically produces realistic hydrology.

## 9. Tiny prototype plan

Do not start by generating an Earth-sized world. Build a low-resolution topology proof first.

### Prototype A — Cubed-sphere patches

1. Create six labelled cube faces with a small integer grid on each face.
2. Map grid points to a sphere using one documented projection.
3. Implement a face-edge adjacency table with coordinate transforms.
4. Verify that each cell has the expected neighbours for the selected cell model.
5. Render a wireframe with face IDs and cell IDs.
6. Raycast from outside the sphere and map the hit to a stable cell ID.
7. Move a test pawn with local gravity and a stable tangent frame.
8. Change one cell's height/material and verify adjacent boundaries remain coherent.
9. Save/load that edit and verify identity after reload.
10. Add a tiny scalar field and confirm neighbour propagation crosses all six face seams.

### Prototype B — Geodesic cell graph

1. Generate a low-resolution icosahedron-derived cell partition.
2. Build explicit vertex, edge and cell adjacency data.
3. Verify expected degree: ordinary hex cells have six neighbours; exceptional pentagons have five in the standard Goldberg-like construction.
4. Compute cell areas and shared-edge lengths.
5. Render each cell with a unique ID colour.
6. Raycast to a cell and edit/save/load it.
7. Run a scalar diffusion or water-transfer toy model and check conservation.

### Compare both using the same tests

- Can every cell be reached from every other cell through neighbours?
- Are there duplicate or missing cell IDs?
- Are shared boundaries represented consistently?
- How much does cell area and edge length vary?
- How hard is it to map a ray hit to a cell?
- How easy is it to subdivide around the player?
- How easy is it to update one edit without rebuilding unrelated areas?
- How clear are save IDs and deterministic regeneration?
- How easy is it to express six-neighbour hex gameplay and the exceptional pentagons?
- What are CPU cost, memory use, and rendering cost at equal approximate surface resolution?

The purpose is not to optimize prematurely. It is to learn which topology creates the fewest painful assumptions.

## 10. Correctness tests to require before choosing

### Topology invariants
- Every cell ID is unique and stable.
- Neighbour relationships are reciprocal for undirected adjacency.
- No cell references a nonexistent neighbour.
- The entire surface graph is connected.
- Patch boundaries have no gaps or duplicated cells.
- Degree expectations are documented for the selected topology.

### Geometry invariants
- Mapped points lie on the expected reference surface within tolerance.
- Cell boundaries agree on both sides of a shared edge.
- Surface normals point consistently outward.
- Local frames remain finite and normalized.
- Raycast-to-cell mapping is stable near patch seams and exceptional cells.

### Persistence and determinism
- Same seed and generator version reproduce unmodified terrain.
- Edits retain stable identity across save/load.
- Patch subdivision or LOD changes do not alter authoritative terrain.
- A patch can be unloaded and reloaded without changing state.

### Simulation invariants
- Water transfers conserve volume within defined numerical tolerance.
- Flux across a shared edge is equal and opposite for the two cells.
- Cell area differences are accounted for.
- Results do not change merely because the camera or LOD changed.

## 11. Decision gate

Choose **cubed-sphere patches** if:
- Hierarchical rendering and scalable local patch detail are the dominant early needs.
- Hexels can be local editable cells rather than a globally regular hexagonal tiling.
- We are comfortable with patch-edge transforms and some projection distortion.

Choose a **geodesic hex/pentagon graph** if:
- A hexagonal surface-cell graph is central to gameplay and simulation.
- Variable-degree cells are acceptable and can be handled explicitly.
- We want neighbours, surface fields, pathfinding and hydrology to use the same global graph.

Do not choose latitude/longitude as the main topology solely because it is familiar. Do not force the existing axial coordinates around a globe without a clear, tested mapping.

**Current recommendation:** prototype cubed-sphere patches first, but keep the topology interface broad enough that a geodesic cell graph can be tested behind it. Make the final choice after the same movement, raycast, edit, persistence and simulation tests run against both.

## 12. Sources

1. Amit Patel, [Wraparound hexagon tile maps on a sphere](https://www.redblobgames.com/x/1640-hexagon-tiling-of-sphere/) — explains why a sphere cannot be tiled with only uniform regular hexagons, discusses pentagon exceptions and explores alternative coordinate schemes.
2. Amit Patel, [Hexagonal Grids](https://www.redblobgames.com/grids/hexagons/) — authoritative practical guide to planar axial/cube coordinates, neighbour operations, distances, storage, and grid algorithms. Useful as the baseline for the existing flat prototype, not as a complete spherical topology.
3. M. Lambers and A. Kolb, [Ellipsoidal Cube Maps for Accurate Rendering of Planetary-Scale Terrain Data](https://marlam.de/ecm/) — research and implementation notes for hierarchical planetary terrain, projection quality and numerical accuracy.
4. Manuel Zechmann and Helmut Hlavacs, [Comparative Analysis of Procedural Planet Generators](https://arxiv.org/html/2510.24764v1) (arXiv preprint, October 2025) — procedural planet generation, quadtree LOD, seam stitching, precision, and a small user study. Treat as a preprint, not settled consensus.
5. [A New Coordinate System for Constructing Spherical Grid Systems](https://www.mdpi.com/2076-3417/10/2/655) — geodesic discrete global grid systems and multiresolution spherical partitions.
6. Mikola Lysenko, [A level of detail method for blocky voxels](https://0fps.net/2018/03/03/a-level-of-detail-method-for-blocky-voxels/) — voxel LOD simplification and transitions.
7. [Why are spherical worlds so rare?](https://gamedev.stackexchange.com/questions/164686/why-are-spherical-worlds-so-rare) — practitioner discussion of the coordinate, movement and camera complications of spherical maps; useful context, not a primary research source.
8. [PlanetSmith on Steam](https://store.steampowered.com/app/2539340/PlanetSmith/) — relevant design reference for a spherical, hex-based voxel planet; public descriptions do not establish its private implementation details.

## Conclusion

A finite spherical world is feasible, but it is a topology decision with consequences throughout the engine. The strongest early move is to make topology, stable cell identity, surface mapping, terrain state, rendering patches and simulation fields separate concepts.

**Build a tiny cubed-sphere prototype first and compare it directly with a geodesic hex/pentagon graph.** If the globe is a core part of Hexel's identity, settling this question while the engine is small will be much cheaper than retrofitting it after movement, world generation, saves and ecological simulation all assume an infinite flat axial grid.

This document records a research recommendation only. It does not modify engine code or decide the final topology.
