//! Experimental geodesic cell-graph topology built from a subdivided icosahedron.
//!
//! Each graph node is a surface cell centre. The resulting graph has twelve
//! degree-five vertices and degree-six vertices elsewhere (for subdivision
//! levels greater than zero), matching the topology of a hex-dominant sphere.
//! This is a topology candidate, not yet a rendered hexagonal-prism world.

use glam::DVec3;
use std::collections::{BTreeSet, HashMap};

/// Stable index into a constructed geodesic topology.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GeodesicCell(pub usize);

/// Finite geodesic graph made by subdividing the faces of an icosahedron.
#[derive(Clone, Debug)]
pub struct GeodesicTopology {
    directions: Vec<DVec3>,
    neighbors: Vec<Vec<GeodesicCell>>,
}

impl GeodesicTopology {
    /// Builds an icosphere graph with the requested integer subdivision frequency.
    /// Frequency 1 is the original icosahedron; higher values add more cells.
    pub fn new(frequency: u32) -> Option<Self> {
        if frequency == 0 {
            return None;
        }

        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
        let raw = [
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
        ];
        let vertices: Vec<DVec3> = raw.into_iter().map(DVec3::normalize).collect();
        let faces: [[usize; 3]; 20] = [
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
            [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
            [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];

        let mut directions = Vec::new();
        let mut index_by_key: HashMap<[i64; 3], usize> = HashMap::new();
        let mut triangles: Vec<[usize; 3]> = Vec::new();
        let n = i64::from(frequency);

        for face in faces {
            let a = vertices[face[0]];
            let b = vertices[face[1]];
            let c = vertices[face[2]];
            let mut grid = vec![vec![0_usize; frequency as usize + 1]; frequency as usize + 1];

            for i in 0..=frequency {
                for j in 0..=frequency - i {
                    let point = (a * f64::from(frequency - i - j)
                        + b * f64::from(i)
                        + c * f64::from(j))
                        .normalize();
                    let key = [
                        (point.x * 1_000_000_000.0).round() as i64,
                        (point.y * 1_000_000_000.0).round() as i64,
                        (point.z * 1_000_000_000.0).round() as i64,
                    ];
                    let idx = *index_by_key.entry(key).or_insert_with(|| {
                        let idx = directions.len();
                        directions.push(point);
                        idx
                    });
                    grid[i as usize][j as usize] = idx;
                }
            }

            for i in 0..frequency {
                for j in 0..frequency - i {
                    triangles.push([
                        grid[i as usize][j as usize],
                        grid[(i + 1) as usize][j as usize],
                        grid[i as usize][(j + 1) as usize],
                    ]);
                    if i + j + 2 <= n as u32 {
                        triangles.push([
                            grid[(i + 1) as usize][j as usize],
                            grid[i as usize][(j + 1) as usize],
                            grid[(i + 1) as usize][(j + 1) as usize],
                        ]);
                    }
                }
            }
        }

        let mut neighbor_sets = vec![BTreeSet::new(); directions.len()];
        for [a, b, c] in triangles {
            for (left, right) in [(a, b), (b, c), (c, a)] {
                if left != right {
                    neighbor_sets[left].insert(right);
                    neighbor_sets[right].insert(left);
                }
            }
        }
        let neighbors = neighbor_sets
            .into_iter()
            .map(|set| set.into_iter().map(GeodesicCell).collect())
            .collect();

        Some(Self { directions, neighbors })
    }

    pub fn cell_count(&self) -> usize {
        self.directions.len()
    }

    pub fn contains(&self, cell: GeodesicCell) -> bool {
        cell.0 < self.directions.len()
    }

    pub fn direction(&self, cell: GeodesicCell) -> Option<DVec3> {
        self.directions.get(cell.0).copied()
    }

    pub fn neighbors(&self, cell: GeodesicCell) -> Option<&[GeodesicCell]> {
        self.neighbors.get(cell.0).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_frequency() {
        assert!(GeodesicTopology::new(0).is_none());
    }

    #[test]
    fn cell_count_matches_icosphere_formula() {
        for frequency in 1..=4 {
            let expected = (10 * frequency * frequency + 2) as usize;
            assert_eq!(GeodesicTopology::new(frequency).unwrap().cell_count(), expected);
        }
    }

    #[test]
    fn directions_are_finite_and_unit_length() {
        let topology = GeodesicTopology::new(3).unwrap();
        for index in 0..topology.cell_count() {
            let direction = topology.direction(GeodesicCell(index)).unwrap();
            assert!(direction.is_finite());
            assert!((direction.length() - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn graph_links_are_valid_and_reciprocal() {
        let topology = GeodesicTopology::new(3).unwrap();
        for index in 0..topology.cell_count() {
            let cell = GeodesicCell(index);
            let neighbors = topology.neighbors(cell).unwrap();
            assert!((5..=6).contains(&neighbors.len()));
            for neighbor in neighbors {
                assert!(topology.contains(*neighbor));
                assert!(topology.neighbors(*neighbor).unwrap().contains(&cell));
            }
        }
    }

    #[test]
    fn sphere_has_exactly_twelve_pentavalent_cells() {
        let topology = GeodesicTopology::new(4).unwrap();
        let pentavalent = topology.neighbors.iter().filter(|n| n.len() == 5).count();
        assert_eq!(pentavalent, 12);
        assert!(topology.neighbors.iter().all(|n| n.len() == 5 || n.len() == 6));
    }

    #[test]
    fn rejects_out_of_range_cell_ids() {
        let topology = GeodesicTopology::new(2).unwrap();
        assert!(topology.direction(GeodesicCell(topology.cell_count())).is_none());
        assert!(topology.neighbors(GeodesicCell(topology.cell_count())).is_none());
    }
}
