# Hexel Engine

A native Rust prototype for a first-person, fully 3D world built from hexagonal-prism voxels (“hexels”).

Hexel Engine's long-term goal is a **finite, spherical world** with coherent planetary geography, system-driven climate, and strict resource conservation. The current build is an early flat-world prototype; the planetary systems described below are design goals, not features claimed to be implemented.

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

## Design roadmap

1. **Define planetary geometry and scale:** choose the planet radius and a spherical surface partition compatible with pure hexagonal prisms.
2. **Establish planet-relative coordinates:** make latitude, longitude, spherical distance, region identity, and chunk addressing consistent across the engine.
3. **Unify geometry queries:** ensure rendering, raycasting, placement, and collision agree on hexel geometry and cell identity.
4. **Create persistent world data:** separate deterministic generated terrain from saved edits and depleted resource inventories.
5. **Build climate and hydrology foundations:** derive baseline climate from latitude and seasons, then add the effects of elevation, continents, winds, rainfall, and water circulation in stages.
6. **Add finite geology and ecosystems:** define material deposits, water inventories, soil nutrients, biomass, and explicit renewal rules.
7. **Scale simulation by distance:** keep nearby chunks detailed while representing distant and untouched regions compactly.

This roadmap describes direction, not completed functionality.

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
