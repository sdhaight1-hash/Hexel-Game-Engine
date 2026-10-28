//! Spherical placement for the engine's fixed 16×16×16 Hexel chunks.
//!
//! A chunk remains a 4,096-cell axial-hex prism volume. This module maps its
//! horizontal chunk address onto a tangent frame on the globe; it does not
//! replace the chunk with a square terrain patch.

use glam::{DVec3, DVec2};

use crate::{ChunkPos, CHUNK_Q, CHUNK_R, CHUNK_Y, planetary_frame::PlanetaryFrame};

/// Stable identity for a chunk anchored to a point on the planetary crust.
/// q/r select the local horizontal chunk grid and y selects its radial layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanetHexChunkKey {
    pub anchor_cell: u32,
    pub q: i32,
    pub r: i32,
    pub y: i32,
}

/// Planet-space placement for one fixed-size Hexel chunk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetHexChunkPlacement {
    pub key: PlanetHexChunkKey,
    pub frame: PlanetaryFrame,
    pub chunk: ChunkPos,
}

impl PlanetHexChunkPlacement {
    pub const CELLS_PER_CHUNK: usize =
        (CHUNK_Q * CHUNK_R * CHUNK_Y) as usize;

    pub fn new(anchor_cell: u32, frame: PlanetaryFrame, chunk: ChunkPos) -> Self {
        Self {
            key: PlanetHexChunkKey {
                anchor_cell,
                q: chunk.q,
                r: chunk.r,
                y: chunk.y,
            },
            frame,
            chunk,
        }
    }

    /// Map a local metre-scale point to the planet's double-precision coordinates.
    /// The frame origin lies on the crust; +Y in chunk space is radial.
    pub fn local_to_planet(self, local: DVec3) -> DVec3 {
        self.frame.origin
            + self.frame.east * local.x
            + self.frame.up * local.y
            + self.frame.north * local.z
    }

    /// Convert a planet-space point into the chunk's tangent coordinates.
    pub fn planet_to_local(self, point: DVec3) -> DVec3 {
        let delta = point - self.frame.origin;
        DVec3::new(
            delta.dot(self.frame.east),
            delta.dot(self.frame.up),
            delta.dot(self.frame.north),
        )
    }

    /// Approximate the surface point for a horizontal chunk offset in metres.
    /// The caller can re-anchor to a neighbouring crust cell when this offset
    /// crosses a topology-cell boundary.
    pub fn surface_offset(self, east_metres: f64, north_metres: f64) -> DVec3 {
        self.frame.move_tangent(east_metres, north_metres)
    }
}

/// Chunk-local horizontal centre in metres for the engine's axial hex grid.
pub fn hex_chunk_horizontal_origin(chunk: ChunkPos, hex_radius_metres: f64) -> DVec2 {
    let q = (chunk.q * CHUNK_Q) as f64;
    let r = (chunk.r * CHUNK_R) as f64;
    let x = hex_radius_metres * 3.0_f64.sqrt() * (q + r * 0.5);
    let z = hex_radius_metres * 1.5 * r;
    DVec2::new(x, z)
}

//! On-demand cube-sphere terrain patches for planetary surface streaming.
//!
//! Each patch is generated independently from a face + integer chunk coordinate.
//! Shared borders sample the same normalized cube-sphere directions, allowing a
//! streaming manager to load/unload patches around the player without building a
//! single planet-sized mesh. Positions stay in f64 until the renderer rebases them
//! around the camera before converting to GPU precision.

use glam::DVec3;

use crate::planet::{Biome, PlanetGenerator};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CubeFace {
    PositiveX,
    NegativeX,
    PositiveY,
    NegativeY,
    PositiveZ,
    NegativeZ,
}

impl CubeFace {
    pub fn basis(self) -> (DVec3, DVec3, DVec3) {
        // (face normal, increasing-u axis, increasing-v axis). All bases are
        // axis-aligned so adjacent faces describe identical shared edge directions.
        match self {
            Self::PositiveX => (DVec3::X, DVec3::Z, DVec3::Y),
            Self::NegativeX => (-DVec3::X, -DVec3::Z, DVec3::Y),
            Self::PositiveY => (DVec3::Y, DVec3::X, -DVec3::Z),
            Self::NegativeY => (-DVec3::Y, DVec3::X, DVec3::Z),
            Self::PositiveZ => (DVec3::Z, -DVec3::X, DVec3::Y),
            Self::NegativeZ => (-DVec3::Z, DVec3::X, DVec3::Y),
        }
    }
}

/// Return a bounded set of cube-sphere patches around a unit-sphere direction.
///
/// Unlike scanning the entire face grid, this planner remains O(r^2) as the
/// planet's patch resolution increases. Patches that cross a cube-face seam are
/// remapped through their direction and deduplicated by canonical face address.
pub fn chunk_keys_near_direction(
    direction: DVec3,
    chunks_per_face: u32,
    radius_in_chunks: u32,
) -> Vec<PlanetChunkKey> {
    use std::collections::BTreeSet;

    if chunks_per_face == 0 || !direction.is_finite() || direction.length_squared() <= f64::EPSILON {
        return Vec::new();
    }

    let direction = direction.normalize();
    let (face, u, v) = project_direction_to_face(direction);
    let grid = chunks_per_face as f64;
    let center_x = (((u + 1.0) * 0.5 * grid).floor() as i64)
        .clamp(0, chunks_per_face as i64 - 1);
    let center_y = (((v + 1.0) * 0.5 * grid).floor() as i64)
        .clamp(0, chunks_per_face as i64 - 1);
    let (normal, axis_u, axis_v) = face.basis();
    let radius = radius_in_chunks as i64;
    let mut keys = BTreeSet::new();

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let x = center_x + dx;
            let y = center_y + dy;
            let patch_u = -1.0 + 2.0 * (x as f64 + 0.5) / grid;
            let patch_v = -1.0 + 2.0 * (y as f64 + 0.5) / grid;
            let patch_direction = (normal + axis_u * patch_u + axis_v * patch_v).normalize();
            let (mapped_face, mapped_u, mapped_v) = project_direction_to_face(patch_direction);
            let mapped_x = (((mapped_u + 1.0) * 0.5 * grid).floor() as i64)
                .clamp(0, chunks_per_face as i64 - 1) as u32;
            let mapped_y = (((mapped_v + 1.0) * 0.5 * grid).floor() as i64)
                .clamp(0, chunks_per_face as i64 - 1) as u32;
            keys.insert(PlanetChunkKey { face: mapped_face, x: mapped_x, y: mapped_y });
        }
    }
    keys.into_iter().collect()
}

fn project_direction_to_face(direction: DVec3) -> (CubeFace, f64, f64) {
    let a = direction.abs();
    let face = if a.x >= a.y && a.x >= a.z {
        if direction.x >= 0.0 { CubeFace::PositiveX } else { CubeFace::NegativeX }
    } else if a.y >= a.z {
        if direction.y >= 0.0 { CubeFace::PositiveY } else { CubeFace::NegativeY }
    } else if direction.z >= 0.0 {
        CubeFace::PositiveZ
    } else {
        CubeFace::NegativeZ
    };
    let (normal, axis_u, axis_v) = face.basis();
    let denominator = direction.dot(normal).abs().max(f64::MIN_POSITIVE);
    (face, direction.dot(axis_u) / denominator, direction.dot(axis_v) / denominator)
}

/// Integer patch address. x/y range from 0 to chunks_per_face - 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanetChunkKey {
    pub face: CubeFace,
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetChunkError {
    InvalidChunkGrid,
    InvalidMeshResolution,
    ChunkOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetChunkVertex {
    /// Planet-centered terrain position. Keep f64 until camera-relative rebasing.
    pub position: DVec3,
    /// Radial normal approximation; detailed terrain normals can be derived from triangles.
    pub normal: DVec3,
    pub biome: Biome,
    pub elevation: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanetChunkMesh {
    pub key: PlanetChunkKey,
    pub resolution: u32,
    pub vertices: Vec<PlanetChunkVertex>,
    /// Triangle-list indices with outward-facing winding.
    pub indices: Vec<u32>,
}

impl PlanetChunkMesh {
    /// Build one square patch on the six-face cube-sphere.
    ///
    /// `chunks_per_face` controls geographic patch size; `resolution` controls
    /// vertices per patch edge. Increase detail near the player by requesting finer
    /// patches in a streaming policy rather than increasing one global planet mesh.
    pub fn generate(
        terrain: &PlanetGenerator,
        key: PlanetChunkKey,
        chunks_per_face: u32,
        resolution: u32,
        season_phase: f64,
    ) -> Result<Self, PlanetChunkError> {
        if chunks_per_face == 0 {
            return Err(PlanetChunkError::InvalidChunkGrid);
        }
        if resolution == 0 {
            return Err(PlanetChunkError::InvalidMeshResolution);
        }
        if key.x >= chunks_per_face || key.y >= chunks_per_face {
            return Err(PlanetChunkError::ChunkOutOfRange);
        }

        let (normal, axis_u, axis_v) = key.face.basis();
        let side = resolution + 1;
        let mut vertices = Vec::with_capacity((side * side) as usize);
        for row in 0..=resolution {
            let local_v = row as f64 / resolution as f64;
            let v = -1.0 + 2.0 * (key.y as f64 + local_v) / chunks_per_face as f64;
            for column in 0..=resolution {
                let local_u = column as f64 / resolution as f64;
                let u = -1.0 + 2.0 * (key.x as f64 + local_u) / chunks_per_face as f64;
                let direction = (normal + axis_u * u + axis_v * v).normalize();
                let sample = terrain.sample_direction(direction, season_phase);
                vertices.push(PlanetChunkVertex {
                    position: sample.surface_position,
                    normal: sample.direction,
                    biome: sample.biome,
                    elevation: sample.elevation,
                });
            }
        }

        let mut indices = Vec::with_capacity((resolution * resolution * 6) as usize);
        for row in 0..resolution {
            for column in 0..resolution {
                let a = row * side + column;
                let b = a + 1;
                let c = a + side;
                let d = c + 1;
                push_outward_triangle(&vertices, &mut indices, a, c, b);
                push_outward_triangle(&vertices, &mut indices, b, c, d);
            }
        }

        Ok(Self { key, resolution, vertices, indices })
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_at(&self, row: u32, column: u32) -> Option<&PlanetChunkVertex> {
        if row > self.resolution || column > self.resolution {
            return None;
        }
        let side = self.resolution + 1;
        self.vertices.get((row * side + column) as usize)
    }
}

fn push_outward_triangle(
    vertices: &[PlanetChunkVertex],
    indices: &mut Vec<u32>,
    a: u32,
    b: u32,
    c: u32,
) {
    let pa = vertices[a as usize].position;
    let pb = vertices[b as usize].position;
    let pc = vertices[c as usize].position;
    if (pb - pa).cross(pc - pa).dot(pa + pb + pc) >= 0.0 {
        indices.extend_from_slice(&[a, b, c]);
    } else {
        indices.extend_from_slice(&[a, c, b]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet::PlanetConfig;

    fn terrain() -> PlanetGenerator {
        PlanetGenerator::with_config(
            71,
            PlanetConfig {
                radius: 6_371_000.0,
                elevation_scale: 4_500.0,
                axial_tilt_degrees: 23.5,
            },
        )
    }

    fn key(face: CubeFace, x: u32, y: u32) -> PlanetChunkKey {
        PlanetChunkKey { face, x, y }
    }

    #[test]
    fn patch_has_expected_grid_and_triangle_counts() {
        let mesh = PlanetChunkMesh::generate(&terrain(), key(CubeFace::PositiveX, 1, 2), 4, 8, 0.0).unwrap();
        assert_eq!(mesh.vertices.len(), 81);
        assert_eq!(mesh.triangle_count(), 128);
        assert_eq!(mesh.indices.len(), 384);
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.position.is_finite()
                && vertex.normal.is_finite()
                && vertex.elevation.is_finite()
        }));
    }

    #[test]
    fn every_triangle_faces_outward() {
        let mesh = PlanetChunkMesh::generate(&terrain(), key(CubeFace::PositiveZ, 0, 0), 3, 7, 0.25).unwrap();
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let a = mesh.vertices[triangle[0] as usize].position;
            let b = mesh.vertices[triangle[1] as usize].position;
            let c = mesh.vertices[triangle[2] as usize].position;
            assert!((b - a).cross(c - a).dot(a + b + c) > 0.0);
        }
    }

    #[test]
    fn same_seed_generates_identical_patches() {
        let a = PlanetChunkMesh::generate(&terrain(), key(CubeFace::NegativeY, 2, 1), 4, 5, 0.5).unwrap();
        let b = PlanetChunkMesh::generate(&terrain(), key(CubeFace::NegativeY, 2, 1), 4, 5, 0.5).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn neighboring_patches_share_exact_edge_positions() {
        let generator = terrain();
        let left = PlanetChunkMesh::generate(&generator, key(CubeFace::PositiveX, 0, 1), 4, 8, 0.0).unwrap();
        let right = PlanetChunkMesh::generate(&generator, key(CubeFace::PositiveX, 1, 1), 4, 8, 0.0).unwrap();
        for row in 0..=8 {
            assert_eq!(
                left.vertex_at(row, 8).unwrap().position,
                right.vertex_at(row, 0).unwrap().position
            );
        }
    }

    #[test]
    fn invalid_patch_coordinates_fail_without_panicking() {
        let generator = terrain();
        assert_eq!(
            PlanetChunkMesh::generate(&generator, key(CubeFace::PositiveX, 0, 0), 0, 8, 0.0),
            Err(PlanetChunkError::InvalidChunkGrid)
        );
        assert_eq!(
            PlanetChunkMesh::generate(&generator, key(CubeFace::PositiveX, 0, 0), 4, 0, 0.0),
            Err(PlanetChunkError::InvalidMeshResolution)
        );
        assert_eq!(
            PlanetChunkMesh::generate(&generator, key(CubeFace::PositiveX, 4, 0), 4, 8, 0.0),
            Err(PlanetChunkError::ChunkOutOfRange)
        );
    }
}
