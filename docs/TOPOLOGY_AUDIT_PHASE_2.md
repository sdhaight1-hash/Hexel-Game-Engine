# Topology Audit — Phase 2 Findings, Phase 3 Results, and Phase 4 Plan

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

## Automated checks — latest local validation

The user pulled the Phase 3 commits and ran the complete test suite twice. Both full runs passed:

- Core engine: 46 passed.
- Cube-sphere geometry checker: 8 passed.
- Geodesic dual geometry checker: 17 passed.
- Main binary and doc tests: 0 tests each.
- **Total: 71 passed, 0 failed.**

The frequency-3 runtime audit also completed successfully with these observed results:

| Metric | Observed result |
|---|---:|
| Cells | 92 |
| Dual vertices | 180 |
| Unique edges | 270 |
| Side distribution | 12 pentagons, 80 hexagons |
| Boundary edges | 0 |
| Non-manifold edges | 0 |
| Degenerate cells | 0 |
| Duplicate cell IDs | 0 |
| Maximum radius error | 0.000000000000 |
| Edge length min / mean / max | 21.915211 / 23.197045 / 24.482140 |
| Cell area min / mean / max | 866.726518 / 1365.909849 / 1526.991978 |
| Cell area coefficient of variation | 0.1455626854 |
| Overlapping cell pairs detected | 0 |
| Relative area coverage error | 0.00000000 |

These are the user's observed local outputs, not a claim that every possible malformed mesh is covered by the tests.

## Interpretation

- **Area uniformity:** the geodesic dual has lower measured cell-area variation than the tested cube-sphere mesh at these specific settings. This is a comparison result, not proof that the geodesic partition is the final design.
- **Edge lengths:** the geodesic dual's measured edge lengths are more uniform in this comparison.
- **Cell shape compatibility:** the geodesic dual provides mostly hexagons, with the expected 12 pentagons. The cube-sphere candidate produces quads, which do not directly satisfy a six-sided horizontal-cell requirement.
- **Coverage signals:** the generated frequency-3 mesh has zero detected overlaps and zero relative summed-area error. Together with its closed edge topology, this is useful evidence, but not a formal proof of complete coverage.
- **Gameplay unchanged:** neither experimental mesh is connected to or alters the playable world's current topology.

## Phase 3 — completed and locally validated

Phase 3 added diagnostic-only code to `src/bin/topology_geometry_check.rs`:

- Counts pairs of cells whose interiors appear to overlap, using spherical-polygon containment and great-circle minor-arc intersection checks.
- Reports relative difference between the sum of valid measured cell areas and the full sphere area.
- Tests the generated mesh, a removed-cell case, and a duplicated-cell case.

The user ran `cargo test` twice: **71 passed, 0 failed** on both runs. The command `cargo run --bin topology_geometry_check -- 3` reported zero detected overlapping pairs and relative area error `0.00000000`.

**Important limitations:** the overlap check is designed for convex spherical polygons bounded by minor great-circle arcs. It is a practical diagnostic, not a formal proof for arbitrary, malformed, concave, or hemisphere-spanning polygons. The summed-area metric cannot by itself locate or prove the absence of gaps. Passing the current tests must not be described as a mathematical guarantee of full coverage.

## Phase 4 — next validation design

The next phase is specified in `docs/TOPOLOGY_AUDIT_PHASE_4_PLAN.md`. Its priorities are:

1. Add adversarial overlap fixtures with distinct vertex IDs/sets and crossing boundaries.
2. Prove shared edges, shared vertices, and disjoint neighbors do not create overlap false positives.
3. Add an independent coverage check that does not merely compare summed area with sphere area.
4. Keep all work in offline diagnostics; do not alter gameplay topology.
5. Record the exact test commands and observed output before calling Phase 4 validated.

Phase 4 should not be considered complete until the tests are implemented, run locally, and their limitations documented.

## Remaining design questions

1. Decide how the 12 pentagonal cells of a geodesic dual could be represented while preserving Hexel's pure hexagonal-prism requirement; do not silently substitute arbitrary polyhedra.
2. Test prism wall construction for cracks, overlaps, and inconsistent shared boundaries once a candidate cell representation is selected.
3. Keep deterministic IDs and neighbor reciprocity as explicit invariants when adjacency data is designed.

## Reproducible commands

Run from the repository root:

```sh
cargo test --bin cube_sphere_geometry_check
cargo test --bin topology_geometry_check
cargo test
cargo run --bin topology_geometry_check -- 3
```

Inspect the final audit output for `Overlapping cell pairs detected` and `Relative area coverage error`. Investigate failures rather than changing tolerances merely to make tests pass.

**Decision at this checkpoint:** keep the work diagnostic-only. Do not change playable-world topology until independent coverage validation, adversarial overlap checks, prism-boundary tests, and the pure-hexagonal-prism compatibility question have adequate evidence.
