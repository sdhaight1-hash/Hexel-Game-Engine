# Planetary Topology Integration Roadmap

**Status:** Phase 1 complete; Phase 2 APIs/adapter verified by reported tests; Phase 3 CPU surface-mesh adapter committed, local verification pending  
**Target:** Integrate the geodesic dual-cell topology into Hexel Engine incrementally without regressing existing gameplay.  
**Primary implementation:** `src/topology.rs`  
**Current verification:** Developer-reported `cargo test` after pulling the Phase 2 changes passed **107 tests, 0 failed** (61 library, 8 cube-sphere geometry, 38 topology geometry; main/doc-test targets had no tests).

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

**Status:** Complete — implementation and regression tests are committed; developer verified the latest full test suite.

**Goal:** Let terrain systems consume geodesic surface cells without removing the current terrain path.

### Work

- [x] Audit current planet/terrain APIs and identify the smallest seam where a surface-cell identity and center direction can be introduced (`PlanetGenerator::sample_direction`).
- [x] Define a terrain-sampling interface that accepts a stable surface direction and returns existing terrain attributes without coupling terrain generation to rendering.
- [x] Add `PlanetGenerator::sample_direction`, allowing a `SurfaceCell.center` to be sampled directly without making topology depend on terrain.
- [x] Document height semantics: `PlanetSample.surface_position` is the sampled radial position at `PlanetConfig.radius + elevation`; topology boundaries remain at the base radius, and sampling alone does not change rendered or gameplay height.
- [x] Preserve seed determinism: added a test comparing samples across every frequency-2 topology cell for two generators with the same seed.
- [ ] Keep the existing latitude/longitude and local axial-hex path working during the transition. Do not make geodesic topology the default until integration tests demonstrate parity for required gameplay.
- [x] Add tests for topology-cell direction sampling, finite outputs, invalid-direction fallback, and radius/elevation behavior. Broader pole/date-line regression coverage remains covered by existing planet tests.
- [x] Re-run `cargo test` after the latest determinism regression test; developer reports 103 passed, 0 failed (57 library, 8 cube-sphere geometry, 38 topology geometry; main/doc targets had no tests).

### Exit criteria

- A topology cell can obtain terrain data through a documented adapter.
- Existing planet and gameplay tests pass unchanged unless a deliberate migration is approved.
- New tests show deterministic, finite terrain data for topology cells.
- No renderer or persistence migration is required just to use the terrain adapter.

## Phase 2 — Surface-cell lookup and neighbor traversal

**Status:** In progress — public lookup/neighbor APIs and regression tests committed and locally verified; gameplay/movement adapter remains pending.

**Goal:** Provide a dependable gameplay API for mapping positions/directions to cells and traversing the surface graph.

### Work

- [x] Specify lookup input semantics: direction or planet-centered position, magnitude ignored, zero/non-finite inputs return `None`, and exact score ties choose the lowest cell ID.
- [x] Keep the linear `nearest_cell` as the correctness reference; no acceleration added without profiling evidence.
- [x] Add `neighbor_ids` and `are_neighbors`; invalid IDs return `None`, while valid non-adjacent pairs return `Some(false)`.
- [x] Add tests for reciprocal adjacency, valid IDs, sorted unique neighbors, and whole-graph connectivity.
- [x] Define deterministic exact-tie behavior (lowest cell ID); non-tied near-boundary inputs use the greatest dot-product score.
- [x] Add tests for center lookup with scaled position vectors, symmetric tie resolution, invalid directions/IDs, neighbor queries, and connected traversal across the sphere.
- [x] Add an opt-in `SurfaceNavigator` adapter for planet-centered position lookup, neighbor access, and choosing an adjacent cell from a tangent-plane travel direction.
- [x] Leave the existing flat-world movement and collision path unchanged; it is not yet compatible with spherical coordinates.
- [x] Run the new adapter tests and full `cargo test` after pulling the adapter commits: developer reports 111 passed, 0 failed (65 library, 8 cube-sphere geometry, 38 topology geometry; main/doc-test targets had no failures).
- [x] Run focused lookup/traversal tests as part of the full `cargo test` run after pulling commit [2831ad2](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/2831ad2a031bc47a702ff7b373bf74c73068a6d3); developer reports 107 passed, 0 failed (61 library, 8 cube-sphere geometry, 38 topology geometry).

### Exit criteria

- Gameplay code can find a surface cell from a direction and inspect its neighbors through a public, tested API.
- Boundary behavior is deterministic and documented.
- Lookup correctness is tested against the linear reference implementation.
- Existing movement behavior remains available until a later migration is explicitly validated.

## Phase 3 — Connect rendering and terrain generation to topology cells

**Goal:** Render the same cell partition represented by the topology model and populate those cells with terrain.

### Work

- [ ] Audit current renderer, mesh, chunk, raycast, and world-edit assumptions before selecting an integration boundary.
- [x] Define a CPU-side surface-mesh adapter that consumes topology boundaries and terrain samples; keep renderer-specific types out of the core topology module. Local verification pending.
- [x] Sample boundary corners by direction so adjacent cells derive identical terrain-displaced positions for shared corners; regression test added, local verification pending.
- [x] Triangulate arbitrary polygon side counts with a center fan; regression test covers pentagonal and hexagonal cells. Local verification pending.
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
- Verification: developer ran `cargo test` after pulling commit [22ff075](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/22ff075a754329a71be9924c5f188201f996515d): **103 passed, 0 failed** (57 library, 8 cube-sphere geometry, 38 topology geometry; main/doc targets had no tests). Developer also confirmed `cargo run --bin hexel-engine` launches and prints the gameplay controls. Phase 1 exit criteria are satisfied for the terrain sampling seam; this does not mean topology is connected to live rendering/gameplay.

### Verification update — 2026-10-25

- Developer pulled `main` and ran `cargo test`: **102 passed, 0 failed**. The new `planet::tests::direction_sampling_matches_geographic_sampling` and `planet::tests::direction_sampling_handles_invalid_directions_without_non_finite_output` tests passed.
- Developer then ran `cargo run`, which reported that Cargo could not choose among three binaries. The manifest lists `hexel-engine`, `cube_sphere_geometry_check`, and `topology_geometry_check`; launching the main app should be attempted with `cargo run --bin hexel-engine`. This is a run-command selection issue, not a test failure.

### Runtime launch verification — 2026-10-25

- Developer reports `cargo run --bin hexel-engine` compiled and launched successfully, printing the world seed and gameplay controls.
- This confirms the main binary starts; it does not establish that geodesic topology is wired into the live renderer or gameplay.

### Phase 1 follow-up — explicit height contract and determinism

- Commit: [22ff075](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/22ff075a754329a71be9924c5f188201f996515d).
- Clarified that `sample_direction` ignores input magnitude and returns `surface_position` at the generator's configured radius plus sampled elevation. It does not move topology boundary vertices or alter current renderer/gameplay height behavior.
- Added a regression test asserting same-seed generators produce identical terrain samples for every cell in a frequency-2 topology.
- Verification pending: run `cargo test` after pulling this commit; only then mark Phase 1 complete.

### Phase 1 verification — 2026-10-25

- Developer pulled the latest commit and ran `cargo test`: **103 passed, 0 failed**.
- The new same-seed/same-cell determinism regression test passed.
- Developer previously confirmed `cargo run --bin hexel-engine` launches successfully.
- Phase 1 is closed for the sampler/height-contract scope. Renderer, movement, collision, persistence, and live gameplay integration remain explicitly out of scope and must be handled in later phases.

### Phase 2 — lookup and neighbor API

- Commit: [2831ad2](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/2831ad2a031bc47a702ff7b373bf74c73068a6d3).
- Added `cell_id_for_direction`, `neighbor_ids`, and `are_neighbors` to `SurfaceTopology` without changing the existing renderer or gameplay path.
- Clarified `nearest_cell` input semantics and made exact-score ties choose the lowest cell ID. Invalid direction inputs return `None`; invalid neighbor IDs return `None`.
- Added regression tests for scaled center lookup, tie determinism, sorted/unique neighbor IDs, reciprocal adjacency, invalid IDs, and full graph connectivity.
- Verification: developer pulled `main` and ran `cargo test`; **107 passed, 0 failed** (61 library, 8 cube-sphere geometry, 38 topology geometry; main/doc-test targets had no tests). New lookup, tie-breaking, neighbor validation, reciprocity, uniqueness, and connectivity tests all passed.

### Phase 2 adapter — 2026-10-25

- Commit [827d5f1](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/827d5f129002a2c403e4602882ddb882058467e9) adds `src/surface_navigation.rs` with `SurfaceNavigator` for planet-centered cell lookup, neighbor access, and selecting an adjacent cell aligned with a tangent travel direction.
- Commit [abb43e5](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/abb43e5b41ad661f1cb402f127ee03d4ccb6e586) exports the adapter from `src/lib.rs`.
- The existing flat X/Z movement, collision, raycast, and edit path remains unchanged because it is not yet compatible with spherical coordinates.
- Adapter tests are committed and locally verified: the developer's full `cargo test` run passed 111 tests with 0 failures (65 library, 8 cube-sphere geometry, 38 topology geometry; main/doc-test targets had no failures).

### Implementation notes

- Update this log after each meaningful phase with commit links/SHAs, tests actually run, and decisions that affect later phases.
- Do not mark a phase complete merely because its code compiles; complete it only when its exit criteria are satisfied.

### Phase 2 verification — 2026-10-25

- Developer pulled the Phase 2 commits from `main` and ran the full `cargo test` suite.
- Result: **107 passed, 0 failed**; 61 library tests, 8 cube-sphere geometry tests, and 38 topology geometry tests passed. Main and documentation test targets had no tests.
- Phase 2 remains in progress because gameplay/movement integration and the remaining exit criteria have not been implemented; the lookup/neighbor API itself is verified.

### Phase 2 adapter verification — 2026-10-25

- Developer ran the full `cargo test` suite after pulling the surface-navigation adapter commits.
- Result: **111 passed, 0 failed**: 65 library tests (including 4 `surface_navigation` tests), 8 cube-sphere geometry tests, and 38 topology geometry tests. Main and documentation test targets had no failures.
- The opt-in navigation adapter is verified. Phase 2 should be reviewed against all exit criteria before closure; the existing flat gameplay path has intentionally not been switched to spherical movement.


### Phase 3 first slice — CPU surface mesh adapter — 2026-10-25

- Commit [64c1e0d](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/64c1e0da04a4859c4dcde73c97c869c23c3babd6) adds `src/surface_mesh.rs` and defines a renderer-independent `SurfaceMesh` built from topology cell centers, polygon boundaries, and `PlanetGenerator` samples.
- Commit [f047d17](https://github.com/sdhaight1-hash/Hexel-Game-Engine/commit/f047d17d9bad2f098c9bcc90ea1685afc8b7374a) exports `surface_mesh` from `src/lib.rs`.
- The mesh adapter triangulates each cell as a center fan, supports non-six-sided cells, corrects triangle winding outward, and terrain-displaces boundary corners by direction so shared corners receive identical positions.
- Added tests for expected triangle count/pentagon coverage, finite positions and elevations, outward winding, and shared-corner consistency. These tests are committed but **local verification is pending**; the developer must pull and run the test suite.
- This is a CPU data adapter only. It is not connected to `renderer.rs`, camera-relative rendering, raycasting, or world edits. The existing flat gameplay path remains unchanged.
