//! CPU-side triangulation of topology cells with terrain-displaced shared corners.
//!
//! This adapter intentionally does not depend on wgpu or the live renderer. Every
//! polygon boundary corner is sampled from its radial direction, so adjacent cells
//! compute identical positions for their shared corners.

use glam::DVec3;

use crate::{
    planet::{PlanetGenerator, PlanetSample},
    topology::{SurfaceCellId, SurfaceTopology},
};

/// One terrain-sampled vertex in a topology-backed surface mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceMeshVertex {
    pub position: DVec3,
    pub sample: PlanetSample,
}

/// A triangle belonging to one source surface cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceMeshTriangle {
    pub cell_id: SurfaceCellId,
    pub vertices: [SurfaceMeshVertex; 3],
}

/// CPU mesh data that can later be converted into renderer-specific buffers.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceMesh {
    pub triangles: Vec<SurfaceMeshTriangle>,
}

impl SurfaceMesh {
    /// Triangulate each topology polygon as a center fan.
    ///
    /// Cell centers and boundary corners are displaced using the same deterministic
    /// terrain sampler. Boundary vertices are sampled by direction rather than cell
    /// ownership, keeping a shared corner's position consistent between adjacent
    /// polygons. The topology's radius controls the input directions only; output
    /// positions use the terrain generator's configured radius plus elevation.
    pub fn generate(
        topology: &SurfaceTopology,
        terrain: &PlanetGenerator,
        season_phase: f64,
    ) -> Self {
        let mut triangles = Vec::new();

        for cell in topology.cells() {
            let center_sample = terrain.sample_direction(cell.center, season_phase);
            let center = SurfaceMeshVertex {
                position: center_sample.surface_position,
                sample: center_sample,
            };
            let boundary = cell
                .boundary
                .iter()
                .map(|corner| {
                    let sample = terrain.sample_direction(*corner, season_phase);
                    SurfaceMeshVertex {
                        position: sample.surface_position,
                        sample,
                    }
                })
                .collect::<Vec<_>>();

            if boundary.len() < 3 {
                continue;
            }

            for index in 0..boundary.len() {
                let next = (index + 1) % boundary.len();
                let mut vertices = [center, boundary[index], boundary[next]];
                let face_normal = (vertices[1].position - vertices[0].position)
                    .cross(vertices[2].position - vertices[0].position);
                if face_normal.dot(center.position) < 0.0 {
                    vertices.swap(1, 2);
                }
                triangles.push(SurfaceMeshTriangle {
                    cell_id: cell.id,
                    vertices,
                });
            }
        }

        Self { triangles }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{planet::PlanetConfig, topology::SurfaceTopology};

    #[test]
    fn triangulates_every_topology_cell_without_assuming_six_sides() {
        let topology = SurfaceTopology::generate(2, 100.0).unwrap();
        let terrain = PlanetGenerator::with_config(
            7,
            PlanetConfig {
                radius: 100.0,
                elevation_scale: 5.0,
                axial_tilt_degrees: 23.5,
            },
        );
        let mesh = SurfaceMesh::generate(&topology, &terrain, 0.25);
        let expected: usize = topology.cells().iter().map(|cell| cell.boundary.len()).sum();

        assert_eq!(mesh.triangles.len(), expected);
        assert_eq!(mesh.triangles.iter().filter(|t| t.cell_id.0 < 12).count(), 60);
        assert!(mesh.triangles.iter().all(|triangle| {
            triangle.vertices.iter().all(|vertex| {
                vertex.position.is_finite() && vertex.sample.elevation.is_finite()
            })
        }));
    }

    #[test]
    fn triangle_winding_faces_outward() {
        let topology = SurfaceTopology::generate(2, 100.0).unwrap();
        let terrain = PlanetGenerator::with_config(
            19,
            PlanetConfig {
                radius: 100.0,
                elevation_scale: 5.0,
                axial_tilt_degrees: 23.5,
            },
        );
        let mesh = SurfaceMesh::generate(&topology, &terrain, 0.0);

        for triangle in mesh.triangles {
            let [a, b, c] = triangle.vertices.map(|vertex| vertex.position);
            let normal = (b - a).cross(c - a);
            assert!(normal.dot(a + b + c) > 0.0);
        }
    }

    #[test]
    fn shared_topology_corners_have_identical_terrain_positions() {
        let topology = SurfaceTopology::generate(2, 100.0).unwrap();
        let terrain = PlanetGenerator::with_config(
            23,
            PlanetConfig {
                radius: 100.0,
                elevation_scale: 5.0,
                axial_tilt_degrees: 23.5,
            },
        );

        let mut positions = std::collections::HashMap::<(u64, u64, u64), DVec3>::new();
        for cell in topology.cells() {
            for corner in &cell.boundary {
                let sample = terrain.sample_direction(*corner, 0.5);
                let key = (
                    corner.x.to_bits(),
                    corner.y.to_bits(),
                    corner.z.to_bits(),
                );
                if let Some(existing) = positions.get(&key) {
                    assert_eq!(*existing, sample.surface_position);
                } else {
                    positions.insert(key, sample.surface_position);
                }
            }
        }
    }
}
