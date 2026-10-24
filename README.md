# Hexel Engine

A native Rust prototype for a first-person, fully 3D hexagonal-prism voxel world.

## Status

This repository is being initialized around a small, testable vertical slice. The world uses axial coordinates `(q, r)` plus integer vertical layer `y`, with chunks sized `16 × 16 × 16`. Hexel height is derived from player capsule height (`3/5`).

## Planned implementation order

1. Coordinate and chunk model, deterministic terrain, prism geometry, and unit tests.
2. Native `wgpu` renderer using Vulkan where supported.
3. First-person controller, collision, raycasting, and block edits.
4. Chunk streaming and distance-based terrain LOD.

The implementation status and verification results will be updated as each stage lands. A successful compile is not treated as proof of interactive gameplay.

## Requirements

- Rust stable toolchain
- A native graphics driver with Vulkan support (or another backend enabled by the platform)

## Build and run

Once the native application stage is implemented:

```sh
cargo run --release
```

## Coordinate convention

- `q, r`: axial coordinates on a pointy-top hex grid.
- `y`: integer vertical layer.
- Chunk coordinates and local cell coordinates are separate.
- World-space geometry is derived from one shared scale configuration.
