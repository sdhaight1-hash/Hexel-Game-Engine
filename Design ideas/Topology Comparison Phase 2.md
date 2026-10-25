# Topology Comparison — Phase 2: Comparable-Scale Geometry Audit

Status: setup plan; results must be collected from actual checker runs before any topology decision.

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

Pending actual command output. Paste the full output from the first comparable-scale run here after running both commands. Record observed numbers only; do not infer missing values.

| Candidate | Parameter | Cells | Area CV | Edge min / mean / max | Area min / mean / max | Structural issues |
|---|---:|---:|---:|---|---|---|
| Cube-sphere | 4 | 96 expected | Pending | Pending | Pending | Pending |
| Geodesic dual | 3 | 92 expected | Pending | Pending | Pending | Pending |
