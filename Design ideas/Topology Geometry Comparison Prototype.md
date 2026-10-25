# Topology Geometry Comparison Prototype

Status: exploratory design prototype; not a topology decision.

## Goal

Compare the cube-sphere and geodesic candidates against Hexel Engine's actual terrain unit: a hexagonal prism (“hexel”) on a finite spherical planet. The current prototypes establish adjacency graphs, but neither yet proves that its cells form a watertight, non-overlapping prism surface.

## Important distinction

- The cube-sphere prototype currently exposes six face grids and four neighbors per cell. That is useful for testing face indexing and seams, but it is not yet a six-sided hexel neighborhood.
- The geodesic prototype is a graph of directions derived from a subdivided icosahedron. Its degree-five and degree-six nodes are hex-dominant connectivity, not a constructed dual polygon mesh. The nodes do not themselves define hexagonal cell boundaries.

A fair comparison must not treat either graph as completed terrain geometry.

## Prototype stages

### 1. Shared input and invariants

Use the same planet radius, resolution target, and numeric tolerances for both candidates. Each candidate must expose stable cell IDs, a unit direction for each cell center, and a list of adjacent cells. Geometry generation must be deterministic for the same input.

### 2. Construct actual cell boundaries

For each candidate, generate a polygon boundary for each surface cell. For the geodesic graph, construct the dual of the triangulated icosphere rather than drawing hexagons directly around graph vertices. For the cube-sphere, define a deliberate cell-boundary construction and document how its four-neighbor grid would be reconciled with six-sided terrain.

Project boundary vertices onto the configured sphere or use a documented tangent-plane approximation only for local diagnostics. Do not silently mix the two models.

### 3. Measure geometry

Record these measurements for each candidate:

- Number of cells and distribution of polygon side counts.
- Edge lengths: minimum, maximum, mean, and coefficient of variation.
- Cell areas: minimum, maximum, mean, and coefficient of variation.
- Maximum deviation of boundary vertices from the configured sphere radius.
- Count of edges not shared by exactly two cells, duplicate edges, and degenerate polygons.
- Neighbor reciprocity and whether each shared polygon edge agrees with the corresponding adjacency link.
- Seam/pole continuity, including no cracks or duplicate cells at patch boundaries.

### 4. Hexel-prism compatibility

Test the boundary polygons as the top faces of terrain prisms, with side faces derived from shared boundary edges. Verify that adjacent prisms agree on shared edge endpoints and that height changes do not create gaps or overlapping side faces. Keep this as a geometry test harness; do not connect it to gameplay or rendering yet.

## Acceptance criteria

A candidate is not ready to replace the current world until:

1. Cell IDs and adjacency are deterministic and stable for a given resolution.
2. All intended surface edges are shared consistently, with no unexplained boundary edges or duplicate faces.
3. Polygon boundaries match adjacency, including across cube-face seams or around geodesic pentagons.
4. Geometry is finite and non-degenerate at poles, seams, and exceptional cells.
5. Cell-size variation and generation cost are measured at equivalent approximate cell counts.
6. The result can produce closed hexel/prism side walls without cracks between neighboring cells.

## Expected trade-offs

- Cube-sphere: potentially convenient face-grid indexing and chunking, but the current four-neighbor grid does not directly satisfy hexel adjacency. A conversion to hex-dominant cells would be a separate design, not a cosmetic adjustment.
- Geodesic: naturally supports a mostly-hexagonal dual mesh with twelve pentagons, but requires explicit dual-polygon construction, careful edge matching, and a policy for pentagon-shaped cells.

## Decision rule

Do not select a winner from adjacency tests alone. First produce boundary polygons and shared-edge validation, then compare geometry metrics and chunk/indexing complexity. Keep the playable terrain on its current implementation until a candidate passes those tests and a rendering integration experiment.
