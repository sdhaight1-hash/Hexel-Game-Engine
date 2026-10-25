# Hexel Engine Architecture Decision

**Status:** Proposed; decisions marked open or provisional require experiments before implementation  
**Date:** 2026-10-24  
**Scope:** World topology, terrain and entity ownership, content identity, derived data, simulation, and implementation order  
**Basis:** Synthesis of the ten exploratory case studies in this directory.

## Purpose

Record a coherent direction for Hexel Engine without treating exploratory research as a final implementation specification. Preserve the working game and validate high-impact choices with small, repeatable experiments before large refactors.

**Latest topology research:** [Planetary Topology Comparative Research](Planetary%20Topology%20Comparative%20Research.md) compares the current audit with PlanetSmith's public devlogs, several open-source planetary voxel experiments, and spherical-grid references. It reinforces that the topology decision remains open: PlanetSmith is a gameplay precedent, not a verified drop-in implementation, and the exact handling of its twelve pentagons is not established by the public sources reviewed.

## Architectural principles

1. **Authoritative world state:** Terrain, persistent entity state, and stored physical quantities are the source of truth. Meshes, lighting caches, surface summaries, debug views, and acceleration structures are derived and rebuildable.
2. **Topology owns spatial relationships:** Gameplay and simulation ask a topology interface for valid locations, neighbours, geometry, and distances. They should not assume planar axial arithmetic defines the eventual globe.
3. **Definitions differ from instances:** Content definitions describe types; world cells and entities hold instance state. Stable namespaced content IDs are distinct from compact runtime IDs.
4. **Terrain and dynamic objects use different storage:** Use compact terrain storage for ordinary cells and ECS entities for independently behaving or persistent objects such as plants, animals, dropped items, and machines.
5. **Mutations have explicit consequences:** Controlled edit APIs mark affected data dirty for rendering, collision, lighting, summaries, persistence, and simulation as appropriate. Scope and batch work where safe.
6. **Events communicate changes; state defines truth:** Notifications do not replace authoritative state. Avoid routing every operation through a global event bus.
7. **Simulation accounts for quantities:** Water transfers account for storage, sources, sinks, and boundaries. Derived fields such as moisture must not silently create physical quantities.
8. **Determinism and observability matter:** Seeded generation, stable identities, debug overlays, inspectable state, reproducible tests, and profiling should be supported early.
9. **Complexity must earn its place:** Benchmark adaptive storage, meshing optimizations, generalized subscriptions, parallel simulation, and LOD against a correct baseline.

## Decisions and current direction

### 1. Surface topology — GEODESIC IMPLEMENTATION STARTED; GAMEPLAY INTEGRATION IN PROGRESS

**Starting implementation:** Geodesic/Goldberg-style cell graph generated as the dual of a frequency-subdivided icosahedron. Each primal vertex becomes a surface cell, primal edges define reciprocal cell adjacency, and primal triangles define ordered dual-polygon boundary corners. The expected result is twelve pentagonal cells and hexagons elsewhere.

The first data-layer implementation is in `src/topology.rs`. It provides deterministic IDs for a fixed frequency/generator version, cell centers and boundary corners on a requested radius, neighbor lists, topology counts, and correctness tests. It is not yet connected to gameplay terrain, prism meshing, raycasting, movement, or save/load.

**Implementation direction:** Continue with this geodesic graph and solve concrete integration issues as they appear. Keep the cubed-sphere approach as a fallback only if the geodesic topology proves incompatible with the pure hexagonal-prism gameplay requirement or creates a demonstrated correctness/performance blocker. Do not pause implementation for another broad comparison exercise.

### 2. Geometry and vertical terrain representation — OPEN

Define what a hexel means geometrically: do hexagonal prisms directly tile a surface, or does a separate spherical topology define surface locations? Define how vertical layers or terrain columns attach to those locations.

Document cell shape, face/edge relationships, neighbour rules, surface mapping, and underground representation. Do not assume an ordinary planar hex grid can be mapped to a sphere without exceptional cases or distortion.

### 3. Stable world identity — PROVISIONAL

Separate saved identities from memory layout and rendering coordinates. A cell identity must remain interpretable across save/load and must not depend on a transient mesh or runtime array position. The exact encoding depends on the topology decision.

Keep topology-specific IDs behind an interface so alternatives can be tested without leaking coordinate systems into every subsystem.

### 4. Terrain storage and content definitions — PROVISIONAL

- Store ordinary terrain cells using compact block/material IDs rather than one ECS entity per cell.
- Keep content definitions separate from world instances.
- Use durable namespaced identifiers such as **hexel:basalt**, resolving them to compact runtime IDs when content is loaded.
- Treat JSON as a validated data-definition and configuration format, not an unrestricted substitute for registered engine behavior.
- Keep world generation separate from runtime storage and edited world state.

Palette-based or adaptive storage is a benchmark question, not a prerequisite. Choose the simplest correct representation that meets measured memory and performance targets.

### 5. ECS and dynamic objects — PROVISIONAL

Use ECS for independently behaving objects that need lifecycle, components, or persistent state—for example, individual plants, animals, dropped items, and machines. Do not represent every ordinary terrain cell as an entity.

Keep species/type definitions separate from per-instance state. Generated plant meshes and render instances are derived; the plant's growth and persistence state is authoritative.

### 6. Derived data and invalidation — ACCEPT AS A DESIGN REQUIREMENT

Keep terrain, entity state, and persistent simulation quantities separate from derived meshes, lighting, surface summaries, collision acceleration structures, and visualizations.

All mutations should go through controlled APIs. A mutation should identify affected regions and dependent data, coalesce duplicate dirty work, and avoid rebuilding unrelated regions. Saving, meshing, lighting, collision, surface summaries, and simulation may need separate dirty reasons.

### 7. Change notifications and reactive systems — INCREMENTAL

First formalize existing terrain invalidation and mutation behavior. Add a small typed change-notification mechanism only where a concrete consumer needs it. Later, evaluate spatial subscriptions and dependency tracking for dynamic entities and derived environmental fields.

Do not build a universal pub/sub framework before real use cases and profiling justify it. Define which updates are immediate and which can be batched into simulation phases.

### 8. Environmental fields and finite hydrology — INCREMENTAL

Begin with a small, inspectable set of fields such as elevation, temperature, permeability, and soil moisture. Clearly distinguish stored quantities from derived values.

For the first hydrology experiment, define finite water storage, transfer rules, boundary behavior, sources/sinks, and accounting. Test conservation and deterministic results on a small topology before scaling up. Document cell areas and shared-edge measures when relevant to the selected topology.

### 9. Ecology and persistent flora — AFTER THE SPATIAL/DATA FOUNDATION

Start with one plant species and a small set of meaningful growth inputs, such as temperature, moisture, soil, and available space. Persist individual plant state; use environmental fields to determine suitability and growth. Add inspection output explaining why a plant can or cannot grow.

Expand to competition, succession, erosion feedbacks, and fauna only after the first connected cause-and-effect chain is reliable and affordable.

### 10. Rendering, meshing, and LOD — PRESERVE BASELINE; MEASURE

Keep the current renderer as a correctness and performance baseline. Meshes remain derived from authoritative terrain. Benchmark representative fixtures before adopting a new mesher or LOD strategy.

Do not assume binary greedy meshing for axis-aligned voxels transfers directly to hexagonal-prism faces or spherical boundaries. Test face correctness, seams, edit latency, build time, vertex/index counts, and memory use. Introduce planetary LOD when topology and performance requirements demonstrate the need.

## Implementation sequence

1. **Planetary topology integration:** Connect the geodesic surface graph to planet-relative cell addressing and terrain columns; add surface lookup/raycast tests and resolve pentagon-specific prism geometry explicitly.
2. **World-state contracts:** Document stable IDs, terrain/entity ownership, controlled mutations, save/load, and invalidation.
3. **Correctness baseline:** Add repeatable tests and debug views for neighbours, seams, ray selection, edits, and persistence.
4. **Finite-water prototype:** Add explicit water storage and transfers; test conservation and determinism.
5. **Persistent flora prototype:** Add one species driven by a small set of inspectable environmental inputs.
6. **Performance experiments:** Benchmark meshing, storage, invalidation, field updates, and later LOD; adopt complexity only when measured.

This is a recommended dependency order, not a claim that these systems already exist.

## Minimum acceptance criteria

Before treating the planetary topology as gameplay-ready:
- Every valid cell has a stable identity within the documented save format.
- Neighbour relations are reciprocal where required by the chosen cell model.
- Face/edge seams have no gaps or duplicate/missing adjacency.
- Surface positions and normals are continuous within a defined tolerance.
- Ray selection identifies the expected cell.
- Edited terrain survives save/load.
- A deterministic field or water test behaves consistently across seams.

Before expanding simulation:
- Stored quantities have explicit sources, sinks, and boundary rules.
- Conservation or error tolerances are tested.
- Updates are reproducible for a fixed seed and input state.
- Debug inspection can explain key outcomes.
- Local changes do not trigger unnecessary whole-world work without a documented reason.

Before adopting an optimization:
- A repeatable baseline and representative test data exist.
- Correctness is tested at chunk/patch boundaries and after edits.
- Performance and memory effects are measured.
- The optimization has a clear fallback or rebuild path where appropriate.

## Risks and safeguards

- **Premature topology commitment:** can force expensive changes to movement, saving, raycasting, and simulation. **Safeguard:** compare small prototypes before migrating the current world.
- **Over-generalized architecture:** can create abstractions without validated consumers. **Safeguard:** implement the smallest interface needed for the next experiment.
- **Derived-state drift:** caches can disagree with terrain after edits. **Safeguard:** controlled mutation APIs, explicit invalidation, and rebuildability.
- **Simulation inconsistency:** fields may imply water or growth without accounting. **Safeguard:** distinguish state from derived values and test conservation.
- **Optimization mismatch:** algorithms may assume different cell geometry. **Safeguard:** benchmark with Hexel-specific geometry and boundary tests.

## Decisions deliberately not made here

- Final spherical topology and projection.
- Exact hexel geometry and underground representation.
- Final chunk/section dimensions and palette encoding.
- Exact field dependency graph and simulation tick schedule.
- Final meshing algorithm, LOD representation, and planetary scale targets.
- Whether or when to add a general subscription framework.

These require experiments, current-code inspection, or explicit project goals; the research alone does not settle them.

## Basis and limitations

This record synthesizes the exploratory case studies in the Design ideas directory. Those documents draw on public documentation, repositories, and other public references; they are not proof of the complete internal implementations of the referenced games or engines. The directions above are proposals for Hexel, not statements that the current code already implements them.

**Next action:** integrate the geodesic topology module into planet-relative terrain addressing, then implement and test surface lookup, ray selection, vertical layers, and persistent cell identity.
