use std::collections::HashMap;

use macroquad::prelude::{Mesh, Vec3, Vertex};

use crate::hexel::{push_quad, terrain_color, HexCoord, HEX_HEIGHT, HEX_RADIUS};

#[derive(Debug)]
pub struct World {
    /// Each hexel exists independently at a coordinate and vertical layer.
    hexels: std::collections::HashSet<(HexCoord, u32)>,
}

impl World {
    
pub fn demo() -> Self {
    let mut world = Self {
        hexels: std::collections::HashSet::new(),
    };

    let radius: i32 = 8;

    for q in -radius..=radius {
        for r in -radius..=radius {
            if (q + r).abs() > radius {
                continue;
            }

            // Deterministic height variation across the island.
            let broad = (q * 3 + r * 2 + 24).rem_euclid(11) as u32;
            let detail = (q * 7 - r * 5 + 40).rem_euclid(3) as u32;

            let extra = if detail == 0 { 1 } else { 0 };
            let height = 1 + broad / 4 + extra;

            for layer in 0..height {
   	    world.hexels.insert((HexCoord::new(q, r), layer));
	    }
        }
    }

        world
}

    pub fn contains(&self, coord: HexCoord, layer: u32) -> bool {
    self.hexels.contains(&(coord, layer))
}

pub fn height(&self, coord: HexCoord) -> u32 {
    self.hexels
        .iter()
        .filter(|(hex_coord, _)| *hex_coord == coord)
        .map(|(_, layer)| layer + 1)
        .max()
        .unwrap_or(0)
}

pub fn add_hexel(&mut self, coord: HexCoord) {
    let mut layer = 0;
    while self.contains(coord, layer) {
        layer += 1;
    }
    self.hexels.insert((coord, layer));
}

pub fn add_hexel_at_layer(&mut self, coord: HexCoord, layer: u32) {
    self.hexels.insert((coord, layer));
}

pub fn remove_hexel_at_layer(&mut self, coord: HexCoord, layer: u32) {
    self.hexels.remove(&(coord, layer));
}
    

    /// Step along the view ray and test against a pointy-top hex footprint.
    
    /// Return the column hit by the view ray and the face hit:
    /// `None` for the top, or `Some(direction)` for a side.
        /// Find the nearest exposed hexel face hit by the view ray.
    /// Returns the coordinate, vertical layer, and side direction.
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
    ) -> Option<(HexCoord, u32, Option<usize>)> {
        let direction = direction.normalize_or_zero();
        if direction.length_squared() == 0.0 {
            return None;
        }

        let mut best: Option<(f32, HexCoord, u32, Option<usize>)> = None;

        for &(coord, layer) in &self.hexels {
            let bottom_y = layer as f32 * HEX_HEIGHT;
            let top_y = bottom_y + HEX_HEIGHT;
            let center = coord.world_xz();

            // Test the top face only when no hexel sits directly above it.
            if !self.contains(coord, layer + 1) && direction.y.abs() > 1.0e-6 {
                let t = (top_y - origin.y) / direction.y;

                if t >= 0.0 {
                    let point = origin + direction * t;
                    let dx = (point.x - center.x).abs();
                    let dz = (point.z - center.y).abs();

                    let inside_hex =
                        dx <= HEX_RADIUS * 0.866_025_4
                        && dz + dx * 0.577_350_3 <= HEX_RADIUS;

                    if inside_hex
                        && best.as_ref().is_none_or(|(best_t, _, _, _)| t < *best_t)
                    {
                        best = Some((t, coord, layer, None));
                    }
                }
            }
            // Test the exposed bottom face when there is space below.
            if layer > 0
                && !self.contains(coord, layer - 1)
                && direction.y.abs() > 1.0e-6
            {
                let t = (bottom_y - origin.y) / direction.y;

                if t >= 0.0 {
                    let point = origin + direction * t;
                    let dx = (point.x - center.x).abs();
                    let dz = (point.z - center.y).abs();

                    let inside_hex =
                        dx <= HEX_RADIUS * 0.866_025_4
                        && dz + dx * 0.577_350_3 <= HEX_RADIUS;

                    if inside_hex
                        && best.as_ref().is_none_or(|(best_t, _, _, _)| t < *best_t)
                    {
                        best = Some((t, coord, layer, Some(6)));
                    }
                }
            }
            // Test exposed vertical sides of this individual hexel.
            let corners = coord_corners(coord, bottom_y);

            for side in 0..6 {
                let neighbor_direction = match side {
                    0 => 5,
                    1 => 4,
                    2 => 3,
                    3 => 2,
                    4 => 1,
                    _ => 0,
                };

                if self.contains(coord.neighbor(neighbor_direction), layer) {
                    continue;
                }

                let a = corners[side];
                let b = corners[(side + 1) % 6];

                let edge_x = b.x - a.x;
                let edge_z = b.z - a.z;
                let denom = direction.x * edge_z - direction.z * edge_x;

                if denom.abs() < 1.0e-6 {
                    continue;
                }

                let rel_x = a.x - origin.x;
                let rel_z = a.z - origin.z;
                let t = (rel_x * edge_z - rel_z * edge_x) / denom;

                if t < 0.0 {
                    continue;
                }

                let point = origin + direction * t;

                if point.y < bottom_y || point.y > top_y {
                    continue;
                }

                let along = if edge_x.abs() > edge_z.abs() {
                    (point.x - a.x) / edge_x
                } else if edge_z.abs() > 1.0e-6 {
                    (point.z - a.z) / edge_z
                } else {
                    -1.0
                };

                if !(0.0..=1.0).contains(&along) {
                    continue;
                }

                if best.as_ref().is_none_or(|(best_t, _, _, _)| t < *best_t) {
                    best = Some((
                        t,
                        coord,
                        layer,
                        Some(neighbor_direction),
                    ));
                }
            }
        }

        best.map(|(_, coord, layer, side)| (coord, layer, side))
    }


    
    /// Build one mesh per occupied coordinate, keeping mesh sizes manageable.
    pub fn meshes(&self) -> Vec<Mesh> {
        let coords: std::collections::HashSet<HexCoord> =
            self.hexels.iter().map(|(coord, _)| *coord).collect();

        coords
            .into_iter()
            .map(|coord| self.column_mesh(coord))
            .collect()
    }

    fn column_mesh(&self, coord: HexCoord) -> Mesh {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let center = coord.world_xz();

        for &(_, layer) in self.hexels.iter().filter(|(c, _)| *c == coord) {
            let bottom_y = layer as f32 * HEX_HEIGHT;
            let top_y = bottom_y + HEX_HEIGHT;
            let bottom_corners = coord_corners(coord, bottom_y);
            let top_corners = coord_corners(coord, top_y);
            let color = terrain_color(coord, layer);

            // Top face: draw only if there is no hexel above.
            if !self.contains(coord, layer + 1) {
                let top_center = Vec3::new(center.x, top_y, center.y);

                for i in 0..6 {
                    push_triangle(
                        &mut vertices,
                        &mut indices,
                        [
                            top_center,
                            top_corners[(i + 1) % 6],
                            top_corners[i],
                        ],
                        color,
                    );
                }
            }

            // Bottom face: draw only if there is no hexel below.
            if layer == 0 || !self.contains(coord, layer - 1) {
                let bottom_center = Vec3::new(center.x, bottom_y, center.y);

                for i in 0..6 {
                    push_triangle(
                        &mut vertices,
                        &mut indices,
                        [
                            bottom_center,
                            bottom_corners[i],
                            bottom_corners[(i + 1) % 6],
                        ],
                        shade(color, 0.65),
                    );
                }
            }

            // Draw a side only when the neighboring hexel at this layer is absent.
            for direction in 0..6 {
                if self.contains(coord.neighbor(direction), layer) {
                    continue;
                }

                // Axial direction 0 matches edge 5 -> 0.
                let edge = (5 + 6 - direction) % 6;
                let a_bottom = bottom_corners[edge];
                let b_bottom = bottom_corners[(edge + 1) % 6];
                let a_top = top_corners[edge];
                let b_top = top_corners[(edge + 1) % 6];

                push_quad(
                    &mut vertices,
                    &mut indices,
                    [b_bottom, a_bottom, a_top, b_top],
                    shade(color, 0.78 + direction as f32 * 0.035),
                );
            }
        }

                Mesh {
            vertices,
            indices,
            texture: None,
        }
    }
}

/// Compute corners from integer grid coordinates

/// Compute corners from integer grid coordinates rather than adding trigonometric
/// offsets to each column center. Adjacent columns therefore produce bit-identical
/// positions for shared vertices, avoiding tiny rasterization cracks at seams.
pub(crate) fn coord_corners(coord: HexCoord, y: f32) -> [Vec3; 6] {
    const X_OFFSETS: [i32; 6] = [1, 0, -1, -1, 0, 1];
    const Z_OFFSETS: [i32; 6] = [1, 2, 1, -1, -2, -1];
    std::array::from_fn(|i| {
        let x_twice = 2 * coord.q + coord.r + X_OFFSETS[i];
        let z_sixths = 3 * coord.r + Z_OFFSETS[i];
        Vec3::new(
            x_twice as f32 * 0.5,
            y,
            z_sixths as f32 * (3.0_f32.sqrt() / 6.0),
        )
    })
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
        assert!(world.hexels.len() > 150);
        assert!(world.height(HexCoord::new(0, 0)) > 0);
    }

    #[test]
    fn editing_changes_column_height() {
        let mut world = World::demo();
        let coord = HexCoord::new(0, 0);
        let before = world.height(coord);
        world.add_hexel(coord);
        assert_eq!(world.height(coord), before + 1);
        world.remove_hexel_at_layer(coord, before);
        assert_eq!(world.height(coord), before);
    }

    #[test]
    fn empty_columns_can_be_added_and_removed() {
        let mut world = World::demo();
        let coord = HexCoord::new(30, -30);
        assert_eq!(world.height(coord), 0);
        world.add_hexel(coord);
        assert_eq!(world.height(coord), 1);
        world.remove_hexel_at_layer(coord, 0);
        assert_eq!(world.height(coord), 0);
    }

    #[test]
    fn all_six_neighbors_share_bit_identical_edges() {
        let origin = HexCoord::new(0, 0);
        let a = coord_corners(origin, 1.0);
        let edge_pairs = [
            (5, 0, 3, 2),
            (4, 5, 2, 1),
            (3, 4, 1, 0),
            (2, 3, 0, 5),
            (1, 2, 5, 4),
            (0, 1, 4, 3),
        ];
        for (direction, (a0, a1, b0, b1)) in edge_pairs.into_iter().enumerate() {
            let b = coord_corners(origin.neighbor(direction), 1.0);
            assert_eq!(a[a0], b[b0], "first shared corner in direction {direction}");
            assert_eq!(
                a[a1], b[b1],
                "second shared corner in direction {direction}"
            );
        }
    }

    #[test]
    fn editing_a_column_preserves_neighbor_mesh_edges() {
        let mut world = World::demo();
        let coord = HexCoord::new(0, 0);
        world.add_hexel(coord);
        world.remove_hexel_at_layer(coord, world.height(coord) - 1);
        let a = coord_corners(coord, 1.0);
        for direction in 0..6 {
            let neighbor = coord.neighbor(direction);
            let b = coord_corners(neighbor, 1.0);
            let (a0, a1, b0, b1) = match direction {
                0 => (5, 0, 3, 2),
                1 => (4, 5, 2, 1),
                2 => (3, 4, 1, 0),
                3 => (2, 3, 0, 5),
                4 => (1, 2, 5, 4),
                _ => (0, 1, 4, 3),
            };
            assert_eq!(a[a0], b[b0]);
            assert_eq!(a[a1], b[b1]);
        }
    }
}
