# ECS, Block Identity, and JSON Content Architecture — Case Study for Hexel Engine

**Research date:** 2026-10-22  
**Purpose:** Compare Vintage Story, Hytale, Eco, and TanTanDev's public Rust/Bevy repositories through three specific lenses: entity-component-system (ECS) architecture, block identity, and JSON-driven content. Translate the useful patterns into a practical architecture for Hexel Engine's finite spherical world.

**Scope and evidence notes:** This is a design study based on public official documentation, community documentation, and repository pages. Vintage Story and Hytale are proprietary games with documented modding/asset systems; Eco exposes a public modding API and ModKit but is not an open-source engine. TanTanDev's repositories are source-code examples, but this study does not claim to have cloned, built, benchmarked, or audited every file. Hytale ECS details below draw on community documentation, while its asset-format claims draw on official Hytale documentation.

## Executive recommendation

Hexel should use a **hybrid architecture** rather than making every terrain block an ECS entity:

1. **Terrain cells live in compact chunk/patch storage** as block-type IDs plus small per-cell state. For the eventual globe, a cell is addressed by a stable topology-owned cell ID, not by a flat XYZ coordinate alone.
2. **Block definitions are data-driven content assets.** Author them using readable, namespaced string IDs; validate and resolve those strings at load time to compact runtime numeric IDs for hot terrain storage.
3. **Placed objects and active actors use ECS entities.** Trees with persistent growth state, animals, dropped items, machines, and simulation actors are good candidates. Static terrain cells usually are not.
4. **Components hold independent data; systems own behavior.** Examples: Transform, Health, Inventory, PlantGrowth, Temperature, Moisture, and Movement. Avoid a giant inheritance tree and avoid placing every environmental field on every entity.
5. **JSON defines content and parameters, not arbitrary executable logic.** JSON should reference registered behavior/system types by stable key; Rust implements and validates those behaviors.
6. **Save files store stable semantic identity and authoritative state**, not runtime numeric registry slots, mesh indices, ECS archetype positions, or entity handles.
7. **World topology is authoritative.** ECS, block definitions, fields, rendering, and raycasting query a topology API; none should hardcode six planar neighbours as the universal rule.

This combines Vintage Story's asset/variant/behavior configuration, Hytale's JSON asset inheritance and ECS separation, Eco's explicit server/client name contracts, and Bevy-style ECS plus measurable rendering experiments.

---

## 1. The three concepts Hexel must keep separate

These terms sound similar but solve different problems.

| Concept | Example | What it identifies |
|---|---|---|
| Block type ID | `hexel:basalt` | The definition shared by all basalt cells |
| Runtime block index | `u16 = 17` (illustrative only) | A compact slot in the currently loaded registry |
| World cell ID | A stable topology-specific key | One particular cell on the spherical surface or in a volume |
| Cell state | Moisture 0.42, damage 3, variant flags | Mutable data for that particular cell |
| ECS entity ID | An opaque generational handle | One live entity in the ECS world |
| Persistent entity ID | UUID or stable save ID | The same actor/object across save and load |
| Asset ID | `hexel:oak_tree` | A definition or prefab/template, not one placed tree |

**Rule:** Never use a block type ID to identify a cell; never use a cell ID as an entity handle; never serialize an ephemeral runtime registry slot as the only identity for saved content.

A block may have many instances. A cell may change block type. A tree definition may produce many individual tree entities, each with its own persistent growth state.

## 2. Vintage Story — JSON assets, variants, behaviors, and block entities

### Sources

- [Official-style Vintage Story JSON reference](https://apidocs.vintagestory.at/json-docs/)
- [RegistryObjectType reference](https://apidocs.vintagestory.at/json-docs/jsondocs/Vintagestory.ServerMods.NoObf.RegistryObjectType.html)
- [BlockType reference](https://apidocs.vintagestory.at/json-docs/jsondocs/Vintagestory.ServerMods.NoObf.BlockType.html)
- [EntityType reference](https://apidocs.vintagestory.at/json-docs/jsondocs/Vintagestory.ServerMods.NoObf.EntityType.html)
- [Block type modding guide](https://wiki.vintagestory.at/Modding%3AAsset_Type_-_BlockTypes)
- [Entity asset modding guide](https://wiki.vintagestory.at/Modding%3AAsset_Type_-_Entities/en)
- [Official modding API update on tags](https://www.vintagestory.at/forums/topic/20897-collectible-entity-tags-from-1220/)

### What it does

Vintage Story's documented asset hierarchy has `RegistryObjectType` as a base for block types, item types, and entity types. It provides a `code`, optional class binding, and variant configuration. BlockType extends CollectibleType, which in turn extends RegistryObjectType. This means common identity and variant rules can be shared while blocks and items add their own fields.

Block types and entity types are authored as JSON assets. JSON can attach behavior definitions; more complex per-placed-block state can use a block entity and block-entity behaviors. The docs explicitly distinguish a shared block class/type from a block entity that holds data for one placed block.

### ECS perspective

Vintage Story's public modding model is **not evidence that the entire engine is a conventional ECS**. The useful pattern here is composable behavior and the separation of a shared type definition from per-instance state. Do not label every behavior attachment as ECS.

### Block IDs and variants

- A registered object's `code` is a semantic code and prefix for variant codes.
- Variant groups can expand a base definition into resolved variants such as material, shape, or stage combinations.
- Namespaced asset locations avoid collisions between mods.
- A resolved registry object has a runtime identity, but save data should not depend on an arbitrary load-order numeric index as its sole meaning.

### JSON lesson

Inheritance of documented JSON properties reduces repetition; explicit variants let a definition describe a family of related assets. Patch assets allow mods to modify content without directly replacing the original definition. However, inheritance and variant expansion can create a large content surface, so validation and clear rules for overrides are essential.

### Hexel takeaways

- Use stable namespaced IDs, e.g. `hexel:basalt`, `hexel:loam`, `hexel:oak_sapling`.
- Allow a block definition to reference registered behaviors such as `mineable`, `supports_plants`, or `fluid_permeable`.
- Distinguish shared definition data from per-cell data and from optional placed-object state.
- Support a small variant model where genuinely useful, but avoid encoding continuous simulation values (moisture, temperature, damage) as thousands of content variants.
- Consider mod patches/overrides after a minimal loader and validation system exist.

---

## 3. Hytale — official JSON assets plus an ECS documented by community references

### Sources

- [Official Hytale documentation](https://docs.hytale.com/)
- [Official asset reference: blocks](https://docs.hytale.com/assets/item/block/blocks/)
- [Official asset reference: items](https://docs.hytale.com/assets/item/items/)
- [Official Hytale block definitions overview](https://github.com/hytale-moddings/hytale-modding-docs/blob/main/src/content/docs/reference/item-system/block-definitions.md)
- [Community Hytale ECS theory guide](https://github.com/HytaleModding/site/blob/main/content/docs/en/guides/ecs/hytale-ecs-theory.mdx)

### What it does

The official docs describe JSON assets and an asset reference generated from the server's codecs. Block-related content can inherit from a parent definition; tags are interpreted by systems, and block definitions can describe material, draw type, collision, interactions, and other behavior-facing configuration. Some block types carry block-entity components for stateful features such as farming, storage, crafting, respawn, or portals.

The community ECS guide describes Hytale's `Store` and archetype-based organization of entities, `EntityStore` and `ChunkStore`, entity references, and the distinction between a persistent/network identity and a temporary in-store handle. This is useful technical documentation, but it is community-authored rather than the official asset reference; verify against the installed version's Java API when implementing a Hytale plugin.

### ECS perspective

The most transferable idea is the split between **archetype-organized live entities** and **chunk-oriented block/world data**. The guide describes block-related components being held through the chunk store, rather than implying every block in the terrain must be a normal actor entity. It also describes safe entity references instead of keeping arbitrary direct references to potentially deleted entities.

### Block IDs and JSON

- Block definitions use named asset references, parents, and tags.
- Tags express capabilities/categories for systems to interpret rather than forcing all logic to test one exact block name.
- JSON inheritance reduces duplication, but inherited/nested merge rules must be explicit and testable.
- Block entities/components are appropriate for blocks that need unique state (e.g. a storage block with contents), not necessarily for every ordinary terrain cell.

### Hexel takeaways

- Add tags/capabilities such as `material=stone`, `tool:pickaxe`, `supports:soil`, `blocks:movement`, and `render:opaque` where those concepts help multiple systems share rules.
- Keep the source definition readable and inherited from a small number of base templates; flatten it into a validated runtime definition at load time.
- Use an ECS for live actors and stateful placed objects, with generational handles and stable persistent IDs.
- Keep chunk/patch terrain in packed cell arrays. A spherical patch or cell graph should own adjacency and stable cell identity.
- If a block has unique inventory, machine state, or growth progress, represent that state in a dedicated component/state record linked to its cell, rather than adding a heavy object to every terrain cell.

---

## 4. Eco — explicit server/client contracts and content types

### Sources

- [Eco modding overview](https://wiki.play.eco/en/Modding)
- [Eco ModKit repository](https://github.com/StrangeLoopGames/EcoModKit)
- [Eco ModKit README](https://github.com/StrangeLoopGames/EcoModKit/blob/main/README.md)
- [Eco guide: modding beyond items and statues](https://wiki.play.eco/en/Modding_Eco_beyond_items_and_statues)
- [Eco guide: modifying the world](https://wiki.play.eco/en/Modifying_the_World)
- [Eco guide: creating custom blocks](https://wiki.play.eco/en/Creating_Custom_Blocks)

### What it does

Eco's public modding documentation describes a server/client split. Server-side C# types own gameplay rules such as mining, drops, plant growth, and animal diets. The Unity ModKit supplies client-side art, block sets, prefabs, and related visual content. These halves are joined by exact naming conventions. A mismatch can cause a block or object to be invisible, even when the server-side type exists.

Eco content is often defined through C# types, attributes, registries, and configuration rather than one universal JSON block-definition file. The ModKit has examples for block sets, items, plants, trees, animals, and world objects. The public documentation also warns that modding APIs change between game versions.

### ECS perspective

Eco's public modding interface should **not be treated as proof of a pure ECS architecture**. Its useful lesson for Hexel is explicit ownership boundaries: gameplay authority, client representation, world generation, and content registration must agree on identifiers.

### Block identity lesson

The server's block type and the client's rendering asset are distinct things joined by a contract. For Hexel, a block's semantic ID should resolve to a definition and rendering/material data through explicit references, not by relying on filenames or fragile naming coincidences. Validate missing references at load time rather than allowing invisible blocks or silent fallback behavior.

### JSON lesson

Even where core gameplay types are code-defined, Eco's world-generation and content configuration demonstrates that definitions often span several linked resources. A single block may involve a type, item, drops/rubble, recipe, world-generation rule, and client block set. Hexel should model these as linked assets with validation rather than one enormous block JSON.

### Hexel takeaways

- Establish clear ownership for authoritative world state versus rendering assets.
- Validate every referenced block ID, material, texture, behavior, recipe, and world-generation rule during content loading.
- Produce actionable errors for missing definitions and duplicate IDs.
- Keep gameplay simulation independent of the renderer so a future backend change doesn't rewrite block rules.
- Version content schemas and provide migrations when save formats or IDs change.

---

## 5. TanTanDev's repositories — Rust/Bevy and the case for keeping terrain compact

### Sources

- [TanTanDev GitHub profile](https://github.com/TanTanDev)
- [Binary greedy mesher demo](https://github.com/TanTanDev/binary_greedy_mesher_demo)
- [First voxel engine](https://github.com/TanTanDev/first_voxel_engine)
- [Worley biomes](https://github.com/TanTanDev/worley_biomes)
- [Shrubbery](https://github.com/TanTanDev/shrubbery)
- [3D cellular automata](https://github.com/TanTanDev/3d_celluar_automata)
- [Bevy ECS documentation](https://bevyengine.org/learn/quick-start/getting-started/ecs/)

### What the repositories contribute

- **Binary greedy mesher demo:** a Rust/Bevy project focused on meshing and benchmarking a culled mesher against a binary greedy mesher. Its main lesson is to measure derived geometry and keep meshing a separate concern.
- **First voxel engine:** a small Rust/wgpu learning project for endless voxel generation. It is explicitly described as unoptimized, so use it as an end-to-end learning example, not as a performance target.
- **Worley biomes:** a separate noise/biome-classification library with optional visualization/integration. It supports treating classification as a module that consumes field inputs.
- **Shrubbery:** a separate procedural vegetation generator. Its useful boundary is between generated plant structure and how the world stores, simulates, and renders that structure.
- **3D cellular automata:** a configurable simulation example. It is useful for local rule systems and visualization, but should not be assumed to conserve water or other quantities without proof.

### ECS perspective

Bevy provides an explicit ECS model, making it a useful reference for separating component data from systems that query and update it. The repositories also demonstrate why not all data belongs in ECS entities: a voxel/terrain grid has regular dense storage and specialized meshing needs. Creating an ECS entity for every terrain cell can add substantial per-object overhead and complicate neighbourhood/mesh operations compared with packed arrays.

The correct design depends on workload. Dynamic actors and sparse stateful objects fit ECS well; regular terrain data often fits chunked arrays and specialized field storage better.

### Block identity lesson

The public repositories are most useful for their meshing, noise, and vegetation boundaries, not as a canonical block-ID schema. Hexel should not infer an ideal persistent ID scheme from a demo project without checking its actual world storage and save/load behavior. Use semantic IDs at content boundaries and compact indices in runtime arrays.

### Hexel takeaways

- Keep meshing as a pure or mostly pure transformation from authoritative terrain data to render buffers.
- Benchmark exposed-face meshing and alternatives against equivalent geometry.
- Keep biome classification and vegetation generation behind narrow APIs.
- Use the ECS for sparse objects and simulation actors, not as a substitute for the topology and terrain data model.
- Keep all systems topology-agnostic: neighbour queries come from the spherical cell graph, not from fixed planar directions.

---

## 6. Recommended Hexel architecture

### A. Content registry and IDs

Each content definition has a stable namespaced string ID, e.g. `hexel:basalt`. At startup, the loader validates definitions and maps IDs to compact runtime indices:

- **Persistent/content identity:** `hexel:basalt`
- **Runtime storage index:** compact integer assigned by the loaded registry; never persisted as the sole identity
- **Cell identity:** stable topology-specific key for one spherical cell
- **Entity identity:** runtime generational handle for ECS access
- **Persistent entity identity:** stable UUID/ID saved to disk

If a save refers to an unknown block ID, report it and use a defined migration/fallback policy. Do not silently reinterpret a saved numeric index after registry order changes.

### B. Terrain storage versus ECS

Recommended starting split:

| Data | Representation | Why |
|---|---|---|
| Block type in each terrain cell | Packed chunk/patch array of runtime block indices | Dense, cache-friendly, fast meshing |
| Small cell state | Compact side arrays or sparse cell-state map | Avoids paying for every field on every cell |
| Cell adjacency and surface geometry | Topology API / cell graph | Spherical patches and exceptional cells need not mimic a flat grid |
| Animals, dropped items, players | ECS entities | Dynamic behavior and independent components |
| Trees/plants with growth state | ECS entities or sparse plant records keyed by stable cell ID | Choose based on density, interaction frequency, and state complexity |
| Machines, containers, special blocks | Sparse component/state record linked to cell ID; optionally ECS entity | Unique state without inflating all cells |
| Meshes, colliders, render caches | Derived data | Rebuild from authoritative state |

Do not begin with an elaborate ECS plugin system. First establish the data boundaries and stable IDs, then adopt a Rust ECS library if it meets the prototype's needs.

### C. Proposed block JSON

This is a **proposed Hexel schema**, not JSON copied from any of the games:

```json
{
  "schemaVersion": 1,
  "id": "hexel:basalt",
  "displayName": "Basalt",
  "tags": ["material:stone", "rock:igneous", "tool:pickaxe"],
  "render": {
    "material": "hexel:rock_basalt",
    "surface": "opaque",
    "mergeGroup": "stone"
  },
  "physics": {
    "collision": "solid",
    "blocksMovement": true,
    "porosity": 0.02
  },
  "mining": {
    "requiredToolTag": "tool:pickaxe",
    "hardness": 1.4,
    "drops": [{ "id": "hexel:basalt", "quantity": 1 }]
  },
  "behaviors": [],
  "stateSchema": null
}
```

Recommended properties:
- `schemaVersion`: enables explicit migrations.
- `id`: stable, namespaced semantic identity.
- `tags`: reusable categories/capabilities for recipes, mining, ecology, and world generation.
- `render`: references to render assets; not authoritative simulation state.
- `physics`: semantic collision and physical properties.
- `mining`: interaction rules and drop references.
- `behaviors`: references to known, registered Rust behavior types with validated parameters; no arbitrary code execution from JSON.
- `stateSchema`: optional declaration of extra per-cell or per-object state, used only when needed.

### D. Proposed entity archetype

This is also a Hexel design sketch, not a literal format from these games:

```json
{
  "schemaVersion": 1,
  "id": "hexel:wild_oak",
  "kind": "entity_template",
  "components": {
    "Transform": { "orientation": "surface_normal" },
    "PlantGrowth": {
      "species": "hexel:oak",
      "growthRate": 0.01,
      "requiresLight": true
    },
    "EnvironmentalNeeds": {
      "temperatureRange": [-5.0, 30.0],
      "minimumMoisture": 0.25,
      "soilTags": ["soil:loam", "soil:forest_floor"]
    }
  }
}
```

At runtime the loader resolves component names to registered types and creates typed component values. A schema should reject unknown components, wrong field types, out-of-range values, and missing asset references before the world starts.

### E. Avoid one mega-JSON file

Separate files by purpose:
- `content/blocks/*.json` — block type definitions
- `content/materials/*.json` — visual/material properties
- `content/entities/*.json` — entity templates and component defaults
- `content/behaviors/*.json` — parameters for registered behavior types, if needed
- `content/biomes/*.json` — biome classification and suitability settings
- `content/worldgen/*.json` — generation rules and weighted selections
- `content/recipes/*.json` — crafting/transformation rules
- `schemas/*.json` — machine-readable schemas
- save data — authoritative world and per-instance state, kept distinct from definitions

References between assets should always use stable IDs. A loader should build an index and validate the entire reference graph.

---

## 7. Block ID design rules

1. **Use namespaced semantic IDs** such as `hexel:basalt` and `modname:crystal_ore`.
2. **Never promise that a runtime integer remains stable across versions or mods.** Assign it at load time and translate stable IDs at save/load boundaries.
3. **Separate type from state.** Wet basalt, damaged basalt, and a basalt cell with temperature are not automatically separate block types. Use variants only when they are genuinely distinct content/rendering states.
4. **Separate block state from entity state.** A chest block type is shared; its inventory belongs to that placed chest instance.
5. **Use tags for cross-system rules**, but keep important properties typed. A mining system should not parse an arbitrary string tag when a validated hardness/tool field is more appropriate.
6. **Version IDs and migrations.** If `hexel:basalt_old` becomes `hexel:basalt`, ship an explicit save/content migration.
7. **Stable spherical cell IDs are independent.** The cell's identity must survive rendering LOD changes and mesh rebuilds; topology redesign requires explicit version/migration handling.
8. **Validate collisions and references early.** Duplicate IDs, missing materials, missing drops, unknown behavior keys, and invalid component values should fail with useful diagnostics.

---

## 8. ECS design rules for Hexel

### Good ECS candidates

- Actors: players, animals, moving objects, projectiles.
- Sparse interactive objects: dropped items, containers, machines, portals.
- Plants/trees that require persistent growth, health, reproduction, or interaction.
- Temporary runtime objects: particles with gameplay meaning, scheduled agents, pathfinding actors.

### Usually poor ECS candidates

- Every ordinary static terrain cell.
- Every derived vertex or mesh face.
- Every temperature/moisture sample if stored as a dense global field.
- Every render-only detail that can be generated from a deterministic seed and nearby terrain.

### Suggested starter components

- `Transform` / `SurfaceAttachment` (position/orientation relative to spherical surface)
- `Velocity` / `Movement`
- `Health` / `Damageable`
- `Inventory`
- `PlantGrowth`
- `EnvironmentalNeeds`
- `Temperature` and `Moisture` only where sparse per-entity state is appropriate
- `PersistentId` for save identity
- `CellAttachment` for objects anchored to a stable topology cell

Systems should be responsible for movement, growth, damage, inventory interaction, and other behavior. Systems that change world cells should issue validated world edits, so the world can invalidate relevant meshes, fields, and caches.

### Keep finite hydrology out of a naive entity-only model

For water, soil moisture, heat, and climate, use field/chunk arrays or a dedicated sparse field representation with explicit units and conservation tests. ECS actors may read and influence those fields, but the existence of an ECS does not make it the best container for every scalar on every cell.

---

## 9. Implementation roadmap

### Phase 1 — Identity and registry
- Define `ContentId`, `BlockTypeId`, `CellId`, `EntityHandle`, and `PersistentEntityId` as distinct Rust types.
- Implement a block registry from JSON string IDs to compact runtime indices.
- Add tests for duplicate IDs, missing references, stable save/load identity, and unknown IDs.

### Phase 2 — JSON schema and validation
- Add a JSON parser and a schema/version validation layer.
- Load one block definition (basalt) and one material definition.
- Resolve references and fail loudly on invalid content.
- Keep code-defined defaults explicit rather than silently filling missing required values.

### Phase 3 — Terrain data versus ECS
- Store terrain cells in packed arrays indexed by topology/chunk structures.
- Add a single ECS entity for a moving test actor and a stateful chest/tree.
- Demonstrate that changing a cell rebuilds derived mesh data without creating an ECS entity for every cell.
- Use a stable persistent ID for stateful objects.

### Phase 4 — Content-driven systems
- Implement one registered behavior such as mineable or plant-supporting.
- Add tags for broad categories and typed fields for numeric/physical properties.
- Build a validation report for all loaded definitions.

### Phase 5 — Spherical topology integration
- Make cell lookup, adjacency, surface normals, and cell identity come from the topology module.
- Keep ECS entities attachable to stable cells/local tangent frames.
- Test cell edits, entity attachment, and save/load across patch seams and exceptional cells.

### Phase 6 — Fields and simulation
- Add a temperature/moisture field independent of block definitions and ECS storage.
- Let plants consume those fields through a system.
- Add conservation/invariant tests before building finite water transfer.
- Keep render meshes and debug visualizations derived from the authoritative world/field state.

---

## 10. Source list and limitations

### Primary and official sources
1. Vintage Story JSON reference: https://apidocs.vintagestory.at/json-docs/
2. Vintage Story RegistryObjectType: https://apidocs.vintagestory.at/json-docs/jsondocs/Vintagestory.ServerMods.NoObf.RegistryObjectType.html
3. Vintage Story BlockType: https://apidocs.vintagestory.at/json-docs/jsondocs/Vintagestory.ServerMods.NoObf.BlockType.html
4. Vintage Story EntityType: https://apidocs.vintagestory.at/json-docs/jsondocs/Vintagestory.ServerMods.NoObf.EntityType.html
5. Vintage Story tags update: https://www.vintagestory.at/forums/topic/20897-collectible-entity-tags-from-1220/
6. Official Hytale documentation: https://docs.hytale.com/
7. Hytale block asset reference: https://docs.hytale.com/assets/item/block/blocks/
8. Hytale item asset reference: https://docs.hytale.com/assets/item/items/
9. Eco modding overview: https://wiki.play.eco/en/Modding
10. Eco ModKit: https://github.com/StrangeLoopGames/EcoModKit
11. Eco ModKit README: https://github.com/StrangeLoopGames/EcoModKit/blob/main/README.md
12. Eco block/world modding: https://wiki.play.eco/en/Modifying_the_World
13. TanTanDev repositories: https://github.com/TanTanDev
14. Binary greedy mesher demo: https://github.com/TanTanDev/binary_greedy_mesher_demo
15. First voxel engine: https://github.com/TanTanDev/first_voxel_engine
16. Worley biomes: https://github.com/TanTanDev/worley_biomes
17. Shrubbery: https://github.com/TanTanDev/shrubbery
18. 3D cellular automata: https://github.com/TanTanDev/3d_celluar_automata
19. Bevy ECS overview: https://bevyengine.org/learn/quick-start/getting-started/ecs/
20. Hytale community ECS guide: https://github.com/HytaleModding/site/blob/main/content/docs/en/guides/ecs/hytale-ecs-theory.mdx

### Limitations

- This is not a source-code audit of Vintage Story, Hytale, or Eco's proprietary engines.
- The Hytale ECS discussion uses community-written technical documentation; official Hytale docs are the source for JSON asset formats. Details may change with game versions.
- Eco's modding API and documentation are evolving; follow the version targeted by a future Hexel comparison.
- TanTanDev's projects demonstrate particular implementations, not a universal recommended ID or save format. Verify exact code paths before using them as implementation references.
- The JSON examples above are Hexel proposals, not the exact schema of any reference game.

## Final recommendation

For Hexel, the strongest design is **data-driven block definitions + stable semantic IDs + compact terrain storage + ECS for dynamic/stateful objects + independent field simulation**. Use the games as references for their individual strengths, not as blueprints to copy wholesale. Most importantly, make spherical topology and persistent identity foundational before expanding the content system.
