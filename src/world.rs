use std::collections::HashMap;

use macroquad::prelude::{Mesh, Vec3, Vertex};

use crate::hexel::{hex_corners, push_quad, terrain_color, HexCoord, HEX_HEIGHT, HEX_RADIUS};

#[derive(Debug)]
pub struct World {
    /// Number of stacked unit-height hexels at each axial coordinate.
    heights: HashMap<HexCoord, u32>,
}

impl World {
    pub fn demo() -> Self {
        let mut world = Self {
            heights: HashMap::new(),
        };
        let radius: i32 = 8;
        for q in -radius..=radius {
            for r in -radius..=radius {
                if (q + r).abs() <= radius {
                    let ripple = (q * 3 + r * 5 + q * r).rem_euclid(4) as u32;
                    let edge = ((q.abs() + r.abs() + (q + r).abs()) / 5).min(2) as u32;
                    world
                        .heights
                        .insert(HexCoord::new(q, r), 2 + ripple - edge.min(1));
                }
            }
        }
        world
    }

    pub fn height(&self, coord: HexCoord) -> u32 {
        self.heights.get(&coord).copied().unwrap_or(0)
    }

    pub fn add_hexel(&mut self, coord: HexCoord) {
        let next = self.height(coord).saturating_add(1).min(32);
        self.heights.insert(coord, next);
    }

    pub fn remove_hexel(&mut self, coord: HexCoord) {
        let height = self.height(coord);
        if height <= 1 {
            self.heights.remove(&coord);
        } else {
            self.heights.insert(coord, height - 1);
        }
    }

    /// Approximate raycast by stepping through the view ray and testing hex footprints.
    pub fn raycast(&self, origin: Vec3, direction: Vec3) -> Option<HexCoord> {
        let mut distance = 0.0;
        while distance < 16.0 {
            let point = origin + direction * distance;
            if point.y >= 0.0 {
                let candidate = self.heights.iter().find_map(|(coord, height)| {
                    let center = coord.world_xz();
                    let dx = point.x - center.x;
                    let dz = point.z - center.y;
                    (dx * dx + dz * dz < HEX_RADIUS * HEX_RADIUS * 0.78
                        && point.y <= *height as f32 * HEX_HEIGHT)
                        .then_some(*coord)
                });
                if candidate.is_some() {
                    return candidate;
                }
            }
            distance += 0.12;
        }
        None
    }

    /// Build one combined, flat-shaded mesh. Neighboring internal faces are omitted.
    pub fn mesh(&self) -> Mesh {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();

        for (coord, height) in &self.heights {
            let center = coord.world_xz();
            for y in 0..*height {
                let corners = hex_corners(center.x, center.y, (y as f32 + 1.0) * HEX_HEIGHT);
                let color = terrain_color(*coord, y);

                // Only the top of a column is visible.
                if y + 1 == *height {
                    let top_center = Vec3::new(center.x, (y as f32 + 1.0) * HEX_HEIGHT, center.y);
                    for i in 0..6 {
                        push_triangle(
                            &mut vertices,
                            &mut indices,
                            [top_center, corners[i], corners[(i + 1) % 6]],
                            color,
                        );
                    }
                }

                // Each axial direction corresponds to one hex edge.
                for direction in 0..6 {
                    if self.height(coord.neighbor(direction)) > y {
                        continue;
                    }
                    let edge = (6 - direction) % 6;
                    let a = corners[edge];
                    let b = corners[(edge + 1) % 6];
                    push_quad(
                        &mut vertices,
                        &mut indices,
                        [
                            Vec3::new(a.x, y as f32 * HEX_HEIGHT, a.z),
                            Vec3::new(b.x, y as f32 * HEX_HEIGHT, b.z),
                            b,
                            a,
                        ],
                        shade(color, 0.78 + direction as f32 * 0.035),
                    );
                }
            }
        }

        Mesh {
            vertices,
            indices,
            texture: None,
        }
    }
}

fn push_triangle(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    corners: [Vec3; 3],
    color: macroquad::prelude::Color,
) {
    let base = vertices.len() as u16;
    for (corner, uv) in corners.into_iter().zip([
        macroquad::prelude::Vec2::new(0.0, 0.0),
        macroquad::prelude::Vec2::new(1.0, 0.0),
        macroquad::prelude::Vec2::new(0.5, 1.0),
    ]) {
        vertices.push(Vertex {
            position: corner,
            uv,
            color: color.into(),
            normal: macroquad::prelude::Vec4::ZERO,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2]);
}

fn shade(color: macroquad::prelude::Color, factor: f32) -> macroquad::prelude::Color {
    macroquad::prelude::Color::new(
        (color.r * factor).min(1.0),
        (color.g * factor).min(1.0),
        (color.b * factor).min(1.0),
        color.a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_world_has_many_columns() {
        let world = World::demo();
        assert!(world.heights.len() > 150);
        assert!(world.height(HexCoord::new(0, 0)) > 0);
    }

    #[test]
    fn editing_changes_column_height() {
        let mut world = World::demo();
        let coord = HexCoord::new(0, 0);
        let before = world.height(coord);
        world.add_hexel(coord);
        assert_eq!(world.height(coord), before + 1);
        world.remove_hexel(coord);
        assert_eq!(world.height(coord), before);
    }

    #[test]
    fn empty_columns_can_be_added_and_removed() {
        let mut world = World::demo();
        let coord = HexCoord::new(30, -30);
        assert_eq!(world.height(coord), 0);
        world.add_hexel(coord);
        assert_eq!(world.height(coord), 1);
        world.remove_hexel(coord);
        assert_eq!(world.height(coord), 0);
    }
}
