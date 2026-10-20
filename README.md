# Hexel Engine

A small Rust prototype for a playable 3D world built from stacked hexagonal-prism cells ("hexels").

## Status

This repository started empty. The first milestone is a deliberately small playable sandbox: walk around a generated hexel landscape, look around with the mouse, and add/remove hexel columns.

## Requirements

- Rust stable (install with [rustup](https://rustup.rs/))
- A desktop graphics environment supported by [macroquad](https://github.com/not-fl3/macroquad)

## Run

```sh
cargo run
```

Controls:

- `W A S D`: move
- Mouse: look around after capturing it with right-click; press `Esc` to release
- `Space` / `Left Shift`: move up / down (prototype free-fly movement)
- Left click: remove the targeted top hexel
- `E`: add a hexel to the targeted column
- Right click: capture the mouse (while released)
- `R`: regenerate the demo terrain
- `Esc`: release mouse capture

## Design assumptions

- **Hexel** means a vertical hexagonal prism, with a discrete axial hex-grid position `(q, r)` and integer height `y`.
- Hexels are rendered as individually colored prism meshes; columns share a regular pointy-top hex grid.
- The initial goal is a playable desktop prototype, not yet a production engine or editor.
- The first implementation favors simple, readable CPU-generated geometry over chunk meshing, streaming, physics, or GPU instancing.
- Movement is free-fly rather than collision-based so the world and editing loop are immediately testable.

## Project layout

- `src/main.rs` — window, camera, input, demo terrain, and rendering
- `src/hexel.rs` — axial grid coordinates and hex-prism mesh generation
- `src/world.rs` — sparse column world and editing operations

## Validation

Run `cargo fmt --check` and `cargo test` before committing changes. `cargo run` launches the interactive prototype. The GitHub Actions workflow checks formatting, tests, and compilation on pushes and pull requests.

## Roadmap

1. Chunked mesh generation and dirty-chunk rebuilds
2. Solid collision and grounded movement
3. Save/load world format
4. Block/hexel materials and lighting improvements
5. Optional in-engine editor

## License

No license has been selected yet. Add one before redistributing the project.
