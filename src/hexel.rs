use macroquad::prelude::{Color, Vec2, Vec3, Vec4, Vertex};

pub const HEX_RADIUS: f32 = 0.577_350_26;
pub const HEX_HEIGHT: f32 = 1.0;
const SQRT_3: f32 = 1.732_050_8;

/// Axial coordinates for a pointy-top hexagonal grid.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

impl HexCoord {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Convert axial coordinates to world-space X/Z coordinates.
    pub fn world_xz(self) -> Vec2 {
        Vec2::new(
            HEX_RADIUS * SQRT_3 * (self.q as f32 + self.r as f32 * 0.5),
            HEX_RADIUS * 1.5 * self.r as f32,
        )
    }

    /// Find the nearest axial cell for a world-space X/Z position.
    pub fn from_world_xz(x: f32, z: f32) -> Self {
        let fractional_r = z / (HEX_RADIUS * 1.5);
        let fractional_q = x / (HEX_RADIUS * SQRT_3) - fractional_r * 0.5;

        // Axial coordinates are rounded together with the implied cube axis,
        // which keeps the result on the nearest hex instead of rounding each
        // axis independently.
        let cube_x = fractional_q;
        let cube_z = fractional_r;
        let cube_y = -cube_x - cube_z;
        let mut rounded_x = cube_x.round();
        let mut rounded_y = cube_y.round();
        let mut rounded_z = cube_z.round();

        let x_error = (rounded_x - cube_x).abs();
        let y_error = (rounded_y - cube_y).abs();
        let z_error = (rounded_z - cube_z).abs();

        if x_error > y_error && x_error > z_error {
            rounded_x = -rounded_y - rounded_z;
        } else if y_error > z_error {
            rounded_y = -rounded_x - rounded_z;
        } else {
            rounded_z = -rounded_x - rounded_y;
        }

        let _ = rounded_y;
        Self::new(rounded_x as i32, rounded_z as i32)
    }

    pub const fn neighbor(self, direction: usize) -> Self {
        const DIRECTIONS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];
        let (dq, dr) = DIRECTIONS[direction % 6];
        Self::new(self.q + dq, self.r + dr)
    }
}

#[cfg(test)]
mod coordinate_tests {
    use super::HexCoord;

    #[test]
    fn world_position_maps_back_to_axial_cell() {
        for q in -20..=20 {
            for r in -20..=20 {
                let coord = HexCoord::new(q, r);
                let world = coord.world_xz();
                assert_eq!(HexCoord::from_world_xz(world.x, world.y), coord);
            }
        }
    }
}

pub fn terrain_color(coord: HexCoord, y: u32) -> Color {
    let variation = ((coord.q * 13 + coord.r * 7).rem_euclid(5)) as f32 * 0.025;
    match y {
        0 => Color::new(0.30, 0.25 + variation, 0.18, 1.0),
        1 => Color::new(0.25, 0.48 + variation, 0.24, 1.0),
        _ => Color::new(0.20, 0.40 + variation, 0.22, 1.0),
    }
}

/// Add a flat-shaded quad to a CPU-built mesh.
pub fn push_quad(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    corners: [Vec3; 4],
    color: Color,
) {
    let base = vertices.len() as u16;
    for (corner, uv) in corners.into_iter().zip([
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ]) {
        vertices.push(Vertex {
            position: corner,
            uv,
            color: color.into(),
            normal: Vec4::ZERO,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Corners of a horizontal hexagon, in consistent winding order.
pub fn hex_corners(center_x: f32, center_z: f32, y: f32) -> [Vec3; 6] {
    std::array::from_fn(|i| {
        let angle = (30.0 + i as f32 * 60.0).to_radians();
        Vec3::new(
            center_x + HEX_RADIUS * angle.cos(),
            y,
            center_z + HEX_RADIUS * angle.sin(),
        )
    })
}
