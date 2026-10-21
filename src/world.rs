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
                    // Keep the starting landscape low and connected; players can
                    // build taller columns with left-click after capturing the mouse.
                    world.heights.insert(HexCoord::new(q, r), 1);
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

    /// Step along the view ray and test against a pointy-top hex footprint.
    pub fn raycast(&self, origin: Vec3, direction: Vec3) -> Option<HexCoord> {
        let mut distance = 0.0;
        while distance < 32.0 {
            let point = origin + direction * distance;
            if point.y >= 0.0 {
                let candidate = self.heights.iter().find_map(|(coord, height)| {
                    let center = coord.world_xz();
                    let dx = (point.x - center.x).abs();
                    let dz = (point.z - center.y).abs();
                    let inside_hex = dx <= HEX_RADIUS * 0.866_025_4
                        && dz + dx * 0.577_350_3 <= HEX_RADIUS;
                    (inside_hex && point.y <= *height as f32 * HEX_HEIGHT)
                        .then_some(*coord)
                });
                if candidate.is_some() {
                    return candidate;
                }
            }
            distance += 0.04;
        }
        None
    }

    /// Build one mesh per column so u16 indices can never overflow as the world grows.
    /// Internal faces between adjacent columns are omitted.
    pub fn meshes(&self) -> Vec<Mesh> {
        self.heights
            .keys()
            .map(|coord| self.column_mesh(*coord))
            .collect()
    }

    fn column_mesh(&self, coord: HexCoord) -> Mesh {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let height = self.height(coord);

        if height == 0 {
            return Mesh { vertices, indices, texture: None };
        }

        let center = coord.world_xz();
        for y in 0..height {
                let corners = hex_corners(center.x, center.y, (y as f32 + 1.0) * HEX_HEIGHT);
                let color = terrain_color(coord, y);

                if y + 1 == height {
                    let top_center = Vec3::new(center.x, (y as f32 + 1.0) * HEX_HEIGHT, center.y);
                    for i in 0..6 {
                        push_triangle(
                            &mut vertices,
                            &mut indices,
                            [top_center, corners[(i + 1) % 6], corners[i]],
                            color,
                        );
                    }
                }

                // Axial direction 0 is +X, matching edge 5 -> 0.
                for direction in 0..6 {
                    if self.height(coord.neighbor(direction)) > y {
                        continue;
                    }
                    let edge = (5 + 6 - direction) % 6;
                    let a = corners[edge];
                    let b = corners[(edge + 1) % 6];
                    push_quad(
                        &mut vertices,
                        &mut indices,
                        [
                            Vec3::new(b.x, y as f32 * HEX_HEIGHT, b.z),
                            Vec3::new(a.x, y as f32 * HEX_HEIGHT, a.z),
                            a,
                            b,
                        ],
                        shade(color, 0.78 + direction as f32 * 0.035),
                    );
                }
        }

        Mesh { vertices, indices, texture: None }
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

    #[test]
    fn adjacent_hexes_share_an_edge() {
        let a = hex_corners(0.0, 0.0, 1.0);
        let east = HexCoord::new(1, 0).world_xz();
        let b = hex_corners(east.x, east.y, 1.0);
        assert!((a[5].x - b[3].x).abs() < 0.0001);
        assert!((a[5].z - b[3].z).abs() < 0.0001);
        assert!((a[0].x - b[2].x).abs() < 0.0001);
        assert!((a[0].z - b[2].z).abs() < 0.0001);
    }
}
