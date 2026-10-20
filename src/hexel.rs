use macroquad::prelude::{Color, Vec2, Vec3, Vertex};

pub const HEX_RADIUS: f32 = 0.58;
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

    pub const fn neighbor(self, direction: usize) -> Self {
        const DIRECTIONS: [(i32, i32); 6] = [
            (1, 0),
            (1, -1),
            (0, -1),
            (-1, 0),
            (-1, 1),
            (0, 1),
        ];
        let (dq, dr) = DIRECTIONS[direction % 6];
        Self::new(self.q + dq, self.r + dr)
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
            color,
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
