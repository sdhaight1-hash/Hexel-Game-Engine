# Topology Audit — Phase 4 Validation Plan

**Status:** Implemented and locally validated; follow-up robustness work remains.    
**Scope:** Offline geometry diagnostics only. This phase must not change the playable world's topology, cell coordinates, rendering, or gameplay behavior.

## Objective

Increase confidence in the spherical overlap and coverage diagnostics before using the topology comparison to make design decisions. Phase 3 passes its current regression tests, but the existing detector assumes convex spherical polygons bounded by minor great-circle arcs and has only limited adversarial coverage.

Phase 4 prioritizes independent, falsifiable tests over simply adding more assertions around the same area-sum metric. The first implementation is in `src/bin/topology_geometry_check.rs`: coordinate-based duplicate detection, adversarial pair fixtures, and deterministic Fibonacci-sphere sampling with 4,096 runtime samples. The user ran the focused topology checker (27 passed) and full test suite (81 passed). The frequency-3 runtime audit produced 4,095 singly-owned samples, zero sampled gaps, zero multiple-owner samples, and one excluded boundary sample out of 4,096. These results validate the implemented regression suite and this sampled run, not all possible spherical meshes.

## Workstream A — Adversarial overlap fixtures

Add small, deterministic synthetic meshes to `src/bin/topology_geometry_check.rs`. Prefer fixtures with analytically understandable spherical geometry and explicitly documented assumptions.

Required cases:

1. **Same interior, different vertex IDs:** represent coincident polygon boundaries using separate vertex records at the same coordinates. The detector must not rely only on matching vertex IDs to recognize the duplicate interior.
2. **Partial overlap with no vertex containment:** create two convex spherical polygons whose edge interiors cross and whose vertices are not strictly inside the other polygon. Expect a positive overlap result.
3. **Strict containment:** one polygon lies strictly inside another. Expect overlap even if no boundaries cross.
4. **Shared edge only:** neighboring polygons share one complete boundary arc and have disjoint interiors. Expect no overlap.
5. **Shared vertex only:** polygons touch at one vertex without interior intersection. Expect no overlap.
6. **Disjoint polygons:** expect no overlap.
7. **Near-boundary cases:** points or intersections within the documented numeric tolerance should have deterministic outcomes; document the chosen tolerance and why it is suitable for the fixture scale.

For each fixture, assert the expected pair-level overlap result directly. Avoid tests that pass merely because the whole-mesh report changes for an unrelated reason.

## Workstream B — Independent coverage validation

Design a coverage check independent of the sum of polygon areas. Select one approach and document its failure modes before implementation. Candidate approaches:

- **Spherical sampling:** deterministically sample directions over the sphere and count how many cell interiors contain each sample. Require exactly one owner for all non-boundary samples. Use a deterministic sequence and enough samples to make the check reproducible; clearly state that finite sampling is evidence, not a proof.
- **Topology plus local geometric validation:** validate closed two-manifold edge incidence, opposite directed traversal, vertex-link cycles, and per-cell convexity/consistent hemisphere assumptions. This gives strong structural evidence but does not alone prove there are no geometric overlaps.
- **Independent reference partition:** where practical, compare generated cells against an independently computed source partition or triangulation rather than reusing the current area and overlap routines.

Preferred first implementation: deterministic spherical sampling, paired with the existing combinatorial checks. Make sample count and seed explicit. Do not present finite sampling as a mathematical proof.

Sampling must distinguish:
- exactly one containing cell (covered),
- no containing cell (sampled gap),
- more than one containing cell (sampled overlap),
- ambiguous boundary sample (excluded or separately counted under a documented rule).

## Workstream C — Robustness and invariants

- Confirm malformed cell indices and repeated vertex IDs are safely rejected or excluded from geometric predicates without panics.
- Confirm generated meshes remain deterministic across repeated generation.
- Retain opposite directed traversal for shared edges, closed edge incidence, reciprocal adjacency, and connectivity checks.
- Keep diagnostics from silently accepting non-finite coordinates or invalid radii.
- Ensure any newly reported invalid/ambiguous cases are visible in the audit output.

## Acceptance criteria

Phase 4 is ready for review only when:

- Every adversarial overlap fixture above has a direct, passing assertion.
- Shared boundaries and vertex-only contact do not count as positive-area overlap.
- The independent coverage check reports no sampled gaps or multiple ownership for the generated frequency-3 mesh at the documented sample count.
- Deliberately removing a cell produces sampled gaps.
- Deliberately duplicating or partially overlapping a cell produces sampled multiple ownership or a positive overlap result.
- Existing cube-sphere, topology, and core engine tests still pass.
- Audit output and this plan document state the coverage method, sample count, ambiguity policy, and limitations.
- No production gameplay topology or rendering code is changed.

A failing adversarial test is useful evidence: fix the predicate or narrow/document the supported geometry contract. Do not weaken the fixture or increase tolerances solely to obtain a green test.

## Local validation record

The user ran these commands successfully after pulling the implementation:

- `cargo test --bin topology_geometry_check` — **27 passed, 0 failed**.
- `cargo test` — **81 passed, 0 failed** across the core engine, cube-sphere checker, and topology checker.
- `cargo run --bin topology_geometry_check -- 3` — 92 cells, 180 dual vertices, 270 unique edges, zero boundary/non-manifold edges, zero detected overlapping pairs, and relative area coverage error `0.00000000`.
- Coverage sampling — 4,096 samples; 4,095 covered by exactly one cell, 0 sampled gaps, 0 samples with multiple owners, 1 boundary sample excluded.

## Suggested next work

1. Add the remaining robustness checks from Workstream C: malformed cell indices, repeated vertex IDs, non-finite coordinates/radii, and clear audit reporting for invalid input.
2. Strengthen the sampling boundary classification and document the numeric tolerances used by the overlap and boundary predicates.
3. Run frequency sweeps (at least frequencies 2, 3, and 4) to compare coverage behavior and runtime scaling.
4. Keep the production topology unchanged while evaluating how the 12 pentagons affect the pure-hexagonal-prism requirement.
5. Only after those checks, design separate prism-boundary diagnostics for cracks and mismatched shared edges.

## Local validation commands

Run from the repository root after implementation is committed:

```sh
cargo test --bin cube_sphere_geometry_check
cargo test --bin topology_geometry_check
cargo test
cargo run --bin topology_geometry_check -- 3
```

## Explicit non-goals

- Do not switch the playable world to a cube-sphere or geodesic mesh.
- Do not invent a workaround for the 12 pentagons before deciding the pure-hexagonal-prism requirement.
- Do not treat zero sampled gaps, zero sampled multiple-ownership points, or matching total area as a formal proof of complete coverage.
- Do not begin prism-wall implementation until the candidate cell geometry and topology constraints are agreed upon.
