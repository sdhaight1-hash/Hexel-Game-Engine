//! Experimental audit tool for explicit spherical cell polygons.
//!
//! This is deliberately independent of either topology candidate. It audits
//! supplied polygon loops; it does not generate cube-sphere or icosphere-dual
//! polygons yet. Run with `cargo run --bin topology_geometry_check`.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Point3 {
    fn distance(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    fn norm(self) -> f64 {
        self.distance(Self { x: 0.0, y: 0.0, z: 0.0 })
    }
}

#[derive(Clone, Debug)]
struct PolygonCell {
    id: usize,
    /// Ordered vertex IDs around this cell's boundary.
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
}

fn edge_key(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

fn audit(mesh: &MeshInput) -> AuditReport {
    let mut edge_uses: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut side_counts = BTreeMap::new();
    let mut seen_ids = std::collections::BTreeSet::new();
    let mut duplicate_cell_ids = 0;
    let mut degenerate_cells = 0;
    let mut edge_lengths = Vec::new();

    for cell in &mesh.cells {
        if !seen_ids.insert(cell.id) {
            duplicate_cell_ids += 1;
        }
        *side_counts.entry(cell.vertices.len()).or_insert(0) += 1;

        if cell.vertices.len() < 3 {
            degenerate_cells += 1;
            continue;
        }

        let mut distinct = std::collections::BTreeSet::new();
        for &vertex_id in &cell.vertices {
            if !distinct.insert(vertex_id) || vertex_id >= mesh.vertices.len() {
                degenerate_cells += 1;
                break;
            }
        }

        for i in 0..cell.vertices.len() {
            let a = cell.vertices[i];
            let b = cell.vertices[(i + 1) % cell.vertices.len()];
            if a >= mesh.vertices.len() || b >= mesh.vertices.len() || a == b {
                continue;
            }
            *edge_uses.entry(edge_key(a, b)).or_insert(0) += 1;
            edge_lengths.push(mesh.vertices[a].distance(mesh.vertices[b]));
        }
    }

    let boundary_edges = edge_uses.values().filter(|&&uses| uses == 1).count();
    let non_manifold_edges = edge_uses.values().filter(|&&uses| uses > 2).count();
    let max_radius_error = if mesh.vertices.is_empty() {
        0.0
    } else {
        mesh.vertices
            .iter()
            .map(|v| (v.norm() - mesh.radius).abs())
            .fold(0.0_f64, f64::max)
    };

    let min_edge_length = edge_lengths.iter().copied().reduce(f64::min);
    let max_edge_length = edge_lengths.iter().copied().reduce(f64::max);
    let mean_edge_length = if edge_lengths.is_empty() {
        None
    } else {
        Some(edge_lengths.iter().sum::<f64>() / edge_lengths.len() as f64)
    };

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
    }
}

fn main() {
    println!("Topology geometry audit prototype");
    println!("This tool audits explicit polygon loops; no candidate mesh is generated yet.");
    println!("Next integration step: feed it cube-sphere and icosphere-dual cell polygons.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_mesh() -> MeshInput {
        MeshInput {
            radius: 1.0,
            vertices: vec![
                Point3 { x: 1.0, y: 0.0, z: 0.0 },
                Point3 { x: 0.0, y: 1.0, z: 0.0 },
                Point3 { x: -1.0, y: 0.0, z: 0.0 },
                Point3 { x: 0.0, y: -1.0, z: 0.0 },
            ],
            cells: vec![
                PolygonCell { id: 10, vertices: vec![0, 1, 2] },
                PolygonCell { id: 11, vertices: vec![0, 2, 3] },
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
        mesh.cells.push(PolygonCell { id: 12, vertices: vec![0, 2, 1] });
        let report = audit(&mesh);
        assert_eq!(report.non_manifold_edges, 1);
    }

    #[test]
    fn detects_duplicate_cell_ids() {
        let mut mesh = square_mesh();
        mesh.cells[1].id = mesh.cells[0].id;
        assert_eq!(audit(&mesh).duplicate_cell_ids, 1);
    }

    #[test]
    fn reports_polygon_side_distribution() {
        let report = audit(&square_mesh());
        assert_eq!(report.side_counts.get(&3), Some(&2));
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
        mesh.cells.push(PolygonCell { id: 12, vertices: vec![0, 1] });
        assert_eq!(audit(&mesh).degenerate_cells, 1);
    }
}
