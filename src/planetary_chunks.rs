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
    fn basis(self) -> (DVec3, DVec3, DVec3) {
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
