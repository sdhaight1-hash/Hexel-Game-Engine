
use std::collections::HashMap;
use macroquad::prelude::{Mesh, Vertex};

const SPHERICAL_HEXEL_HEIGHT: f32 = 1.0;

/// A point on a unit sphere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereVertex {
    pub position: [f32; 3],
}

/// A triangular face, expressed as indices into `vertices`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triangle(pub [usize; 3]);

/// A closed, subdivided icosphere.
///
/// This is the geometric foundation for the later hexagon/pentagon
/// cell topology. It does not replace the existing flat world.
#[derive(Clone, Debug)]
pub struct Icosphere {
    pub vertices: Vec<SphereVertex>,
    pub triangles: Vec<Triangle>,
}

impl Icosphere {
    /// Create an icosphere with `subdivisions` rounds of triangle splitting.
    ///
    /// Subdivision 0: 12 vertices, 20 triangles.
    /// Each additional round splits every triangle into four.
    pub fn new(subdivisions: u32) -> Self {
        let t = (1.0 + 5.0_f32.sqrt()) / 2.0;

        let raw_vertices = [
            [-1.0, t, 0.0],
            [1.0, t, 0.0],
            [-1.0, -t, 0.0],
            [1.0, -t, 0.0],
            [0.0, -1.0, t],
            [0.0, 1.0, t],
            [0.0, -1.0, -t],
            [0.0, 1.0, -t],
            [t, 0.0, -1.0],
            [t, 0.0, 1.0],
            [-t, 0.0, -1.0],
            [-t, 0.0, 1.0],
        ];

        let mut vertices: Vec<SphereVertex> = raw_vertices
            .into_iter()
            .map(|p| SphereVertex {
                position: normalize(p),
            })
            .collect();

        let mut triangles = vec![
            Triangle([0, 11, 5]),
            Triangle([0, 5, 1]),
            Triangle([0, 1, 7]),
            Triangle([0, 7, 10]),
            Triangle([0, 10, 11]),
            Triangle([1, 5, 9]),
            Triangle([5, 11, 4]),
            Triangle([11, 10, 2]),
            Triangle([10, 7, 6]),
            Triangle([7, 1, 8]),
            Triangle([3, 9, 4]),
            Triangle([3, 4, 2]),
            Triangle([3, 2, 6]),
            Triangle([3, 6, 8]),
            Triangle([3, 8, 9]),
            Triangle([4, 9, 5]),
            Triangle([2, 4, 11]),
            Triangle([6, 2, 10]),
            Triangle([8, 6, 7]),
            Triangle([9, 8, 1]),
        ];

        for _ in 0..subdivisions {
            let mut midpoint_cache = HashMap::new();
            let mut refined = Vec::with_capacity(triangles.len() * 4);

            for Triangle([a, b, c]) in triangles {
                let ab = midpoint(a, b, &mut vertices, &mut midpoint_cache);
                let bc = midpoint(b, c, &mut vertices, &mut midpoint_cache);
                let ca = midpoint(c, a, &mut vertices, &mut midpoint_cache);

                refined.push(Triangle([a, ab, ca]));
                refined.push(Triangle([b, bc, ab]));
                refined.push(Triangle([c, ca, bc]));
                refined.push(Triangle([ab, bc, ca]));
            }

            triangles = refined;
        }

        Self {
            vertices,
            triangles,
        }
    }

    /// Scale a unit-sphere vertex to a sphere of the requested radius.
    pub fn position_on_sphere(&self, vertex: usize, radius: f32) -> [f32; 3] {
        let p = self.vertices[vertex].position;
        [p[0] * radius, p[1] * radius, p[2] * radius]
    }

    /// The unit-sphere position is also the outward surface normal.
    pub fn outward_normal(&self, vertex: usize) -> [f32; 3] {
        self.vertices[vertex].position
    }
}

fn midpoint(
    a: usize,
    b: usize,
    vertices: &mut Vec<SphereVertex>,
    cache: &mut HashMap<(usize, usize), usize>,
) -> usize {
    let key = if a < b { (a, b) } else { (b, a) };

    if let Some(&index) = cache.get(&key) {
        return index;
    }

    let pa = vertices[a].position;
    let pb = vertices[b].position;

    let index = vertices.len();
    vertices.push(SphereVertex {
        position: normalize([
            (pa[0] + pb[0]) * 0.5,
            (pa[1] + pb[1]) * 0.5,
            (pa[2] + pb[2]) * 0.5,
        ]),
    });

    cache.insert(key, index);
    index
}

fn normalize(p: [f32; 3]) -> [f32; 3] {
    let length = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();

    [p[0] / length, p[1] / length, p[2] / length]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_icosphere_has_expected_counts() {
        let sphere = Icosphere::new(0);
        assert_eq!(sphere.vertices.len(), 12);
        assert_eq!(sphere.triangles.len(), 20);
    }

    #[test]
    fn subdivision_preserves_spherical_mesh_counts() {
        let sphere = Icosphere::new(2);

        assert_eq!(sphere.vertices.len(), 162);
        assert_eq!(sphere.triangles.len(), 320);
    }

    #[test]
    fn vertices_are_on_the_unit_sphere() {
        let sphere = Icosphere::new(2);

        for vertex in &sphere.vertices {
            let [x, y, z] = vertex.position;
            let length = (x * x + y * y + z * z).sqrt();
            assert!((length - 1.0).abs() < 0.0001);
        }
    }
    
    #[test]
    fn dual_of_base_icosphere_has_twelve_pentagons() {
        let sphere = Icosphere::new(0);
        let world = SphericalWorld::from_icosphere(&sphere, 10.0);

        assert_eq!(world.cells.len(), 12);
        assert_eq!(world.pentagon_count(), 12);
        assert_eq!(world.hexagon_count(), 0);
    }

    #[test]
    fn subdivided_dual_has_twelve_pentagons_and_hexagons() {
        let sphere = Icosphere::new(2);
        let world = SphericalWorld::from_icosphere(&sphere, 10.0);

        assert_eq!(world.cells.len(), 162);
        assert_eq!(world.pentagon_count(), 12);
        assert_eq!(world.hexagon_count(), 150);
    }

    #[test]
    fn every_cell_has_five_or_six_corners() {
        let sphere = Icosphere::new(2);
        let world = SphericalWorld::from_icosphere(&sphere, 10.0);

        assert!(world.cells.iter().all(|cell| {
            cell.corners.len() == 5 || cell.corners.len() == 6
        }));
    }
    
    #[test]
    fn every_cell_has_five_or_six_neighbors() {
        let sphere = Icosphere::new(2);
        let world = SphericalWorld::from_icosphere(&sphere, 10.0);

        assert!(world.cells.iter().all(|cell| {
            cell.neighbors.len() == 5 || cell.neighbors.len() == 6
        }));
    }

    #[test]
    fn cell_adjacency_is_reciprocal() {
        let sphere = Icosphere::new(2);
        let world = SphericalWorld::from_icosphere(&sphere, 10.0);

        for (cell_index, cell) in world.cells.iter().enumerate() {
            for &neighbor_index in &cell.neighbors {
                assert!(
                    world.cells[neighbor_index]
                        .neighbors
                        .contains(&cell_index),
                    "Cell {cell_index} lists {neighbor_index} as a neighbor, \
                     but the reverse connection is missing"
                );
            }
        }
    }

        #[test]
    fn spherical_cells_start_at_zero_height() {
        let sphere = Icosphere::new(2);
        let world = SphericalWorld::from_icosphere(&sphere, 10.0);

        assert!(!world.cells.is_empty());

        for index in 0..world.cells.len() {
            assert_eq!(world.cell_height(index), Some(0));
        }
    }

    #[test]
    fn spherical_cell_height_can_be_raised_and_lowered() {
        let sphere = Icosphere::new(2);
        let mut world = SphericalWorld::from_icosphere(&sphere, 10.0);

        assert!(world.raise_cell(0));
        assert_eq!(world.cell_height(0), Some(1));

        assert!(world.raise_cell(0));
        assert_eq!(world.cell_height(0), Some(2));

        assert!(world.lower_cell(0));
        assert_eq!(world.cell_height(0), Some(1));

        assert!(world.lower_cell(0));
        assert_eq!(world.cell_height(0), Some(0));

        assert!(!world.lower_cell(0));
        assert_eq!(world.cell_height(0), Some(0));
    }

    #[test]
    fn spherical_cell_height_handles_invalid_indices() {
        let sphere = Icosphere::new(2);
        let mut world = SphericalWorld::from_icosphere(&sphere, 10.0);
        let invalid_index = world.cells.len();

        assert_eq!(world.cell_height(invalid_index), None);
        assert!(!world.set_cell_height(invalid_index, 3));
        assert!(!world.raise_cell(invalid_index));
        assert!(!world.lower_cell(invalid_index));
    }

    #[test]
    fn spherical_cell_height_can_be_set_directly() {
        let sphere = Icosphere::new(2);
        let mut world = SphericalWorld::from_icosphere(&sphere, 10.0);

        assert!(world.set_cell_height(0, 5));
        assert_eq!(world.cell_height(0), Some(5));
    }

}




/// A polygonal cell on the spherical surface.
#[derive(Clone, Debug)]
pub struct SphericalCell {
    /// Vertex of the original icosphere at the center of this cell.
    pub center_vertex: usize,
    /// Indices into `SphericalWorld::corners`, ordered around the cell.
    pub corners: Vec<usize>,
    /// Adjacent cell indices.
    pub neighbors: Vec<usize>,
    /// Number of hexel layers extending outward from the base sphere.
    pub height: u32,
}

/// The polygonal dual of an icosphere.
///
/// Original icosphere vertices become cells.
/// Original triangle centers become polygon corners.
#[derive(Clone, Debug)]
pub struct SphericalWorld {
    pub radius: f32,
    pub corners: Vec<[f32; 3]>,
    pub cells: Vec<SphericalCell>,
}

impl SphericalWorld {
    pub fn from_icosphere(sphere: &Icosphere, radius: f32) -> Self {
        assert!(radius > 0.0, "Sphere radius must be positive");

        // Each triangle contributes one polygon corner at its normalized
        // center. This keeps all corners on the requested sphere.
        let corners: Vec<[f32; 3]> = sphere
            .triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle.0;
                let pa = sphere.vertices[a].position;
                let pb = sphere.vertices[b].position;
                let pc = sphere.vertices[c].position;

                normalize([
                    (pa[0] + pb[0] + pc[0]) / 3.0,
                    (pa[1] + pb[1] + pc[1]) / 3.0,
                    (pa[2] + pb[2] + pc[2]) / 3.0,
                ])
            })
            .map(|p| [p[0] * radius, p[1] * radius, p[2] * radius])
            .collect();

        // Record which triangles touch each original vertex.
        let mut incident_triangles = vec![Vec::<usize>::new(); sphere.vertices.len()];
        let mut vertex_neighbors = vec![Vec::<usize>::new(); sphere.vertices.len()];

        for (triangle_index, triangle) in sphere.triangles.iter().enumerate() {
            let [a, b, c] = triangle.0;

            incident_triangles[a].push(triangle_index);
            incident_triangles[b].push(triangle_index);
            incident_triangles[c].push(triangle_index);

            for (from, to) in [(a, b), (a, c), (b, a), (b, c), (c, a), (c, b)] {
                if !vertex_neighbors[from].contains(&to) {
                    vertex_neighbors[from].push(to);
                }
            }
        }

        let cells = incident_triangles
            .iter()
            .enumerate()
            .map(|(vertex_index, incident)| {
                let center = sphere.vertices[vertex_index].position;

                // Establish a local tangent frame to order corners around
                // this cell center.
                let reference = if center[1].abs() < 0.9 {
                    [0.0, 1.0, 0.0]
                } else {
                    [1.0, 0.0, 0.0]
                };

                let tangent_x = normalize(cross(reference, center));
                let tangent_y = cross(center, tangent_x);

                let mut ordered = incident.clone();
                ordered.sort_by(|&left, &right| {
                    let angle = |triangle_index: usize| {
                        let p = corners[triangle_index];
                        let dot_center = [
                            p[0] / radius - center[0],
                            p[1] / radius - center[1],
                            p[2] / radius - center[2],
                        ];

                        dot_center
                            .iter()
                            .zip(tangent_y)
                            .map(|(a, b)| a * b)
                            .sum::<f32>()
                            .atan2(
                                dot_center
                                    .iter()
                                    .zip(tangent_x)
                                    .map(|(a, b)| a * b)
                                    .sum::<f32>(),
                            )
                    };

                    angle(left).total_cmp(&angle(right))
                });

                SphericalCell {
                    center_vertex: vertex_index,
                    corners: ordered,
                    neighbors: vertex_neighbors[vertex_index].clone(),
                    height: 0,
                }
            })
            .collect();

        Self {
            radius,
            corners,
            cells,
        }
    }

    /// Return the outward direction at the center of a cell.
    pub fn cell_normal(&self, cell_index: usize, sphere: &Icosphere) -> [f32; 3] {
        sphere.outward_normal(self.cells[cell_index].center_vertex)
    }

    pub fn pentagon_count(&self) -> usize {
        self.cells.iter().filter(|cell| cell.corners.len() == 5).count()
    }

    pub fn hexagon_count(&self) -> usize {
        self.cells.iter().filter(|cell| cell.corners.len() == 6).count()
    }

        /// Return the height of a spherical cell, if the index exists.
    pub fn cell_height(&self, cell_index: usize) -> Option<u32> {
        self.cells.get(cell_index).map(|cell| cell.height)
    }

    /// Set the height of a spherical cell.
    /// Returns false if the cell index does not exist.
    pub fn set_cell_height(&mut self, cell_index: usize, height: u32) -> bool {
        if let Some(cell) = self.cells.get_mut(cell_index) {
            cell.height = height;
            true
        } else {
            false
        }
    }

    /// Increase a cell's height by one layer.
    /// Returns false if the cell index does not exist.
    pub fn raise_cell(&mut self, cell_index: usize) -> bool {
        if let Some(cell) = self.cells.get_mut(cell_index) {
            if let Some(height) = cell.height.checked_add(1) {
                cell.height = height;
                return true;
            }
        }

        false
    }

    /// Remove one layer from a cell.
    /// Returns false if the cell does not exist or is already at zero.
    pub fn lower_cell(&mut self, cell_index: usize) -> bool {
        if let Some(cell) = self.cells.get_mut(cell_index) {
            if cell.height > 0 {
                cell.height -= 1;
                return true;
            }
        }

        false
    }

    
    /// Build a triangle-fan mesh for the spherical polygon cells.
    ///
    /// Each cell is triangulated from its center to consecutive corners.
    /// The mesh is intended for debugging the spherical surface.
    
    /// Build the spherical terrain mesh, including raised cell tops and walls.
    pub fn build_surface_mesh(&self, sphere: &Icosphere) -> Mesh {
        use macroquad::prelude::{Vec2, Vec3, Vec4};

        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();

        // Add one vertex and return its mesh index.
       
        fn add_vertex(
            vertices: &mut Vec<Vertex>,
            position: [f32; 3],
            normal: [f32; 3],
            color: [f32; 4],
        ) -> Option<u16> {
            if vertices.len() >= u16::MAX as usize {
                return None;
            }

            let index = vertices.len() as u16;
            vertices.push(Vertex {
                position: Vec3::new(position[0], position[1], position[2]),
                uv: Vec2::ZERO,
                color: macroquad::prelude::Color::new(
                    color[0],
                    color[1],
                    color[2],
                    color[3],
                ).into(),
                normal: Vec4::new(normal[0], normal[1], normal[2], 1.0),
            });
            Some(index)
        }


        // Scale a point radially to the requested distance from the origin.
        fn at_radius(point: [f32; 3], radius: f32) -> [f32; 3] {
            let length = (point[0] * point[0]
                + point[1] * point[1]
                + point[2] * point[2])
                .sqrt();

            if length <= f32::EPSILON {
                return point;
            }

            [
                point[0] * radius / length,
                point[1] * radius / length,
                point[2] * radius / length,
            ]
        }

        // Draw each cell's top polygon at its own height.
        for (cell_index, cell) in self.cells.iter().enumerate() {
            let seed = cell_index as u32;
            let color = [
                0.25 + ((seed.wrapping_mul(97) % 70) as f32 / 100.0),
                0.25 + ((seed.wrapping_mul(57) % 70) as f32 / 100.0),
                0.25 + ((seed.wrapping_mul(23) % 70) as f32 / 100.0),
                1.0,
            ];
            if cell.corners.len() < 3 {
                continue;
            }

            let outer_radius =
                self.radius + cell.height as f32 * SPHERICAL_HEXEL_HEIGHT;

            let center = at_radius(
                sphere.vertices[cell.center_vertex].position,
                outer_radius,
            );

            let Some(center_index) =
                add_vertex(&mut vertices, center, normalize(center), color)
            else {
                break;
            };

            let mut corner_indices = Vec::with_capacity(cell.corners.len());

            for &corner_id in &cell.corners {
                let point = at_radius(self.corners[corner_id], outer_radius);

                let Some(index) =
                    add_vertex(&mut vertices, point, normalize(point), color)
                else {
                    break;
                };

                corner_indices.push(index);
            }

            if corner_indices.len() != cell.corners.len() {
                break;
            }

            for i in 0..corner_indices.len() {
                let a = center_index;
                let b = corner_indices[i];
                let c = corner_indices[(i + 1) % corner_indices.len()];

                indices.extend_from_slice(&[a, b, c]);
            }
        }

        // Add walls along shared cell edges wherever heights differ.
        for (cell_index, cell) in self.cells.iter().enumerate() {
            let cell_radius =
                self.radius + cell.height as f32 * SPHERICAL_HEXEL_HEIGHT;

            for &neighbor_index in &cell.neighbors {
                // Each shared edge is handled once.
                if neighbor_index <= cell_index {
                    continue;
                }

                let neighbor = &self.cells[neighbor_index];

                if cell.height == neighbor.height {
                    continue;
                }

                let neighbor_radius =
                    self.radius + neighbor.height as f32 * SPHERICAL_HEXEL_HEIGHT;

                let lower_radius = cell_radius.min(neighbor_radius);
                let upper_radius = cell_radius.max(neighbor_radius);

                // The two cells share the edge formed by their common corners.
                let shared: Vec<usize> = cell
                    .corners
                    .iter()
                    .copied()
                    .filter(|corner| neighbor.corners.contains(corner))
                    .collect();

                if shared.len() != 2 {
                    continue;
                }

                let p0 = at_radius(self.corners[shared[0]], lower_radius);
                let p1 = at_radius(self.corners[shared[1]], lower_radius);
                let p2 = at_radius(self.corners[shared[1]], upper_radius);
                let p3 = at_radius(self.corners[shared[0]], upper_radius);

                let midpoint = [
                    (p0[0] + p1[0] + p2[0] + p3[0]) * 0.25,
                    (p0[1] + p1[1] + p2[1] + p3[1]) * 0.25,
                    (p0[2] + p1[2] + p2[2] + p3[2]) * 0.25,
                ];
                let normal = normalize(midpoint);

                let wall_color = [0.18, 0.18, 0.18, 1.0];

                let Some(a) = add_vertex(&mut vertices, p0, normal, wall_color) else {
                    return Mesh { vertices, indices, texture: None };
                };
                let Some(b) = add_vertex(&mut vertices, p1, normal, wall_color) else {
                    return Mesh { vertices, indices, texture: None };
                };
                let Some(c) = add_vertex(&mut vertices, p2, normal, wall_color) else {
                    return Mesh { vertices, indices, texture: None };
                };
                let Some(d) = add_vertex(&mut vertices, p3, normal, wall_color) else {
                    return Mesh { vertices, indices, texture: None };
                };

                indices.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }

        Mesh {
            vertices,
            indices,
            texture: None,
        }
    }


}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
