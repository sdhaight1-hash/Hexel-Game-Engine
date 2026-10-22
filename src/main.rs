mod hexel;
mod world;

use macroquad::prelude::*;

use world::World;

const DEFAULT_LOOK_SENSITIVITY: f32 = 0.00015;
const DEFAULT_MOVE_SPEED: f32 = 8.0;
const DEFAULT_CAMERA_FOV: f32 = 50.0;
const DEFAULT_WHEEL_VERTICAL_SPEED: f32 = 0.15;
const DEFAULT_CAMERA_SPEED_MULTIPLIER: f32 = 1.0;
const DEFAULT_CROSSHAIR_SIZE: f32 = 7.0;
const DEFAULT_CROSSHAIR_COLOR: Color = WHITE;


fn window_conf() -> Conf {
    Conf {
        window_title: "Hexel Engine — Prototype".to_owned(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        ..Default::default()
    }
}

fn debug_slider(
    label: &str,
    x: f32,
    y: f32,
    width: f32,
    value: &mut f32,
    min: f32,
    max: f32,
) {
    draw_text(label, x, y, 20.0, WHITE);
    draw_text(
        &format!("{:.5}", *value),
        x + width - 85.0,
        y,
        18.0,
        LIGHTGRAY,
    );

    let track_x = x;
    let track_y = y + 18.0;
    let track_width = width;
    let track_height = 8.0;

    draw_rectangle(
        track_x,
        track_y,
        track_width,
        track_height,
        DARKGRAY,
    );

    let mut fraction = (*value - min) / (max - min);
    fraction = fraction.clamp(0.0, 1.0);

    let mouse = mouse_position();
    let inside_track =
        mouse.0 >= track_x
        && mouse.0 <= track_x + track_width
        && mouse.1 >= track_y - 8.0
        && mouse.1 <= track_y + track_height + 8.0;

    if is_mouse_button_down(MouseButton::Left) && inside_track {
        fraction = ((mouse.0 - track_x) / track_width).clamp(0.0, 1.0);
        *value = min + fraction * (max - min);
    }

    draw_rectangle(
        track_x,
        track_y,
        track_width * fraction,
        track_height,
        SKYBLUE,
    );

    draw_circle(
        track_x + track_width * fraction,
        track_y + track_height * 0.5,
        7.0,
        WHITE,
    );
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut world = World::demo();
    let mut look_sensitivity = DEFAULT_LOOK_SENSITIVITY;
    let mut camera_speed_multiplier = DEFAULT_CAMERA_SPEED_MULTIPLIER;
    let mut crosshair_size = DEFAULT_CROSSHAIR_SIZE;
    let mut crosshair_color = DEFAULT_CROSSHAIR_COLOR;
    let mut move_speed = DEFAULT_MOVE_SPEED;
    let mut camera_fov = DEFAULT_CAMERA_FOV;
    let mut wheel_vertical_speed = DEFAULT_WHEEL_VERTICAL_SPEED;
    let mut debug_menu_open = false;
    let mut show_grid = true;
    let mut show_target_outline = true;
    let mut show_chunk_bounds = false;
    let mut show_crosshair = true;
    let mut position = vec3(0.0, 9.0, -16.0);
    let mut yaw = 0.0_f32;
    let mut pitch = -0.35_f32;
    let mut captured = false;
    let mut previous_mouse = mouse_position();
    let mut fps_timer = 0.0_f32;
    let mut fps_frames = 0_u32;
    let mut displayed_fps = 0_u32;
    let mut displayed_frame_ms = 0.0_f32;

    loop {
        let dt = get_frame_time().min(0.05);
        fps_timer += get_frame_time();
        fps_frames += 1;

        if fps_timer >= 0.5 {
            displayed_fps = (fps_frames as f32 / fps_timer) as u32;
            displayed_frame_ms = 1000.0 / displayed_fps.max(1) as f32;

            fps_timer = 0.0;
            fps_frames = 0;
        }

        if is_key_pressed(KeyCode::Escape) {
            captured = false;
            show_mouse(true);
            set_cursor_grab(false);
        }

        if is_key_pressed(KeyCode::F1) {
            debug_menu_open = !debug_menu_open;

        if debug_menu_open {
            captured = false;
            show_mouse(true);
            set_cursor_grab(false);
        }
        }

        // C toggles mouse capture so both mouse buttons remain available for editing.
        if is_key_pressed(KeyCode::C) && !debug_menu_open {
            captured = !captured;
            show_mouse(!captured);
            set_cursor_grab(captured);
            previous_mouse = mouse_position();
        }

        if captured {
            let mouse = mouse_position();
            yaw -= (mouse.0 - previous_mouse.0) * look_sensitivity;
            pitch -= (mouse.1 - previous_mouse.1) * look_sensitivity;
            pitch = pitch.clamp(-1.45, 1.45);
            previous_mouse = mouse;
        }

        // Mouse-wheel deltas are discrete; clamp each frame's input and use a
        // small step so a scroll notch nudges the camera rather than jumping it.
        let wheel = mouse_wheel();
        position.y += wheel.1.clamp(-1.0, 1.0) * wheel_vertical_speed;

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
            position += movement.normalize() * move_speed * camera_speed_multiplier * dt;
        }

        if is_key_pressed(KeyCode::R) {
            world = World::demo();
        }

        
        
        let mut target = None;

        if captured {
            target = world.raycast(position, forward);

                        if is_mouse_button_pressed(MouseButton::Left) {
                if let Some((coord, layer, side)) = target {
                     match side {
                        Some(6) if layer > 0 => {
                            world.add_hexel_at_layer(coord, layer - 1);
                        }
                        Some(direction) => {
                            world.add_hexel_at_layer(
                                coord.neighbor(direction),
                                layer,
                            );
                        }
                        None => {
                            world.add_hexel_at_layer(coord, layer + 1);
                        }
                    }
                }
            }
            if is_mouse_button_pressed(MouseButton::Right) {
                if let Some((coord, layer, _side)) = target {
                    world.remove_hexel_at_layer(coord, layer);
                }
            }
        }



        clear_background(Color::new(0.055, 0.075, 0.11, 1.0));
        let camera = Camera3D {
            position,
            target: position + forward,
            up: Vec3::new(0.0, -1.0, 0.0),
            fovy: camera_fov,
            ..Default::default()
        };
        
        set_camera(&camera);
        if show_grid {
            draw_grid(40, 1.0, GRAY, Color::new(0.12, 0.15, 0.19, 1.0));
        }
       
        
        
        world.rebuild_dirty_chunks();

        for world_mesh in world.meshes() {
            draw_mesh(world_mesh);
        }

        // Draw cyan outlines around chunk boundaries.
if show_chunk_bounds {
    let chunk_coords = world.occupied_chunk_coords();

    for (chunk_q, chunk_r) in chunk_coords {
        let start_q = chunk_q * 16;
        let start_r = chunk_r * 16;

        for q in start_q..start_q + 16 {
            for r in start_r..start_r + 16 {
                let coord = hexel::HexCoord::new(q, r);
                if world.height(coord) == 0 {
                continue;
                }

                // Only draw edges that face a different chunk.
                let height = world.height(coord) as f32 * hexel::HEX_HEIGHT + 0.04;
                let corners = world::coord_corners(coord, height);

                for direction in 0..6 {
                    let neighbor = coord.neighbor(direction);

                    let neighbor_chunk_q = neighbor.q.div_euclid(16);
                    let neighbor_chunk_r = neighbor.r.div_euclid(16);

                    if neighbor_chunk_q != chunk_q
                        || neighbor_chunk_r != chunk_r
                    {
                        let edge = (5 + 6 - direction) % 6;

                        draw_line_3d(
                            corners[edge],
                            corners[(edge + 1) % 6],
                            SKYBLUE,
                        );
                    }
                }
            }
        }
    }
}

       // Outline the full targeted hex column.
        if show_target_outline {
            if let Some((coord, layer, _side)) = target {
                let height = world.height(coord);

                if height > 0 {
                    let bottom = layer as f32 * hexel::HEX_HEIGHT + 0.015;
                    let top = (layer as f32 + 1.0) * hexel::HEX_HEIGHT + 0.025;

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
        }

        
        set_default_camera();

        // Performance display — fixed in the top-right corner.
        let fps_text = format!("FPS: {}", displayed_fps);
        let frame_text = format!("Frame: {:.1} ms", displayed_frame_ms);

        let padding = 12.0;
        let text_size = 20.0;
        let fps_width = measure_text(&fps_text, None, text_size as u16, 1.0).width;
        let frame_width = measure_text(&frame_text, None, text_size as u16, 1.0).width;
        let box_width = fps_width.max(frame_width) + padding * 2.0;
        let box_height = 58.0;
        let box_x = screen_width() - box_width - 12.0;
        let box_y = 12.0;

        draw_rectangle(
            box_x,
            box_y,
            box_width,
            box_height,
            Color::new(0.035, 0.05, 0.08, 0.85),
        );

        draw_text(
            &fps_text,
            box_x + padding,
            box_y + 23.0,
            text_size,
            LIME,
        );

        draw_text(
            &frame_text,
            box_x + padding,
            box_y + 47.0,
            text_size,
            WHITE,
        );

        let (screen_w, screen_h) = (screen_width(), screen_height());
        if debug_menu_open {

    let panel_x = screen_w * 0.5 - 230.0;
    let panel_y = 90.0;
    let panel_w = 460.0;
    let panel_h = 650.0;

    draw_rectangle(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        Color::new(0.035, 0.05, 0.08, 0.97),
    );

    draw_rectangle_lines(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        2.0,
        SKYBLUE,
    );

    draw_text(
        "DEBUG OPTIONS",
        panel_x + 20.0,
        panel_y + 35.0,
        28.0,
        SKYBLUE,
    );

    draw_text(
        "Camera and movement settings",
        panel_x + 20.0,
        panel_y + 60.0,
        18.0,
        LIGHTGRAY,
    );

    let slider_x = panel_x + 20.0;
    let slider_w = panel_w - 40.0;

    debug_slider(
        "Mouse sensitivity",
        slider_x,
        panel_y + 100.0,
        slider_w,
        &mut look_sensitivity,
        0.00002,
        0.001,
    );

    debug_slider(
        "Camera field of view",
        slider_x,
        panel_y + 165.0,
        slider_w,
        &mut camera_fov,
        25.0,
        85.0,
    );

    debug_slider(
        "Movement speed",
        slider_x,
        panel_y + 230.0,
        slider_w,
        &mut move_speed,
        1.0,
        20.0,
    );

    debug_slider(
        "Wheel vertical speed",
        slider_x,
        panel_y + 295.0,
        slider_w,
        &mut wheel_vertical_speed,
        0.02,
        1.0,
    );
        debug_slider(
        "Camera speed multiplier",
        slider_x,
        panel_y + 350.0,
        slider_w,
        &mut camera_speed_multiplier,
        0.25,
        3.0,
    );
        debug_slider(
        "Crosshair size",
        slider_x,
        panel_y + 405.0,
        slider_w,
        &mut crosshair_size,
        2.0,
        20.0,
    );
        draw_text(
        "Rendering",
        panel_x + 20.0,
        panel_y + 460.0,
        20.0,
        SKYBLUE,
    );

    let toggle_x = panel_x + 20.0;
    let toggle_y = panel_y + 475.0;
    let toggle_w = 130.0;
    let toggle_h = 28.0;
    let toggle_gap = 145.0;
    let mouse = mouse_position();

    // Grid toggle
    let grid_hovered =
        mouse.0 >= toggle_x
        && mouse.0 <= toggle_x + toggle_w
        && mouse.1 >= toggle_y
        && mouse.1 <= toggle_y + toggle_h;

    draw_rectangle(
        toggle_x,
        toggle_y,
        toggle_w,
        toggle_h,
        if grid_hovered { BLUE } else { DARKGRAY },
    );
    draw_text(
        if show_grid { "Grid: ON" } else { "Grid: OFF" },
        toggle_x + 10.0,
        toggle_y + 20.0,
        18.0,
        WHITE,
    );
    if grid_hovered && is_mouse_button_pressed(MouseButton::Left) {
        show_grid = !show_grid;
    }

    // Target outline toggle
    let outline_x = toggle_x + toggle_gap;
    let outline_hovered =
        mouse.0 >= outline_x
        && mouse.0 <= outline_x + toggle_w
        && mouse.1 >= toggle_y
        && mouse.1 <= toggle_y + toggle_h;

    draw_rectangle(
        outline_x,
        toggle_y,
        toggle_w,
        toggle_h,
        if outline_hovered { BLUE } else { DARKGRAY },
    );
    draw_text(
        if show_target_outline { "Outline: ON" } else { "Outline: OFF" },
        outline_x + 8.0,
        toggle_y + 20.0,
        17.0,
        WHITE,
    );
    if outline_hovered && is_mouse_button_pressed(MouseButton::Left) {
        show_target_outline = !show_target_outline;
    }

   // Chunk boundaries toggle
    let chunk_x = toggle_x + toggle_gap;
    let chunk_y = toggle_y + 35.0;

    let chunk_hovered =
        mouse.0 >= chunk_x
        && mouse.0 <= chunk_x + toggle_w
        && mouse.1 >= chunk_y
        && mouse.1 <= chunk_y + toggle_h;

    draw_rectangle(
        chunk_x,
        chunk_y,
        toggle_w,
        toggle_h,
    if chunk_hovered { BLUE } else { DARKGRAY },
);

    draw_text(
    if show_chunk_bounds {
        "Chunks: ON"
    } else {
        "Chunks: OFF"
    },
    chunk_x + 8.0,
    chunk_y + 20.0,
    17.0,
    WHITE,
);

    if chunk_hovered && is_mouse_button_pressed(MouseButton::Left) {
    show_chunk_bounds = !show_chunk_bounds;
}
        // Crosshair color buttons
    let color_y = toggle_y + 70.0;
    let color_w = 95.0;
    let color_h = 28.0;
    let color_gap = 10.0;

    let white_x = toggle_x;
    let red_x = white_x + color_w + color_gap;
    let green_x = red_x + color_w + color_gap;
    let yellow_x = green_x + color_w + color_gap;

    let white_hovered =
        mouse.0 >= white_x
        && mouse.0 <= white_x + color_w
        && mouse.1 >= color_y
        && mouse.1 <= color_y + color_h;

    draw_rectangle(
        white_x, color_y, color_w, color_h,
        if white_hovered { BLUE } else { DARKGRAY },
    );
    draw_text("White", white_x + 20.0, color_y + 20.0, 16.0, WHITE);

    if white_hovered && is_mouse_button_pressed(MouseButton::Left) {
        crosshair_color = WHITE;
    }

    let red_hovered =
        mouse.0 >= red_x
        && mouse.0 <= red_x + color_w
        && mouse.1 >= color_y
        && mouse.1 <= color_y + color_h;

    draw_rectangle(
        red_x, color_y, color_w, color_h,
        if red_hovered { BLUE } else { DARKGRAY },
    );
    draw_text("Red", red_x + 30.0, color_y + 20.0, 16.0, WHITE);

    if red_hovered && is_mouse_button_pressed(MouseButton::Left) {
        crosshair_color = RED;
    }

    let green_hovered =
        mouse.0 >= green_x
        && mouse.0 <= green_x + color_w
        && mouse.1 >= color_y
        && mouse.1 <= color_y + color_h;

    draw_rectangle(
        green_x, color_y, color_w, color_h,
        if green_hovered { BLUE } else { DARKGRAY },
    );
    draw_text("Green", green_x + 25.0, color_y + 20.0, 16.0, WHITE);

    if green_hovered && is_mouse_button_pressed(MouseButton::Left) {
        crosshair_color = GREEN;
    }

    let yellow_hovered =
        mouse.0 >= yellow_x
        && mouse.0 <= yellow_x + color_w
        && mouse.1 >= color_y
        && mouse.1 <= color_y + color_h;

    draw_rectangle(
        yellow_x, color_y, color_w, color_h,
        if yellow_hovered { BLUE } else { DARKGRAY },
    );
    draw_text("Yellow", yellow_x + 22.0, color_y + 20.0, 16.0, WHITE);

    if yellow_hovered && is_mouse_button_pressed(MouseButton::Left) {
        crosshair_color = YELLOW;
    }

    let reset_x = panel_x + 20.0;
    let reset_y = panel_y + 600.0;
    let reset_w = 180.0;
    let reset_h = 38.0;
    let mouse = mouse_position();

    let reset_hovered =
        mouse.0 >= reset_x
        && mouse.0 <= reset_x + reset_w
        && mouse.1 >= reset_y
        && mouse.1 <= reset_y + reset_h;

    draw_rectangle(
        reset_x,
        reset_y,
        reset_w,
        reset_h,
        if reset_hovered { BLUE } else { DARKGRAY },
    );

    draw_text(
        "Reset defaults",
        reset_x + 15.0,
        reset_y + 25.0,
        20.0,
        WHITE,
    );

    if reset_hovered && is_mouse_button_pressed(MouseButton::Left) {
        look_sensitivity = DEFAULT_LOOK_SENSITIVITY;
        move_speed = DEFAULT_MOVE_SPEED;
        camera_speed_multiplier = DEFAULT_CAMERA_SPEED_MULTIPLIER;
        crosshair_size = DEFAULT_CROSSHAIR_SIZE;
        crosshair_color = DEFAULT_CROSSHAIR_COLOR;
        camera_fov = DEFAULT_CAMERA_FOV;
        wheel_vertical_speed = DEFAULT_WHEEL_VERTICAL_SPEED;
    }

        draw_text(
        "F1: close menu",
        panel_x + 245.0,
        reset_y + 25.0,
        18.0,
        LIGHTGRAY,
    );
    }

    if show_crosshair {
       draw_line(
        screen_w * 0.5 - crosshair_size,
        screen_h * 0.5,
        screen_w * 0.5 + crosshair_size,
        screen_h * 0.5,
        2.0,
        crosshair_color,
        );
        draw_line(
        screen_w * 0.5,
        screen_h * 0.5 - crosshair_size,
        screen_w * 0.5,
        screen_h * 0.5 + crosshair_size,
        2.0,
        crosshair_color,
        );
    }

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
