# Hexel Engine

A native Rust prototype for a first-person, fully 3D hexagonal-prism voxel world.

## Current prototype

The app opens a native window and renders a small, seeded terrain patch built from real hexagonal-prism geometry using `wgpu`. A free-fly first-person camera is included so the terrain can be inspected from different angles.

### Controls

- **W / A / S / D** — fly through the world
- **Mouse** — look around (captured on launch)
- **Space / Shift** — fly up / down
- **Esc** — release or recapture the mouse
- **Left click** — recapture the mouse when released
- **Close window** — quit

This is an early visual slice, not a complete game yet. Movement is free-fly: gravity, terrain collision, block selection/editing, and chunk streaming are not implemented.

## Implemented core

- Axial hex coordinates `(q, r)` plus vertical layer `y`.
- Chunk storage sized `16 × 16 × 16`.
- Grass, dirt, stone, air, and foundation block types.
- Seeded Perlin terrain-height generation.
- Shared player-to-hexel height scale (`3/5`).
- Ray/prism intersection math and unit tests.
- Native `winit` window and `wgpu` renderer with depth testing.

## Requirements

- Rust stable toolchain
- A graphics driver supporting a backend enabled by `wgpu` on your platform.

## Build, test, and run

```sh
cargo fmt --all -- --check
cargo test
cargo run
```

The first run may take longer while Rust compiles the graphics dependencies.

## Next milestones

1. Verify the rendered terrain on supported Windows GPUs and tune visibility/camera defaults.
2. Add solid-body collision and grounded first-person movement.
3. Add target highlighting, block breaking, and placement.
4. Move terrain mesh generation into streamed chunks and add distance-based LOD.

A successful compile is not treated as proof of interactive gameplay; the rendering slice should be inspected on the target machine.
