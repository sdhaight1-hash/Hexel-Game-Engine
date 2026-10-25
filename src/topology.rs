//! Experimental finite spherical surface topology.
//!
//! This is a topology spike, not the final hexel layout. It models a cube-sphere
//! as six connected quad patches so we can test stable cell identity, seam-safe
//! neighbour lookup, and surface mapping independently of rendering. The playable
//! world remains on its existing flat axial grid until competing candidates are
//! implemented and compared.

use glam::DVec3;

/// One of the six faces of a cube-sphere patch layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum CubeFace {
    PositiveX = 0,
    NegativeX = 1,
    PositiveY = 2,
    NegativeY = 3,
    PositiveZ = 4,
    NegativeZ = 5,
}

/// Stable address within a fixed-resolution cube-sphere prototype.
///
/// `u` and `v` are integer cell coordinates in `0..resolution`. The face and
/// coordinates are deliberately independent of render-mesh vertex indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CubeSphereCell {
    pub face: CubeFace,
    pub u: u32,
    pub v: u32,
}

/// Small fixed-resolution cube-sphere topology used for architecture experiments.
///
/// Each face has `resolution * resolution` quad cells. This is a patch-topology
/// candidate, not a claim that the visible terrain should use square prisms. Hexel
/// geometry and the relationship between surface cells and hexagonal-prism terrain
/// remain a separate design decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CubeSphereTopology {
    resolution: u32,
}

impl CubeSphereTopology {
    pub fn new(resolution: u32) -> Option<Self> {
        (resolution > 0).then_some(Self { resolution })
    }

    pub const fn resolution(self) -> u32 {
        self.resolution
    }

    pub fn cell_count(self) -> u64 {
        6 * u64::from(self.resolution) * u64::from(self.resolution)
    }

    pub fn contains(self, cell: CubeSphereCell) -> bool {
        cell.u < self.resolution && cell.v < self.resolution
    }

    /// Maps a cell centre to a unit-sphere direction using normalized cube projection.
    pub fn direction(self, cell: CubeSphereCell) -> Option<DVec3> {
        if !self.contains(cell) {
            return None;
        }
        let s = 2.0 * (f64::from(cell.u) + 0.5) / f64::from(self.resolution) - 1.0;
        let t = 2.0 * (f64::from(cell.v) + 0.5) / f64::from(self.resolution) - 1.0;
        Some(face_point(cell.face, s, t).normalize())
    }

    /// Returns the four patch-grid neighbours, including neighbours across cube seams.
    ///
    /// The order is +u, -u, +v, -v. At a seam, the adjacent cell is resolved by
    /// projecting the one-cell step onto the dominant cube face and quantizing there.
    pub fn neighbors(self, cell: CubeSphereCell) -> Option<[CubeSphereCell; 4]> {
        if !self.contains(cell) {
            return None;
        }
        let n = f64::from(self.resolution);
        let s = 2.0 * (f64::from(cell.u) + 0.5) / n - 1.0;
        let t = 2.0 * (f64::from(cell.v) + 0.5) / n - 1.0;
        let step = 2.0 / n;
        Some([
            self.project_cell(face_point(cell.face, s + step, t)),
            self.project_cell(face_point(cell.face, s - step, t)),
            self.project_cell(face_point(cell.face, s, t + step)),
            self.project_cell(face_point(cell.face, s, t - step)),
        ])
    }

    fn project_cell(self, point: DVec3) -> CubeSphereCell {
        let ax = point.x.abs();
        let ay = point.y.abs();
        let az = point.z.abs();
        let (face, s, t) = if ax >= ay && ax >= az {
            if point.x >= 0.0 {
                (CubeFace::PositiveX, -point.z / ax, point.y / ax)
            } else {
                (CubeFace::NegativeX, point.z / ax, point.y / ax)
            }
        } else if ay >= ax && ay >= az {
            if point.y >= 0.0 {
                (CubeFace::PositiveY, point.x / ay, -point.z / ay)
            } else {
                (CubeFace::NegativeY, point.x / ay, point.z / ay)
            }
        } else if point.z >= 0.0 {
            (CubeFace::PositiveZ, point.x / az, point.y / az)
        } else {
            (CubeFace::NegativeZ, -point.x / az, point.y / az)
        };
        CubeSphereCell {
            face,
            u: quantize(s, self.resolution),
            v: quantize(t, self.resolution),
        }
    }
}

fn quantize(coordinate: f64, resolution: u32) -> u32 {
    (((coordinate + 1.0) * 0.5 * f64::from(resolution)).floor() as i64)
        .clamp(0, i64::from(resolution) - 1) as u32
}

fn face_point(face: CubeFace, s: f64, t: f64) -> DVec3 {
    match face {
        CubeFace::PositiveX => DVec3::new(1.0, t, -s),
        CubeFace::NegativeX => DVec3::new(-1.0, t, s),
        CubeFace::PositiveY => DVec3::new(s, 1.0, -t),
        CubeFace::NegativeY => DVec3::new(s, -1.0, t),
        CubeFace::PositiveZ => DVec3::new(s, t, 1.0),
        CubeFace::NegativeZ => DVec3::new(-s, t, -1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_resolution() {
        assert!(CubeSphereTopology::new(0).is_none());
        assert!(CubeSphereTopology::new(1).is_some());
    }

    #[test]
    fn cell_count_covers_six_faces() {
        assert_eq!(CubeSphereTopology::new(8).unwrap().cell_count(), 384);
    }

    #[test]
    fn directions_are_finite_and_unit_length() {
        let topology = CubeSphereTopology::new(8).unwrap();
        for face in [
            CubeFace::PositiveX,
            CubeFace::NegativeX,
            CubeFace::PositiveY,
            CubeFace::NegativeY,
            CubeFace::PositiveZ,
            CubeFace::NegativeZ,
        ] {
            for u in 0..8 {
                for v in 0..8 {
                    let direction = topology.direction(CubeSphereCell { face, u, v }).unwrap();
                    assert!(direction.is_finite());
                    assert!((direction.length() - 1.0).abs() < 1e-12);
                }
            }
        }
    }

    #[test]
    fn every_cell_has_four_valid_neighbours() {
        let topology = CubeSphereTopology::new(5).unwrap();
        for face in [
            CubeFace::PositiveX,
            CubeFace::NegativeX,
            CubeFace::PositiveY,
            CubeFace::NegativeY,
            CubeFace::PositiveZ,
            CubeFace::NegativeZ,
        ] {
            for u in 0..5 {
                for v in 0..5 {
                    let cell = CubeSphereCell { face, u, v };
                    for neighbour in topology.neighbors(cell).unwrap() {
                        assert!(topology.contains(neighbour));
                    }
                }
            }
        }
    }

    #[test]
    fn neighbours_cross_face_seams() {
        let topology = CubeSphereTopology::new(4).unwrap();
        let cell = CubeSphereCell { face: CubeFace::PositiveX, u: 3, v: 1 };
        let neighbours = topology.neighbors(cell).unwrap();
        assert!(neighbours.iter().any(|n| n.face != cell.face));
    }

    #[test]
    fn neighbour_links_are_reciprocal_across_all_faces() {
        let topology = CubeSphereTopology::new(6).unwrap();
        for face in [
            CubeFace::PositiveX,
            CubeFace::NegativeX,
            CubeFace::PositiveY,
            CubeFace::NegativeY,
            CubeFace::PositiveZ,
            CubeFace::NegativeZ,
        ] {
            for u in 0..6 {
                for v in 0..6 {
                    let cell = CubeSphereCell { face, u, v };
                    for neighbour in topology.neighbors(cell).unwrap() {
                        assert!(
                            topology.neighbors(neighbour).unwrap().contains(&cell),
                            "neighbour link is not reciprocal: {cell:?} -> {neighbour:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_out_of_range_cells() {
        let topology = CubeSphereTopology::new(4).unwrap();
        assert!(topology.direction(CubeSphereCell { face: CubeFace::PositiveX, u: 4, v: 0 }).is_none());
        assert!(topology.neighbors(CubeSphereCell { face: CubeFace::PositiveX, u: 0, v: 4 }).is_none());
    }
}
