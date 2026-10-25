//! Deterministic geodesic surface topology for finite spherical worlds.
//!
//! The topology is the dual of a frequency-subdivided icosahedron: each primal
//! vertex becomes a surface cell, each primal triangle becomes a dual boundary
//! vertex, and primal edges define cell adjacency. This yields twelve pentagons
//! and hexagons everywhere else. It is a topology/data layer, not yet a prism
//! mesher or a replacement for the current flat-world renderer.
//!
//! Cell IDs are deterministic for a given frequency and this generator version.
//! They are not promised to remain the same when frequency or generator ordering
//! changes; persistent saves should record a topology/version identifier.

use glam::DVec3;
use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    fmt,
};

const ICOSAHEDRON_FACES: [[usize; 3]; 20] = [
    [0, 11, 5],
    [0, 5, 1],
    [0, 1, 7],
    [0, 7, 10],
    [0, 10, 11],
    [1, 5, 9],
    [5, 11, 4],
    [11, 10, 2],
    [10, 7, 6],
    [7, 1, 8],
    [3, 9, 4],
    [3, 4, 2],
    [3, 2, 6],
    [3, 6, 8],
    [3, 8, 9],
    [4, 9, 5],
    [2, 4, 11],
    [6, 2, 10],
    [8, 6, 7],
    [9, 8, 1],
];

const MAX_FREQUENCY: u32 = 256;
const VERTEX_KEY_SCALE: f64 = 1_000_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceCellId(pub u32);

impl SurfaceCellId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceCell {
    pub id: SurfaceCellId,
    /// Radial cell-center position in world units.
    pub center: DVec3,
    /// Ordered dual-polygon corners on the planet surface.
    pub boundary: Vec<DVec3>,
    /// Sorted, reciprocal neighboring cell IDs.
    pub neighbors: Vec<SurfaceCellId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceTopology {
    radius: f64,
    frequency: u32,
    cells: Vec<SurfaceCell>,
    triangle_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyError {
    InvalidFrequency,
    InvalidRadius,
}

impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFrequency => write!(
                f,
                "icosphere frequency must be between 1 and {MAX_FREQUENCY}"
            ),
            Self::InvalidRadius => write!(f, "planet radius must be finite and greater than zero"),
        }
    }
}

impl Error for TopologyError {}

impl SurfaceTopology {
    /// Generate a finite geodesic dual surface.
    ///
    /// Frequency 1 is the base icosahedron (12 pentagonal cells); frequency n
    /// has 10*n*n + 2 cells and 20*n*n primal triangles.
    pub fn generate(frequency: u32, radius: f64) -> Result<Self, TopologyError> {
        if frequency == 0 || frequency > MAX_FREQUENCY {
            return Err(TopologyError::InvalidFrequency);
        }
        if !radius.is_finite() || radius <= 0.0 {
            return Err(TopologyError::InvalidRadius);
        }

        let primal_vertices = icosahedron_vertices();
        let mut vertices = Vec::<DVec3>::new();
        let mut vertex_lookup = HashMap::<(i64, i64, i64), u32>::new();
        let mut triangles =
            Vec::<[u32; 3]>::with_capacity(20 * frequency as usize * frequency as usize);

        for face in ICOSAHEDRON_FACES {
            let a = primal_vertices[face[0]];
            let b = primal_vertices[face[1]];
            let c = primal_vertices[face[2]];
            let mut grid = Vec::<Vec<u32>>::with_capacity(frequency as usize + 1);

            for i in 0..=frequency {
                let mut row = Vec::with_capacity((frequency - i + 1) as usize);
                for j in 0..=(frequency - i) {
                    let wb = i as f64 / frequency as f64;
                    let wc = j as f64 / frequency as f64;
                    let wa = 1.0 - wb - wc;
                    let direction = (a * wa + b * wb + c * wc).normalize();
                    let key = vertex_key(direction);
                    let id = if let Some(existing) = vertex_lookup.get(&key) {
                        *existing
                    } else {
                        let id = vertices.len() as u32;
                        vertices.push(direction);
                        vertex_lookup.insert(key, id);
                        id
                    };
                    row.push(id);
                }
                grid.push(row);
            }

            for i in 0..frequency as usize {
                for j in 0..(frequency as usize - i) {
                    triangles.push([grid[i][j], grid[i + 1][j], grid[i][j + 1]]);
                    if j < frequency as usize - i - 1 {
                        triangles.push([grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]]);
                    }
                }
            }
        }

        let mut neighbors = vec![BTreeSet::<u32>::new(); vertices.len()];
        let mut incident_faces = vec![Vec::<usize>::new(); vertices.len()];
        let mut dual_corners = Vec::<DVec3>::with_capacity(triangles.len());

        for (face_id, triangle) in triangles.iter().enumerate() {
            let [a, b, c] = *triangle;
            incident_faces[a as usize].push(face_id);
            incident_faces[b as usize].push(face_id);
            incident_faces[c as usize].push(face_id);
            add_neighbor_pair(&mut neighbors, a, b);
            add_neighbor_pair(&mut neighbors, b, c);
            add_neighbor_pair(&mut neighbors, c, a);

            let center = (vertices[a as usize] + vertices[b as usize] + vertices[c as usize])
                .normalize()
                * radius;
            dual_corners.push(center);
        }

        let mut cells = Vec::with_capacity(vertices.len());
        for (index, direction) in vertices.iter().enumerate() {
            let center = *direction * radius;
            let mut boundary = incident_faces[index]
                .iter()
                .map(|face_id| dual_corners[*face_id])
                .collect::<Vec<_>>();
            sort_boundary_around(center, &mut boundary);

            cells.push(SurfaceCell {
                id: SurfaceCellId(index as u32),
                center,
                boundary,
                neighbors: neighbors[index]
                    .iter()
                    .copied()
                    .map(SurfaceCellId)
                    .collect(),
            });
        }

        Ok(Self {
            radius,
            frequency,
            cells,
            triangle_count: triangles.len(),
        })
    }

    pub const fn radius(&self) -> f64 {
        self.radius
    }
    pub const fn frequency(&self) -> u32 {
        self.frequency
    }
    pub fn cells(&self) -> &[SurfaceCell] {
        &self.cells
    }

    pub fn cell(&self, id: SurfaceCellId) -> Option<&SurfaceCell> {
        self.cells.get(id.index()).filter(|cell| cell.id == id)
    }

    /// Find the cell whose center direction is closest to a planet-centered position
    /// or direction. Input magnitude is ignored; zero and non-finite inputs return `None`.
    ///
    /// The linear scan is the correctness reference. If scores are exactly equal,
    /// the cell with the lowest ID wins, making boundary ties deterministic.
    /// Near-boundary inputs are resolved by the greatest dot-product score.
    pub fn nearest_cell(&self, direction: DVec3) -> Option<&SurfaceCell> {
        if !direction.x.is_finite()
            || !direction.y.is_finite()
            || !direction.z.is_finite()
            || direction.length_squared() <= f64::EPSILON
        {
            return None;
        }
        let direction = direction.normalize();
        self.cells.iter().min_by(|a, b| {
            b.center
                .dot(direction)
                .total_cmp(&a.center.dot(direction))
                .then_with(|| a.id.cmp(&b.id))
        })
    }

    /// Return the ID of the cell nearest a planet-centered position or direction.
    pub fn cell_id_for_direction(&self, direction: DVec3) -> Option<SurfaceCellId> {
        self.nearest_cell(direction).map(|cell| cell.id)
    }

    /// Return sorted neighbor IDs, or `None` when `id` is not in this topology.
    pub fn neighbor_ids(&self, id: SurfaceCellId) -> Option<&[SurfaceCellId]> {
        self.cell(id).map(|cell| cell.neighbors.as_slice())
    }

    /// Return whether two valid cells share an edge.
    ///
    /// Returns `None` if either ID is invalid; valid but non-adjacent cells return
    /// `Some(false)`.
    pub fn are_neighbors(&self, a: SurfaceCellId, b: SurfaceCellId) -> Option<bool> {
        let cell = self.cell(a)?;
        self.cell(b)?;
        Some(cell.neighbors.binary_search(&b).is_ok())
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
    pub const fn triangle_count(&self) -> usize {
        self.triangle_count
    }

    /// Number of unique undirected edges in the primal mesh / dual cell graph.
    pub fn edge_count(&self) -> usize {
        self.cells
            .iter()
            .map(|cell| cell.neighbors.len())
            .sum::<usize>()
            / 2
    }

    /// Counts of cells grouped by number of sides.
    pub fn side_count_histogram(&self) -> std::collections::BTreeMap<usize, usize> {
        let mut counts = std::collections::BTreeMap::<usize, usize>::new();
        for cell in &self.cells {
            *counts.entry(cell.boundary.len()).or_default() += 1;
        }
        counts
    }
}

fn add_neighbor_pair(neighbors: &mut [BTreeSet<u32>], a: u32, b: u32) {
    if a == b {
        return;
    }
    neighbors[a as usize].insert(b);
    neighbors[b as usize].insert(a);
}

fn vertex_key(direction: DVec3) -> (i64, i64, i64) {
    (
        (direction.x * VERTEX_KEY_SCALE).round() as i64,
        (direction.y * VERTEX_KEY_SCALE).round() as i64,
        (direction.z * VERTEX_KEY_SCALE).round() as i64,
    )
}

fn icosahedron_vertices() -> [DVec3; 12] {
    let phi = (1.0 + 5.0_f64.sqrt()) * 0.5;
    [
        DVec3::new(-1.0, phi, 0.0),
        DVec3::new(1.0, phi, 0.0),
        DVec3::new(-1.0, -phi, 0.0),
        DVec3::new(1.0, -phi, 0.0),
        DVec3::new(0.0, -1.0, phi),
        DVec3::new(0.0, 1.0, phi),
        DVec3::new(0.0, -1.0, -phi),
        DVec3::new(0.0, 1.0, -phi),
        DVec3::new(phi, 0.0, -1.0),
        DVec3::new(phi, 0.0, 1.0),
        DVec3::new(-phi, 0.0, -1.0),
        DVec3::new(-phi, 0.0, 1.0),
    ]
    .map(|vertex| vertex.normalize())
}

fn sort_boundary_around(center: DVec3, boundary: &mut [DVec3]) {
    if boundary.len() < 3 {
        return;
    }
    let normal = center.normalize();
    let reference = if normal.y.abs() < 0.9 {
        DVec3::Y
    } else {
        DVec3::X
    };
    let tangent_u = reference.cross(normal).normalize();
    let tangent_v = normal.cross(tangent_u).normalize();

    boundary.sort_by(|a, b| {
        let da = *a - center;
        let db = *b - center;
        let angle_a = da.dot(tangent_v).atan2(da.dot(tangent_u));
        let angle_b = db.dot(tangent_v).atan2(db.dot(tangent_u));
        angle_a.total_cmp(&angle_b)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_one_is_an_icosahedron_dual() {
        let topology = SurfaceTopology::generate(1, 100.0).unwrap();
        assert_eq!(topology.cell_count(), 12);
        assert_eq!(topology.triangle_count(), 20);
        assert_eq!(topology.edge_count(), 30);
        assert!(
            topology
                .cells()
                .iter()
                .all(|cell| cell.neighbors.len() == 5)
        );
        assert!(topology.cells().iter().all(|cell| cell.boundary.len() == 5));
    }

    #[test]
    fn frequency_two_has_twelve_pentagons_and_thirty_hexagons() {
        let topology = SurfaceTopology::generate(2, 100.0).unwrap();
        assert_eq!(topology.cell_count(), 42);
        assert_eq!(topology.triangle_count(), 80);
        assert_eq!(topology.edge_count(), 120);
        assert_eq!(
            topology.side_count_histogram(),
            std::collections::BTreeMap::from([(5, 12), (6, 30)])
        );
    }

    #[test]
    fn frequency_three_has_expected_euler_counts() {
        let topology = SurfaceTopology::generate(3, 1_000.0).unwrap();
        assert_eq!(topology.cell_count(), 92);
        assert_eq!(topology.triangle_count(), 180);
        assert_eq!(topology.edge_count(), 270);
        assert_eq!(
            topology.cell_count() as isize - topology.edge_count() as isize
                + topology.triangle_count() as isize,
            2
        );
    }

    #[test]
    fn adjacency_is_reciprocal_and_ids_are_in_range() {
        let topology = SurfaceTopology::generate(4, 10.0).unwrap();
        for cell in topology.cells() {
            assert_eq!(topology.cell(cell.id), Some(cell));
            for neighbor_id in &cell.neighbors {
                let neighbor = topology.cell(*neighbor_id).unwrap();
                assert!(neighbor.neighbors.contains(&cell.id));
            }
        }
    }

    #[test]
    fn centers_and_boundary_corners_lie_on_the_requested_radius() {
        let radius = 123.5;
        let topology = SurfaceTopology::generate(3, radius).unwrap();
        for cell in topology.cells() {
            assert!((cell.center.length() - radius).abs() < 1e-8);
            assert!(cell.boundary.len() >= 5);
            assert!(
                cell.boundary
                    .iter()
                    .all(|corner| (corner.length() - radius).abs() < 1e-8)
            );
        }
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert_eq!(
            SurfaceTopology::generate(0, 100.0),
            Err(TopologyError::InvalidFrequency)
        );
        assert_eq!(
            SurfaceTopology::generate(MAX_FREQUENCY + 1, 100.0),
            Err(TopologyError::InvalidFrequency)
        );
        assert_eq!(
            SurfaceTopology::generate(2, 0.0),
            Err(TopologyError::InvalidRadius)
        );
        assert_eq!(
            SurfaceTopology::generate(2, f64::NAN),
            Err(TopologyError::InvalidRadius)
        );
    }

    #[test]
    fn nearest_cell_resolves_cell_center_direction() {
        let topology = SurfaceTopology::generate(3, 100.0).unwrap();
        for cell in topology.cells() {
            assert_eq!(topology.nearest_cell(cell.center), Some(cell));
        }
        assert!(topology.nearest_cell(DVec3::ZERO).is_none());
        assert!(topology.nearest_cell(DVec3::splat(f64::NAN)).is_none());
    }

    #[test]
    fn direction_lookup_ignores_position_magnitude() {
        let topology = SurfaceTopology::generate(3, 100.0).unwrap();
        for cell in topology.cells() {
            assert_eq!(topology.cell_id_for_direction(cell.center), Some(cell.id));
            assert_eq!(
                topology.cell_id_for_direction(cell.center * 7.0),
                Some(cell.id)
            );
        }
        assert_eq!(topology.cell_id_for_direction(DVec3::ZERO), None);
        assert_eq!(topology.cell_id_for_direction(DVec3::splat(f64::NAN)), None);
    }

    #[test]
    fn boundary_ties_resolve_deterministically_to_lowest_id() {
        let topology = SurfaceTopology::generate(1, 100.0).unwrap();
        let direction = DVec3::X;
        let selected = topology.nearest_cell(direction).unwrap();
        for _ in 0..10 {
            assert_eq!(topology.nearest_cell(direction).unwrap().id, selected.id);
        }

        let tied = topology
            .cells()
            .iter()
            .filter(|cell| {
                (cell.center.normalize().dot(direction)
                    - selected.center.normalize().dot(direction))
                .abs()
                    < 1e-12
            })
            .map(|cell| cell.id)
            .min()
            .unwrap();
        assert_eq!(selected.id, tied);
    }

    #[test]
    fn neighbor_queries_validate_ids_and_report_adjacency() {
        let topology = SurfaceTopology::generate(3, 100.0).unwrap();
        let cell = &topology.cells()[0];
        let neighbors = topology.neighbor_ids(cell.id).unwrap();
        assert!(!neighbors.is_empty());
        assert!(neighbors.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(topology.are_neighbors(cell.id, neighbors[0]), Some(true));
        assert_eq!(topology.are_neighbors(cell.id, cell.id), Some(false));
        assert_eq!(topology.neighbor_ids(SurfaceCellId(u32::MAX)), None);
        assert_eq!(
            topology.are_neighbors(cell.id, SurfaceCellId(u32::MAX)),
            None
        );
    }

    #[test]
    fn neighbor_graph_is_reciprocal_unique_and_connected() {
        let topology = SurfaceTopology::generate(4, 100.0).unwrap();
        for cell in topology.cells() {
            let neighbors = topology.neighbor_ids(cell.id).unwrap();
            let unique = neighbors.iter().copied().collect::<BTreeSet<_>>();
            assert_eq!(unique.len(), neighbors.len());
            for neighbor in neighbors {
                assert_eq!(topology.are_neighbors(*neighbor, cell.id), Some(true));
            }
        }

        let mut visited = vec![false; topology.cell_count()];
        let mut pending = vec![topology.cells()[0].id];
        visited[pending[0].index()] = true;
        while let Some(id) = pending.pop() {
            for neighbor in topology.neighbor_ids(id).unwrap() {
                if !visited[neighbor.index()] {
                    visited[neighbor.index()] = true;
                    pending.push(*neighbor);
                }
            }
        }
        assert!(visited.into_iter().all(|was_visited| was_visited));
    }

    #[test]
    fn generation_is_deterministic_for_same_settings() {
        let a = SurfaceTopology::generate(3, 100.0).unwrap();
        let b = SurfaceTopology::generate(3, 100.0).unwrap();
        assert_eq!(a, b);
    }
}
