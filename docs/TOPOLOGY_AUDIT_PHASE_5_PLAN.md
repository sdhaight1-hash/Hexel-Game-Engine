# Topology Audit — Phase 5 Robustness and Frequency Scaling

**Status:** Phase 5 validated by the user; Phase 6 audit hardening implemented, local validation pending.  
**Scope:** Offline diagnostic tooling only. Do not change gameplay topology, rendering, or production world generation.

## Goals

1. Make malformed mesh inputs visible and measurable rather than relying on aggregate geometry metrics.
2. Validate tolerance behavior across coordinate scales.
3. Exercise generated geodesic dual meshes at frequencies 2, 3, and 4.
4. Preserve all Phase 4 overlap and sampled coverage regressions.

## Implemented diagnostics

The Phase 5 changes in `src/bin/topology_geometry_check.rs` add explicit counts for:

- Invalid mesh radius (non-finite or non-positive).
- Non-finite vertex coordinates.
- Zero-length vertices.
- Vertices outside the requested radius tolerance.
- Cell references to missing vertex indices.
- Repeated vertex IDs within a cell.

The radius check uses a scale-aware tolerance:

`max(1e-10, abs(radius) * 1e-8)`

This is a diagnostic tolerance, not a universal geometric guarantee. A cell with invalid indices or repeated vertex IDs is counted as degenerate by the existing audit. New diagnostics should be reviewed together with the existing geometry metrics.

Regression tests cover invalid radius and non-finite coordinates, zero-length and off-radius vertices, invalid indices and repeated IDs, tolerance scaling, and deterministic coverage sampling at frequencies 2, 3, and 4 using 1,024 samples per frequency.

## Phase 6 audit-hardening changes

The current branch adds these review-driven checks to `src/bin/topology_geometry_check.rs`:

- Unit-normalized edge-plane signed-side tests with an explicit spherical side tolerance.
- Input guards for non-finite and zero-length point/cell coordinates.
- One-to-one coordinate-set matching for duplicate polygon detection.
- Runtime counts for same-direction traversals of edges used by exactly two cells.
- Runtime vertex-link validation: each referenced mesh vertex should have a nonempty, connected link in which every link node has degree two (one cycle).
- Regression tests for tiny cells, one-to-one coordinate matching, malformed coordinates, inconsistent orientation, disconnected vertex links, and generated frequencies 2–4.

The new runtime metrics are `Inconsistent edge orientations` and `Invalid vertex links`. A clean generated mesh should report zero for both. The checks intentionally do not claim to certify arbitrary concave or hemisphere-spanning polygons.

**Phase 6 local validation is pending.** The code has been pushed, but no post-change Rust test execution is claimed here.

## Local validation commands

Run from the repository root after pulling the branch:

```bat
git pull origin feat/topology-comparison-phase-2
cargo test --bin topology_geometry_check
cargo test --bin cube_sphere_geometry_check
cargo test
cargo run --bin topology_geometry_check -- 2
cargo run --bin topology_geometry_check -- 3
cargo run --bin topology_geometry_check -- 4
```

The runtime audit now prints the additional validation counts. For generated frequencies 2–4, inspect boundary and non-manifold edges, degenerate cells, duplicate IDs, invalid input counts, overlap count, area error, and the 4,096-point coverage report.

The focused test `frequency_sweep_has_no_sampled_gaps_or_multiple_ownership` checks 1,024 deterministic samples at each frequency 2, 3, and 4. Runtime runs use 4,096 samples. These sample counts are reproducible but finite.

## Acceptance criteria

- The focused topology checker and complete test suite pass after Phase 6.
- Generated frequency 2, 3, and 4 meshes have zero invalid radius, non-finite vertices, zero-length vertices, off-radius vertices, invalid cell indices, and repeated vertex IDs.
- Generated frequency 2, 3, and 4 meshes report zero inconsistent edge orientations and zero invalid vertex links.
- The frequency sweep reports zero sampled gaps and zero multiple ownership for all three generated meshes.
- Tiny-cell containment, one-to-one coordinate matching, malformed-coordinate handling, inconsistent orientation, and disconnected vertex-link fixtures behave as expected.
- Deliberately malformed fixtures are diagnosed without panics.
- Existing adversarial overlap and missing-cell/duplicate-cell tests continue to pass.
- Runtime audits at frequencies 2, 3, and 4 complete, and the new orientation/link counts are reviewed in their output.
- No production gameplay topology or rendering code is changed.

## Recorded local validation results

The user ran the following commands successfully on the branch:

- `cargo test --bin topology_geometry_check`: **32 passed, 0 failed**.
- `cargo test --bin cube_sphere_geometry_check`: **8 passed, 0 failed**.
- `cargo test`: **86 passed, 0 failed** across the core library (46), cube-sphere checker (8), and topology checker (32); main and doc-test targets contained zero tests.
- Runtime audits at frequencies 2, 3, and 4 all completed successfully.

Runtime results (radius 100; 4,096 coverage samples per run):

| Metric | Frequency 2 | Frequency 3 | Frequency 4 |
|---|---:|---:|---:|
| Cells | 42 | 92 | 162 |
| Dual vertices | 80 | 180 | 320 |
| Unique edges | 120 | 270 | 480 |
| Boundary edges | 0 | 0 | 0 |
| Non-manifold edges | 0 | 0 | 0 |
| Degenerate cells | 0 | 0 | 0 |
| Invalid radius / non-finite vertices | 0 / 0 | 0 / 0 | 0 / 0 |
| Zero-length / off-radius vertices | 0 / 0 | 0 / 0 | 0 / 0 |
| Invalid cell indices / repeated vertex IDs | 0 / 0 | 0 / 0 | 0 / 0 |
| Detected overlapping cell pairs | 0 | 0 | 0 |
| Relative area coverage error | 0.00000000 | 0.00000000 | 0.00000000 |
| Samples covered by exactly one cell | 4,096 | 4,095 | 4,096 |
| Sampled gaps / multiple owners | 0 / 0 | 0 / 0 | 0 / 0 |
| Boundary samples excluded | 0 | 1 | 0 |
| Maximum radius error | 0.000000000000 | 0.000000000000 | 0.000000000000 |

The frequency-3 run excluded one sample on a cell boundary under the audit's ownership rules; it still found zero sampled gaps and zero multiple ownership. These are finite-sampling diagnostics, not a formal proof of complete coverage. No gameplay topology or rendering code was changed.

## Interpretation limits and next steps

Zero sampled gaps and multiple ownership do not prove full spherical coverage. The overlap detector is still intended for convex spherical polygons bounded by minor great-circle arcs. If frequency 4 exposes false positives, numerical instability, or performance issues, investigate the predicate and sampling cost instead of loosening tolerances to force a pass.

After this validation, the next separate design problem is compatibility between the geodesic dual's 12 pentagons and Hexel's pure-hexagonal-prism gameplay-cell requirement. Do not start prism-wall or production-topology changes until that requirement is explicitly resolved.
