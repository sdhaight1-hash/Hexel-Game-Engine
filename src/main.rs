mod renderer;

use std::{collections::HashSet, sync::Arc, time::{Instant, SystemTime, UNIX_EPOCH}};

use glam::Vec3;
use hexel_engine::BlockId;
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
}
impl Default for HexelApp {
    fn default() -> Self {
        Self {
            window: None, renderer: None, camera: Camera::default(),
            player_feet: Vec3::ZERO, vertical_velocity: 0.0, grounded: false,
            pressed: HashSet::new(), jump_requested: false, last_frame: Instant::now(), cursor_locked: false,
        }
    }
}
impl HexelApp {
    fn lock_cursor(&mut self) {
        let Some(window) = self.window.as_ref() else { return; };
        let locked = window.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        if let Err(error) = locked { eprintln!("Could not capture mouse: {error}"); return; }
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
    fn update_player(&mut self, dt: f32) {
        let Some(renderer) = self.renderer.as_ref() else { return; };
        let dt = dt.min(0.04);
        let forward = Vec3::new(self.camera.yaw.sin(), 0.0, self.camera.yaw.cos()).normalize_or_zero();
        let right = Vec3::new(self.camera.yaw.cos(), 0.0, -self.camera.yaw.sin()).normalize_or_zero();
        let mut direction = Vec3::ZERO;
        if self.pressed.contains(&KeyCode::KeyW) { direction += forward; }
        if self.pressed.contains(&KeyCode::KeyS) { direction -= forward; }
        if self.pressed.contains(&KeyCode::KeyD) { direction += right; }
        if self.pressed.contains(&KeyCode::KeyA) { direction -= right; }
        direction = direction.normalize_or_zero();
        let proposed = self.player_feet + direction * self.camera.speed * dt;

        // Keep the capsule grounded on the sampled hexel surface and prevent walking up sheer cliffs.
        let current_cell = Renderer::cell_from_world(self.player_feet + Vec3::new(0.0, 0.01, 0.0));
        let next_cell = Renderer::cell_from_world(proposed + Vec3::new(0.0, 0.01, 0.0));
        let current_ground = renderer.ground_height(current_cell.q, current_cell.r);
        let next_ground = renderer.ground_height(next_cell.q, next_cell.r);
        // Sample the full circular footprint, including diagonal corners. Four cardinal
        // samples alone can let the player capsule clip into a hexel at its corners.
        let diagonal = PLAYER_RADIUS * std::f32::consts::FRAC_1_SQRT_2;
        let footprint_offsets = [
            (PLAYER_RADIUS, 0.0), (-PLAYER_RADIUS, 0.0),
            (0.0, PLAYER_RADIUS), (0.0, -PLAYER_RADIUS),
            (diagonal, diagonal), (diagonal, -diagonal),
            (-diagonal, diagonal), (-diagonal, -diagonal),
        ];
        let blocked_by_step = footprint_offsets.iter().any(|(ox, oz)| {
            let sample = Renderer::cell_from_world(proposed + Vec3::new(*ox, 0.01, *oz));
            renderer.ground_height(sample.q, sample.r) > current_ground + 0.65
        });
        if (!blocked_by_step && next_ground <= current_ground + 0.65) || self.player_feet.y > next_ground + 0.1 {
            self.player_feet.x = proposed.x;
            self.player_feet.z = proposed.z;
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
    }
    fn edit_target(&mut self, remove: bool) {
        let Some(renderer) = self.renderer.as_mut() else { return; };
        let Some((hit, placement)) = renderer.raycast(self.camera.position, self.camera.forward(), 8.0) else { return; };
        let target = if remove { hit } else { placement };
        if !remove {
            let player_cell = Renderer::cell_from_world(self.player_feet + Vec3::Y * (PLAYER_HEIGHT * 0.5));
            if target.q == player_cell.q && target.r == player_cell.r && (target.y - player_cell.y).abs() <= 2 { return; }
            if renderer.block_at(target).is_solid() { return; }
        }
        renderer.edit_block(target, if remove { BlockId::Air } else { BlockId::Grass });
    }
}

impl ApplicationHandler for HexelApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u32
            ^ std::process::id().rotate_left(13);
        let attributes = Window::default_attributes()
            .with_title("Hexel Engine | WASD move | Space jump | Left remove | Right place | Esc mouse")
            .with_inner_size(PhysicalSize::new(1280, 800));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => { error!(%error, "Failed to create window"); event_loop.exit(); return; }
        };
        match pollster::block_on(Renderer::new(window.clone(), seed)) {
            Ok(renderer) => {
                let ground = renderer.ground_height(0, 0);
                self.player_feet = Vec3::new(0.0, ground + 0.02, 0.0);
                self.camera.position = self.player_feet + Vec3::Y * PLAYER_EYE_HEIGHT;
                info!(seed, "Generated 32 terrain chunks (8 x 4); use this seed to identify this world");
                println!("Hexel Engine world seed: {seed}");
                println!("WASD move | Mouse look | Space jump | Left-click remove | Right-click place | Esc release mouse");
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
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => if let Some(renderer) = self.renderer.as_mut() { renderer.resize(size); },
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        if self.cursor_locked { self.unlock_cursor(); } else { self.lock_cursor(); }
                    }
                    if event.state == ElementState::Pressed {
                        if code == KeyCode::Space && !self.pressed.contains(&code) { self.jump_requested = true; }
                        self.pressed.insert(code);
                    } else { self.pressed.remove(&code); }
                }
            }
            WindowEvent::MouseInput { button: MouseButton::Left, state: ElementState::Pressed, .. } if self.cursor_locked => self.edit_target(true),
            WindowEvent::MouseInput { button: MouseButton::Right, state: ElementState::Pressed, .. } if self.cursor_locked => self.edit_target(false),
            WindowEvent::MouseInput { button: MouseButton::Left, state: ElementState::Pressed, .. } if !self.cursor_locked => self.lock_cursor(),
            WindowEvent::RedrawRequested => {
                let Some(renderer) = self.renderer.as_mut() else { return; };
                match renderer.render(&self.camera) {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => if let Some(window) = self.window.as_ref() { renderer.resize(window.inner_size()); },
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(error) => error!(%error, "Surface rendering error"),
                }
            }
            _ => {}
        }
    }
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: winit::event::DeviceId, event: DeviceEvent) {
        if self.cursor_locked && let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            let sensitivity = 0.0022;
            self.camera.yaw -= dx as f32 * sensitivity;
            self.camera.pitch = (self.camera.pitch - dy as f32 * sensitivity).clamp(-1.52, 1.52);
        }
    }
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.update_player(dt);
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }
}
fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = HexelApp::default();
    if let Err(error) = event_loop.run_app(&mut app) { eprintln!("Hexel Engine stopped with an event-loop error: {error}"); }
}
