use std::collections::{HashMap, HashSet};

use macroquad::prelude::{Mesh, Vec3, Vertex};

use crate::hexel::{push_quad, terrain_color, HexCoord, HEX_HEIGHT, HEX_RADIUS};

const CHUNK_SIZE: i32 = 16;
const CHUNK_HEIGHT: u32 = 16;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ChunkCoord {
    q: i32,
    r: i32,
    y: u32,
}

impl ChunkCoord {
    fn from_hex(coord: HexCoord, layer: u32) -> Self {
        Self {
            q: coord.q.div_euclid(CHUNK_SIZE),
            r: coord.r.div_euclid(CHUNK_SIZE),
            y: layer / CHUNK_HEIGHT,
        }
    }
}

pub struct World {
    /// Each hexel exists independently at a coordinate and vertical layer.
    hexels: HashSet<(HexCoord, u32)>,

    /// Hexels grouped by their owning chunk for faster mesh rebuilding.
    chunk_hexels: HashMap<ChunkCoord, HashSet<(HexCoord, u32)>>,

    /// Chunks whose meshes need to be rebuilt.
    dirty_chunks: HashSet<ChunkCoord>,

    /// Cached terrain mesh for each chunk.
    chunk_meshes: HashMap<ChunkCoord, Mesh>,

    /// Number of chunk meshes rebuilt during the latest frame.
    chunks_rebuilt_this_frame: usize,
}

impl World {
    pub fn demo() -> Self {
        let mut world = Self {
            hexels: HashSet::new(),
            chunk_hexels: HashMap::new(),
            dirty_chunks: HashSet::new(),
            chunk_meshes: HashMap::new(),
            chunks_rebuilt_this_frame: 0,
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
                    let coord = HexCoord::new(q, r);
                    world.hexels.insert((coord, layer));

                    let chunk = ChunkCoord::from_hex(coord, layer);
                    world
                        .chunk_hexels
                        .entry(chunk)
                        .or_default()
                        .insert((coord, layer));
                }
            }
        }

        world
    }

    /// Generate a large, deterministic hexel island for performance testing.
    pub fn stress_test(radius: i32) -> Self {
        let mut world = Self {
            hexels: HashSet::new(),
            chunk_hexels: HashMap::new(),
            dirty_chunks: HashSet::new(),
            chunk_meshes: HashMap::new(),
            chunks_rebuilt_this_frame: 0,
        };

        for q in -radius..=radius {
            for r in -radius..=radius {
                if (q + r).abs() > radius {
                    continue;
                }

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

    fn mark_dirty(&mut self, coord: HexCoord, layer: u32) {
        let chunk = ChunkCoord::from_hex(coord, layer);
        self.dirty_chunks.insert(chunk);

        // Neighboring chunks may need new faces at a shared boundary.
        for direction in 0..6 {
            let neighbor = coord.neighbor(direction);
            let neighbor_chunk = ChunkCoord::from_hex(neighbor, layer);

            if neighbor_chunk != chunk {
                self.dirty_chunks.insert(neighbor_chunk);
            }
        }
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

    pub fn total_hexels(&self) -> usize {
        self.hexels.len()
    }

    pub fn occupied_chunk_count(&self) -> usize {
        self.chunk_meshes.len()
    }

    pub fn chunks_rebuilt_this_frame(&self) -> usize {
        self.chunks_rebuilt_this_frame
    }

    pub fn add_hexel(&mut self, coord: HexCoord) {
        let mut layer = 0;
        while self.contains(coord, layer) {
            layer += 1;
        }

        if self.hexels.insert((coord, layer)) {
            self.mark_dirty(coord, layer);
        }
    }

    pub fn add_hexel_at_layer(&mut self, coord: HexCoord, layer: u32) {
        if self.hexels.insert((coord, layer)) {
            let chunk = ChunkCoord::from_hex(coord, layer);

            self.chunk_hexels
                .entry(chunk)
                .or_default()
                .insert((coord, layer));

            self.mark_dirty(coord, layer);
        }
    }

    pub fn remove_hexel_at_layer(&mut self, coord: HexCoord, layer: u32) {
        if self.hexels.remove(&(coord, layer)) {
            let chunk = ChunkCoord::from_hex(coord, layer);

            if let Some(hexels) = self.chunk_hexels.get_mut(&chunk) {
                hexels.remove(&(coord, layer));

                if hexels.is_empty() {
                    self.chunk_hexels.remove(&chunk);
                }
            }

            self.mark_dirty(coord, layer);
        }
    }

    /// Step along the view ray and test against a pointy-top hex footprint.

    /// Return the column hit by the view ray and the face hit:
    /// `None` for the top, or `Some(direction)` for a side.
    /// Find the nearest exposed hexel face hit by the view ray.
    /// Returns the coordinate, vertical layer, and side direction.
    pub fn raycast(&self, origin: Vec3, direction: Vec3) -> Option<(HexCoord, u32, Option<usize>)> {
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
                        dx <= HEX_RADIUS * 0.866_025_4 && dz + dx * 0.577_350_3 <= HEX_RADIUS;

                    if inside_hex && best.as_ref().is_none_or(|(best_t, _, _, _)| t < *best_t) {
                        best = Some((t, coord, layer, None));
                    }
                }
            }
            // Test the exposed bottom face when there is space below.
            if layer > 0 && !self.contains(coord, layer - 1) && direction.y.abs() > 1.0e-6 {
                let t = (bottom_y - origin.y) / direction.y;

                if t >= 0.0 {
                    let point = origin + direction * t;
                    let dx = (point.x - center.x).abs();
                    let dz = (point.z - center.y).abs();

                    let inside_hex =
                        dx <= HEX_RADIUS * 0.866_025_4 && dz + dx * 0.577_350_3 <= HEX_RADIUS;

                    if inside_hex && best.as_ref().is_none_or(|(best_t, _, _, _)| t < *best_t) {
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
                    best = Some((t, coord, layer, Some(neighbor_direction)));
                }
            }
        }

        best.map(|(_, coord, layer, side)| (coord, layer, side))
    }

    /// Build one mesh per occupied coordinate, keeping mesh sizes manageable.

    /// Build one mesh per occupied chunk, keeping mesh sizes manageable.
    pub fn rebuild_dirty_chunks(&mut self) {
        self.chunks_rebuilt_this_frame = 0;

        let occupied_chunks: HashSet<ChunkCoord> = self.chunk_hexels.keys().copied().collect();

        self.chunk_meshes
            .retain(|chunk, _| occupied_chunks.contains(chunk));

        self.dirty_chunks
            .retain(|chunk| occupied_chunks.contains(chunk));

        let dirty: Vec<ChunkCoord> = self.dirty_chunks.drain().collect();

        for chunk in dirty {
            let mesh = self.chunk_mesh(chunk);
            self.chunk_meshes.insert(chunk, mesh);
            self.chunks_rebuilt_this_frame += 1;
        }

        for chunk in occupied_chunks {
            if !self.chunk_meshes.contains_key(&chunk) {
                let mesh = self.chunk_mesh(chunk);
                self.chunk_meshes.insert(chunk, mesh);
                self.chunks_rebuilt_this_frame += 1;
            }
        }
    }

    pub fn meshes_with_coords(&self) -> impl Iterator<Item = ((i32, i32, u32), &Mesh)> {
        self.chunk_meshes
            .iter()
            .map(|(chunk, mesh)| ((chunk.q, chunk.r, chunk.y), mesh))
    }

    /// Return the coordinates of chunks containing hexels.

    pub fn occupied_chunk_coords(&self) -> Vec<(i32, i32, u32)> {
        self.hexels
            .iter()
            .map(|(coord, layer)| ChunkCoord::from_hex(*coord, *layer))
            .collect::<HashSet<_>>()
            .into_iter()
            .map(|chunk| (chunk.q, chunk.r, chunk.y))
            .collect()
    }

    fn chunk_mesh(&self, chunk: ChunkCoord) -> Mesh {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        if let Some(hexels) = self.chunk_hexels.get(&chunk) {
            for &(coord, layer) in hexels {
                self.append_hexel_faces(coord, layer, &mut vertices, &mut indices);
            }
        }

        Mesh {
            vertices,
            indices,
            texture: None,
        }
    }

    fn append_hexel_faces(
        &self,
        coord: HexCoord,
        layer: u32,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u16>,
    ) {
        let bottom_y = layer as f32 * HEX_HEIGHT;
        let top_y = bottom_y + HEX_HEIGHT;
        let center = coord.world_xz();
        let bottom_corners = coord_corners(coord, bottom_y);
        let top_corners = coord_corners(coord, top_y);
        let color = terrain_color(coord, layer);

        // Top face: draw only if there is no hexel above.
        if !self.contains(coord, layer + 1) {
            let top_center = Vec3::new(center.x, top_y, center.y);

            for i in 0..6 {
                push_triangle(
                    vertices,
                    indices,
                    [top_center, top_corners[(i + 1) % 6], top_corners[i]],
                    color,
                );
            }
        }

        // Bottom face: draw only if there is no hexel below.
        if layer == 0 || !self.contains(coord, layer - 1) {
            let bottom_center = Vec3::new(center.x, bottom_y, center.y);

            for i in 0..6 {
                push_triangle(
                    vertices,
                    indices,
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

            let edge = (5 + 6 - direction) % 6;
            let a_bottom = bottom_corners[edge];
            let b_bottom = bottom_corners[(edge + 1) % 6];
            let a_top = top_corners[edge];
            let b_top = top_corners[(edge + 1) % 6];

            push_quad(
                vertices,
                indices,
                [b_bottom, a_bottom, a_top, b_top],
                shade(color, 0.78 + direction as f32 * 0.035),
            );
        }
    }

    // Close impl World before defining the standalone helper.
}

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
#[path = "tests/world_tests.rs"]
mod tests;
