# Hexel Engine

A native Rust prototype for a first-person, fully 3D world built from hexagonal-prism voxels (“hexels”).

Hexel Engine's long-term goal is a **finite, spherical world** with coherent planetary geography, system-driven climate, and strict resource conservation. The current build is an early flat-world prototype; the planetary systems described below are design goals, not features claimed to be implemented.

## Playable-world additions in this branch

The flat prototype now renders deterministic, coordinate-stable trees in streamed terrain chunks, with several canopy silhouettes/colors chosen from a lightweight local climate proxy. Surface grass, soil, and exposed bedrock also have deterministic color variation so the ground is less visually uniform. These are presentation-level first steps: they are not yet full species-driven tree placement from planetary biomes, and the visible rock bands do not yet represent actual `RockType` strata or spawned ore deposits.

**What am I looking at?** Aim at a solid block and press **I**. The console prints its current material/surface-layer description and cell coordinates. This is an initial inspection affordance, not yet an in-game Vintage Story-style HUD panel. Tree rendering uses species-inspired, metre-scaled height ranges and hexagonal-prism trunks. Foliage now uses crossed, double-sided textured cards with RGBA alpha-cutout shading, while trunks and major branches remain solid hexagonal-prism geometry. Canopy placement is deterministic but intentionally sparse, allowing sky gaps and branch structure to show through; broadleaf, conifer, and tropical profiles now use different silhouette rules. The current leaf atlas is procedurally generated at startup as a self-contained placeholder, not an artist-authored asset, and is the next texture-art improvement. Tall-tree visibility is accounted for in terrain chunk bounds. The current species selection is still a renderer-side climate proxy, not yet fully driven by the shared tree-species data module. It deliberately identifies geology and soil integration as incomplete rather than claiming that the data modules already drive the playable world.


## Design vision

### A real, finite globe

- Build toward one closed, spherical planet rather than an infinite or repeating world.
- Use real planet-relative latitude and longitude. Longitude wraps seamlessly around the globe; latitude runs from the south pole through the equator to the north pole.
- Make distances, travel routes, and regional geography account for planetary curvature.
- Keep the visible building material as **pure hexagonal prisms**. The spherical surface grid and its topology must be designed to work with those prisms, rather than silently replacing them with wedges or arbitrary polyhedra.
- Define the planet's size, surface, crust, and resource inventory as finite. The world should not have an edge that loops into a second copy of itself.

### System-driven climate and geography

Climate should emerge from interacting planetary conditions, not from a one-dimensional north–south temperature strip with unrelated random wet and dry patches.

The intended climate model will account for factors such as:

- Latitude, axial tilt, seasonal sunlight, and day length.
- Elevation, mountain ranges, landmasses, and ocean placement.
- Atmospheric heat and moisture transport, prevailing winds, and rain-shadow effects.
- Ocean temperatures and currents, where supported by the simulation.
- Temperature and precipitation as drivers of biomes, soil, vegetation, and water availability.

Latitude should establish important baseline patterns, but longitude must matter through actual geography and circulation. Two places at the same latitude should be able to have different climates for understandable reasons—for example, a wet coast, a dry inland rain shadow, and a region affected by a cold ocean current.

The goal is a geographically coherent planet with climate zones comparable in principle to Earth’s, not a promise of a full real-time atmospheric or ocean simulation.

### Strict conservation and finite resources

Resources should be finite by default, with renewal only through defined processes.

- **Water:** track water through reservoirs and transfers such as precipitation, rivers, oceans, groundwater, ice, and evaporation. Water can cycle and be renewable, but it must not be created for free.
- **Minerals:** place finite deposits in geological locations. Mining reduces the remaining quantity; reloading or regenerating a chunk must not restore extracted ore.
- **Soil:** nutrients can be depleted and replenished through explicit processes such as decomposition and weathering.
- **Biomass:** forests and other living resources can regrow only at defined rates under suitable conditions.
- **Crafting:** account for consumed inputs, produced materials, and any by-products or losses.

“Renewable” does not mean “infinite.” A river has a flow rate, a forest needs time and suitable conditions to recover, and an ore deposit remains depleted once extracted.

### A crust-focused, finite planet

The detailed, playable digging experience is intended to focus on the planet's crust: terrain, caves, geological formations, groundwater, and finite mineral deposits. The deeper interior can have a defined geological structure and be represented compactly where individual hexels are not needed. A fully simulated, individually stored core is not a requirement for the initial design.

### Multiple scales of play

Hexel Engine should support all three scales of play:

- **Survival scale:** meaningful local travel, resource gathering, and settlement within a region.
- **Exploration scale:** expeditions between climate zones and continents, with distance and travel time that matter.
- **Civilization scale:** long-distance transport, trade networks, and settlements distributed across the planet.

These are compatible gameplay layers, not mutually exclusive world-size options. The planet should be large enough that its curvature is not constantly obvious during ordinary local play, while remaining a genuinely finite globe.

## Current playable prototype

The current build streams a bounded neighborhood of **16 × 16 × 16 hexel-prism chunks (Q × R × Y)** around the player from a random seed. Chunk edits are kept separately from active meshes so unloading a chunk does not itself erase edits. The world renderer and player are still in the flat-world prototype stage; the spherical planet generator has not yet been connected to playable terrain. The seed is printed to the terminal at startup so you can record a particular world.

### Controls

- **W / A / S / D** — walk
- **Mouse** — look around
- **Space** — jump (gravity pulls you back down)
- **Left click** — remove the targeted hexel
- **Right click** — place a grass hexel on the targeted surface
- **Esc** — release or recapture the mouse
- **Close window** — quit

Terrain faces use darker side shading and lighter top faces to make the hexagonal-prism shape easier to read. This is still an early prototype: the player uses a simple capsule-like collision footprint, terrain meshes rebuild after each edit, and this pass uses baked face shading rather than real-time shadow maps. Terrain collision and block editing are first-pass implementations.

**Important status note:** the current prototype is not yet the finite spherical planet described in the design vision. Planetary coordinates, spherical surface topology, global climate and circulation, persistent resource accounting, and the broader simulation systems remain future design and implementation work.

## Implemented core

- Axial hex coordinates `(q, r)` plus vertical layer `y`.
- Chunk storage sized `16 × 16 × 16`.
- Grass, dirt, stone, air, and foundation block types.
- Seeded Perlin terrain-height generation.
- Shared player-to-hexel height scale (`3/5`).
- Ray/prism intersection math and unit tests.
- Native `winit` window and `wgpu` renderer with depth testing.
- First-person mouse look, walking, jumping, gravity, and basic hexel placement/removal.
- Random per-launch world seed, printed in the terminal.

## Köppen climate and vegetation rules

`src/climate.rs` defines monthly climate normals and a deterministic Köppen-Geiger classifier for tropical (A), arid (B), temperate (C), continental (D), polar (E), and highland (H) outcomes. It exposes subtype codes such as Af, Am, Aw, BWh, Csa, Cfb, Dfc, ET, and EF, with tests for representative rainforest, desert, tundra, ice-cap, and highland inputs.

Planet samples now carry a `koppen` class, and the shared tree roster applies a climate-envelope gate as well as its existing temperature, moisture, elevation, and broad-biome checks. This prevents tropical hardwoods from appearing in cold continental climates and restricts cold-climate trees to appropriate cold subtypes.

**Climate-model caveat:** monthly temperature and precipitation are currently synthesized from axial tilt, latitude, elevation, the existing temperature/moisture fields, and a seasonal rainfall curve. The classification thresholds operate on monthly normals, but those inputs are procedural estimates—not validated real-world climatology. Wind fields, circulation, ocean currents, orographic precipitation, and calibration against gridded climate data remain necessary before claiming Earth-like Köppen maps.

## Tree species and generation foundation

The canonical starting roster is data-defined in `src/trees.rs`:

- **Common temperate trees:** Oak, Birch, Maple, Walnut, and the narrow-temperature Crimson King Maple.
- **Cold-climate trees:** Pine and Larch.
- **Warm and dry trees:** Mediterranean Cypress and Acacia, including the Prickly Moses variant.
- **Wetland trees:** Bald Cypress and Willow; Willow exposes logs, flexible twigs, and medicinal bark as distinct harvest products.
- **Tropical trees:** Kapok, Bamboo, Ebony, and rare Purpleheart.
- **Special mountain/forest tree:** Redwood, including a giant variant.
- **Variants:** Birch cultivars, Maple cultivars, Pine forms, swamp Bald Cypress, Kapok growth forms, Bamboo colors, old-growth Oak, and more are represented separately from base species.

Each species profile records its temperature and moisture bounds, relative rarity, foliage style, expected height range, and harvest products. Placement is deterministic from the world seed and stable slot ID. Candidate trunks retain a diameter from **0.25 to 1.0 hexel diameters**, a hexagonal cross-section volume estimate, planet-relative root position, and local outward surface normal. Felling a live instance returns its trunk yield once; callers must persist state and awarded inventory by stable tree ID.

**Current limitation:** these are generation/data foundations, not completed playable vegetation. The current climate sample has no river network, bog map, soil survey, or Kapok-forest depth model, so wetland and deep-forest rules are approximated using moisture, elevation, and biome proxies. Tree rendering, collision, physical falling, save-file integration, and ecological regrowth are not yet connected. The source notes about plank crafting and replanting are roster requirements to implement in later gameplay systems, not claims that those mechanics already work.

## Geology, ores, and soil foundation

The renderer-independent geology rules live in `src/geology.rs`; soil and nutrient rules live in `src/soil.rs`.

- **Rock categories:** sedimentary, metamorphic, extrusive igneous, and intrusive igneous. The registry includes Chert, Claystone, Conglomerate, Limestone, Sandstone, Shale, Chalk, Metamorphosed Claystone, Marble, Phyllite, Quartzite, Schist, Slate, Gneiss, Andesite, Basalt, Obsidian, Peridotite, Rhyolite, Scoria, Tuff, Granite, and Kimberlite.
- **Strata:** a starter depth profile and depth-query helper establish the API for region-specific horizontal rock layers. The default profile is a placeholder, not a simulated planetary geology map.
- **Host-rock rules:** ore profiles define mining tier, eligible host rocks, rarity weight, finite quantity range, and whether surface nuggets are allowed. Malachite is restricted to Limestone; Cassiterite to Granite/Gneiss; Diamond to Kimberlite. Meteoric Iron is represented as a surface meteorite rather than a normal vein.
- **Finite deposits:** deterministic stable-slot generation produces bounded deposit quantities. Extraction reduces the stored quantity and cannot remove more than remains. The game must persist changed deposits by ID to preserve depletion across reloads.
- **Soil:** Barren, Low, Medium, High, Terra Preta, and Peat profiles include fertility targets, nutrient capacity, and visual color. Farming consumes nutrients; fallowing and fertilization are explicit capped operations. Terra Preta does not passively regenerate, and peat is a finite harvestable deposit.

**Current limitation:** this is a data and rules foundation only. Rock strata and ore deposits are not yet connected to generated terrain, mining tools, bloomery/crafting gameplay, or save files. The starter strata profile and host-rock list are intentionally simple; regional geological history, deposit association (such as Silver alongside Gold or Lead), mineral by-products, and real persistence remain follow-up work. Soil nutrient values are abstract balance units, not a completed crop simulation.

## Design roadmap

The roadmap prioritizes shared foundations before broad content. The current renderer is still a flat-world prototype, so gameplay features below are planned unless explicitly listed as implemented elsewhere in this README.

### Near-term: make the local world coherent and interactive

1. **Unify geometry queries:** make rendering, raycasting, placement, collision, targeting, and cell identity agree on the same hexel geometry.
2. **Establish persistent world state:** separate deterministic generation from player edits, harvested flora, depleted deposits, and inventory rewards. Unloading or reloading a chunk must not restore removed resources.
3. **Build a general procedural flora system:** treat trees as one kind of flora, not as a separate hard-coded gameplay system. Define shared species profiles, deterministic placement, lifecycle/state, interaction, and resource-yield rules that can support ground cover, herbs, ferns, shrubs, vines, crops, aquatic plants, and trees.
4. **Deliver one end-to-end flora vertical slice:** implement grass or ground cover, a resource-producing shrub, and a tree through the same species/instance/state architecture, while allowing each form its own suitable representation. Validate harvesting, persistence, targeting, and regrowth rules before expanding the roster.
5. **Make flora physically and visually legible:** derive collision and interaction from authoritative flora instances; support aiming at individual harvestable parts, clear per-hexel selection feedback where appropriate, and consistent mesh/collision updates when parts are removed. Reduce over-dense placement and improve canopy readability, trunk taper, and bark continuity without coupling gameplay data to placeholder textures.

### Flora system: planned capabilities

- **Species definitions:** form, habitat envelope, climate and moisture tolerance, soil needs, growth form, mature size, seasonal behavior, reproduction, and harvestable products.
- **Deterministic ecological placement:** use the world seed and stable IDs alongside biome, climate, elevation, slope, soil, water proximity, and local competition. Similar conditions should produce coherent communities, not uniform scatter or trees everywhere.
- **Lifecycle and persistent instances:** track growth stage, age, health, seasonal state, harvested parts, seeds, and other state only where needed. Stable IDs must survive chunk streaming so harvesting and regrowth remain consistent.
- **Different representations, shared rules:** detailed hexel parts can represent trunks, branches, roots, or other gameplay-relevant structures; lightweight geometry can represent grass, flowers, and fine foliage. Rendering detail must not dictate resource accounting or plant identity.
- **Resource and ecology loops:** make yields finite per instance, define regrowth and propagation conditions, and account for soil nutrients, moisture, seasons, decomposition, and disturbance as those systems arrive.
- **Vintage Story as a design reference:** study its breadth of useful wild plants, environmental requirements, growth stages, seasons, farming, fruiting, and resource uses. Use it as inspiration for systemic depth, not as a direct content or implementation copy.
- **Planet-scale world and rendering research — PlanetSmith:** use [PlanetSmith's Steam news/devlog archive](https://steamcommunity.com/app/2539340/allnews/) as a technical reference for adjacent problems in planetary/world generation, rendering architecture, and performance optimization. Start with the *New World Generation DevLog*, *Devlog — Rendering Without Meshes*, and *DevLog 12 — Optimizing Data* where available. Extract documented techniques and compare them against Hexel's finite spherical-world goal, pure hexagonal-prism geometry, Rust/`wgpu` renderer, and streamed chunk architecture. Treat these as study topics—not proof that PlanetSmith's methods are directly compatible—and record findings before adopting any approach. This is a reference/research task, not a claim that PlanetSmith technology is integrated.
- **Procedural tree-generation research — Shrubbery:** evaluate [TanTanDev's GitHub profile](https://github.com/TanTanDev) and the [TanTanDev/shrubbery repository](https://github.com/TanTanDev/shrubbery) as technical references for deterministic procedural vegetation. Investigate its space-colonization/attractor-based branching, species-specific shapes (including conifer forms), serializable growth recipes, and voxelization output. Build a Hexel-native proof of concept (initially a seeded oak and pine) and compare it with the current tree generator before choosing what to adapt, depend on, or reimplement. Do not pull in the Bevy integration merely to use the growth ideas; keep Hexel's renderer, plant data, targeting, harvesting, collision, and persistence authoritative. Validate generated geometry, performance, deterministic output, per-part interaction, and license/attribution requirements before wider adoption. This is a research/integration task, not a claim that Shrubbery is already integrated.
- **Eco: Global Survival as a systems reference:** study how ecological changes, resource extraction, production, and environmental consequences can connect across scales. Use that inspiration to make local flora and resource decisions matter to soils, habitats, water, and long-term planetary health, while keeping the simulation staged and performant rather than attempting to simulate everything at once.

### Planet and world simulation

6. **Define planetary geometry and scale:** choose the planet radius and a spherical surface partition compatible with pure hexagonal prisms.
7. **Establish planet-relative coordinates:** make latitude, longitude, spherical distance, region identity, and chunk addressing consistent across the engine.
8. **Build climate and hydrology in stages:** start from latitude and seasons, then incorporate elevation, continents, winds, rainfall, rivers, groundwater, and water circulation as supported by the simulation.
9. **Connect finite geology and soil to playable terrain:** generate regionally plausible strata and host-rock deposits; make mining deplete persistent finite quantities; connect soil nutrients and water availability to flora and farming.
10. **Scale simulation by distance:** keep nearby chunks detailed while representing distant and untouched regions compactly, without changing the world's underlying finite resource accounting.

This roadmap describes direction, not completed functionality. In particular, the current tree roster and tree-block gameplay model are foundations rather than a fully integrated flora system; do not treat flora collision, per-part targeting/harvesting, persistent plant state, seasonal growth, or regrowth as complete until they are connected end to end and tested.

## Requirements

- Rust stable toolchain
- A graphics driver supporting a backend enabled by `wgpu` on your platform.

## Build, test, and run

```sh
cargo fmt --all -- --check
cargo test
cargo run
```

The first run may take longer while Rust compiles the graphics dependencies and generates the world mesh.
