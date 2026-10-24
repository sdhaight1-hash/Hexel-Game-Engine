mod renderer;

use std::{collections::HashSet, sync::Arc, time::Instant};

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

struct HexelApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    pressed: HashSet<KeyCode>,
    last_frame: Instant,
    cursor_locked: bool,
}

impl Default for HexelApp {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            camera: Camera::default(),
            pressed: HashSet::new(),
            last_frame: Instant::now(),
            cursor_locked: false,
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

    fn update_camera(&mut self, dt: f32) {
        let mut movement = Vec3Movement::default();
        let forward = self.camera.forward();
        let right = self.camera.right();
        if self.pressed.contains(&KeyCode::KeyW) {
            movement.add(forward);
        }
        if self.pressed.contains(&KeyCode::KeyS) {
            movement.add(-forward);
        }
        if self.pressed.contains(&KeyCode::KeyD) {
            movement.add(right);
        }
        if self.pressed.contains(&KeyCode::KeyA) {
            movement.add(-right);
        }
        if self.pressed.contains(&KeyCode::Space) {
            movement.y += 1.0;
        }
        if self.pressed.contains(&KeyCode::ShiftLeft) || self.pressed.contains(&KeyCode::ShiftRight)
        {
            movement.y -= 1.0;
        }
        let direction = movement.normalized();
        if direction.length_squared() > 0.0 {
            self.camera.position += direction * self.camera.speed * dt.min(0.05);
        }
    }
}

#[derive(Default)]
struct Vec3Movement {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3Movement {
    fn add(&mut self, vector: glam::Vec3) {
        self.x += vector.x;
        self.y += vector.y;
        self.z += vector.z;
    }

    fn normalized(self) -> glam::Vec3 {
        glam::Vec3::new(self.x, self.y, self.z).normalize_or_zero()
    }
}

impl ApplicationHandler for HexelApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(
                "Hexel Engine — WASD move · mouse look · Space/Shift fly · Esc release mouse",
            )
            .with_inner_size(PhysicalSize::new(1280, 800));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                error!(%error, "Failed to create window");
                event_loop.exit();
                return;
            }
        };

        match pollster::block_on(Renderer::new(window.clone())) {
            Ok(renderer) => {
                info!("GPU renderer initialized; drawing a generated hex-prism terrain patch");
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
            } if !self.cursor_locked => self.lock_cursor(),
            WindowEvent::RedrawRequested => {
                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                match renderer.render(&self.camera) {
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
        if self.cursor_locked {
            if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
                let sensitivity = 0.0022;
                self.camera.yaw += dx as f32 * sensitivity;
                self.camera.pitch =
                    (self.camera.pitch - dy as f32 * sensitivity).clamp(-1.52, 1.52);
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.update_camera(dt);
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
