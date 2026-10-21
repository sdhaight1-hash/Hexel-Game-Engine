mod hexel;
mod world;

use macroquad::prelude::*;

use world::World;

const LOOK_SENSITIVITY: f32 = 0.003;
const MOVE_SPEED: f32 = 8.0;
const WHEEL_VERTICAL_SPEED: f32 = 0.15;

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
    let mut position = vec3(0.0, 9.0, -16.0);
    let mut yaw = 0.0_f32;
    let mut pitch = -0.35_f32;
    let mut captured = false;
    let mut previous_mouse = mouse_position();

    loop {
        let dt = get_frame_time().min(0.05);

        if is_key_pressed(KeyCode::Escape) {
            captured = false;
            show_mouse(true);
            set_cursor_grab(false);
        }
        // C toggles mouse capture so both mouse buttons remain available for editing.
        if is_key_pressed(KeyCode::C) {
            captured = !captured;
            show_mouse(!captured);
            set_cursor_grab(captured);
            previous_mouse = mouse_position();
        }

        if captured {
            let mouse = mouse_position();
            yaw -= (mouse.0 - previous_mouse.0) * LOOK_SENSITIVITY;
            pitch -= (mouse.1 - previous_mouse.1) * LOOK_SENSITIVITY;
            pitch = pitch.clamp(-1.45, 1.45);
            previous_mouse = mouse;
        }

        // Mouse-wheel deltas are discrete; clamp each frame's input and use a
        // small step so a scroll notch nudges the camera rather than jumping it.
        let wheel = mouse_wheel();
        position.y += wheel.1.clamp(-1.0, 1.0) * WHEEL_VERTICAL_SPEED;

        let forward = vec3(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        )
        .normalize();
        let flat_forward = vec3(forward.x, 0.0, forward.z).normalize_or_zero();
        let right = vec3(flat_forward.z, 0.0, -flat_forward.x);
        let mut movement = Vec3::ZERO;
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
            movement += flat_forward;
        }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
            movement -= flat_forward;
        }

        // Keep the requested A/D mapping: A moves left, D moves right.
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left)  {
            movement += right;
        }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
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

        
        
        let mut target = None;

        if captured {
            target = world.raycast(position, forward);

            if is_mouse_button_pressed(MouseButton::Left) {
                if let Some((coord, side)) = target {
                    let destination = match side {
                        Some(direction) => coord.neighbor(direction),
                        None => coord,
                    };
                    world.add_hexel(destination);
                }
            }

            if is_mouse_button_pressed(MouseButton::Right) {
                if let Some((coord, _side)) = target {
                    world.remove_hexel(coord);
                }
            }
        }



        clear_background(Color::new(0.055, 0.075, 0.11, 1.0));
        let camera = Camera3D {
            position,
            target: position + forward,
            up: Vec3::new(0.0, -1.0, 0.0),
            fovy: 50.0,
            ..Default::default()
        };
        set_camera(&camera);
        draw_grid(40, 1.0, GRAY, Color::new(0.12, 0.15, 0.19, 1.0));
        
        
        for world_mesh in world.meshes() {
            draw_mesh(&world_mesh);
        }

        // Outline the full targeted hex column.
        if let Some((coord, _side)) = target {
            let height = world.height(coord);

            if height > 0 {
                let bottom = hexel::HEX_HEIGHT * 0.015;
                let top = height as f32 * hexel::HEX_HEIGHT + 0.025;

                let bottom_corners = world::coord_corners(coord, bottom);
                let top_corners = world::coord_corners(coord, top);

                for i in 0..6 {
                    let next = (i + 1) % 6;

                    draw_line_3d(
                        bottom_corners[i],
                        bottom_corners[next],
                        YELLOW,
                    );
                    draw_line_3d(
                        top_corners[i],
                        top_corners[next],
                        YELLOW,
                    );
                    draw_line_3d(
                        bottom_corners[i],
                        top_corners[i],
                        YELLOW,
                    );
                }
            }
        }

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
            "WASD move  |  Mouse look  |  Left click add  |  Right click remove  |  R reset",
            20.0,
            56.0,
            18.0,
            LIGHTGRAY,
        );
        draw_text(
            "Mouse wheel: fine vertical move  |  Space / Shift: vertical move  |  C: capture/release  |  Esc: release",
            20.0,
            80.0,
            16.0,
            LIGHTGRAY,
        );
        if !captured {
            draw_text(
                "Press C to capture the mouse and edit; press Esc to release it",
                20.0,
                screen_h - 24.0,
                18.0,
                YELLOW,
            );
        }

        next_frame().await;
    }
}
