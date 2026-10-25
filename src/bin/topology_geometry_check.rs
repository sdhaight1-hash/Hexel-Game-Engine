//! Experimental audit tool for explicit spherical cell polygons.
//!
//! Generates the dual polygon mesh of a subdivided icosahedron, then audits
//! the resulting closed spherical cells. This remains an offline experiment:
//! it does not alter or render the playable world.

use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Point3 {
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
    fn scale(self, s: f64) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
    fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
    fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }
    fn normalized(self) -> Self {
        self.scale(1.0 / self.norm())
    }
    fn distance(self, other: Self) -> f64 {
        self.sub(other).norm()
    }
}

#[derive(Clone, Debug)]
struct PolygonCell {
    id: usize,
    /// Ordered dual-vertex IDs around this cell's boundary.
    vertices: Vec<usize>,
}

#[derive(Clone, Debug)]
struct MeshInput {
    radius: f64,
    vertices: Vec<Point3>,
    cells: Vec<PolygonCell>,
}

#[derive(Clone, Debug, PartialEq)]
struct AuditReport {
    cell_count: usize,
    vertex_count: usize,
    edge_count: usize,
    side_counts: BTreeMap<usize, usize>,
    boundary_edges: usize,
    non_manifold_edges: usize,
    degenerate_cells: usize,
    duplicate_cell_ids: usize,
    max_radius_error: f64,
    min_edge_length: Option<f64>,
    max_edge_length: Option<f64>,
    mean_edge_length: Option<f64>,
    min_cell_area: Option<f64>,
    max_cell_area: Option<f64>,
    mean_cell_area: Option<f64>,
    cell_area_coefficient_of_variation: Option<f64>,
}

fn edge_key(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

fn spherical_polygon_area(mesh: &MeshInput, cell: &PolygonCell) -> f64 {
    if cell.vertices.len() < 3 {
        return 0.0;
    }

    let points: Vec<Point3> = cell
        .vertices
        .iter()
        .map(|&id| mesh.vertices[id].normalized())
        .collect();
    let anchor = points[0];
    let mut unit_area = 0.0;

    for i in 1..points.len() - 1 {
        let b = points[i];
        let c = points[i + 1];
        let numerator = anchor.dot(b.cross(c)).abs();
        let denominator = 1.0 + anchor.dot(b) + b.dot(c) + c.dot(anchor);
        unit_area += 2.0 * numerator.atan2(denominator);
    }

    unit_area * mesh.radius * mesh.radius
}

fn audit(mesh: &MeshInput) -> AuditReport {
    let mut edge_uses: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut side_counts = BTreeMap::new();
    let mut seen_ids = BTreeSet::new();
    let mut duplicate_cell_ids = 0;
    let mut degenerate_cells = 0;
    let mut edge_lengths = Vec::new();
    let mut cell_areas = Vec::new();

    for cell in &mesh.cells {
        if !seen_ids.insert(cell.id) {
            duplicate_cell_ids += 1;
        }
        *side_counts.entry(cell.vertices.len()).or_insert(0) += 1;
        if cell.vertices.len() < 3 {
            degenerate_cells += 1;
            continue;
        }

        let mut distinct = BTreeSet::new();
        let invalid = cell
            .vertices
            .iter()
            .any(|&id| id >= mesh.vertices.len() || !distinct.insert(id));
        if invalid {
            degenerate_cells += 1;
            continue;
        }

        let area = spherical_polygon_area(mesh, cell);
        if area.is_finite() && area > 0.0 {
            cell_areas.push(area);
        }

        for i in 0..cell.vertices.len() {
            let a = cell.vertices[i];
            let b = cell.vertices[(i + 1) % cell.vertices.len()];
            if a == b {
                continue;
            }
            *edge_uses.entry(edge_key(a, b)).or_insert(0) += 1;
            edge_lengths.push(mesh.vertices[a].distance(mesh.vertices[b]));
        }
    }

    let boundary_edges = edge_uses.values().filter(|&&uses| uses == 1).count();
    let non_manifold_edges = edge_uses.values().filter(|&&uses| uses > 2).count();
    let max_radius_error = mesh
        .vertices
        .iter()
        .map(|v| (v.norm() - mesh.radius).abs())
        .fold(0.0_f64, f64::max);
    let min_edge_length = edge_lengths.iter().copied().reduce(f64::min);
    let max_edge_length = edge_lengths.iter().copied().reduce(f64::max);
    let mean_edge_length = if edge_lengths.is_empty() {
        None
    } else {
        Some(edge_lengths.iter().sum::<f64>() / edge_lengths.len() as f64)
    };
    let min_cell_area = cell_areas.iter().copied().reduce(f64::min);
    let max_cell_area = cell_areas.iter().copied().reduce(f64::max);
    let mean_cell_area = if cell_areas.is_empty() {
        None
    } else {
        Some(cell_areas.iter().sum::<f64>() / cell_areas.len() as f64)
    };
    let cell_area_coefficient_of_variation = mean_cell_area.map(|mean| {
        let variance = cell_areas
            .iter()
            .map(|area| (area - mean) * (area - mean))
            .sum::<f64>()
            / cell_areas.len() as f64;
        variance.sqrt() / mean
    });

    AuditReport {
        cell_count: mesh.cells.len(),
        vertex_count: mesh.vertices.len(),
        edge_count: edge_uses.len(),
        side_counts,
        boundary_edges,
        non_manifold_edges,
        degenerate_cells,
        duplicate_cell_ids,
        max_radius_error,
        min_edge_length,
        max_edge_length,
        mean_edge_length,
        min_cell_area,
        max_cell_area,
        mean_cell_area,
        cell_area_coefficient_of_variation,
    }
}

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

/// Build an icosphere triangulation, then its spherical dual polygon mesh.
/// Each original icosphere vertex becomes a cell; each triangle center becomes
/// a shared dual vertex. Cell loops are angularly ordered in each vertex's
/// tangent plane, giving 12 pentagons and hexagons elsewhere.
fn generate_icosphere_dual(frequency: u32, radius: f64) -> Option<MeshInput> {
    if frequency == 0 || !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let raw = [
        Point3 {
            x: -1.0,
            y: phi,
            z: 0.0,
        },
        Point3 {
            x: 1.0,
            y: phi,
            z: 0.0,
        },
        Point3 {
            x: -1.0,
            y: -phi,
            z: 0.0,
        },
        Point3 {
            x: 1.0,
            y: -phi,
            z: 0.0,
        },
        Point3 {
            x: 0.0,
            y: -1.0,
            z: phi,
        },
        Point3 {
            x: 0.0,
            y: 1.0,
            z: phi,
        },
        Point3 {
            x: 0.0,
            y: -1.0,
            z: -phi,
        },
        Point3 {
            x: 0.0,
            y: 1.0,
            z: -phi,
        },
        Point3 {
            x: phi,
            y: 0.0,
            z: -1.0,
        },
        Point3 {
            x: phi,
            y: 0.0,
            z: 1.0,
        },
        Point3 {
            x: -phi,
            y: 0.0,
            z: -1.0,
        },
        Point3 {
            x: -phi,
            y: 0.0,
            z: 1.0,
        },
    ];
    let base: Vec<Point3> = raw.into_iter().map(Point3::normalized).collect();
    let mut sphere_vertices = Vec::<Point3>::new();
    let mut vertex_by_key = HashMap::<[i64; 3], usize>::new();
    let mut triangles = Vec::<[usize; 3]>::new();
    let n = frequency as usize;

    for face in ICOSAHEDRON_FACES {
        let a = base[face[0]];
        let b = base[face[1]];
        let c = base[face[2]];
        let mut grid = vec![vec![0usize; n + 1]; n + 1];
        for (i, row) in grid.iter_mut().enumerate().take(n + 1) {
            for (j, slot) in row.iter_mut().enumerate().take(n - i + 1) {
                let point = a
                    .scale((n - i - j) as f64)
                    .add(b.scale(i as f64))
                    .add(c.scale(j as f64))
                    .normalized();
                let key = [
                    (point.x * 1_000_000_000.0).round() as i64,
                    (point.y * 1_000_000_000.0).round() as i64,
                    (point.z * 1_000_000_000.0).round() as i64,
                ];
                let id = *vertex_by_key.entry(key).or_insert_with(|| {
                    let id = sphere_vertices.len();
                    sphere_vertices.push(point);
                    id
                });
                *slot = id;
            }
        }
        for i in 0..n {
            for j in 0..n - i {
                triangles.push([grid[i][j], grid[i + 1][j], grid[i][j + 1]]);
                if i + j + 2 <= n {
                    triangles.push([grid[i + 1][j], grid[i][j + 1], grid[i + 1][j + 1]]);
                }
            }
        }
    }

    let mut incident_triangles = vec![Vec::<usize>::new(); sphere_vertices.len()];
    let mut dual_vertices = Vec::with_capacity(triangles.len());
    for (triangle_id, tri) in triangles.iter().enumerate() {
        let center = sphere_vertices[tri[0]]
            .add(sphere_vertices[tri[1]])
            .add(sphere_vertices[tri[2]])
            .scale(1.0 / 3.0)
            .normalized()
            .scale(radius);
        dual_vertices.push(center);
        for &vertex_id in tri {
            incident_triangles[vertex_id].push(triangle_id);
        }
    }

    let mut cells = Vec::with_capacity(sphere_vertices.len());
    for (cell_id, &center_dir) in sphere_vertices.iter().enumerate() {
        let mut tangent = Point3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        };
        if center_dir.dot(tangent).abs() > 0.9 {
            tangent = Point3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            };
        }
        let axis_x = tangent.cross(center_dir).normalized();
        let axis_y = center_dir.cross(axis_x).normalized();
        let mut around: Vec<(f64, usize)> = incident_triangles[cell_id]
            .iter()
            .map(|&tri_id| {
                let p = dual_vertices[tri_id].scale(1.0 / radius);
                let angle = p
                    .sub(center_dir)
                    .dot(axis_y)
                    .atan2(p.sub(center_dir).dot(axis_x));
                (angle, tri_id)
            })
            .collect();
        around.sort_by(|a, b| a.0.total_cmp(&b.0));
        cells.push(PolygonCell {
            id: cell_id,
            vertices: around.into_iter().map(|(_, tri_id)| tri_id).collect(),
        });
    }

    Some(MeshInput {
        radius,
        vertices: dual_vertices,
        cells,
    })
}

fn main() {
    let frequency = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(2);
    let radius = 100.0;
    println!("Geodesic dual polygon audit (icosphere frequency {frequency}, radius {radius})");
    match generate_icosphere_dual(frequency, radius) {
        None => {
            eprintln!("Invalid input: frequency must be positive and radius finite/positive.");
            std::process::exit(2);
        }
        Some(mesh) => {
            let report = audit(&mesh);
            println!("Cells: {}", report.cell_count);
            println!("Dual vertices: {}", report.vertex_count);
            println!("Unique edges: {}", report.edge_count);
            println!("Polygon side counts: {:?}", report.side_counts);
            println!("Boundary edges: {}", report.boundary_edges);
            println!("Non-manifold edges: {}", report.non_manifold_edges);
            println!("Degenerate cells: {}", report.degenerate_cells);
            println!("Duplicate cell IDs: {}", report.duplicate_cell_ids);
            println!("Maximum radius error: {:.12}", report.max_radius_error);
            println!(
                "Edge length min / mean / max: {:?} / {:?} / {:?}",
                report.min_edge_length, report.mean_edge_length, report.max_edge_length
            );
            println!(
                "Spherical cell area min / mean / max: {:?} / {:?} / {:?}",
                report.min_cell_area, report.mean_cell_area, report.max_cell_area
            );
            println!(
                "Cell area coefficient of variation: {:?}",
                report.cell_area_coefficient_of_variation
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_mesh() -> MeshInput {
        MeshInput {
            radius: 1.0,
            vertices: vec![
                Point3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
                Point3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                Point3 {
                    x: -1.0,
                    y: 0.0,
                    z: 0.0,
                },
                Point3 {
                    x: 0.0,
                    y: -1.0,
                    z: 0.0,
                },
            ],
            cells: vec![
                PolygonCell {
                    id: 10,
                    vertices: vec![0, 1, 2],
                },
                PolygonCell {
                    id: 11,
                    vertices: vec![0, 2, 3],
                },
            ],
        }
    }

    #[test]
    fn shared_edge_is_counted_once_and_not_as_boundary() {
        let report = audit(&square_mesh());
        assert_eq!(report.edge_count, 5);
        assert_eq!(report.boundary_edges, 4);
        assert_eq!(report.non_manifold_edges, 0);
    }

    #[test]
    fn detects_non_manifold_edge_used_by_three_cells() {
        let mut mesh = square_mesh();
        mesh.cells.push(PolygonCell {
            id: 12,
            vertices: vec![0, 2, 1],
        });
        assert_eq!(audit(&mesh).non_manifold_edges, 1);
    }

    #[test]
    fn detects_duplicate_cell_ids() {
        let mut mesh = square_mesh();
        mesh.cells[1].id = mesh.cells[0].id;
        assert_eq!(audit(&mesh).duplicate_cell_ids, 1);
    }

    #[test]
    fn reports_polygon_side_distribution() {
        assert_eq!(audit(&square_mesh()).side_counts.get(&3), Some(&2));
    }

    #[test]
    fn measures_radius_error_and_edge_lengths() {
        let mut mesh = square_mesh();
        mesh.vertices[0].x = 1.1;
        let report = audit(&mesh);
        assert!((report.max_radius_error - 0.1).abs() < 1e-9);
        assert!(report.min_edge_length.unwrap() > 0.0);
        assert!(report.max_edge_length.unwrap() >= report.min_edge_length.unwrap());
    }

    #[test]
    fn marks_cells_with_too_few_vertices_as_degenerate() {
        let mut mesh = square_mesh();
        mesh.cells.push(PolygonCell {
            id: 12,
            vertices: vec![0, 1],
        });
        assert_eq!(audit(&mesh).degenerate_cells, 1);
    }

    #[test]
    fn frequency_two_dual_has_twelve_pentagons_and_thirty_hexagons() {
        let mesh = generate_icosphere_dual(2, 100.0).unwrap();
        let report = audit(&mesh);
        assert_eq!(report.cell_count, 42);
        assert_eq!(report.vertex_count, 80);
        assert_eq!(report.side_counts.get(&5), Some(&12));
        assert_eq!(report.side_counts.get(&6), Some(&30));
        assert_eq!(report.boundary_edges, 0);
        assert_eq!(report.non_manifold_edges, 0);
        assert_eq!(report.degenerate_cells, 0);
        assert_eq!(report.duplicate_cell_ids, 0);
        assert!(report.max_radius_error < 1e-10);
        assert!(report.min_cell_area.unwrap() > 0.0);
        assert!(report.max_cell_area.unwrap() >= report.min_cell_area.unwrap());
        assert!(
            report
                .cell_area_coefficient_of_variation
                .unwrap()
                .is_finite()
        );
    }

    #[test]
    fn frequency_three_dual_is_closed_and_has_expected_counts() {
        let mesh = generate_icosphere_dual(3, 25.0).unwrap();
        let report = audit(&mesh);
        assert_eq!(report.cell_count, 92);
        assert_eq!(report.side_counts.get(&5), Some(&12));
        assert_eq!(report.side_counts.get(&6), Some(&80));
        assert_eq!(report.boundary_edges, 0);
        assert_eq!(report.non_manifold_edges, 0);
        assert_eq!(report.degenerate_cells, 0);
        assert!(report.max_radius_error < 1e-10);
        assert!(report.min_cell_area.unwrap() > 0.0);
        assert!(report.max_cell_area.unwrap() >= report.min_cell_area.unwrap());
        assert!(
            report
                .cell_area_coefficient_of_variation
                .unwrap()
                .is_finite()
        );
    }

    #[test]
    fn rejects_invalid_generation_parameters() {
        assert!(generate_icosphere_dual(0, 1.0).is_none());
        assert!(generate_icosphere_dual(1, 0.0).is_none());
        assert!(generate_icosphere_dual(1, f64::NAN).is_none());
    }

    #[test]
    fn generated_mesh_is_deterministic() {
        let a = generate_icosphere_dual(2, 1.0).unwrap();
        let b = generate_icosphere_dual(2, 1.0).unwrap();
        assert_eq!(audit(&a), audit(&b));
        assert_eq!(
            a.cells.iter().map(|c| &c.vertices).collect::<Vec<_>>(),
            b.cells.iter().map(|c| &c.vertices).collect::<Vec<_>>()
        );
    }
}
