# Topology Audit — Phase 2 Findings and Phase 3 Validation

**Status:** Experimental/offline diagnostics only. These findings do not change the playable world's topology or rendering.

## Scope

The comparison evaluates two candidate spherical surface meshes:

- A cube-sphere surface partition.
- The dual polygon mesh of a subdivided icosahedron (geodesic dual).

The goal is to measure geometry and topology before choosing a surface partition compatible with Hexel Engine's pure hexagonal-prism gameplay cells.

## Recorded geometry results

Measurements below use radius 100 and the settings recorded during the Phase 2 audit.

| Metric | Cube-sphere (resolution 4) | Geodesic dual (frequency 3) |
|---|---:|---:|
| Cells | 96 | 92 |
| Surface vertices | 98 | 180 |
| Unique edges | 192 | 270 |
| Cell side distribution | 96 quads | 12 pentagons, 80 hexagons |
| Area coefficient of variation | 0.333649 | 0.145563 |
| Edge length min / mean / max | 27.477100 / 35.545705 / 45.950584 | 21.915211 / 23.197045 / 24.482140 |
| Cell area min / mean / max | 814.555876 / 1308.996939 / 2013.579208 | 866.726518 / 1365.909849 / 1526.991978 |

Both audited meshes were closed in these runs, with zero boundary edges, zero non-manifold edges, and zero measured radius error.

## Automated checks — latest completed local run

The most recent full local test run completed successfully before Phase 3 coverage/overlap diagnostics were added:

- Core engine: 46 tests passed.
- Cube-sphere geometry checker: 8 tests passed.
- Geodesic dual geometry checker: 14 tests passed.
- **Total: 68 passed, 0 failed.**

The geodesic checker at that checkpoint included expected frequency-2 and frequency-3 counts, pentagon/hexagon distribution, deterministic generation, invalid inputs, degenerate cells, duplicate cell IDs, edge-use counts, shared-edge traversal in opposite directions, reciprocal and connected cell adjacency, total spherical area, and positive finite signed spherical area for generated cells.

The cube-sphere checker included expected mesh counts, quad loops, edge sharing, radius, positive cell areas, and total spherical area.

The 14-test checkpoint was committed and pushed as `31e0f07` (`test: strengthen topology mesh validation`). Phase 3 diagnostics have since been added to the branch, but **their compilation, tests, and runtime audit have not yet been confirmed locally**. Run the commands at the end of this report after pulling.

## Interpretation

- **Area uniformity:** the geodesic dual has lower measured cell-area variation than the tested cube-sphere mesh at these specific settings. This is a comparison result, not proof that the geodesic partition is the final design.
- **Edge lengths:** the geodesic dual's measured edge lengths are more uniform in this comparison.
- **Cell shape compatibility:** the geodesic dual provides mostly hexagons, with the expected 12 pentagons. The cube-sphere candidate produces quads, which do not directly satisfy a six-sided horizontal-cell requirement.
- **Earlier limitation:** positive signed area and total-area agreement alone do not prove global non-overlap or complete geometric coverage.
- **Gameplay unchanged:** neither experimental mesh is connected to or alters the playable world's current topology.

## Phase 3 — coverage and overlap diagnostics added, validation pending

Commit `4cd1c8d` adds diagnostic-only code to `src/bin/topology_geometry_check.rs`:

- Counts pairs of cells whose interiors appear to overlap, using spherical-polygon containment and great-circle minor-arc intersection checks.
- Reports relative difference between the sum of valid measured cell areas and the full sphere area.
- Adds a generated-mesh regression test expecting zero detected overlap pairs and relative area error below 1%.
- Adds a missing-cell regression test expecting boundary edges and a measurable area deficit.
- Adds a duplicate-cell regression test expecting overlap detection and excess summed area.

**Important limitations:** the overlap check is designed for convex spherical polygons bounded by minor great-circle arcs. It is a practical diagnostic, not a formal proof for arbitrary, malformed, concave, or hemisphere-spanning polygons. The summed-area metric is a useful consistency signal but cannot by itself locate or prove the absence of gaps. Passing these tests must not be described as a mathematical guarantee of full coverage. Review the test results and observed metrics before treating this phase as validated.

## Remaining validation work

1. Run and review the new Phase 3 tests and geometry audit output on the local machine.
2. Add adversarial tests for overlapping cells with different vertex sets and for boundary crossings, ensuring ordinary shared edges and shared vertices are not false positives.
3. Strengthen independent coverage validation beyond total-area agreement and missing-cell detection.
4. Decide how the 12 pentagonal cells of a geodesic dual could be represented while preserving Hexel's pure hexagonal-prism requirement; do not silently substitute arbitrary polyhedra.
5. Test prism wall construction for cracks, overlaps, and inconsistent shared boundaries.
6. Keep deterministic IDs and neighbor reciprocity as explicit invariants when adjacency data is designed.

## Reproducible commands

Run from the repository root:

```sh
cargo test --bin cube_sphere_geometry_check
cargo test --bin topology_geometry_check
cargo test
cargo run --bin topology_geometry_check -- 3
```

Inspect the final audit output for `Overlapping cell pairs detected` and `Relative area coverage error`. The intended result for the generated frequency-3 mesh is zero detected overlap pairs and relative area error below 1%; investigate any failure rather than changing tolerances merely to make tests pass.

**Decision at this checkpoint:** keep the work diagnostic-only. Do not change playable-world topology until Phase 3 has been tested locally, the remaining invariants have evidence, and the pure-hexagonal-prism compatibility question is resolved.
