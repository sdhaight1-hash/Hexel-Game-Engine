# Topology Comparison — Phase 2: Comparable-Scale Geometry Audit

Status: first comparable-scale run recorded; topology decision remains open.

## Goal

Use the existing standalone checkers to collect a repeatable first comparison at similar surface-cell counts, then identify what additional geometry work is needed before choosing a topology for production Hexels.

This phase is diagnostic only. The cube-sphere checker produces quads; the geodesic checker produces a pentagon/hexagon dual candidate. Similar cell counts do not make those cell shapes or their neighborhood rules equivalent.

## First comparable-scale run

Both checkers currently use a planet radius of 100 and accept the subdivision parameter as the first command-line argument.

From the repository root, run:

```cmd
cargo run --bin cube_sphere_geometry_check -- 4
cargo run --bin topology_geometry_check -- 3
```

Expected cell counts from the current generators are 96 cube-sphere quads and 92 geodesic dual cells. Capture the complete output from both commands. Do not estimate or hand-fill the area statistics.

Also collect a smaller reference pair if useful:

```cmd
cargo run --bin cube_sphere_geometry_check -- 3
cargo run --bin topology_geometry_check -- 2
```

That pair has 54 cube-sphere quads and 42 geodesic cells, so it is a less comparable scale and should be labelled accordingly.

## Record for each run

- Cell, vertex, and unique-edge counts.
- Polygon side-count distribution.
- Boundary-edge, non-manifold-edge, degenerate-cell, and duplicate-ID counts where reported.
- Maximum radius error.
- Minimum, mean, and maximum edge length.
- Minimum, mean, and maximum spherical cell area.
- Cell-area coefficient of variation (CV = population standard deviation / mean).
- Command, parameter, radius, commit SHA, and any warnings/errors.

Use the coefficient of variation to describe relative spread, not as a stand-alone winner score. Compare area and edge metrics together; the cell constructions are not yet equivalent.

## Checks before interpreting numbers

1. Confirm both commands complete successfully and values are finite.
2. Check whether total cell area is close to the theoretical sphere area, (4\pi R^2). The cube-sphere checker has an area-sum test; the geodesic checker should gain an equivalent test in a follow-up change before relying on that invariant equally.
3. Treat edge lengths cautiously: the geodesic checker measures edges in its dual boundary mesh, while the cube-sphere checker measures quad boundary edges. They are both spherical chords, but belong to different cell designs.
4. Area and edge statistics do not prove that polygons do not overlap, have consistent winding, or make compatible prism side walls.

## Exit criteria

This phase is complete only when the raw outputs are recorded in the comparison document and the following open items are explicitly resolved or assigned:

- A consistent total-area sanity check for both candidates.
- Polygon winding and non-overlap validation.
- Adjacency-to-shared-edge consistency.
- Stable, deterministic IDs and neighbor reciprocity.
- A documented plan for cube-sphere's four-neighbor quads versus the engine's six-sided Hexel requirement.
- A prism-wall test demonstrating that adjacent terrain cells can share boundaries without cracks or overlapping faces.

Do not change the playable-world topology during this phase. Do not claim a winner based on the current diagnostics alone.

## Results log

Runs were executed locally at radius 100. Both commands completed successfully.

| Candidate | Parameter | Cells | Vertices | Unique edges | Area CV | Edge min / mean / max | Area min / mean / max |
|---|---:|---:|---:|---:|---:|---|---|
| Cube-sphere quads | Resolution 4 | 96 | 98 | 192 | 0.333649 (33.36%) | 27.477100 / 35.545705 / 45.950584 | 814.555876 / 1308.996939 / 2013.579208 |
| Geodesic dual | Frequency 3 | 92 | 180 | 270 | 0.145563 (14.56%) | 21.915211 / 23.197045 / 24.482140 | 866.726518 / 1365.909849 / 1526.991978 |

Structural checks observed:

- Cube-sphere: all cells are quads; 96 unique cell loops; 0 boundary edges; 0 non-manifold edges; maximum radius error 0.
- Geodesic dual: 12 pentagons and 80 hexagons; 0 boundary edges; 0 non-manifold edges; 0 degenerate cells; 0 duplicate IDs; maximum radius error 0.

The displayed mean areas multiplied by cell count are consistent, within printed precision, with the theoretical sphere area 4πR² ≈ 125,663.706 for radius 100. This is a useful cross-check, not a substitute for explicit area-sum assertions in both checkers.

## Initial interpretation (not a final topology decision)

At these comparable cell counts, the geodesic dual's measured area coefficient of variation is about 56% lower than the cube-sphere quad grid's. Its measured edge-length range is also much narrower in absolute terms, although the edge lengths belong to different polygon constructions and should not be treated as a fully apples-to-apples score.

This supports continuing the geodesic candidate as the leading shape-fit hypothesis for a hex-dominant world, but does not prove it is production-ready. The cube-sphere prototype is still a four-neighbor quad grid, not a six-sided Hexel topology. Neither checker yet establishes polygon non-overlap, consistent winding, adjacency-to-boundary agreement, or gap-free prism side walls.

## Next experiment

Add/confirm equivalent total-area assertions for both generators, then validate polygon winding and shared-edge/adjacency consistency. After that, prototype prism side walls on the geodesic dual—including how the twelve pentagonal cells are represented—before any production topology decision.
