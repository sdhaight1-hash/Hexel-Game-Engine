# Hexel Engine

A native Rust prototype for a first-person, fully 3D hexagonal-prism voxel world.

## Playable prototype

The current build generates **32 terrain chunks (8 × 4 chunks, each 16 × 16 hexel columns)** from a random seed and opens a native first-person sandbox. The seed is printed to the terminal at startup so you can record a particular world.

### Controls

- **W / A / S / D** — walk
- **Mouse** — look around
- **Space** — jump (gravity pulls you back down)
- **Left click** — remove the targeted hexel
- **Right click** — place a grass hexel on the targeted surface
- **Esc** — release or recapture the mouse
- **Close window** — quit

Terrain faces use darker side shading and lighter top faces to make the hexagonal-prism shape easier to read. This is still an early prototype: the player uses a simple capsule-like collision footprint, terrain meshes rebuild after each edit, and this pass uses baked face shading rather than real-time shadow maps. Terrain collision and block editing are first-pass implementations.

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
