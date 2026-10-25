use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
struct Point3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Point3 {
    fn norm(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    fn normalized(self) -> Self {
        let length = self.norm();
        Self {
            x: self.x / length,
            y: self.y / length,
            z: self.z / length,
        }
    }

    fn distance(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
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
}

#[derive(Debug)]
struct Mesh {
    vertices: Vec<Point3>,
    cells: Vec<Vec<usize>>,
}

fn vertex_key(point: Point3) -> (i64, i64, i64) {
    const SCALE: f64 = 1_000_000_000.0;
    (
        (point.x * SCALE).round() as i64,
        (point.y * SCALE).round() as i64,
        (point.z * SCALE).round() as i64,
    )
}

/// Map a point on a cube face to a point on a sphere using radial projection.
fn cube_to_sphere(point: Point3, radius: f64) -> Point3 {
    point.normalized().scale(radius)
}

impl Point3 {
    fn scale(self, factor: f64) -> Self {
        Self {
            x: self.x * factor,
            y: self.y * factor,
            z: self.z * factor,
        }
    }
}

/// Generate six cube-face grids. Matching edge and corner vertices are welded
/// by their projected coordinates, so neighboring faces share vertex IDs.
fn generate_cube_sphere(resolution: u32, radius: f64) -> Option<Mesh> {
    if resolution == 0 || !radius.is_finite() || radius <= 0.0 {
        return None;
    }

    let n = resolution as usize;
    let mut vertices = Vec::new();
    let mut cells = Vec::new();
    let mut vertex_ids = BTreeMap::new();

    // Each face is defined by a fixed axis and two axes that vary from -1 to 1.
    let faces = [
        (0, 1.0),
        (0, -1.0),
        (1, 1.0),
        (1, -1.0),
        (2, 1.0),
        (2, -1.0),
    ];

    for (axis, sign) in faces {
        let mut grid = vec![vec![0usize; n + 1]; n + 1];

        for (i, row) in grid.iter_mut().enumerate() {
            for (j, id_slot) in row.iter_mut().enumerate() {
                let u = -1.0 + 2.0 * i as f64 / n as f64;
                let v = -1.0 + 2.0 * j as f64 / n as f64;

                let cube_point = match axis {
                    0 => Point3 {
                        x: sign,
                        y: u,
                        z: v,
                    },
                    1 => Point3 {
                        x: u,
                        y: sign,
                        z: v,
                    },
                    _ => Point3 {
                        x: u,
                        y: v,
                        z: sign,
                    },
                };

                let point = cube_to_sphere(cube_point, radius);
                let key = vertex_key(point);

                let id = *vertex_ids.entry(key).or_insert_with(|| {
                    let new_id = vertices.len();
                    vertices.push(point);
                    new_id
                });

                *id_slot = id;
            }
        }

        for i in 0..n {
            for j in 0..n {
                cells.push(vec![
                    grid[i][j],
                    grid[i + 1][j],
                    grid[i + 1][j + 1],
                    grid[i][j + 1],
                ]);
            }
        }
    }

    Some(Mesh { vertices, cells })
}

/// Approximate the area of a spherical polygon by triangulating it from its
/// first vertex. The vertices are projected onto the requested sphere first.
fn spherical_polygon_area(mesh: &Mesh, cell: &[usize], radius: f64) -> f64 {
    if cell.len() < 3 {
        return 0.0;
    }

    let points: Vec<Point3> = cell
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

    unit_area * radius * radius
}

fn area_statistics(mesh: &Mesh, radius: f64) -> (f64, f64, f64, f64) {
    let areas: Vec<f64> = mesh
        .cells
        .iter()
        .map(|cell| spherical_polygon_area(mesh, cell, radius))
        .collect();
    let min = areas.iter().copied().fold(f64::INFINITY, f64::min);
    let max = areas.iter().copied().fold(0.0_f64, f64::max);
    let mean = areas.iter().sum::<f64>() / areas.len() as f64;
    let variance = areas
        .iter()
        .map(|area| (area - mean) * (area - mean))
        .sum::<f64>()
        / areas.len() as f64;
    let coefficient_of_variation = variance.sqrt() / mean;
    (min, max, mean, coefficient_of_variation)
}

fn main() {
    let resolution = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(2);
    let radius = 100.0;

    let Some(mesh) = generate_cube_sphere(resolution, radius) else {
        eprintln!("Resolution must be positive; radius must be finite and positive.");
        std::process::exit(2);
    };

    let mut edges = BTreeMap::<(usize, usize), usize>::new();
    let mut edge_lengths = Vec::new();

    for cell in &mesh.cells {
        for i in 0..cell.len() {
            let a = cell[i];
            let b = cell[(i + 1) % cell.len()];
            let edge = if a < b { (a, b) } else { (b, a) };
            *edges.entry(edge).or_insert(0) += 1;
            edge_lengths.push(mesh.vertices[a].distance(mesh.vertices[b]));
        }
    }

    let boundary_edges = edges.values().filter(|&&uses| uses == 1).count();
    let non_manifold_edges = edges.values().filter(|&&uses| uses > 2).count();
    let max_radius_error = mesh
        .vertices
        .iter()
        .map(|point| (point.norm() - radius).abs())
        .fold(0.0_f64, f64::max);
    let mean_edge_length = edge_lengths.iter().sum::<f64>() / edge_lengths.len() as f64;
    let min_edge_length = edge_lengths.iter().copied().fold(f64::INFINITY, f64::min);
    let max_edge_length = edge_lengths.iter().copied().fold(0.0_f64, f64::max);
    let all_quads = mesh.cells.iter().all(|cell| cell.len() == 4);
    let unique_cell_count = mesh.cells.iter().collect::<BTreeSet<_>>().len();
    let (min_area, max_area, mean_area, area_cv) = area_statistics(&mesh, radius);

    println!("Cube-sphere quad mesh audit (resolution {resolution}, radius {radius})");
    println!("Cells: {}", mesh.cells.len());
    println!("Vertices: {}", mesh.vertices.len());
    println!("Unique edges: {}", edges.len());
    println!("All cells are quads: {all_quads}");
    println!("Unique cell loops: {unique_cell_count}");
    println!("Boundary edges: {boundary_edges}");
    println!("Non-manifold edges: {non_manifold_edges}");
    println!("Maximum radius error: {max_radius_error:.12}");
    println!(
        "Edge length min / mean / max: {min_edge_length:.6} / {mean_edge_length:.6} / {max_edge_length:.6}"
    );
    println!(
        "Spherical cell area min / mean / max: {min_area:.6} / {mean_area:.6} / {max_area:.6}"
    );
    println!("Cell area coefficient of variation: {area_cv:.6}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge_use_counts(mesh: &Mesh) -> std::collections::BTreeMap<(usize, usize), usize> {
        let mut edges = std::collections::BTreeMap::new();

        for cell in &mesh.cells {
            for i in 0..cell.len() {
                let a = cell[i];
                let b = cell[(i + 1) % cell.len()];
                let edge = if a < b { (a, b) } else { (b, a) };
                *edges.entry(edge).or_insert(0) += 1;
            }
        }

        edges
    }

    #[test]
    fn resolution_two_has_expected_mesh_counts() {
        let mesh = generate_cube_sphere(2, 100.0).expect("valid mesh");

        assert_eq!(mesh.cells.len(), 24);
        assert_eq!(mesh.vertices.len(), 26);
        assert_eq!(edge_use_counts(&mesh).len(), 48);
    }

    #[test]
    fn resolution_three_has_expected_mesh_counts() {
        let mesh = generate_cube_sphere(3, 100.0).expect("valid mesh");

        assert_eq!(mesh.cells.len(), 54);
        assert_eq!(mesh.vertices.len(), 56);
        assert_eq!(edge_use_counts(&mesh).len(), 108);
    }

    #[test]
    fn every_edge_is_shared_by_exactly_two_cells() {
        let mesh = generate_cube_sphere(3, 100.0).expect("valid mesh");
        let edges = edge_use_counts(&mesh);

        assert!(!edges.is_empty());
        assert!(edges.values().all(|&uses| uses == 2));
    }

    #[test]
    fn cells_are_quads_with_unique_loops() {
        let mesh = generate_cube_sphere(3, 100.0).expect("valid mesh");
        let mut loops = std::collections::BTreeSet::new();

        for cell in &mesh.cells {
            assert_eq!(cell.len(), 4);
            assert!(loops.insert(cell.clone()));
        }

        assert_eq!(loops.len(), mesh.cells.len());
    }

    #[test]
    fn generated_vertices_stay_on_requested_radius() {
        let radius = 100.0;
        let mesh = generate_cube_sphere(3, radius).expect("valid mesh");

        for vertex in &mesh.vertices {
            let error = (vertex.norm() - radius).abs();
            assert!(error < 1.0e-8, "radius error was {error}");
        }
    }

    #[test]
    fn spherical_cell_areas_are_positive_and_have_measurable_variation() {
        let mesh = generate_cube_sphere(3, 100.0).expect("valid mesh");
        let (min, max, mean, cv) = area_statistics(&mesh, 100.0);

        assert!(min.is_finite() && min > 0.0);
        assert!(max.is_finite() && max >= min);
        assert!(mean.is_finite() && mean > 0.0);
        assert!(cv.is_finite() && cv > 0.0);
    }

    #[test]
    fn total_spherical_area_is_close_to_sphere_area() {
        let radius = 100.0;
        let mesh = generate_cube_sphere(4, radius).expect("valid mesh");
        let total: f64 = mesh
            .cells
            .iter()
            .map(|cell| spherical_polygon_area(&mesh, cell, radius))
            .sum();
        let sphere_area = 4.0 * std::f64::consts::PI * radius * radius;

        assert!((total - sphere_area).abs() / sphere_area < 0.01);
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(generate_cube_sphere(0, 100.0).is_none());
        assert!(generate_cube_sphere(2, 0.0).is_none());
        assert!(generate_cube_sphere(2, -100.0).is_none());
        assert!(generate_cube_sphere(2, f64::NAN).is_none());
        assert!(generate_cube_sphere(2, f64::INFINITY).is_none());
    }
}
