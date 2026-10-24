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
- The main prototype uses CPU-generated, chunked meshes with dirty-chunk rebuilds; chunk size and mesh behavior are covered by tests.
- The initial goal is a playable desktop prototype, not yet a production engine or editor.
- Movement is currently free-fly rather than collision-based, keeping world editing easy to test.
- `hexel_vulkan` is an experimental standalone Vulkan rendering prototype. It is separate from the main Macroquad app and is not yet the default game renderer.

## Project layout

- `src/main.rs` — window, camera, input, demo terrain, and rendering
- `src/hexel.rs` — axial grid coordinates and hex-prism mesh generation
- `src/world.rs` — sparse column world and editing operations

## Validation

Run `cargo fmt --check` and `cargo test` before committing changes. `cargo run` launches the interactive prototype. The GitHub Actions workflow checks formatting, tests, and compilation on pushes and pull requests.

## Roadmap

- [x] Chunked mesh generation and dirty-chunk rebuilds
- [ ] Solid collision and grounded movement
- [ ] Save/load world format with versioned data
- [ ] Hexel materials and lighting improvements
- [ ] Optional in-engine editor
- [ ] Stabilize the experimental Vulkan renderer before considering it as a replacement for the main renderer

The Vulkan prototype can be launched separately with `cargo run --bin hexel_vulkan`. Its renderer is still under development; the default `cargo run` command launches the Macroquad prototype.

## License

No license has been selected yet. Add one before redistributing the project.
