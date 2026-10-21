mod hexel;
mod world;

use macroquad::prelude::*;

use world::World;

const LOOK_SENSITIVITY: f32 = 0.003;
const MOVE_SPEED: f32 = 8.0;
const WHEEL_VERTICAL_SPEED: f32 = 1.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "Hexel Engine — Prototype".to_owned(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut world = World::demo();
    let mut position = vec3(0.0, 7.0, -13.0);
    let mut yaw = 0.0_f32;
    let mut pitch = -0.22_f32;
    let mut captured = false;
    let mut previous_mouse = mouse_position();

    loop {
        let dt = get_frame_time().min(0.05);

        if is_key_pressed(KeyCode::Escape) {
            captured = false;
            show_mouse(true);
            set_cursor_grab(false);
        }
        if is_mouse_button_pressed(MouseButton::Right) && !captured {
            captured = true;
            show_mouse(false);
            set_cursor_grab(true);
            previous_mouse = mouse_position();
        }

        if captured {
            let mouse = mouse_position();
            yaw -= (mouse.0 - previous_mouse.0) * LOOK_SENSITIVITY;
            pitch -= (mouse.1 - previous_mouse.1) * LOOK_SENSITIVITY;
            pitch = pitch.clamp(-1.45, 1.45);
            previous_mouse = mouse;
        }

        // The wheel moves the free-fly camera vertically. This is independent
        // of frame time because mouse-wheel input is a per-frame scroll delta.
        let wheel = mouse_wheel();
        position.y += wheel.1 * WHEEL_VERTICAL_SPEED;

        let forward = vec3(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        )
        .normalize();
        let flat_forward = vec3(forward.x, 0.0, forward.z).normalize_or_zero();
        let right = vec3(flat_forward.z, 0.0, -flat_forward.x);
        let mut movement = Vec3::ZERO;
        if is_key_down(KeyCode::W) {
            movement += flat_forward;
        }
        if is_key_down(KeyCode::S) {
            movement -= flat_forward;
        }

        // Keep the requested A/D mapping: A moves left, D moves right.
        if is_key_down(KeyCode::A) {
            movement += right;
        }
        if is_key_down(KeyCode::D) {
            movement -= right;
        }

        // Space and Left Shift remain as alternate vertical controls.
        if is_key_down(KeyCode::Space) {
            movement.y += 1.0;
        }
        if is_key_down(KeyCode::LeftShift) {
            movement.y -= 1.0;
        }
        if movement.length_squared() > 0.0 {
            position += movement.normalize() * MOVE_SPEED * dt;
        }

        if is_key_pressed(KeyCode::R) {
            world = World::demo();
        }

        if captured {
            let target = world.raycast(position, forward);
            if is_mouse_button_pressed(MouseButton::Left) {
                if let Some(coord) = target {
                    world.remove_hexel(coord);
                }
            }
            if is_key_pressed(KeyCode::E) {
                if let Some(coord) = target {
                    world.add_hexel(coord);
                }
            }
        }

        clear_background(Color::new(0.055, 0.075, 0.11, 1.0));
        let camera = Camera3D {
            position,
            target: position + forward,
            up: Vec3::Y,
            fovy: 65.0,
            ..Default::default()
        };
        set_camera(&camera);
        draw_grid(40, 1.0, GRAY, Color::new(0.12, 0.15, 0.19, 1.0));
        let world_mesh = world.mesh();
        draw_mesh(&world_mesh);
        set_default_camera();

        let (screen_w, screen_h) = (screen_width(), screen_height());
        draw_line(
            screen_w * 0.5 - 7.0,
            screen_h * 0.5,
            screen_w * 0.5 + 7.0,
            screen_h * 0.5,
            2.0,
            WHITE,
        );
        draw_line(
            screen_w * 0.5,
            screen_h * 0.5 - 7.0,
            screen_w * 0.5,
            screen_h * 0.5 + 7.0,
            2.0,
            WHITE,
        );
        draw_text(
            "HEXEL ENGINE  /  PLAYABLE PROTOTYPE",
            20.0,
            30.0,
            22.0,
            WHITE,
        );
        draw_text(
            "WASD move  |  Mouse look  |  Left click remove  |  E add  |  R reset",
            20.0,
            56.0,
            18.0,
            LIGHTGRAY,
        );
        draw_text(
            "Mouse wheel: vertical move  |  Space / Shift: vertical move  |  Right click: capture  |  Esc: release",
            20.0,
            80.0,
            16.0,
            LIGHTGRAY,
        );
        if !captured {
            draw_text(
                "Right-click the window to capture the mouse and begin editing",
                20.0,
                screen_h - 24.0,
                18.0,
                YELLOW,
            );
        }

        next_frame().await;
    }
}
