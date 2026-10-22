# Field-Based World Simulation and Finite Hydrology

## Design idea for Hexel Engine

**Status:** Exploratory design proposal — not an implementation specification  
**Scope:** Spatial fields, finite water, environmental dependencies, and the foundations of an interconnected ecosystem  
**Related design:** [Reactive World Architecture — PubSub and Fields](./Reactive%20World%20Architecture%20-%20PubSub%20and%20Fields.md)

---

## 1. The core idea

Hexel should aim for a world in which terrain, water, climate, geology, vegetation, and fauna are not independently decorated features. They are parts of an interconnected system whose local conditions explain what can exist there and how it changes.

A mountain river should have a source and a water budget. Its route should depend on terrain and geology. A forest should depend on climate, water availability, soil, and other ecological conditions. Plants should affect their surroundings, and animals should depend on plants, prey, water, and habitat. Changes should propagate through the systems that depend on them.

**Design principle:** The world is a network of spatial fields, physical stores, entities, and interacting processes. Visible terrain and ecosystem features should be supported by those underlying systems rather than existing as unrelated decorations.

This is an architectural direction, not a requirement to simulate every natural process at full scientific fidelity. The goal is coherent cause and effect at a level of detail that fits the game and its performance budget.

## 2. Why fields?

A field describes a value or set of values over space. Conceptually, a field evaluates a property at a location:

`field(position) -> value`

Examples include elevation, temperature, soil moisture, geological permeability, vegetation suitability, and water flow. A field may be generated from a seed and parameters, sampled and stored, derived from other fields, or updated as a simulation runs.

Fields provide a shared language for environmental conditions. They do **not** imply that every property must be stored in one enormous grid or that every distinct thing should stop being an entity.

### Candidate field families

| Family | Example values | Typical role |
|---|---|---|
| Geology | Material, porosity, permeability, hardness | Controls infiltration, groundwater movement, erosion resistance, and material behavior |
| Terrain | Elevation, slope, landform, surface orientation | Influences runoff, climate, erosion, and construction |
| Hydrology | Surface-water storage, soil moisture, groundwater storage, discharge, saturation | Represents water availability, transport, and exchange |
| Climate | Temperature, precipitation, wind, seasonality | Drives snow, evaporation, plant tolerances, and broad biome suitability |
| Energy and exposure | Sunlight, shade, solar input | Influences local temperature, plant growth, and snowmelt |
| Soil and nutrients | Soil depth, fertility, nutrient availability | Influences plant growth and habitat |
| Ecology | Plant biomass, species suitability, population density, habitat suitability | Represents ecosystem potential and/or evolving ecological state |
| Process fields | Flow direction, erosion rate, sediment transport, infiltration rate | Describes how matter or energy moves and how state changes |

These families should not all have the same implementation or update frequency. They are conceptual groupings, not a demand for one data structure per row.

## 3. Finite water: a first-class world rule

A key motivation is to avoid effectively infinite water sources. Rivers, lakes, aquifers, soil moisture, and oceans should be parts of a water system, not unrelated decorative blocks that can create unlimited water.

Water can be held in reservoirs, transferred between them, and exchanged with the atmosphere or simulation boundaries. A simplified model can still conserve water without tracking individual molecules.

### Candidate water reservoirs

- **Surface water:** Rivers, lakes, ponds, wetlands, and temporary pools.
- **Snow and ice:** Seasonal snowpack and longer-lived ice, where included in the game's scope.
- **Soil moisture:** Water held in soil and available to plants or to further drainage.
- **Groundwater and aquifers:** Subsurface storage influenced by geology, porosity, permeability, hydraulic head, and connections between geological regions.
- **Ocean storage:** A large surface reservoir and potential boundary condition for a regional or global water cycle.
- **Atmospheric moisture:** Water moved by evaporation, transport, condensation, and precipitation. It can initially be modeled with simplified regional climate rules rather than a detailed atmospheric simulation.

### Conservation rule

For a reservoir over a simulation step:

`change in stored water = inflows - outflows`

Inflows and outflows can include precipitation, snowmelt, runoff, river discharge, infiltration, groundwater exchange, pumping, evaporation, transpiration, and export across a modeled boundary.

Transfers between reservoirs should be accounted for on both sides. If a river transfers 10 units of water into a lake, the river loses 10 and the lake gains 10, subject to the model's units and numerical tolerance. Water should not appear or disappear silently.

A world model may choose explicit external inputs and sinks. For example, precipitation can be an input if the atmospheric cycle is not simulated in full, but that choice should be explicit. If atmospheric water is modeled as part of a closed global cycle, precipitation must draw from atmospheric storage and evaporation must add to it.

**Finite does not mean microscopically detailed.** It means the system accounts for storage and transfers consistently at its chosen scale.

### Geology makes hydrology interesting

Water behavior should depend on the world it moves through:

- Porous, permeable material may admit infiltration and allow groundwater flow.
- Low-permeability layers can restrict drainage and redirect groundwater.
- Elevation and hydraulic head influence surface and subsurface flow.
- Soil depth and saturation affect how much rainfall infiltrates versus runs off.
- Springs may appear where groundwater intersects the surface or where pressure and geology create an outlet.
- Lakes and wetlands form and persist according to terrain, inflows, outflows, and local geology.
- River discharge depends on accumulated upstream contributions and local inputs and losses.

The exact hydraulic model can begin simply. The essential architectural point is that water quantity, terrain, and geological properties must interact.

### Consequences worth enabling

- Drought reduces soil moisture, wetland area, lake levels, and river discharge.
- Heavy rain or snowmelt increases runoff and can cause floods.
- Groundwater extraction can lower aquifer storage and dry springs.
- A dam redistributes water, potentially flooding upstream land and reducing downstream flow.
- Changing a riverbank or opening a channel can change drainage.
- Forest loss can alter infiltration, runoff, transpiration, and erosion.
- Seasonal snowpack can store water and release it during warmer periods.

These are possible simulation outcomes, not promises that every effect will be implemented in the first version.

## 4. Climate, biomes, flora, and fauna

Biomes should ideally be derived from environmental conditions rather than assigned as arbitrary labels alone.

A useful conceptual dependency is:

1. **Climate and physical geography:** Temperature, precipitation, seasonality, sunlight, elevation, wind, and geology.
2. **Environmental conditions:** Water availability, soil, nutrients, temperature extremes, and shelter.
3. **Biome and habitat suitability:** Which broad ecological communities could persist under those conditions.
4. **Flora:** Which plant species can establish, grow, compete, and reproduce.
5. **Fauna:** Which animals can survive given food, water, shelter, prey, predators, and habitat connectivity.
6. **Feedback:** Organisms alter shade, soil, nutrients, infiltration, transpiration, grazing pressure, and habitat.

This is a dependency network, not a one-way pipeline. For example, a forest can increase shade, change soil structure, intercept rainfall, and return water to the atmosphere. Grazing can reduce vegetation. Beavers can modify streams and create wetlands. Predators can change prey populations, which can change vegetation pressure.

A biome label is best treated as a useful classification or generation guide, not as the ultimate source of truth. A region's conditions may support several possible species, and actual populations can depend on dispersal, disturbance, competition, and history.

## 5. Field types and storage strategies

The field interface should describe what a field means and how consumers can query or invalidate it without forcing every field to use the same storage strategy.

### Evaluated or procedural fields

Computed from position, seed, and parameters. Useful for reproducible base terrain, broad climate patterns, and initial geological distributions.

### Sampled or stateful fields

Stored at discrete cells, regions, or simulation nodes. Useful for water storage, soil moisture, snowpack, temperature state, and ecological populations. Stateful values must persist or be reproducibly reconstructed according to the game's save model.

### Sparse or hierarchical fields

Store data only where useful, or use different resolutions for different areas. Potentially useful for large worlds, localized high-detail simulations, caves, or volumetric properties. This can save memory, but adds complexity in interpolation, neighbor queries, boundary handling, and transitions between resolutions.

### Network representations

Not every environmental process is naturally a dense field. Rivers can also be represented as connected reaches, aquifers as connected geological regions, and food webs as relationships between populations. Fields can describe properties of those networks, such as discharge, storage, or suitability.

**A field is a semantic contract, not a storage mandate.** Horizontal regional fields, surface-cell fields, 3D volumetric fields, and graph/network data can coexist.

## 6. Separate environmental truth from derived data

A proposed separation for Hexel:

- **Authoritative world state:** Hexel occupancy, materials, persistent edits, and other state that must be saved and treated as real.
- **Environmental state:** Water storage, moisture, snowpack, and other evolving quantities.
- **Generated inputs:** Seeded base landforms, initial geology, and world parameters.
- **Derived fields:** Biome suitability, flood risk, vegetation suitability, and other values computed from inputs and state.
- **Processes:** Water transfer, erosion, sediment transport, plant growth, decay, and ecological interactions.
- **Derived engine data:** Meshes, collision, navigation data, visual water surfaces, and caches.

The exact boundary between authoritative terrain and environmental state needs careful design. For example, an erosion process may calculate a desired change, but a terrain-mutation system should apply the resulting Hexel changes. The renderer should not become the owner of terrain or water truth.

Derived values should not silently overwrite their inputs. Changing a biome classification should not change precipitation. A deliberate process may change vegetation, and vegetation may subsequently influence hydrology through an explicit feedback rule.

## 7. Pub/Sub and dependency-aware invalidation

Fields pair naturally with the existing [Reactive World Architecture — PubSub and Fields](./Reactive%20World%20Architecture%20-%20PubSub%20and%20Fields.md) design.

Fields express spatial conditions and relationships. Pub/Sub communicates changes and wakes the relevant consumers. A terrain edit should not cause every system to rebuild the entire world.

### Example: player changes a riverbank

1. The authoritative terrain or material state changes.
2. A spatial change event identifies the affected region and change type.
3. The dependency system marks affected calculations dirty: local slope, drainage, permeability, or flow may need updating.
4. Hydrology recalculates the affected region and any downstream or connected areas that depend on it.
5. Consumers update as required: water presentation, meshes, navigation, vegetation suitability, or habitat.

Local changes can have nonlocal consequences. A small edit may redirect a river and affect downstream storage, so invalidation cannot always be limited to a fixed-radius neighborhood. Connected flow networks and aquifers may require propagation until the affected state stabilizes or a defined update budget is reached.

### Avoid uncontrolled feedback loops

Environmental systems form cycles: vegetation influences infiltration; water affects vegetation; roots influence erosion; erosion changes terrain; terrain changes flow.

Do not implement these as unrestricted synchronous callbacks that recursively trigger each other. Use scheduled simulation phases, queued changes, explicit iteration rules, or another controlled update model. Define which state is read and written in each phase so results are reproducible and debuggable.

Pub/Sub should notify and coordinate changes; it should not itself become the physics solver.

## 8. Example workflow: a watershed

A first watershed prototype could proceed as follows:

1. Generate seeded terrain and geology.
2. Assign material properties such as permeability and erosion resistance.
3. Establish a rainfall or snowmelt input and finite initial surface, soil, and groundwater storage.
4. Calculate a simplified surface drainage network from terrain.
5. Transfer water between soil, surface channels, lakes, and groundwater according to explicit rules.
6. Track discharge and storage, checking water accounting after each step.
7. Derive soil moisture and water availability.
8. Derive biome and vegetation suitability from moisture, temperature, elevation, and soil.
9. Allow vegetation to affect selected hydrological properties.
10. Edit the terrain and verify that relevant drainage, storage, and derived results update correctly.

A 2D elevation model can be a practical first step for watershed experiments, but it cannot by itself represent arbitrary caves, overhangs, or complex underground flow. Those would require 3D material/occupancy rules, volumetric fields, or an explicit underground network. The first prototype should state its limitations rather than pretending a surface model covers all geology.

## 9. Suggested development sequence

### Prototype A — reproducible environmental fields

- Generate elevation and a simple geological material distribution from a seed.
- Give materials a small set of meaningful physical properties.
- Verify that identical seeds and parameters produce identical starting conditions.
- Ensure player edits can override or persist independently of generated defaults.

### Prototype B — finite local hydrology

- Define water units and storage capacity where appropriate.
- Implement a small set of reservoirs and explicit transfers.
- Begin with precipitation/runoff, surface storage, infiltration, drainage, and evaporation.
- Add a conservation check and expose numerical error in tests or debug output.
- Keep the model small enough to reason about before introducing global weather.

### Prototype C — connected flow and geology

- Add downstream discharge, simple groundwater storage, and permeability-dependent transfer.
- Test dry conditions, rainfall, blocked channels, permeable versus impermeable layers, and connected reservoirs.
- Distinguish local recalculation from downstream/network propagation.

### Prototype D — derived ecology

- Derive moisture and biome suitability.
- Add a few plant types with different tolerances.
- Add one herbivore dependent on available forage and water.
- Introduce feedback only where it is intentional and testable.

### Prototype E — reactive terrain edits

- Publish spatial changes through the existing event architecture.
- Invalidate only the dependencies affected by a change.
- Test edits that have both local and downstream consequences.
- Measure evaluation cost, memory, dirty-region size, propagation cost, and determinism.

These are investigation steps, not a commitment to a specific implementation order if profiling or engine constraints suggest otherwise.

## 10. Questions to settle before implementation

- What is the water unit: volume per cell, normalized quantity, or another consistent measure?
- What simulation timestep and update frequency are appropriate?
- Is the modeled atmosphere part of the water budget, or is precipitation an explicit external input?
- How are oceans represented: a global reservoir, regional boundaries, or both?
- What determines river capacity, lake levels, overflow, and groundwater exchange?
- How are vertical geology and underground storage represented in a Hexel world?
- Which fields are procedural, cached, stateful, sparse, or network-based?
- How does persistence distinguish generated defaults from player changes and simulated state?
- How are long-range dependencies invalidated after a terrain edit?
- Which environmental cycles are simulated, and which are approximated?
- How do we make updates deterministic enough for testing, saves, and reproducible debugging?
- What fidelity is worth its CPU and memory cost for the intended gameplay?

## 11. Research starting points

These sources inform the direction, but none should be treated as a ready-made architecture for Hexel.

- **Abhradip Dey, “Field-Based Modeling Philosophy.”** Introduces the idea of modeling properties as spatial functions and discusses concepts including signed distance fields and implicit surfaces. Useful conceptual entry point; its object-versus-field framing should not be taken as a rule that all game entities must become fields.  
  https://medium.com/@deyabhradip594/field-based-modeling-philosophy-686c7e121040

- **Génevaux et al. (2013), “Terrain Generation Using Procedural Models Based on Hydrology.”** Research on procedural terrain generation shaped by drainage networks and hydrological structure. Supports the idea that water systems should influence terrain rather than simply be painted onto an unrelated heightmap.  
  https://doi.org/10.1145/2461912.2461996

- **“AutoBiomes: Procedural Generation of Multi-Biome Landscapes” (2020).** Explores combining terrain and simplified climate information to derive biome distributions. Relevant to deriving ecosystem possibilities from environmental conditions.  
  https://link.springer.com/article/10.1007/s00371-020-01920-7

- **OpenVDB Technical Overview.** Describes sparse hierarchical volumetric data structures and grids for applications including signed distance fields and fog volumes. Useful as a reference for sparse storage concepts, not a drop-in Rust recommendation.  
  https://www.openvdb.org/documentation/doxygen/overview.html

- **Unreal Engine Procedural Content Generation overview.** A practical reference for graph-driven spatial data and attribute-based content generation. The useful lesson is the data flow between spatial properties and generation rules, not a requirement to adopt Unreal's implementation.  
  https://dev.epicgames.com/documentation/unreal-engine/procedural-content-generation-overview?lang=en-US

## 12. Summary

The field-based direction gives Hexel a way to express environmental conditions consistently. Finite hydrology gives those fields a demanding, measurable first application: water must be stored, moved, and accounted for. Climate and geology influence water; water and soil influence biomes; biomes constrain flora; flora and fauna interact; those interactions feed back into the environment.

The architecture should combine fields with explicit entities, authoritative state, scheduled processes, and dependency-aware Pub/Sub. It should favor conservation, reproducibility, incremental updates, and measurable fidelity over a claim to simulate every natural detail.

**Recommended first proof of concept:** a seeded watershed with terrain-dependent drainage, finite surface/soil/groundwater storage, basic water accounting, and a moisture field derived from the simulation. Make a terrain edit and verify both the hydrological consequences and the dependency invalidation. If this works, the same architecture can expand toward climate, biomes, vegetation, and fauna.
