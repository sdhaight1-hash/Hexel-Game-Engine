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
    save::GameSave,
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
    planet_harvest_target: Option<u32>,
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
            planet_harvest_target: None,
            harvest_progress: 0.0,
            target_ui_tick: 0.0,
            inventory: Inventory::default(),
            inventory_open: false,
        }
    }
}
impl HexelApp {
    fn save_game(&self) {
        let Some(renderer) = self.renderer.as_ref() else { return; };
        let mut save = GameSave::fresh(renderer.world_seed());
        save.player_position = [self.player_feet.x, self.player_feet.y, self.player_feet.z];
        save.yaw = self.camera.yaw;
        save.pitch = self.camera.pitch;
        save.planet_mode = true;
        save.selected_hotbar = self.inventory.selected_hotbar();
        save.inventory = *self.inventory.slots();
        save.planet_edits = renderer.planet_edits_snapshot();
        save.planet_voxel_edits = renderer.planet_voxel_edits_snapshot();
        match std::fs::write("hexel-world.save", save.encode()) {
            Ok(()) => println!("Saved Hexel world to hexel-world.save"),
            Err(error) => eprintln!("Could not save Hexel world: {error}"),
        }
    }

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
    fn update_planet_player(&mut self, dt: f32) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let dt = dt.min(0.04);
        let radial = self.player_feet.normalize_or_zero();
        if radial == Vec3::ZERO {
            return;
        }
        let forward = self.camera.forward();
        let tangent_forward = (forward - radial * forward.dot(radial)).normalize_or_zero();
        let tangent_right = tangent_forward.cross(radial).normalize_or_zero();
        let mut direction = Vec3::ZERO;
        if self.pressed.contains(&KeyCode::KeyW) {
            direction += tangent_forward;
        }
        if self.pressed.contains(&KeyCode::KeyS) {
            direction -= tangent_forward;
        }
        if self.pressed.contains(&KeyCode::KeyD) {
            direction += tangent_right;
        }
        if self.pressed.contains(&KeyCode::KeyA) {
            direction -= tangent_right;
        }
        direction = direction.normalize_or_zero();
        let surface = renderer.planet_surface_position(radial);
        let radius = surface.length().max(1.0);
        let altitude = (self.player_feet.length() - surface.length()).max(0.0);
        let mut next_radial = if direction == Vec3::ZERO {
            radial
        } else {
            (radial + direction * (self.camera.speed * dt / radius)).normalize_or_zero()
        };
        let mut height = altitude;
        if self.jump_requested && self.grounded {
            self.vertical_velocity = JUMP_SPEED;
            self.grounded = false;
        }
        self.jump_requested = false;
        if !self.grounded || self.vertical_velocity > 0.0 {
            self.vertical_velocity -= GRAVITY * dt;
            height = (altitude + self.vertical_velocity * dt).max(0.0);
        }
        if height <= 0.0 {
            height = 0.0;
            self.vertical_velocity = 0.0;
            self.grounded = true;
        } else {
            self.grounded = false;
        }
        let proposed_feet =
            renderer.planet_surface_position(next_radial) + next_radial * height;
        if renderer.planet_tree_blocks_player(proposed_feet, PLAYER_RADIUS, PLAYER_HEIGHT) {
            // Stop horizontal movement at tree trunks while preserving the current
            // radial position; the same deterministic rule builds the visible trees.
            next_radial = radial;
            height = altitude;
        }
        self.player_feet = renderer.planet_surface_position(next_radial) + next_radial * height;
        self.camera.up = next_radial;
        self.camera.position = self.player_feet + next_radial * PLAYER_EYE_HEIGHT;
    }

    fn update_planet_harvesting(&mut self, dt: f32) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let hit = renderer.planet_cell_at_ray(self.camera.position, self.camera.forward());
        if !self.harvest_held {
            self.planet_harvest_target = None;
            self.harvest_progress = 0.0;
            return;
        }
        if hit != self.planet_harvest_target {
            self.planet_harvest_target = hit;
            self.harvest_progress = 0.0;
        }
        let Some(target) = hit else {
            self.harvest_progress = 0.0;
            return;
        };
        let block = renderer.planet_block_at(target);
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
            let (layer, _) = renderer.planet_top_solid_voxel(target);
            if renderer.edit_planet_voxel(target, layer, BlockId::Air) {
                let recovered = match block {
                    BlockId::Grass => Some(ItemId::GrassBlock),
                    BlockId::Dirt => Some(ItemId::DirtBlock),
                    BlockId::Stone => Some(ItemId::StoneBlock),
                    BlockId::Foundation | BlockId::Air => None,
                };
                if let Some(item) = recovered {
                    let _overflow = self.inventory.add(item, 1);
                }
                if block == BlockId::Grass {
                    let _overflow = self.inventory.add(ItemId::GrassFiber, 1);
                }
            }
            self.harvest_held = false;
            self.planet_harvest_target = None;
            self.harvest_target = None;
            self.harvest_progress = 0.0;
        }
    }

    fn edit_planet_target(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let Some(target) = renderer
            .planet_cell_at_ray(self.camera.position, self.camera.forward())
        else {
            return;
        };
        let Some(stack) = self.inventory.selected_stack() else {
            return;
        };
        let block = match stack.item {
            ItemId::GrassBlock => BlockId::Grass,
            ItemId::DirtBlock => BlockId::Dirt,
            ItemId::StoneBlock => BlockId::Stone,
            ItemId::OakWood | ItemId::BirchWood | ItemId::PineWood
            | ItemId::GenericWood | ItemId::Stick | ItemId::GrassFiber => return,
        };
        if renderer.planet_voxel_block_at(target, 0) == BlockId::Air
            && renderer.edit_planet_voxel(target, 0, block)
        {
            let slot = self.inventory.selected_hotbar();
            self.inventory.remove_from_slot(slot, 1);
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
        let message = renderer
            .planet_cell_at_ray(self.camera.position, self.camera.forward())
            .map(|id| {
                format!(
                    "Hexel Engine | Globe | Cell {id} ({:?}) | Hotbar [{}] {} | E inventory | LMB harvest / RMB place",
                    renderer.planet_block_at(id),
                    self.inventory.selected_hotbar() + 1,
                    self.inventory
                        .selected_stack()
                        .map(|s| format!("{:?} x{}", s.item, s.count))
                        .unwrap_or_else(|| "Empty".to_string())
                )
            })
            .unwrap_or_else(|| {
                "Hexel Engine | Globe | WASD move | Space jump | E inventory".to_string()
            });
        window.set_title(&message);
    }
}

impl ApplicationHandler for HexelApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let loaded_save = std::fs::read_to_string("hexel-world.save")
            .ok()
            .and_then(|contents| match GameSave::decode(&contents) {
                Ok(save) => Some(save),
                Err(error) => {
                    eprintln!("Ignoring invalid hexel-world.save: {error:?}");
                    None
                }
            });
        let seed = loaded_save.as_ref().map(|save| save.seed).unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u32
                ^ std::process::id().rotate_left(13)
        });
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
            Ok(mut renderer) => {
                if let Some(save) = loaded_save.as_ref() {
                    renderer.restore_planet_edits(&save.planet_edits);
                    renderer.restore_planet_voxel_edits(&save.planet_voxel_edits);
                    self.player_feet = Vec3::from_array(save.player_position);
                    self.camera.yaw = save.yaw;
                    self.camera.pitch = save.pitch.clamp(-1.52, 1.52);
                    self.inventory = Inventory::default();
                    for (index, stack) in save.inventory.iter().enumerate() {
                        self.inventory.set_slot(index, *stack);
                    }
                    self.inventory.select_hotbar(save.selected_hotbar);
                    if save.planet_mode {
                        self.camera.up = self.player_feet.normalize_or_zero();
                        if self.camera.up == Vec3::ZERO { self.camera.up = Vec3::X; }
                        self.camera.position = self.player_feet + self.camera.up * PLAYER_EYE_HEIGHT;
                    } else {
                        // Legacy flat-world coordinates are not planet-centred coordinates.
                        // Keep the player's inventory and planetary edits, but spawn safely
                        // on the globe instead of silently placing them at an arbitrary location.
                        self.player_feet = renderer.planet_surface_position(Vec3::X);
                        self.camera.up = Vec3::X;
                        self.camera.position = self.player_feet + Vec3::X * PLAYER_EYE_HEIGHT;
                        self.camera.yaw = 0.0;
                        self.camera.pitch = -0.15;
                    }
                    println!("Loaded saved Hexel world.");
                } else {
                    self.player_feet = renderer.planet_surface_position(Vec3::X);
                    self.camera.up = Vec3::X;
                    self.camera.position = self.player_feet + Vec3::X * PLAYER_EYE_HEIGHT;
                    self.camera.yaw = 0.0;
                    self.camera.pitch = -0.15;
                }
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
            WindowEvent::CloseRequested => { self.save_game(); event_loop.exit(); }
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
                            KeyCode::F5 => {
                                self.save_game();
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
            } if self.cursor_locked => {
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
            } if self.cursor_locked => {
                self.edit_planet_target();
            },
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
        self.update_planet_player(dt);
        self.update_planet_harvesting(dt);
        self.update_target_title(dt);
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
