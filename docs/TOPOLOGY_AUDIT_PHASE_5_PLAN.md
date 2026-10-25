# Topology Audit — Phase 5 Robustness and Frequency Scaling

**Status:** Initial implementation committed; local validation pending.  
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

- The focused topology checker and complete test suite pass.
- Generated frequency 2, 3, and 4 meshes have zero invalid radius, non-finite vertices, zero-length vertices, off-radius vertices, invalid cell indices, and repeated vertex IDs.
- The frequency sweep reports zero sampled gaps and zero multiple ownership for all three generated meshes.
- Deliberately malformed fixtures are counted by the intended diagnostics without panics.
- Existing Phase 4 adversarial overlap and missing-cell/duplicate-cell tests continue to pass.
- No production gameplay topology or rendering code is changed.

## Interpretation limits and next steps

Zero sampled gaps and multiple ownership do not prove full spherical coverage. The overlap detector is still intended for convex spherical polygons bounded by minor great-circle arcs. If frequency 4 exposes false positives, numerical instability, or performance issues, investigate the predicate and sampling cost instead of loosening tolerances to force a pass.

After this validation, the next separate design problem is compatibility between the geodesic dual's 12 pentagons and Hexel's pure-hexagonal-prism gameplay-cell requirement. Do not start prism-wall or production-topology changes until that requirement is explicitly resolved.
