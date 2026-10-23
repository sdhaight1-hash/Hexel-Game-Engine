# Vulkan renderer migration

## Current checkpoint

The Macroquad executable remains the working reference implementation. The standalone `hexel_vulkan` binary currently initializes a winit window, Vulkan instance/device, surface, swapchain, image views, command buffers, and synchronization primitives. It does **not** yet record rendering commands or submit/present frames, and it does not draw the Hexel world.

Keep the existing Macroquad executable runnable while the Vulkan path is developed in parallel.

## Migration stages

1. **Context and surface correctness**
   - Select a physical device and queue family that support both graphics and presentation.
   - Check required device extensions and supported surface capabilities before creating the swapchain.
   - Centralize ownership and destruction order for Vulkan resources.
2. **First visible frame**
   - Add a render pass and framebuffer for each swapchain image (or an equivalent supported rendering path).
   - Record commands that clear the color attachment.
   - Acquire an image, wait/reset the correct fence, submit work, and present while handling out-of-date/suboptimal results.
3. **Resize and lifecycle**
   - Recreate swapchain-dependent objects when the window resizes or the swapchain becomes out of date.
   - Skip rendering for zero-sized/minimized windows.
   - Wait for device idle and destroy resources in dependency order during shutdown.
4. **Geometry pipeline**
   - Add shader source and a reproducible shader-compilation step.
   - Create a graphics pipeline, vertex/index buffers, and camera uniform data.
   - Start by drawing a simple test triangle before importing world meshes.
5. **Hexel world integration**
   - Convert existing Macroquad `Mesh` data into Vulkan vertex/index data.
   - Preserve the current world, editing, chunking, and raycast logic while replacing only rendering-facing interfaces.
   - Add visible selection/highlight rendering and camera controls after static geometry works.
6. **Parity and cleanup**
   - Compare Vulkan output with the Macroquad reference.
   - Add tests for CPU-side mesh conversion and world behavior that do not require a GPU.
   - Keep hardware-dependent Vulkan smoke tests separately identifiable.
   - Remove Macroquad rendering only after Vulkan parity is verified; retain Macroquad dependencies only if still used elsewhere.

## Acceptance checks

- `cargo fmt --check`
- `cargo test`
- `cargo check --bin hexel_vulkan`
- `cargo run --bin hexel_vulkan` displays a stable, cleared window and presents frames without validation/runtime errors.
- The existing `cargo run` Macroquad prototype continues to work until its functionality has been ported.

Do not describe the renderer as migrated until the Vulkan binary renders world geometry and the existing gameplay-facing behavior has been checked.
