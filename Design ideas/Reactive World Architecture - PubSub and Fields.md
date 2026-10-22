# Reactive World Architecture: Pub/Sub, Dependencies, and World Fields

**Status:** Exploratory design notes — not an implementation specification  
**Project:** Hexel Engine  
**Purpose:** Capture ideas about pub/sub-style communication, SpaceTimeDB concepts, chunk dependencies, entities, and environmental simulation.

## 1. Core idea

Hexel Engine could be designed around a reactive world: systems and objects publish meaningful changes and declare which information they depend on. The world remains the authoritative owner of game state, while rendering, simulation, AI, and environmental systems respond to relevant changes.

The goal is not to force every object through one universal event bus. It is to give the engine a consistent way to express relationships and dependencies, then use an efficient mechanism appropriate to each relationship.

A useful guiding principle:

> Static subscriptions describe how the world is structurally connected. Dynamic subscriptions describe what currently cares about what. Change propagation carries relevant updates through both.

These ideas are exploratory. They should be tested incrementally against the engine's needs and measured for performance before adding a general framework.

## 2. Two kinds of subscriptions

### 2.1 Static subscriptions — structural dependencies

Static subscriptions represent relationships established by the structure of the world. They are expected to persist and change mainly when the world structure itself changes.

Examples:
- A chunk section tracks its adjacent chunk sections, including the six horizontal hex-grid neighbors and vertical sections above and below.
- A chunk section depends on neighboring boundary cells when deciding which mesh faces are exposed.
- A chunk belongs to a biome, geographical region, or other world partition.
- Hydrology depends on terrain elevation, geology, and permeability.
- Vegetation depends on soil, moisture, sunlight, and climate.
- Temperature or weather calculations depend on relevant sunlight, elevation, and regional climate inputs.

“Static” does not mean impossible to change. World expansion, chunk loading/unloading, or changing region boundaries may require relationships to be rebuilt. The distinction is that these dependencies are structurally defined rather than constantly changing with entity behavior.

### 2.2 Dynamic subscriptions — runtime or hot-swappable interests

Dynamic subscriptions are created, changed, and removed as the simulation evolves.

Examples:
- An NPC listens for nearby threats and stops tracking them when they leave its interest area.
- A player or observer requests updates for chunks near its current position.
- An entity follows a water source and switches its target when its needs change.
- A temporary fire responds to wind and nearby fuel conditions while it exists.
- A chunk temporarily registers interest in a weather event or regional condition.
- An NPC changes which biome, region, entities, or environmental data it cares about as it moves.

Subscriptions should have a clear lifecycle: establish interest, obtain the current state when needed, receive relevant future changes, and unsubscribe when the interest ends.

## 3. Three mechanisms to keep distinct

The two subscription categories are useful at the design level. Underneath them, it helps to distinguish three separate mechanisms:

| Mechanism | Responsibility | Example |
|---|---|---|
| Structural dependency tracking | Record persistent relationships | A chunk section depends on a neighbor's shared boundary |
| Dynamic subscriptions | Track changing interests | An NPC tracks entities in its current perception radius |
| Change propagation and invalidation | Notify or schedule relevant work | A terrain edit marks affected meshes or field results as stale |

A dependency does not necessarily require an immediate callback for every change. A chunk may depend on a neighbor's boundary, but an edit deep inside that neighbor should not rebuild this chunk's mesh. Only a relevant boundary change should invalidate it.

Similarly, a climate field may depend on sunlight but process accumulated changes once per simulation step rather than recalculating immediately after each small update.

## 4. SpaceTimeDB concepts worth borrowing

Reference: [What is SpaceTimeDB?](https://spacetimedb.com/docs/intro/what-is-spacetimedb/)

SpaceTimeDB combines application logic and authoritative database state, and lets clients subscribe to data so their local views can update when that data changes. Hexel Engine does not need to adopt SpaceTimeDB or become a multiplayer game to learn from these ideas.

Potentially useful concepts:
- **Authoritative state:** keep one clear source of truth for terrain and simulation data.
- **Change-driven updates:** notify interested systems when relevant state changes instead of rebuilding everything continually.
- **Subscriptions as declared interests:** track which consumers need which data.
- **Local views/caches:** let render or simulation systems maintain derived representations without treating them as authoritative world state.
- **Indexes based on access patterns:** choose spatial indexes or lookup structures based on actual queries, balancing speed against memory and update cost.
- **Batching and transaction-like operations:** group related mutations where that helps keep the world consistent and avoids repeated work.

These are architectural inspirations, not a recommendation to add a database server, networking stack, or SpaceTimeDB dependency.

## 5. The world as authority; systems as interested consumers

The “world as server, entities as clients” analogy can be useful conceptually, even for a single-player engine. The world owns authoritative state and exposes controlled ways to query or mutate it. Entities and systems request or subscribe to the information they need.

Possible participants include:
- **World state:** occupied hexels, chunk layout, terrain and region metadata.
- **Chunk sections:** local terrain representation, neighbor relationships, cached meshes.
- **Entities:** player, NPCs, animals, projectiles, temporary objects.
- **Rendering:** visible geometry, highlighting, lighting data.
- **Simulation systems:** physics, navigation, AI, ecology.
- **Environmental fields:** geology, geography, hydrology, sunlight, climate, weather, biology.

Not every participant needs the same subscription implementation. A common model can sit above specialized mechanisms chosen for performance.

## 6. World fields and derived dependencies

Environmental systems can be modeled as distinct fields. A field owns or calculates a category of information, publishes meaningful changes, and declares dependencies on other fields.

Illustrative relationships:
- Geology → hydrology: rock type and permeability influence water flow.
- Terrain elevation and hydrology → erosion: water flow and slope influence terrain change.
- Sunlight and climate → local temperature.
- Climate, water availability, and soil → biology and vegetation.
- Geography and terrain → biome or regional classification.
- Terrain and occupancy → navigation and collision representations.

These are dependency examples, not a fixed processing order. Real dependency graphs can have multiple inputs and may need carefully defined update phases.

If rainfall changes in one region, the engine should identify the affected hydrology and vegetation results instead of recalculating the entire world. Dependent systems can mark results as stale and recompute them when scheduled.

A field can publish a region-level revision or change notification. Subscribers then query the current authoritative inputs when updating their derived state. Notifications should generally signal that something changed, not carry a potentially stale snapshot that consumers blindly trust.

## 7. Spatial scope and interest management

Subscriptions should support more than “everything” or “one hexel.” Useful scopes might include:
- One hexel or cell.
- One chunk section.
- A radius around an entity.
- A biome or geographical region.
- A field or field layer, such as temperature or moisture.
- A class of events, such as terrain edits or weather events.
- A specific entity or system.

An interest manager could translate broad requests such as “nearby threats” or “weather in my region” into efficient internal queries and subscriptions. Spatial indexes can help identify relevant entities and chunks without creating a literal subscription edge between every possible pair.

Use hierarchical propagation and filtering: a region can summarize or forward relevant changes to local consumers, while unrelated regions remain unaffected.

## 8. Events versus state subscriptions

These should have different semantics:

- **State subscription:** “Tell me the current moisture in this region and keep me informed when it changes.” A new subscriber needs an initial/current view as well as future updates.
- **Event subscription:** “Tell me when lightning strikes this region.” This is a transient occurrence; a new subscriber usually should not receive old events unless replay is explicitly requested.

Events are useful for occurrences and commands/results. State subscriptions are useful for values that remain meaningful over time. Some systems may use both.

## 9. Change propagation, batching, and ordering

A change should invalidate only the derived data that actually depends on it. Recommended principles:
1. Declare dependencies where possible.
2. Filter notifications by spatial and semantic relevance.
3. Batch repeated changes.
4. Deduplicate pending invalidations so a chunk or field region is processed once per update pass where possible.
5. Process dependent systems in a defined order when correctness requires it.
6. Avoid uncontrolled synchronous callback chains and recursive event cascades; queued or phase-based processing can make updates easier to reason about.
7. Give subscriptions clear ownership and cleanup rules so destroyed entities or unloaded chunks do not leave stale listeners.
8. Measure queue size, notification count, rebuild count, memory use, and frame time before adding complexity.

High-frequency data does not always belong on a general event bus. Per-frame transforms or physics may be better handled by direct system queries and scheduled updates. Discrete terrain edits can use typed events, while expensive derived data can use coalesced invalidation queues.

## 10. Example: terrain edit

An illustrative flow for editing a hexel:
1. A controlled world API changes authoritative occupancy.
2. The world identifies which cell and boundaries changed.
3. The relevant chunk mesh is marked dirty.
4. Neighboring chunk sections are marked dirty only if the edit affects their shared boundary or another relevant dependency.
5. Hydrology, lighting, navigation, or other systems are invalidated only if they depend on the changed data.
6. Pending work is deduplicated.
7. Each system queries current world state when it processes the invalidation and updates its derived data.

This builds on the engine's existing idea of dirty chunks and cached meshes instead of replacing it with a heavyweight event architecture.

## 11. Example: moving entity

An illustrative dynamic-subscription flow:
1. An entity moves into a new chunk or region.
2. The interest manager updates the entity's region of interest.
3. The entity stops receiving region-specific updates it no longer needs and gains the new relevant interests.
4. It obtains the current state for newly relevant subscriptions.
5. Future updates are filtered to its current interests.

The entity should not need a direct subscription to every hexel in its radius. The engine can resolve the request using spatial indexes, chunk-level interests, or regional field queries.

## 12. Relationship to ECS

Entity-Component-System architecture and pub/sub solve related but different problems:
- **ECS** describes what data entities have and which systems process that data.
- **Pub/sub** describes which consumers are interested in changes or events.
- **Spatial partitioning** describes which entities and regions are relevant to one another.
- **Dependency tracking** describes which derived results need to be recalculated when inputs change.

They can complement one another. For example, an NPC might have Position, Health, AIState, and NavigationAgent components. An AI system processes appropriate components on a schedule and uses dynamic interests or spatial queries to discover relevant threats. There is no need to make every component a literal event publisher or subscriber.

## 13. Incremental implementation plan

Do not build a universal framework before the engine has concrete requirements. Test the design with small, measurable cases.

### Stage 1 — Preserve authoritative world state
Keep world mutations behind controlled APIs. Rendering meshes and other caches remain derived data, not the source of truth.

### Stage 2 — Formalize terrain invalidation
Use a terrain edit to identify the exact chunk sections and shared boundaries affected. Deduplicate dirty work and avoid rebuilding unrelated meshes.

### Stage 3 — Add typed change notifications where useful
Introduce a small event/change representation for discrete world mutations. Keep high-frequency paths simple and do not route every operation through a global bus.

### Stage 4 — Test dynamic interests
When entities are introduced, prototype an entity moving between regions and updating its subscriptions or regional interests.

### Stage 5 — Add field dependencies
Introduce a simple derived field that depends on terrain or another field. Verify that changing a local input invalidates only the affected region.

### Stage 6 — Profile before generalizing
Measure notification volume, dirty work, mesh/field rebuild times, memory use, and frame time. Compare alternatives before adding parallel queues, complex subscription graphs, or more advanced indexing.

## 14. Open design questions

- What is the smallest useful unit of subscription: cell, chunk section, region, field, or entity?
- Which changes need immediate processing, and which can be coalesced until a simulation phase?
- How should dependency regions be expanded when calculations need neighboring samples?
- Should field dependencies form a directed acyclic graph, or do some simulations require iterative/cyclic solving?
- How should subscriptions behave when chunks are unloaded and later loaded?
- Which systems need deterministic update ordering?
- What data should be indexed, based on measured access patterns?
- When does a regional subscription justify explicit listener tracking versus a spatial query at update time?

## 15. Reference links

- [SpaceTimeDB — What is SpaceTimeDB?](https://spacetimedb.com/docs/intro/what-is-spacetimedb/)
- [SpaceTimeDB — Client subscriptions](https://spacetimedb.com/docs/clients/subscriptions)
- [SpaceTimeDB — Tables](https://spacetimedb.com/docs/tables)
- [SpaceTimeDB — Indexes](https://spacetimedb.com/docs/tables/indexes)
- [SpaceTimeDB — Transaction atomicity](https://spacetimedb.com/docs/databases/transactions-atomicity)

## Summary

The proposed direction is a **reactive, spatially scoped world architecture**:

- Static subscriptions describe structural relationships.
- Dynamic subscriptions represent changing runtime interests.
- Change propagation carries filtered, relevant updates.
- The world remains the authoritative state owner.
- Fields and systems declare dependencies and update derived results selectively.
- Work is batched, deduplicated, ordered where necessary, and profiled.

The aim is not “everything listens to everything.” It is a common relationship model with specialized, efficient implementations beneath it.
