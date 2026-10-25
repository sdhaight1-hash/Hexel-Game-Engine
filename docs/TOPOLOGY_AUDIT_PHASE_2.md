# Topology Audit — Phase 2 Findings and Phases 3–5 Results

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

The user pulled the Phase 4 implementation and ran the focused topology checker and full test suite. All tests passed:

- Core engine: 46 passed.
- Cube-sphere geometry checker: 8 passed.
- Geodesic dual geometry checker: 27 passed.
- Main binary and doc tests: 0 tests each.
- **Total: 81 passed, 0 failed.**

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

The user previously ran `cargo test` twice: **71 passed, 0 failed**. After Phase 4, the focused topology checker passed **27/27 tests** and `cargo test` passed **81/81 tests**. The command `cargo run --bin topology_geometry_check -- 3` reported zero detected overlapping pairs, relative area error `0.00000000`, 4,095 singly-owned samples, zero sampled gaps, zero multiple-owner samples, and one excluded boundary sample.

**Important limitations:** the overlap check is designed for convex spherical polygons bounded by minor great-circle arcs. It is a practical diagnostic, not a formal proof for arbitrary, malformed, concave, or hemisphere-spanning polygons. The summed-area metric cannot by itself locate or prove the absence of gaps. Passing the current tests must not be described as a mathematical guarantee of full coverage.

## Phase 4 — implemented and locally validated

Phase 4 implementation in `src/bin/topology_geometry_check.rs` has been pulled and tested locally. The focused topology checker passed **27/27 tests** and the full suite passed **81/81 tests**. The implementation adds:

1. Coordinate-set duplicate detection for coincident cells with different vertex IDs.
2. Direct synthetic tests for duplicate interiors, crossing/partial overlap, strict containment, shared-edge-only contact, shared-vertex-only contact, disjoint polygons, and a small positive gap.
3. Deterministic Fibonacci-sphere coverage sampling with 4,096 sample directions in the runtime audit.
4. Separate counts for samples owned by exactly one cell, sampled gaps, multiple ownership, and boundary samples excluded from ownership counts.
5. Regression tests for generated coverage, a removed cell, and a duplicated cell.

**Observed coverage result:** 4,095 samples were covered by exactly one cell, zero sampled gaps, zero multiple-owner samples, and one boundary sample excluded from ownership counts. The runtime audit also reported zero detected overlapping cell pairs and zero relative area coverage error. This is a successful finite-sampling check, not proof of complete coverage. The overlap predicates remain scoped to convex spherical polygons bounded by minor great-circle arcs.

## Phase 5 — robustness implementation and local validation complete

Phase 5 extends the offline checker with explicit malformed-input diagnostics in `src/bin/topology_geometry_check.rs`:

- Flags non-finite or non-positive mesh radii.
- Counts non-finite, zero-length, and off-radius vertices.
- Counts invalid cell vertex indices and repeated vertex IDs.
- Prints these diagnostics in the runtime audit instead of relying only on aggregate degenerate-cell counts.
- Uses a documented scale-aware radius tolerance: max(`1e-10`, abs(radius) × `1e-8`).
- Adds regression tests for invalid radii, non-finite coordinates, zero-length/off-radius vertices, invalid indices, repeated IDs, tolerance scaling, and a frequency 2–4 sampling sweep (1,024 deterministic samples per frequency).

**User-reported local validation:** `cargo test --bin topology_geometry_check` passed **32/32**; `cargo test --bin cube_sphere_geometry_check` passed **8/8**; and `cargo test` passed **86/86** tests (46 core, 8 cube-sphere, 32 topology; main and doc-test targets each contained zero tests). No failures were reported.

The user also ran `cargo run --bin topology_geometry_check -- 2`, `-- 3`, and `-- 4`. Each runtime audit reported zero boundary edges, non-manifold edges, degenerate cells, duplicate cell IDs, invalid radius, non-finite vertices, zero-length vertices, off-radius vertices, invalid cell indices, repeated vertex IDs, detected overlapping cell pairs, and relative area coverage error. Each reported zero sampled gaps and zero multiple-owner samples across 4,096 coverage samples. Frequency 3 excluded one boundary sample and counted 4,095 samples with exactly one owner; frequencies 2 and 4 counted all 4,096 as singly owned. Maximum radius error was zero at all three frequencies.

These are user-reported local results, not a claim that every malformed mesh is covered or that finite sampling proves complete coverage. The overlap detector remains scoped to convex spherical polygons bounded by minor great-circle arcs.

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

## Phase 6 — audit hardening implemented and locally validated

Phase 6 hardened `src/bin/topology_geometry_check.rs` based on the audit review. This remains offline diagnostic tooling; gameplay topology and rendering are unchanged.

- **Containment tolerance:** edge-plane normals are normalized before signed-side tests. The predicate compares signed sides using an explicit spherical tolerance, avoiding the previous edge-length-dependent product threshold. Non-finite and zero-length query directions are rejected.
- **Duplicate-coordinate comparison:** duplicate polygon coordinate sets are matched one-to-one; a coordinate in one polygon cannot be reused to satisfy multiple vertices in the other.
- **Runtime orientation:** the audit reports shared edges used twice with the same directed traversal.
- **Runtime vertex neighborhoods:** the audit constructs each vertex's link graph and reports vertices whose link is not one connected cycle. This complements, rather than replaces, edge-incidence checks.
- **Adversarial regressions:** tests cover tiny spherical polygons, one-to-one coordinate matching, malformed non-finite coordinates, same-direction shared edges, disconnected vertex links, and valid generated meshes at frequencies 2–4.
- The runtime audit prints `Inconsistent edge orientations` and `Invalid vertex links`.

**User-reported local test results after Phase 6:**

- `cargo test --bin topology_geometry_check`: **38 passed, 0 failed**.
- `cargo test --bin cube_sphere_geometry_check`: **8 passed, 0 failed**.
- `cargo test`: **92 passed, 0 failed** (46 core, 8 cube-sphere, 38 topology; main and doc-test targets had zero tests).

**User-reported runtime audit results** at radius 100:

| Metric | Frequency 2 | Frequency 3 | Frequency 4 |
|---|---:|---:|---:|
| Cells | 42 | 92 | 162 |
| Dual vertices | 80 | 180 | 320 |
| Unique edges | 120 | 270 | 480 |
| Boundary edges | 0 | 0 | 0 |
| Non-manifold edges | 0 | 0 | 0 |
| Degenerate cells | 0 | 0 | 0 |
| Invalid radius | false | false | false |
| Non-finite / zero-length / off-radius vertices | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| Invalid indices / repeated vertex IDs | 0 / 0 | 0 / 0 | 0 / 0 |
| Inconsistent edge orientations | 0 | 0 | 0 |
| Invalid vertex links | 0 | 0 | 0 |
| Maximum radius error | 0.000000000000 | 0.000000000000 | 0.000000000000 |
| Detected overlapping cell pairs | 0 | 0 | 0 |
| Relative area coverage error | 0.00000000 | 0.00000000 | 0.00000000 |

The pasted frequency-2 coverage report showed 4,096 singly-owned samples, zero sampled gaps, zero multiple-owner samples, and zero excluded boundary samples. Frequency 3 showed 4,095 singly-owned samples, zero sampled gaps, zero multiple-owner samples, and one excluded boundary sample. The shared frequency-4 output excerpt ended at the area-coverage line, so its 4,096-sample ownership summary was not visible in the message; the other frequency-4 topology and area metrics above were visible. The frequency-4 runtime audit completed through the reported area-coverage result.

These results are strong evidence for the generated meshes under the implemented checks, not a formal proof of complete coverage or a guarantee for every malformed input. The containment and vertex-link checks assume valid convex spherical cells bounded by minor great-circle arcs. Finite coverage sampling remains a diagnostic.
