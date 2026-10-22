# Case Study: Vintage Story World Data Structures

**Project:** Hexel Engine  
**Status:** Research notes and design implications — not an implementation specification  
**Primary question:** What can Hexel learn from a mature voxel game's separation of block storage, lighting, entities, summaries, and invalidation?

## Executive summary

Vintage Story's public API describes a chunk system with several distinct data layers rather than one all-purpose array:

- **Block data** stores the authoritative block IDs for 3D positions, with palette-aware operations.
- **Lighting** is stored separately from block IDs.
- **Light-source indices, block entities, and entities** have their own structures.
- **Map-chunk data** stores 2D column summaries such as height and surface information.
- **Mutation APIs** can mark chunks as needing a save, while rendering and other derived data have their own update requirements.

The main lesson for Hexel is architectural: keep authoritative world occupancy separate from derived caches and from other systems' data. A world edit should trigger the specific work that depends on it, not force every subsystem to rebuild.

## 1. How the data is organized

### 1.1 World chunks and block storage

The public `IWorldChunk` API exposes an `IChunkBlocks` block-data object. The documented 3D linear index is:

```text
index = (y * chunkSize + z) * chunkSize + x
```

This is an indexing convention for a rectangular voxel chunk. Hexel uses axial horizontal coordinates `(q, r)` plus vertical layer `y`, so it should not copy this formula without defining its own chunk-local axial mapping.

The `IChunkBlocks` API includes indexed access, block-ID reads, fluid reads, palette-aware queries such as `ContainsBlock`, and mutation/bulk operations. Some methods have explicit safety constraints: for example, `SetBlockUnsafe` is not thread-safe and expects the palette to have been initialized.

### 1.2 Palette-aware storage

The API exposes palette-aware block operations rather than requiring every query to scan every cell. This can support efficient checks such as whether a chunk contains a given block ID.

**Evidence boundary:** the public API does not, by itself, establish the exact in-memory bit layout, packing scheme, or memory saved by each palette representation. Hexel should treat palette-indexed storage as a candidate to benchmark, not assume a particular implementation is optimal.

### 1.3 Separate world data by responsibility

| Data | Public API representation | Why the separation matters |
|---|---|---|
| 3D block IDs | `IChunkBlocks` | Authoritative block content |
| Lighting | `IChunkLight` | Lighting can change without changing block IDs |
| Light-source positions | `HashSet<int>` of in-chunk indices | Supports locating light sources |
| Block entities | `Dictionary<BlockPos, BlockEntity>` | Rich state belongs only to special blocks |
| Entities | Entity array plus count | Moving actors are not ordinary terrain cells |
| Map/column summaries | `IMapChunk` | Useful 2D data without replacing 3D terrain |
| Mod/runtime data | `LiveModData` and related APIs | Runtime-only state can have a different lifecycle |

This separation is a useful model for Hexel: terrain occupancy, lighting, meshes, entities, and surface summaries should not be forced into one shared representation.

## 2. The 3D terrain and the 2D map are different things

Vintage Story's `IMapChunk` represents data for a chunk column and exposes values such as height maps, rain height, top-rock information, sediment thickness, snow accumulation, and maximum height.

These are **summaries**, not a replacement for the authoritative 3D block data. A surface height is useful for quick queries, but cannot describe caves, tunnels, arches, floating islands, or multiple disconnected surfaces.

### Hexel implication

Keep the full 3D set of occupied hexels authoritative. Add 2D or column-like summaries only as derived acceleration structures. They must be invalidated and rebuilt when edits make them stale, and they must never become the only source of truth for occupancy.

## 3. Editing, threading, and invalidation

The API's mutation surface shows that changing world data is more than writing an ID:

- `IBlockAccessor.SetBlock` can mark the containing chunk as needing a save.
- Some operations may require neighboring redraws when effects such as ambient occlusion are involved.
- Bulk and unsafe writes have constraints that callers must respect.
- `IWorldChunk` exposes read-locking support. The API warns that holding a blocking read lock can delay client rendering work such as tessellation.
- `MaybeBlocks` offers a faster non-blocking read path whose documented behavior can return an empty value when the actual block is not immediately available. It is a performance trade-off, not a general replacement for authoritative reads.

The lesson is to make mutation rules and thread-safety contracts explicit. Fast paths should clearly state whether they can return incomplete or approximate information.

### Recommended invalidation categories for Hexel

| Change | Likely work to invalidate |
|---|---|
| Occupancy or block type changes | Neighboring exposed-face meshes; collision data if present |
| Light source or opacity changes | Affected lighting region and meshes using light |
| Entity/block-entity state changes | Relevant entity or special-block systems |
| Surface edit | Derived height/surface summaries |
| Any persistent world edit | Save-dirty state |
| Chunk load/unload | Runtime caches and references tied to that chunk |

These categories are a proposed Hexel design, not a claim that Vintage Story uses these exact flags internally.

## 4. A small illustrative Hexel data model

The following is conceptual JSON for Hexel, **not Vintage Story's serialization format**:

```json
{
  "chunk": { "q": 0, "r": 0, "y": 0 },
  "cells": {
    "storage": "sparse-or-palette-indexed",
    "occupied": [
      { "q": 0, "r": 0, "y": 0, "block": "stone" },
      { "q": 0, "r": 0, "y": 1, "block": "stone" },
      { "q": 1, "r": 0, "y": 0, "block": "soil" }
    ]
  },
  "derived": {
    "mesh_dirty": true,
    "lighting_dirty": false,
    "surface_summary_dirty": true
  },
  "persistence": { "needs_save": true }
}
```

The important idea is not this exact schema. It is that authoritative cells, derived results, and persistence state are different concerns.

## 5. What Hexel should take from this

### Adopt

1. **Keep authoritative occupancy independent of rendering.** A mesh is a cache built from terrain, not terrain itself.
2. **Separate independent data domains.** Lighting, entities, surface summaries, and terrain have different access patterns and lifecycles.
3. **Make mutation side effects explicit.** An edit should mark the relevant data dirty in a predictable way.
4. **Use summaries as acceleration structures.** They help answer common questions quickly but must be rebuildable from the 3D world.
5. **Define safe and fast access paths separately.** If a fast path may return incomplete information, document that contract.

### Benchmark before adopting

| Candidate | Strength | Main risk |
|---|---|---|
| Sparse occupied-cell map/set | Naturally represents caves and empty space; simple to start with | Hash overhead and expensive full-region iteration |
| Dense chunk-local array | Fast predictable indexing and iteration | Allocates space for empty cells |
| Palette-indexed chunk | Can reduce repeated block-ID storage | More complex writes and palette promotion/demotion |
| Hybrid representation | Can fit different density patterns | More code paths, testing, and migration complexity |

Do not choose by intuition alone. Measure memory per chunk, lookup speed, edit speed, exposed-face generation, save/load cost, and correctness under cross-chunk edits.

## 6. Suggested Hexel experiments

1. Establish a correctness baseline for sparse occupancy and independent 3D cells.
2. Generate identical terrain fixtures using sparse and dense chunk-local storage.
3. Compare memory use for empty, mostly empty, uniform solid, and highly varied chunks.
4. Measure single-cell edits and mesh rebuild time, including edits on chunk boundaries.
5. Test caves, tunnels, arches, overhangs, and floating terrain to ensure no column-only assumptions remain.
6. Only then prototype palette-indexed storage and compare it against the baseline.

## 7. Sources and evidence limits

- [Vintage Story API repository](https://github.com/anegostudios/vsapi)
- [`IChunkBlocks`](https://apidocs.vintagestory.at/api/Vintagestory.API.Common.IChunkBlocks.html)
- [`IWorldChunk`](https://apidocs.vintagestory.at/api/Vintagestory.API.Common.IWorldChunk.html)
- [`IMapChunk`](https://apidocs.vintagestory.at/api/Vintagestory.API.Common.IMapChunk.html)
- [`IBlockAccessor`](https://apidocs.vintagestory.at/api/Vintagestory.API.Common.IBlockAccessor.html)
- [Vintage Story API license](https://github.com/anegostudios/vsapi/blob/master/license.txt)

This study relies on public API documentation. It does not claim access to private implementation details or establish exact memory footprints, internal bit packing, or the complete on-disk chunk format.
