mod renderer;

use std::{
    collections::HashSet,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use glam::Vec3;
use hexel_engine::{
    BlockId,
    inventory::{Inventory, ItemId},
};
use renderer::{Camera, Renderer};
use tracing::{error, info};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

const PLAYER_EYE_HEIGHT: f32 = 1.55;
const PLAYER_RADIUS: f32 = 0.30;
const PLAYER_HEIGHT: f32 = 1.80;
const GRAVITY: f32 = 22.0;
const JUMP_SPEED: f32 = 8.5;

struct HexelApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    player_feet: Vec3,
    vertical_velocity: f32,
    grounded: bool,
    pressed: HashSet<KeyCode>,
    jump_requested: bool,
    last_frame: Instant,
    cursor_locked: bool,
    harvest_held: bool,
    harvest_target: Option<hexel_engine::CellPos>,
    harvest_progress: f32,
    target_ui_tick: f32,
    inventory: Inventory,
    inventory_open: bool,
}
impl Default for HexelApp {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            camera: Camera::default(),
            player_feet: Vec3::ZERO,
            vertical_velocity: 0.0,
            grounded: false,
            pressed: HashSet::new(),
            jump_requested: false,
            last_frame: Instant::now(),
            cursor_locked: false,
            harvest_held: false,
            harvest_target: None,
            harvest_progress: 0.0,
            target_ui_tick: 0.0,
            inventory: Inventory::default(),
            inventory_open: false,
        }
    }
}
impl HexelApp {
    fn lock_cursor(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let locked = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        if let Err(error) = locked {
            eprintln!("Could not capture mouse: {error}");
            return;
        }
        window.set_cursor_visible(false);
        self.cursor_locked = true;
    }
    fn unlock_cursor(&mut self) {
        if let Some(window) = self.window.as_ref() {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
        }
        self.cursor_locked = false;
    }
    fn planet_preview_active(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(Renderer::planet_preview_enabled)
    }

    fn update_planet_player(&mut self, dt: f32) {
        let Some(renderer) = self.renderer.as_ref() else { return; };
        let dt = dt.min(0.04);
        let radial = self.player_feet.normalize_or_zero();
        if radial == Vec3::ZERO { return; }
        let forward = self.camera.forward();
        let tangent_forward = (forward - radial * forward.dot(radial)).normalize_or_zero();
        let tangent_right = radial.cross(tangent_forward).normalize_or_zero();
        let mut direction = Vec3::ZERO;
        if self.pressed.contains(&KeyCode::KeyW) { direction += tangent_forward; }
        if self.pressed.contains(&KeyCode::KeyS) { direction -= tangent_forward; }
        if self.pressed.contains(&KeyCode::KeyD) { direction += tangent_right; }
        if self.pressed.contains(&KeyCode::KeyA) { direction -= tangent_right; }
        direction = direction.normalize_or_zero();
        let surface = renderer.planet_surface_position(radial);
        let radius = surface.length().max(1.0);
        let next_radial = if direction == Vec3::ZERO { radial } else { (radial + direction * (self.camera.speed * dt / radius)).normalize_or_zero() };
        self.player_feet = renderer.planet_surface_position(next_radial);
        self.camera.up = next_radial;
        self.camera.position = self.player_feet + next_radial * PLAYER_EYE_HEIGHT;
        self.grounded = true;
    }

    fn update_player(&mut self, dt: f32) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let dt = dt.min(0.04);
        let forward =
            Vec3::new(self.camera.yaw.sin(), 0.0, self.camera.yaw.cos()).normalize_or_zero();
        // `look_to_rh` defines screen-right as forward × world-up. With yaw=0
        // the camera looks along +Z, so its actual right vector is -X, not +X.
        // Keep this basis independent from mouse input: yaw only orients movement.
        let right =
            Vec3::new(-self.camera.yaw.cos(), 0.0, self.camera.yaw.sin()).normalize_or_zero();
        let mut direction = Vec3::ZERO;
        if self.pressed.contains(&KeyCode::KeyW) {
            direction += forward;
        }
        if self.pressed.contains(&KeyCode::KeyS) {
            direction -= forward;
        }
        if self.pressed.contains(&KeyCode::KeyD) {
            direction += right;
        }
        if self.pressed.contains(&KeyCode::KeyA) {
            direction -= right;
        }
        direction = direction.normalize_or_zero();
        let proposed = self.player_feet + direction * self.camera.speed * dt;

        // Keep the capsule outside higher terrain. Eight footprint samples catch hex
        // corners; axis-separated fallback lets the player slide along a blocked face
        // instead of becoming stuck whenever a diagonal step is obstructed.
        let current_cell = Renderer::cell_from_world(self.player_feet + Vec3::new(0.0, 0.01, 0.0));
        let current_ground = renderer.ground_height(current_cell.q, current_cell.r);
        let diagonal = PLAYER_RADIUS * std::f32::consts::FRAC_1_SQRT_2;
        let footprint_offsets = [
            (PLAYER_RADIUS, 0.0),
            (-PLAYER_RADIUS, 0.0),
            (0.0, PLAYER_RADIUS),
            (0.0, -PLAYER_RADIUS),
            (diagonal, diagonal),
            (diagonal, -diagonal),
            (-diagonal, diagonal),
            (-diagonal, -diagonal),
        ];
        let can_move_to = |x: f32, z: f32| {
            let candidate = Vec3::new(x, self.player_feet.y, z);
            let next_cell = Renderer::cell_from_world(candidate + Vec3::new(0.0, 0.01, 0.0));
            let next_ground = renderer.ground_height(next_cell.q, next_cell.r);
            let blocked_by_step = footprint_offsets.iter().any(|(ox, oz)| {
                let sample = Renderer::cell_from_world(candidate + Vec3::new(*ox, 0.01, *oz));
                renderer.ground_height(sample.q, sample.r) > current_ground + 0.65
            });
            let blocked_by_tree =
                renderer.tree_blocks_player(candidate, PLAYER_RADIUS, PLAYER_HEIGHT);
            !blocked_by_tree
                && ((!blocked_by_step && next_ground <= current_ground + 0.65)
                    || self.player_feet.y > next_ground + 0.1)
        };
        if can_move_to(proposed.x, proposed.z) {
            self.player_feet.x = proposed.x;
            self.player_feet.z = proposed.z;
        } else {
            if can_move_to(proposed.x, self.player_feet.z) {
                self.player_feet.x = proposed.x;
            }
            if can_move_to(self.player_feet.x, proposed.z) {
                self.player_feet.z = proposed.z;
            }
        }

        if self.jump_requested && self.grounded {
            self.vertical_velocity = JUMP_SPEED;
            self.grounded = false;
        }
        self.jump_requested = false;
        self.vertical_velocity -= GRAVITY * dt;
        self.player_feet.y += self.vertical_velocity * dt;

        let cell = Renderer::cell_from_world(self.player_feet + Vec3::new(0.0, 0.01, 0.0));
        let ground = renderer.ground_height(cell.q, cell.r);
        if self.player_feet.y <= ground {
            self.player_feet.y = ground;
            self.vertical_velocity = 0.0;
            self.grounded = true;
        } else {
            self.grounded = false;
        }
        self.camera.position = self.player_feet + Vec3::Y * PLAYER_EYE_HEIGHT;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update_streaming(self.player_feet);
        }
    }
    fn update_harvesting(&mut self, dt: f32) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let hit = renderer
            .raycast(self.camera.position, self.camera.forward(), 8.0)
            .map(|(cell, _)| cell);
        if !self.harvest_held {
            self.harvest_target = None;
            self.harvest_progress = 0.0;
            return;
        }
        if hit != self.harvest_target {
            self.harvest_target = hit;
            self.harvest_progress = 0.0;
        }
        let Some(target) = hit else {
            self.harvest_progress = 0.0;
            return;
        };
        let block = renderer.block_at(target);
        if !block.is_solid() || block.is_indestructible() {
            self.harvest_progress = 0.0;
            return;
        }
        let seconds = match block {
            BlockId::Grass => 0.55,
            BlockId::Dirt => 0.8,
            BlockId::Stone => 1.8,
            BlockId::Foundation | BlockId::Air => {
                self.harvest_progress = 0.0;
                return;
            }
        };
        self.harvest_progress += dt.max(0.0);
        if self.harvest_progress >= seconds {
            let item = match block {
                BlockId::Grass => Some(ItemId::GrassFiber),
                BlockId::Dirt => None,
                BlockId::Stone => None,
                BlockId::Foundation | BlockId::Air => None,
            };
            renderer.edit_block(target, BlockId::Air);
            if let Some(item) = item {
                let _overflow = self.inventory.add(item, 1);
            }
            self.harvest_held = false;
            self.harvest_target = None;
            self.harvest_progress = 0.0;
        }
    }

    fn update_target_title(&mut self, dt: f32) {
        self.target_ui_tick += dt;
        if self.target_ui_tick < 0.12 {
            return;
        }
        self.target_ui_tick = 0.0;
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        if self.planet_preview_active() {
            let message = renderer.planet_cell_at_ray(self.camera.position, self.camera.forward())
                .map(|id| format!("Hexel Engine | PLANET WALK | Surface cell {id} | WASD move | Mouse look | F3 flat world"))
                .unwrap_or_else(|| "Hexel Engine | PLANET WALK | Looking into space | WASD move | Mouse look | F3 flat world".to_string());
            window.set_title(&message);
            return;
        }
        let message = if let Some((cell, _)) =
            renderer.raycast(self.camera.position, self.camera.forward(), 8.0)
        {
            let block = renderer.block_at(cell);
            let name = match block {
                BlockId::Grass => "Grass",
                BlockId::Dirt => "Dirt",
                BlockId::Stone => "Stone",
                BlockId::Foundation => "Foundation (unbreakable)",
                BlockId::Air => "Air",
            };
            let progress = if self.harvest_held && self.harvest_target == Some(cell) {
                format!(
                    " | Breaking: {:.0}%",
                    (self.harvest_progress / 1.8 * 100.0).min(100.0)
                )
            } else {
                String::new()
            };
            format!(
                "Hexel Engine | Looking at: {name} [{}, {}, {}]{progress} | Hotbar [{}] {} | E: {} | LMB break / RMB place",
                cell.q,
                cell.r,
                cell.y,
                self.inventory.selected_hotbar() + 1,
                self.inventory
                    .selected_stack()
                    .map(|s| format!("{:?} x{}", s.item, s.count))
                    .unwrap_or_else(|| "Empty".to_string()),
                if self.inventory_open {
                    "inventory open (UI pending)"
                } else {
                    "inventory"
                }
            )
        } else {
            format!(
                "Hexel Engine | Looking at: nothing | Hotbar [{}] {} | E: {} | LMB break / RMB place",
                self.inventory.selected_hotbar() + 1,
                self.inventory
                    .selected_stack()
                    .map(|s| format!("{:?} x{}", s.item, s.count))
                    .unwrap_or_else(|| "Empty".to_string()),
                if self.inventory_open {
                    "inventory open (UI pending)"
                } else {
                    "inventory"
                }
            )
        };
        window.set_title(&message);
    }
    fn edit_target(&mut self, remove: bool) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let Some((hit, placement)) =
            renderer.raycast(self.camera.position, self.camera.forward(), 8.0)
        else {
            return;
        };
        let target = if remove { hit } else { placement };
        if !remove {
            let player_cell =
                Renderer::cell_from_world(self.player_feet + Vec3::Y * (PLAYER_HEIGHT * 0.5));
            if target.q == player_cell.q
                && target.r == player_cell.r
                && (target.y - player_cell.y).abs() <= 2
            {
                return;
            }
            if renderer.block_at(target).is_solid() {
                return;
            }
        }
        renderer.edit_block(target, if remove { BlockId::Air } else { BlockId::Grass });
    }
}

impl ApplicationHandler for HexelApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u32
            ^ std::process::id().rotate_left(13);
        let attributes = Window::default_attributes()
            .with_title("Hexel Engine | WASD move | Space jump | Hold left to break | Right place | Esc mouse")
            .with_inner_size(PhysicalSize::new(1280, 800));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                error!(%error, "Failed to create window");
                event_loop.exit();
                return;
            }
        };
        match pollster::block_on(Renderer::new(window.clone(), seed)) {
            Ok(renderer) => {
                let ground = renderer.ground_height(0, 0);
                self.player_feet = Vec3::new(0.0, ground + 0.02, 0.0);
                self.camera.position = self.player_feet + Vec3::Y * PLAYER_EYE_HEIGHT;
                info!(
                    seed,
                    "Generated streamed 16×16×16 hexel prism chunks; use this seed to identify this world"
                );
                println!("Hexel Engine world seed: {seed}");
                println!(
                    "WASD move | Mouse look | Space jump | Hold left mouse to break terrain | Right-click place | Esc release mouse"
                );
                self.window = Some(window);
                self.renderer = Some(renderer);
                self.lock_cursor();
                self.last_frame = Instant::now();
            }
            Err(error) => {
                eprintln!("Hexel Engine could not initialize graphics: {error}");
                eprintln!("Update your graphics driver and try again.");
                event_loop.exit();
            }
        }
    }
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        if self.cursor_locked {
                            self.unlock_cursor();
                        } else {
                            self.lock_cursor();
                        }
                    }
                    if event.state == ElementState::Pressed {
                        match code {
                            KeyCode::Digit1 => {
                                self.inventory.select_hotbar(0);
                            }
                            KeyCode::Digit2 => {
                                self.inventory.select_hotbar(1);
                            }
                            KeyCode::Digit3 => {
                                self.inventory.select_hotbar(2);
                            }
                            KeyCode::Digit4 => {
                                self.inventory.select_hotbar(3);
                            }
                            KeyCode::Digit5 => {
                                self.inventory.select_hotbar(4);
                            }
                            KeyCode::Digit6 => {
                                self.inventory.select_hotbar(5);
                            }
                            KeyCode::Digit7 => {
                                self.inventory.select_hotbar(6);
                            }
                            KeyCode::Digit8 => {
                                self.inventory.select_hotbar(7);
                            }
                            KeyCode::Digit9 => {
                                self.inventory.select_hotbar(8);
                            }
                            KeyCode::KeyE => {
                                self.inventory_open = !self.inventory_open;
                            }
                            KeyCode::F3 if !self.pressed.contains(&code) => {
                                let preview = if let Some(renderer) = self.renderer.as_mut() {
                                    renderer.toggle_planet_preview();
                                    renderer.planet_preview_enabled()
                                } else {
                                    false
                                };
                                if preview {
                                    self.harvest_held = false;
                                    self.harvest_target = None;
                                    self.harvest_progress = 0.0;
                                    self.jump_requested = false;
                                    if let Some(renderer) = self.renderer.as_ref() {
                                        self.player_feet = renderer.planet_surface_position(Vec3::X);
                                        self.camera.up = Vec3::X;
                                        self.camera.position = self.player_feet + Vec3::X * PLAYER_EYE_HEIGHT;
                                    }
                                    self.camera.yaw = 0.0;
                                    self.camera.pitch = 0.0;
                                } else {
                                    self.camera.up = Vec3::Y;
                                    if let Some(renderer) = self.renderer.as_ref() {
                                        let ground = renderer.ground_height(0, 0);
                                        self.player_feet = Vec3::new(0.0, ground + 0.02, 0.0);
                                        self.camera.position = self.player_feet + Vec3::Y * PLAYER_EYE_HEIGHT;
                                        renderer.update_streaming(self.player_feet);
                                    }
                                    self.camera.yaw = 0.0;
                                    self.camera.pitch = -0.15;
                                }
                                if let Some(window) = self.window.as_ref() {
                                    window.set_title(if preview {
                                        "Hexel Engine | PLANET WALK | WASD move | Mouse look | F3 flat world"
                                    } else {
                                        "Hexel Engine | WASD move | Space jump | Hold left to break | Right place | Esc mouse"
                                    });
                                }
                            }
                            _ => {}
                        }
                        if code == KeyCode::Space && !self.pressed.contains(&code) {
                            self.jump_requested = true;
                        }
                        self.pressed.insert(code);
                    } else {
                        self.pressed.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } if self.cursor_locked && !self.planet_preview_active() => {
                self.harvest_held = true;
                self.harvest_target = None;
                self.harvest_progress = 0.0;
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                self.harvest_held = false;
                self.harvest_target = None;
                self.harvest_progress = 0.0;
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } if self.cursor_locked && !self.planet_preview_active() => self.edit_target(false),
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } if !self.cursor_locked => self.lock_cursor(),
            WindowEvent::RedrawRequested => {
                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                let counts: [u16; Inventory::TOTAL_SLOTS] = std::array::from_fn(|i| {
                    self.inventory.slot(i).map(|stack| stack.count).unwrap_or(0)
                });
                match renderer.render(
                    &self.camera,
                    self.inventory.selected_hotbar(),
                    self.inventory_open,
                    counts,
                ) {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        if let Some(window) = self.window.as_ref() {
                            renderer.resize(window.inner_size());
                        }
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(error) => error!(%error, "Surface rendering error"),
                }
            }
            _ => {}
        }
    }
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self.cursor_locked
            && let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
        {
            let sensitivity = 0.0022;
            self.camera.yaw -= dx as f32 * sensitivity;
            self.camera.pitch = (self.camera.pitch - dy as f32 * sensitivity).clamp(-1.52, 1.52);
        }
    }
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        if self.planet_preview_active() {
            self.update_planet_player(dt);
            self.update_target_title(dt);
        } else {
            self.update_player(dt);
            self.update_harvesting(dt);
            self.update_target_title(dt);
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = HexelApp::default();
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("Hexel Engine stopped with an event-loop error: {error}");
    }
}
