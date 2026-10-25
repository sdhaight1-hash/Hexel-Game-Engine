# Planetary Topology Integration Roadmap

**Status:** Phase 1 in progress; direction-based terrain sampling is implemented and the developer-reported full test suite passes  
**Target:** Integrate the geodesic dual-cell topology into Hexel Engine incrementally without regressing existing gameplay.  
**Primary implementation:** `src/topology.rs`  
**Current verification:** After pulling the direction-sampling change, the developer-reported `cargo test` run passed all 102 tests: 56 library, 8 cube-sphere geometry, and 38 topology geometry tests; main and doc-test targets had zero tests and no failures.

This document is the working implementation roadmap. Update the phase status, decisions, test results, and any deviations as work lands. Prefer small, reviewable commits and keep `main` buildable.

## Guiding principles

1. **Preserve the playable baseline.** Do not replace the current latitude/longitude planet sampling, axial hex gameplay, chunk/world-edit model, or existing renderer in one step.
2. **Integrate behind explicit interfaces.** Add adapters and opt-in paths before changing defaults. Keep old behavior available until the new path has feature parity and tests.
3. **Make geometry authoritative.** Shared topology data should define cell centers, boundaries, adjacency, and lookup; avoid independently regenerating subtly different versions in gameplay and rendering.
4. **Separate topology from terrain.** Topology describes the surface partition. Elevation, biome, soil, geology, and other gameplay properties are sampled or stored per cell on top of it.
5. **Treat IDs as scoped identifiers, not permanent identity.** Generated IDs are deterministic only for the same generator/version/settings. Never assume a numeric cell ID remains stable across topology revisions.
6. **Validate each phase.** Run focused tests first, then `cargo test`; document failures rather than hiding or weakening assertions.

## Phase 0 — Establish and protect the baseline

**Status:** Complete (reported by developer)

- [x] Pull the topology implementation on `main`.
- [x] Run the full `cargo test` suite.
- [x] Confirm 138 tests pass with no failures.
- [ ] Keep this result as the comparison baseline for integration changes.

**Exit criteria:** The current test suite remains green before starting each integration phase. If a later change breaks existing tests, fix the regression or document an explicitly approved behavior change.

## Phase 1 — Integrate topology with planetary terrain

**Status:** In progress — direction-based sampling API and regression tests committed; local full test suite passes. Remaining terrain integration decisions and Phase 1 exit criteria are still open.

**Goal:** Let terrain systems consume geodesic surface cells without removing the current terrain path.

### Work

- [ ] Audit current planet/terrain APIs and identify the smallest seam where a surface-cell identity and center direction can be introduced.
- [x] Define a terrain-sampling interface that accepts a stable surface direction and returns existing terrain attributes without coupling terrain generation to rendering.
- [x] Add `PlanetGenerator::sample_direction`, allowing a `SurfaceCell.center` to be sampled directly without making topology depend on terrain.
- [ ] Decide and document how cell elevation affects the cell's rendered surface and gameplay height; do not silently change the meaning of existing radius/height values.
- [ ] Preserve seed determinism: the same seed and same logical surface location must produce the same terrain sample.
- [ ] Keep the existing latitude/longitude and local axial-hex path working during the transition. Do not make geodesic topology the default until integration tests demonstrate parity for required gameplay.
- [x] Add tests for topology-cell direction sampling, finite outputs, invalid-direction fallback, and radius/elevation behavior. Broader pole/date-line regression coverage remains covered by existing planet tests.
- [x] Run `cargo test`; developer reports 102 passed, 0 failed (56 library, 8 cube-sphere geometry, 38 topology geometry; main/doc targets had no tests). Focused tests for direction sampling also passed within the library suite.

### Exit criteria

- A topology cell can obtain terrain data through a documented adapter.
- Existing planet and gameplay tests pass unchanged unless a deliberate migration is approved.
- New tests show deterministic, finite terrain data for topology cells.
- No renderer or persistence migration is required just to use the terrain adapter.

## Phase 2 — Surface-cell lookup and neighbor traversal

**Goal:** Provide a dependable gameplay API for mapping positions/directions to cells and traversing the surface graph.

### Work

- [ ] Specify lookup input semantics: normalized direction versus world-space position, radius handling, zero/non-finite input handling, and behavior for points near boundaries.
- [ ] Keep the existing linear `nearest_cell` as the correctness reference; add an accelerated lookup only when profiling justifies it.
- [ ] Add APIs for neighbor iteration and direct neighbor queries, with invalid-ID behavior made explicit.
- [ ] Verify adjacency is reciprocal, neighbor IDs are valid, and traversal does not emit duplicates.
- [ ] Define deterministic tie-breaking for directions exactly or nearly on cell boundaries.
- [ ] Add tests for cell-center lookup, boundary cases, invalid directions, neighbor traversal, and connectivity across the sphere.
- [ ] Introduce gameplay/movement integration through an adapter rather than replacing movement or collision in one change.
- [ ] Run focused lookup/traversal tests, then `cargo test`.

### Exit criteria

- Gameplay code can find a surface cell from a direction and inspect its neighbors through a public, tested API.
- Boundary behavior is deterministic and documented.
- Lookup correctness is tested against the linear reference implementation.
- Existing movement behavior remains available until a later migration is explicitly validated.

## Phase 3 — Connect rendering and terrain generation to topology cells

**Goal:** Render the same cell partition represented by the topology model and populate those cells with terrain.

### Work

- [ ] Audit current renderer, mesh, chunk, raycast, and world-edit assumptions before selecting an integration boundary.
- [ ] Define a render-mesh adapter that consumes topology boundaries and cell terrain data; keep rendering types out of the core topology module.
- [ ] Ensure boundary vertices are shared or canonicalized consistently so adjacent cells do not develop cracks due to independent calculations.
- [ ] Define how pentagons, hexagons, and any future cell side counts map to mesh construction; do not assume every surface cell is a hexagon.
- [ ] Preserve winding, outward normals, radius constraints, and shared-edge consistency.
- [ ] Connect raycasting and selection to the same cell geometry or a verified equivalent, avoiding visual/gameplay disagreement.
- [ ] Map edits to a surface-cell identity through a world-state adapter. Keep current world-edit behavior intact until equivalent semantics are tested.
- [ ] Add geometry, adjacency, coverage, visible-boundary, ray-hit, and terrain-sampling tests where the current architecture permits.
- [ ] Run the existing geometry audit binaries/tests and the full `cargo test` suite.

### Exit criteria

- The renderer can build surface geometry from topology cells without assuming a fixed number of sides.
- Selection/raycasting identifies the cells represented by the rendered geometry.
- Geometry validation detects no new gaps, overlaps, winding errors, or inconsistent shared edges.
- Existing gameplay/render paths remain usable until the new path is demonstrably ready to become the default.

## Phase 4 — Persistence and topology versioning safeguards

**Goal:** Prevent saved worlds from silently loading against incompatible topology IDs or generation rules.

### Work

- [ ] Inventory current save/load support and identify which world data is persisted today; do not invent a migration requirement for data that is not currently saved.
- [ ] Define a topology descriptor containing at least a schema/version identifier, generator algorithm version, frequency/resolution, radius or relevant world-scale settings, and world seed where applicable.
- [ ] Distinguish ephemeral runtime cell indices from persistent logical references. Never persist a bare `SurfaceCellId` without enough topology identity to validate it.
- [ ] Choose a persistence strategy for edited cells: stable logical key, direction/coordinate-based key, or explicit migration mapping. Document the trade-offs before committing to a format.
- [ ] Reject incompatible or malformed topology metadata with a clear error; do not silently reinterpret IDs.
- [ ] Add compatibility tests for supported versions, mismatched versions/settings, missing metadata, and malformed data.
- [ ] Add migration code only when an actual prior persisted format needs it, and test old-to-new conversion.
- [ ] Run the full `cargo test` suite and any save/load validation commands.

### Exit criteria

- Persistent topology references are validated against explicit topology metadata.
- Incompatible data fails safely or migrates through a tested path; it is never silently attached to the wrong cell.
- Versioning policy and supported compatibility guarantees are documented.

## Phase 5 — Full verification and rollout decision

**Goal:** Make an evidence-based decision about whether the topology-backed path is ready to become the default.

- [ ] Run `cargo fmt --check` (format first if needed).
- [ ] Run `cargo test`.
- [ ] Run relevant geometry-audit binaries at representative frequencies/resolutions, including the lowest supported and several larger settings.
- [ ] Review determinism, invalid-input handling, cell lookup performance, memory use, and mesh construction cost.
- [ ] Confirm existing gameplay systems remain functional or list the intentional changes and their tests.
- [ ] Document known limitations and the exact status of rendering, movement, raycasting, editing, and persistence.
- [ ] Switch defaults only after the relevant exit criteria pass; otherwise retain the old path and keep integration opt-in.

**Exit criteria:** All required tests and geometry checks pass, performance is understood for target resolutions, and remaining gaps are explicitly recorded.

## Cross-phase acceptance checklist

- [ ] Existing full test suite stays green.
- [ ] New behavior has targeted regression tests.
- [ ] Same seed/settings produce deterministic topology and terrain results.
- [ ] Invalid values return controlled errors or documented safe fallbacks, never panics.
- [ ] Topology, terrain, rendering, gameplay, and persistence remain separate enough to test independently.
- [ ] No stable-save guarantee is implied for raw generated cell IDs.
- [ ] README and architecture decision record reflect implemented behavior, not just planned behavior.

## Working log

### Baseline — topology data layer

- Added `src/topology.rs` with frequency-subdivided icosahedral primal mesh and dual surface cells.
- Exposes cell centers, ordered boundary corners, adjacency, side-count histogram, and nearest-cell lookup.
- The topology module is not yet wired into rendering, movement, terrain prisms, raycasting, or persistence.
- Developer-reported local test result after pulling: **138 passed, 0 failed**.

### Phase 1 — direction-based terrain sampling

- Commit: [5d8ea6d](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/5d8ea6d8618d2c250838e669372c85387f57555e).
- Added `PlanetGenerator::sample_direction`, which accepts a world-space direction or topology cell center and reuses the existing deterministic terrain sampler.
- Zero and non-finite directions safely fall back to the positive X axis. Existing geographic sampling remains unchanged.
- Added regression tests across frequency-2 topology cells for equivalence with geographic sampling, finite terrain output, and radial surface-position consistency, plus invalid-direction fallback.
- Verification status: developer ran `cargo test` after pulling; **102 passed, 0 failed**. The direction-sampling tests passed. Phase 1 remains in progress because elevation-to-rendered/gameplay-height semantics and the other integration exit criteria are not yet resolved.

### Verification update — 2026-10-25

- Developer pulled `main` and ran `cargo test`: **102 passed, 0 failed**. The new `planet::tests::direction_sampling_matches_geographic_sampling` and `planet::tests::direction_sampling_handles_invalid_directions_without_non_finite_output` tests passed.
- Developer then ran `cargo run`, which reported that Cargo could not choose among three binaries. The manifest lists `hexel-engine`, `cube_sphere_geometry_check`, and `topology_geometry_check`; launching the main app should be attempted with `cargo run --bin hexel-engine`. This is a run-command selection issue, not a test failure.

### Runtime launch verification — 2026-10-25

- Developer reports `cargo run --bin hexel-engine` compiled and launched successfully, printing the world seed and gameplay controls.
- This confirms the main binary starts; it does not establish that geodesic topology is wired into the live renderer or gameplay.

### Implementation notes

- Update this log after each meaningful phase with commit links/SHAs, tests actually run, and decisions that affect later phases.
- Do not mark a phase complete merely because its code compiles; complete it only when its exit criteria are satisfied.
