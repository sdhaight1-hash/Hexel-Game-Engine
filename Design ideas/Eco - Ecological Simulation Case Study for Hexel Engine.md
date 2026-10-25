# Eco Case Study: Ecological Simulation Lessons for Hexel Engine

## Design research document

**Status:** Research and design recommendations; not an implementation specification  
**Primary subject:** Eco by Strange Loop Games  
**Primary question:** Which ideas from Eco's simulated ecosystem are useful for Hexel Engine's field-based world model, finite hydrology, and reactive Pub/Sub architecture?  
**Related Hexel designs:**
- [Field-Based World Simulation and Finite Hydrology](./Field-Based%20World%20Simulation%20and%20Finite%20Hydrology.md)
- [Reactive World Architecture — PubSub and Fields](./Reactive%20World%20Architecture%20-%20PubSub%20and%20Fields.md)

---

## 1. Executive summary

Eco is a useful design reference because it presents the environment as a connected simulation with consequences, rather than a static backdrop. Its official site describes an ecosystem containing growing plants and animals, with player actions affecting the world. It highlights cascading ecological effects, pollution, resource extraction, farming conditions, and tools for inspecting environmental data. The Steam description frames the core challenge as building civilization while avoiding destruction of the ecosystem.  
Sources: [Eco official site](https://play.eco/), [Eco on Steam](https://store.steampowered.com/app/382310/Eco/).

The most relevant lesson for Hexel is not to reproduce Eco's gameplay loop or social systems. It is to build a world in which:

1. Environmental properties influence what can grow and survive.
2. Changes in one system can propagate into others.
3. Resource use and pollution have persistent, legible consequences.
4. Environmental information can be inspected rather than hidden behind opaque rules.
5. Simulation rules and world state remain coherent as the player changes the terrain.

This aligns strongly with Hexel's interest in fields, finite water, climate-driven biomes, vegetation, fauna, and Pub/Sub dependency invalidation.

**Recommended takeaway:** Use Eco as a reference for *ecological coupling and visible consequences*, while Hexel develops its own field interface, water accounting, state ownership, and update scheduling. Do not infer Eco's internal implementation from its visible behavior or public descriptions.

## 2. What Eco documents publicly

Eco is developed and published by Strange Loop Games. Its official site describes a simulated ecosystem with many growing plants and animals, and says that a disruption to one species can have cascading effects. It gives examples such as tree removal damaging habitat and mining waste polluting rivers and poisoning farms. The site also describes simulation data being exposed through graphs and heat maps.  
Source: [Eco official site](https://play.eco/).

The official Eco wiki describes the world as reactive and says player actions affect the underlying ecosystem. Its agriculture documentation describes crop success as dependent on local temperature, moisture, and nutrients, and its pollution documentation describes pollution types and environmental effects. These are public descriptions of gameplay systems; they do not establish exactly how Eco's internal code, data structures, or solvers work.  
Sources: [Eco Wiki](https://wiki.play.eco/en/Eco_Wiki), [Agriculture](https://wiki.play.eco/en/Agriculture), [Pollution](https://wiki.play.eco/en/Pollution).

**Version caveat:** At the time of research, the Eco wiki displayed a notice that its documentation was being updated for version 14.x and that errors could occur during the transition. Specific mechanics described by wiki pages may therefore be version-dependent or in flux. The official site and Steam page are useful for high-level design intent; the wiki is useful for concrete examples but should be checked against the target game version if exact behavior matters.

## 3. Case study: environment as a dependency network

### 3.1 Ecological consequences are connected

Eco's official site explicitly describes cascading effects: removing trees can damage habitat, while mining waste can pollute rivers and harm farms. The useful principle is that an action does not stop at the object directly changed. Its consequences propagate through environmental relationships.  
Source: [Eco — A Rich Simulation](https://play.eco/).

For Hexel, this suggests dependency paths such as:

- Terrain edit → changed slope or material exposure → altered runoff or infiltration → changed local water availability → changed vegetation suitability.
- Tree removal → reduced canopy and root cover → altered shade, interception, infiltration, and erosion resistance → changed soil and stream conditions.
- Mining or industrial activity → waste/pollution state → transport or seepage → affected soil or water quality → reduced suitability for sensitive species.
- Species decline → reduced food or ecological interaction → changed population pressure on other species.

These are proposed Hexel relationships, not claims that Eco uses these exact causal equations.

### 3.2 Avoid isolated gameplay flags

A weak implementation might mark a region as “polluted” and apply a visual tint. A more useful system represents pollution as a quantity or type with sources, transport, persistence, attenuation, and effects on dependent systems.

Similarly, a weak biome implementation assigns “forest” to a region and spawns trees. A stronger model derives a forest's suitability from temperature, moisture, soil, elevation, and other conditions, then uses species-specific tolerances and ecological state to determine what actually lives there.

Hexel does not need every relationship to be scientifically complete. It does benefit from making relationships explicit so they can be tested, expanded, and inspected.

## 4. Case study: environmental suitability as a field

Eco's agriculture documentation provides a practical example of spatial suitability. It says crop yield potential reflects how well soil conditions match a plant, and identifies temperature and moisture as major factors, with nutrients also influencing growth. It describes soil moisture as affected by rainfall and water spread.  
Source: [Eco Wiki — Agriculture](https://wiki.play.eco/en/Agriculture).

The generalizable pattern is:

1. Store or derive environmental conditions.
2. Define the requirements of a species or process.
3. Evaluate suitability at a location.
4. Use suitability to influence growth, survival, or output.
5. Update the result when relevant environmental inputs change.

For Hexel, a species' suitability might be expressed conceptually as a function of temperature, soil moisture, nutrient availability, sunlight, elevation, and pollution. This does not need to be a single magic number forever; the model can begin with simple ranges and gradually add more meaningful constraints.

Example conceptual interface:

`suitability(species, location, environmental_state) -> score`

The suitability score is a derived result, not the authoritative state of a living plant. Actual presence, biomass, age, health, and reproduction are separate state. This distinction prevents a common design mistake: treating “this species could thrive here” as equivalent to “this species is already present here.”

### Hexel recommendation

Use separate concepts for:
- **Environmental fields:** current temperature, moisture, soil properties, sunlight, pollution.
- **Species parameters:** tolerance ranges, water demand, growth rate, reproductive conditions.
- **Suitability fields:** derived estimates for a species or species group.
- **Ecological state:** actual populations, biomass, growth, health, and local absence/presence.

This lets the world support both procedural generation and change over time without confusing potential with reality.

## 5. Case study: pollution and transported consequences

Eco's wiki documents ground pollution, air pollution, and different pollution sources. It says ground pollution includes water and spreads according to hydrology, seeping into nearby blocks; air pollution spreads through air and can dissipate. It also describes plants and trees as varying in pollution resistance. The wiki warns that pollution details are being updated for version 14.x, so treat the exact rules as version-sensitive.  
Source: [Eco Wiki — Pollution](https://wiki.play.eco/en/Pollution).

The broader lesson for Hexel is that environmental hazards should have both **a source and a path**.

A pollution model can be designed around:
- Source terms: waste deposits, industrial processes, mining, fires, or other causes.
- Environmental state: concentration or burden by pollutant type.
- Transport: water flow, groundwater seepage, wind, or direct contact.
- Transformation: dilution, degradation, containment, or other explicitly modeled processes.
- Effects: species-specific tolerance, crop suitability, water quality, soil quality, and potentially human use.
- Observability: maps or queries showing source, concentration, spread, and affected areas.

A simple version could begin with one pollutant and one transport pathway. The important thing is to avoid coding a one-off effect that is disconnected from the environmental model.

### Relationship to finite hydrology

Finite water and pollution should be designed together, but they are distinct systems. Water storage and flow move water; pollution has its own mass or concentration behavior. If the model transports a pollutant with water, it must define whether it tracks pollutant mass, concentration, or a simplified scalar and avoid accidentally creating or destroying pollutant when water volume changes.

For a mass-based model, a simple conceptual transfer is: when a water parcel moves, it carries a corresponding amount of dissolved pollutant. A receiving reservoir gains that mass; the source loses it. Degradation, precipitation, filtration, or cleanup must be explicit processes.

This is a later extension, not a requirement for the first hydrology prototype.

## 6. Case study: visible simulation data

Eco's official site says data generated by interactions between plants, animals, climate, and players can be inspected using graphs and heat maps. This is a valuable design lesson: if a world has complex cause and effect, the player and developer need ways to see why the world behaves as it does.  
Source: [Eco official site](https://play.eco/).

Hexel should consider environmental inspection tools from the beginning, even if the first versions are developer-only:

- Toggle elevation, slope, geology, permeability, water storage, flow direction, moisture, and biome suitability overlays.
- Inspect a hexel or region to see the values contributing to its state.
- Show why a species is suitable or unsuitable at a location.
- Visualize which areas were invalidated by an edit and which dependencies were recalculated.
- Track water-budget totals and residual error over a simulation step.
- Show pollution sources and spread when that system exists.

These tools are not just quality-of-life features. They help validate the architecture, diagnose bugs, and tune simulation parameters. Without them, field interactions can become a collection of hidden rules that are difficult to reason about.

## 7. Translating Eco's lessons into Hexel architecture

Eco is a game design reference, not evidence for a specific engine architecture. The following is a Hexel-specific proposal.

### 7.1 Fields express conditions

Examples include elevation, permeability, temperature, soil moisture, pollution concentration, and species suitability. Fields should have clear semantics, units or value ranges, and dependencies.

### 7.2 Entities represent distinct things

Trees, animals, dams, buildings, mines, and other discrete objects may need identity, ownership, age, inventory, or behavior. They can read environmental fields and modify the world through explicit systems. Not every object needs to be converted into a field.

### 7.3 Processes change state

Hydrology, erosion, pollution transport, plant growth, decay, and population dynamics should run as controlled processes with explicit inputs and outputs. A process computes changes; a world mutation layer applies changes to authoritative state.

### 7.4 Pub/Sub communicates changes

Events can identify which state changed and which regions or dependencies may need recalculation. Events should not themselves implement all physics or cause unlimited recursive chains of callbacks.

### 7.5 Derived engine data stays derived

Meshes, collision, navigation, and visualization should update from authoritative world and environmental state. They should not become independent owners of water, terrain, or ecosystem truth.

## 8. A concrete scenario: player alters a stream

Consider a player digging a channel that redirects a small stream.

1. The authoritative Hexel terrain changes.
2. A terrain-change event identifies the edited region and material changes.
3. The dependency system invalidates affected slope, drainage, and permeability calculations.
4. Hydrology recalculates local flow and propagates changes through the connected downstream network where needed.
5. Water is transferred between reservoirs under the finite-water rules; it is not created by the new channel.
6. Moisture and water availability update where affected.
7. Plant suitability changes in the affected area. Existing plants respond according to growth and survival rules rather than instantly teleporting or respawning.
8. Visual water, terrain meshes, navigation, and debug overlays refresh from the resulting state.
9. Inspection tools let the developer compare storage, flow, moisture, and invalidation before and after the edit.

This scenario combines Hexel's core interests: terrain editing, finite hydrology, environmental fields, dependency propagation, and derived rendering.

## 9. A second scenario: forest loss and recovery

A player clears a forest. The engine removes or changes the authoritative vegetation state, then the ecological processes respond over time.

Possible modeled effects include:
- Reduced canopy shade and interception.
- Reduced root reinforcement of soil.
- Changed evapotranspiration and soil moisture.
- Changed erosion risk or sediment supply.
- Loss or relocation of habitat for species dependent on the forest.
- A changed suitability map for future plant growth.
- Gradual regrowth if seeds, conditions, and time allow it.

The first prototype need not implement all these effects. Select a small number of explicit, testable relationships and add others when the core model is stable. Avoid treating each effect as a hard-coded consequence of the “forest” label if the underlying conditions can express it more generally.

## 10. What not to copy blindly

### Do not reproduce the entire scope

Eco's civilization, crafting, economy, governance, and multiplayer systems are not necessary to validate Hexel's environmental architecture. Borrow ecological design lessons without inheriting the whole game's scope.

### Do not mistake visible outcomes for implementation details

Public pages describe gameplay and design intent. They do not reveal enough to conclude that Eco uses a particular field abstraction, solver, grid resolution, or Pub/Sub system internally. Hexel should design for its own world representation and performance constraints.

### Do not simulate every process at the same frequency

Hydrology, plant growth, animal populations, and climate may need different update schedules. A coherent model can use coarse timesteps, regional summaries, cached derived fields, and selective high-detail updates.

### Do not make every interaction fully local

A terrain edit may be local, but its downstream effects can propagate along a river or through an aquifer. Invalidation must follow dependencies, not just a fixed-radius neighborhood.

### Do not confuse potential with current state

A field indicating that a tree species could grow somewhere does not mean a tree should instantly exist there. Suitability, population state, and world mutation are different concepts.

### Do not let environmental events recursively trigger uncontrolled updates

Cycles such as vegetation → moisture → vegetation need explicit phases, queued changes, or bounded iterative rules. This improves determinism and makes the simulation testable.

## 11. Recommended Hexel prototype sequence

### Phase A — inspection and reproducibility

- Generate seeded terrain and geology.
- Expose debug overlays and per-hexel field inspection.
- Verify deterministic initial state for a fixed seed.

### Phase B — finite watershed

- Add surface-water storage, runoff, and a simple drainage network.
- Add soil moisture and basic infiltration.
- Track water entering, leaving, and stored; test conservation within a documented tolerance.
- Make terrain edits invalidate and recalculate affected flow.

### Phase C — species suitability

- Add temperature, moisture, and soil-property fields.
- Define two or three plant types with different environmental requirements.
- Derive suitability independently of actual plant presence.
- Test how drought, terrain changes, and changed water availability affect suitability.

### Phase D — persistent ecological state

- Add plant growth and survival on a slower simulation schedule.
- Ensure a plant population changes over time instead of appearing solely from a biome label.
- Add one fauna species with explicit dependencies on food and water.

### Phase E — environmental disturbance

- Add one pollutant or sediment transport system if it supports intended gameplay.
- Track sources, transport, persistence, and effects separately.
- Build overlays and regression tests for spread and cleanup.
- Measure cost and determine which processes need regional, local, or sparse representations.

At each phase, test both fresh world generation and player edits. Record CPU cost, memory use, affected-region size, propagation cost, and determinism.

## 12. Suggested tests and success criteria

- **Determinism:** Identical seed and parameters produce identical initial environmental fields.
- **Conservation:** Water accounting residual remains within a defined tolerance in closed-system tests.
- **Causality:** Changing a relevant input changes the expected dependent result.
- **Noninterference:** An unrelated edit does not invalidate every field in the world.
- **Propagation:** A change that redirects a stream updates relevant downstream state.
- **Persistence:** Player edits and simulated state survive saving/loading according to the chosen persistence model.
- **Suitability versus occupancy:** A suitable region can remain unoccupied; occupied species respond through defined ecological rules.
- **Observability:** Developers can inspect the inputs and dependencies responsible for a result.
- **Performance:** Update costs remain bounded and measured as world size and dependency complexity increase.

## 13. Research sources

1. **Eco official website — overview of the simulated ecosystem, cascading ecological effects, pollution, and environmental data tools.**  
   https://play.eco/

2. **Eco on Steam — product overview and high-level design framing.**  
   https://store.steampowered.com/app/382310/Eco/

3. **Eco English Wiki — general overview of Eco as a reactive simulated world.**  
   https://wiki.play.eco/en/Eco_Wiki

4. **Eco English Wiki — Agriculture.** Describes crop suitability and the role of temperature, moisture, and nutrients. Wiki content is version-sensitive.  
   https://wiki.play.eco/en/Agriculture

5. **Eco English Wiki — Pollution.** Describes ground and air pollution, sources, spread, and ecological effects. The page notes ongoing version 14.x documentation updates at the time researched.  
   https://wiki.play.eco/en/Pollution

## 14. Conclusion

Eco is a useful case study because it demonstrates the design value of connecting environmental state to gameplay consequences and making ecological interactions inspectable. Its public descriptions provide concrete examples of species dependencies, pollution, agriculture, and environmental data visualization.

For Hexel, the strongest lesson is not “make an Eco clone.” It is to build a reusable environmental foundation where fields describe conditions, entities represent distinct organisms and objects, processes change state, and Pub/Sub helps propagate the consequences of edits.

Finite hydrology is a particularly good first proof because it requires explicit storage, transport, dependencies, and accounting. Once a terrain edit can change a watershed coherently—and developers can inspect why—the same architecture can support moisture-dependent plants, ecological disturbance, pollution transport, and eventually fauna.

**Recommended next step:** prototype a seeded watershed with finite water storage and a derived soil-moisture field, expose both through debug overlays, and verify dependency-aware updates after terrain edits. Treat the larger ecosystem as a series of measurable extensions rather than a single enormous simulation.
