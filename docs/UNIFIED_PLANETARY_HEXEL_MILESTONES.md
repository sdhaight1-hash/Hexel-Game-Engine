# Unified Planetary Hexel Milestones

## Goal

Deliver one playable, Earth-scale spherical Hexel world. The fine Hexel crust is the gameplay terrain; it is not a second mode or a small overlay on a separate procedural planet. The immutable spherical reference shell is not rendered. Its permanent base is unbreakable bedrock Hexels.

The world uses human-scale editable Hexels (1 m horizontal scale, 0.6 m layer height), 16×16×16 chunks, stable region/hyper-region grouping, and render-distance-driven residency. LOD is a representation of the same world data, not a parallel world.

## Milestone 1 — Visible chunk-backed crust

- Make the near-field visible surface come from resident chunk occupancy and materials.
- Remove dependence on a separately generated procedural surface for gameplay-visible ground in the near field.
- Preserve correct six-sided Hexel faces, exposed sides, vertical layers, and camera-relative rendering.
- Provide a clear in-game visual result that confirms the chunk-backed crust is the surface the player sees.
- Do not remove the distant fallback until a same-data LOD replacement exists.

**Acceptance:** near-field terrain shape and material are derived from chunk data; air cells do not render; edits to chunk data visibly rebuild the affected surface.

## Milestone 2 — Unified editing and persistence

- Route mining and block placement into the chunk-backed world store.
- Ensure the targeted fine Hexel is the one edited, rather than only editing the legacy coarse topology cell.
- Rebuild affected chunk surfaces and neighbors after edits.
- Preserve edits across chunk eviction, reload, save, and load.
- Enforce the unbreakable bedrock rule and build-height limits.

**Acceptance:** mining removes the intended Hexel; placing restores/adds a Hexel; the change remains after leaving and returning, and after save/load.

## Milestone 3 — Collision and movement use the same terrain

- Ground the player against the chunk-backed Hexel surface.
- Align ray targeting, mining/building, movement collision, and visible prism geometry.
- Handle exposed walls, steps, jumps, and falling without relying on the old procedural height sample as gameplay authority.
- Keep player scale at approximately 1.8 m against 0.6 m vertical Hexel layers.

**Acceptance:** the player stands on, walks over, jumps onto, and falls beside the same Hexels shown and edited in-game.

## Milestone 4 — Stable globe-wide addressing

- Replace player-anchor-relative chunk identity with stable spherical/geodesic addressing.
- Define deterministic mapping from globe position to fine Hexel/chunk coordinates across face/region boundaries.
- Keep chunk, region, and hyper-region identities stable as the player travels and the render window moves.
- Retain a dynamic active render region controlled by render distance, separate from persistent logical region grouping.

**Acceptance:** crossing topology/region boundaries does not duplicate, reset, or misplace terrain; returning to a location resolves to the same Hexels and edits.

## Milestone 5 — Same-data distant LOD

- Generate distant terrain LOD from the same chunk-backed world and its deterministic generated base, including saved edits.
- Replace the old independent procedural planet mesh as the distant terrain authority only when a matching LOD path is ready.
- Keep transitions between detailed Hexels and distant LOD stable and visually continuous.
- Keep the unrendered immutable reference shell out of the render path.

**Acceptance:** terrain remains continuous around the globe; distant terrain represents the same world and reflects persisted edits at the supported LOD/update policy; no F3 toggle or separate flat/globe mode exists.

## Engineering constraints

- No new crates unless explicitly approved.
- Do not run Windows release builds. Validate locally with `cargo run` and relevant tests.
- Report build/runtime verification honestly; a Git commit is not evidence that code compiles.
- Prefer end-to-end playable increments over isolated architecture-only changes.

## Current known gaps

- The legacy procedural global surface is still rendered.
- The near-field chunk-backed surface is still a limited local window and is not yet the sole terrain authority.
- Existing mining/building APIs still primarily target the legacy `PlanetVoxelVolume` / `PlanetaryWorld` store.
- Player grounding and ray targeting are not yet fully based on the fine chunk-backed Hexel geometry.
- Chunk keys still include a local anchor cell, so they are not yet stable globe-wide addresses.
- Distant LOD is not yet derived from the same editable chunk world.
