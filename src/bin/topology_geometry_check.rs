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
    fn angle(self, other: Self) -> f64 {
        self.cross(other).norm().atan2(self.dot(other))
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
    invalid_radius: bool,
    non_finite_vertices: usize,
    zero_length_vertices: usize,
    off_radius_vertices: usize,
    invalid_cell_indices: usize,
    repeated_vertex_ids: usize,
    inconsistent_edge_orientations: usize,
    invalid_vertex_links: usize,
    max_radius_error: f64,
    min_edge_length: Option<f64>,
    max_edge_length: Option<f64>,
    mean_edge_length: Option<f64>,
    min_cell_area: Option<f64>,
    max_cell_area: Option<f64>,
    mean_cell_area: Option<f64>,
    cell_area_coefficient_of_variation: Option<f64>,
    /// Number of cell pairs with overlapping interiors under the convex spherical-cell model.
    overlapping_cell_pairs: usize,
    /// Relative difference between summed cell area and the full sphere area.
    relative_area_coverage_error: f64,
}

/// Relative radius tolerance used by the audit for vertex-on-sphere checks.
const RADIUS_RELATIVE_TOLERANCE: f64 = 1e-8;
/// Absolute floor prevents an unrealistically strict tolerance for small radii.
const RADIUS_ABSOLUTE_TOLERANCE: f64 = 1e-10;

#[derive(Clone, Debug, PartialEq)]
struct MeshValidation {
    invalid_radius: bool,
    non_finite_vertices: usize,
    zero_length_vertices: usize,
    off_radius_vertices: usize,
    invalid_cell_indices: usize,
    repeated_vertex_ids: usize,
}

fn validate_mesh(mesh: &MeshInput) -> MeshValidation {
    let invalid_radius = !mesh.radius.is_finite() || mesh.radius <= 0.0;
    let mut non_finite_vertices = 0;
    let mut zero_length_vertices = 0;
    let mut off_radius_vertices = 0;

    for vertex in &mesh.vertices {
        let finite_components =
            vertex.x.is_finite() && vertex.y.is_finite() && vertex.z.is_finite();
        let norm = vertex.norm();
        if !finite_components || !norm.is_finite() {
            non_finite_vertices += 1;
        } else if norm <= 1e-12 {
            zero_length_vertices += 1;
        } else if !invalid_radius {
            let tolerance =
                RADIUS_ABSOLUTE_TOLERANCE.max(mesh.radius.abs() * RADIUS_RELATIVE_TOLERANCE);
            if (norm - mesh.radius).abs() > tolerance {
                off_radius_vertices += 1;
            }
        }
    }

    let mut invalid_cell_indices = 0;
    let mut repeated_vertex_ids = 0;
    for cell in &mesh.cells {
        let mut seen = BTreeSet::new();
        for &id in &cell.vertices {
            if id >= mesh.vertices.len() {
                invalid_cell_indices += 1;
            } else if !seen.insert(id) {
                repeated_vertex_ids += 1;
            }
        }
    }

    MeshValidation {
        invalid_radius,
        non_finite_vertices,
        zero_length_vertices,
        off_radius_vertices,
        invalid_cell_indices,
        repeated_vertex_ids,
    }
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

fn cell_points(mesh: &MeshInput, cell: &PolygonCell) -> Option<Vec<Point3>> {
    if cell.vertices.len() < 3 {
        return None;
    }

    let mut distinct = BTreeSet::new();
    let mut points = Vec::with_capacity(cell.vertices.len());
    for &vertex_id in &cell.vertices {
        if vertex_id >= mesh.vertices.len() || !distinct.insert(vertex_id) {
            return None;
        }
        let raw = mesh.vertices[vertex_id];
        if !raw.x.is_finite() || !raw.y.is_finite() || !raw.z.is_finite() {
            return None;
        }
        let norm = raw.norm();
        if !norm.is_finite() || norm <= 1e-12 {
            return None;
        }
        let point = raw.scale(1.0 / norm);
        points.push(point);
    }

    Some(points)
}

/// Absolute tolerance for signed distance to a great-circle plane, after its normal
/// has been normalized. This is independent of polygon edge length.
const SPHERICAL_SIDE_TOLERANCE: f64 = 1e-12;

/// Strict containment for a convex spherical polygon whose edges are minor great-circle arcs.
fn point_strictly_inside_spherical_cell(
    mesh: &MeshInput,
    cell: &PolygonCell,
    point: Point3,
) -> bool {
    let Some(points) = cell_points(mesh, cell) else {
        return false;
    };
    if !point.x.is_finite() || !point.y.is_finite() || !point.z.is_finite() {
        return false;
    }
    let point_norm = point.norm();
    if !point_norm.is_finite() || point_norm <= 1e-12 {
        return false;
    }
    let point = point.scale(1.0 / point_norm);

    let zero = Point3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let center_sum = points.iter().copied().fold(zero, Point3::add);
    if center_sum.norm() <= 1e-12 {
        return false;
    }
    let center = center_sum.normalized();

    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let raw_normal = a.cross(b);
        let normal_length = raw_normal.norm();
        if !normal_length.is_finite() || normal_length <= 1e-15 {
            return false;
        }
        let edge_normal = raw_normal.scale(1.0 / normal_length);

        let center_side = edge_normal.dot(center);
        let point_side = edge_normal.dot(point);
        if !center_side.is_finite()
            || !point_side.is_finite()
            || center_side.abs() <= SPHERICAL_SIDE_TOLERANCE
            || point_side.abs() <= SPHERICAL_SIDE_TOLERANCE
            || center_side.is_sign_positive() != point_side.is_sign_positive()
        {
            return false;
        }
    }

    true
}

fn point_is_in_minor_arc_interior(a: Point3, b: Point3, point: Point3) -> bool {
    let angle_ab = a.angle(b);
    let angle_ap = a.angle(point);
    let angle_pb = point.angle(b);

    angle_ap > 1e-8 && angle_pb > 1e-8 && (angle_ap + angle_pb - angle_ab).abs() < 1e-7
}

fn coordinate_sets_match_one_to_one(
    points_a: &[Point3],
    points_b: &[Point3],
    tolerance: f64,
) -> bool {
    if points_a.len() != points_b.len() {
        return false;
    }
    let mut matched_b = vec![false; points_b.len()];
    for pa in points_a {
        let Some(index) = points_b.iter().enumerate().find_map(|(index, pb)| {
            (!matched_b[index] && pa.distance(*pb) <= tolerance).then_some(index)
        }) else {
            return false;
        };
        matched_b[index] = true;
    }
    matched_b.iter().all(|matched| *matched)
}

/// Detects positive-area overlap between convex spherical polygons.
/// Shared boundary edges and shared vertices alone are not overlaps.
fn spherical_cells_overlap(mesh: &MeshInput, a: &PolygonCell, b: &PolygonCell) -> bool {
    let (Some(points_a), Some(points_b)) = (cell_points(mesh, a), cell_points(mesh, b)) else {
        return false;
    };

    // Duplicate interiors may use the same IDs or distinct vertex records at the
    // same coordinates. Match coordinates one-to-one so one vertex cannot satisfy
    // multiple matches in the other polygon.
    let ids_a: BTreeSet<usize> = a.vertices.iter().copied().collect();
    let ids_b: BTreeSet<usize> = b.vertices.iter().copied().collect();
    let same_coordinate_set = coordinate_sets_match_one_to_one(&points_a, &points_b, 1e-8);
    if ids_a == ids_b || same_coordinate_set {
        return true;
    }

    if points_a
        .iter()
        .any(|&point| point_strictly_inside_spherical_cell(mesh, b, point))
        || points_b
            .iter()
            .any(|&point| point_strictly_inside_spherical_cell(mesh, a, point))
    {
        return true;
    }

    // If neither polygon contains a vertex of the other, overlapping boundaries
    // must cross in the interiors of two minor arcs.
    for i in 0..points_a.len() {
        let a0 = points_a[i];
        let a1 = points_a[(i + 1) % points_a.len()];
        let normal_a = a0.cross(a1);
        for j in 0..points_b.len() {
            let b0 = points_b[j];
            let b1 = points_b[(j + 1) % points_b.len()];
            let normal_b = b0.cross(b1);
            let intersection_axis = normal_a.cross(normal_b);
            if intersection_axis.norm() <= 1e-10 {
                continue;
            }

            let intersection = intersection_axis.normalized();
            for candidate in [intersection, intersection.scale(-1.0)] {
                if point_is_in_minor_arc_interior(a0, a1, candidate)
                    && point_is_in_minor_arc_interior(b0, b1, candidate)
                {
                    return true;
                }
            }
        }
    }

    false
}

/// Deterministic coverage sampling report. Samples on any cell boundary are
/// excluded from ownership counts because strict point-in-cell predicates
/// intentionally classify boundaries as neither inside nor outside.
#[derive(Clone, Debug, PartialEq)]
struct CoverageReport {
    sample_count: usize,
    covered_samples: usize,
    gap_samples: usize,
    multiply_owned_samples: usize,
    boundary_samples: usize,
}

fn point_on_minor_arc(a: Point3, b: Point3, point: Point3) -> bool {
    let normal = a.cross(b);
    if normal.norm() <= 1e-12 {
        return false;
    }
    normal.dot(point).abs() <= 1e-9 && (a.angle(point) + point.angle(b) - a.angle(b)).abs() <= 1e-8
}

fn point_on_any_cell_boundary(mesh: &MeshInput, point: Point3) -> bool {
    mesh.cells.iter().any(|cell| {
        let Some(points) = cell_points(mesh, cell) else {
            return false;
        };
        (0..points.len())
            .any(|i| point_on_minor_arc(points[i], points[(i + 1) % points.len()], point))
    })
}

/// Sample directions using a deterministic Fibonacci-sphere sequence.
/// This is an independent coverage signal, not a formal proof of coverage.
fn sample_spherical_coverage(mesh: &MeshInput, sample_count: usize) -> CoverageReport {
    let mut report = CoverageReport {
        sample_count,
        covered_samples: 0,
        gap_samples: 0,
        multiply_owned_samples: 0,
        boundary_samples: 0,
    };
    if sample_count == 0 {
        return report;
    }

    let golden_angle = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    for i in 0..sample_count {
        let z = 1.0 - 2.0 * (i as f64 + 0.5) / sample_count as f64;
        let radial = (1.0 - z * z).max(0.0).sqrt();
        let longitude = golden_angle * i as f64;
        let point = Point3 {
            x: radial * longitude.cos(),
            y: radial * longitude.sin(),
            z,
        };

        if point_on_any_cell_boundary(mesh, point) {
            report.boundary_samples += 1;
            continue;
        }

        let owners = mesh
            .cells
            .iter()
            .filter(|cell| point_strictly_inside_spherical_cell(mesh, cell, point))
            .count();
        match owners {
            0 => report.gap_samples += 1,
            1 => report.covered_samples += 1,
            _ => report.multiply_owned_samples += 1,
        }
    }
    report
}

#[derive(Clone, Debug, PartialEq)]
struct TopologyValidation {
    /// Edges used by exactly two cells whose directed traversals are not opposite.
    inconsistent_edge_orientations: usize,
    /// Vertices whose incident-cell link is not one connected cycle.
    invalid_vertex_links: usize,
}

/// Check orientation and local vertex-manifold structure independently of coverage.
fn validate_topology(mesh: &MeshInput) -> TopologyValidation {
    let mut directed_uses: BTreeMap<(usize, usize), Vec<(usize, usize)>> = BTreeMap::new();
    // For each vertex, record the link graph induced by its incident cells.
    let mut link_adjacency: Vec<BTreeMap<usize, BTreeSet<usize>>> =
        vec![BTreeMap::new(); mesh.vertices.len()];
    let mut link_degrees: Vec<BTreeMap<usize, usize>> = vec![BTreeMap::new(); mesh.vertices.len()];

    for cell in &mesh.cells {
        if cell.vertices.len() < 3 {
            continue;
        }
        let mut unique = BTreeSet::new();
        if cell
            .vertices
            .iter()
            .any(|&id| id >= mesh.vertices.len() || !unique.insert(id))
        {
            continue;
        }

        for i in 0..cell.vertices.len() {
            let a = cell.vertices[i];
            let b = cell.vertices[(i + 1) % cell.vertices.len()];
            directed_uses
                .entry(edge_key(a, b))
                .or_default()
                .push((a, b));

            let previous = cell.vertices[(i + cell.vertices.len() - 1) % cell.vertices.len()];
            let next = b;
            link_adjacency[a].entry(previous).or_default().insert(next);
            link_adjacency[a].entry(next).or_default().insert(previous);
            *link_degrees[a].entry(previous).or_insert(0) += 1;
            *link_degrees[a].entry(next).or_insert(0) += 1;
        }
    }

    let inconsistent_edge_orientations = directed_uses
        .values()
        .filter(|uses| uses.len() == 2 && !(uses[0].0 == uses[1].1 && uses[0].1 == uses[1].0))
        .count();

    let mut invalid_vertex_links = 0;
    for vertex_id in 0..mesh.vertices.len() {
        let adjacency = &link_adjacency[vertex_id];
        let degrees = &link_degrees[vertex_id];
        if adjacency.is_empty() || degrees.values().any(|&degree| degree != 2) {
            invalid_vertex_links += 1;
            continue;
        }

        let start = *adjacency.keys().next().expect("non-empty link adjacency");
        let mut visited = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(neighbors) = adjacency.get(&current) {
                stack.extend(
                    neighbors
                        .iter()
                        .copied()
                        .filter(|neighbor| !visited.contains(neighbor)),
                );
            }
        }
        if visited.len() != adjacency.len() {
            invalid_vertex_links += 1;
        }
    }

    TopologyValidation {
        inconsistent_edge_orientations,
        invalid_vertex_links,
    }
}

fn audit(mesh: &MeshInput) -> AuditReport {
    let validation = validate_mesh(mesh);
    let topology_validation = validate_topology(mesh);
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
    let max_radius_error = if validation.invalid_radius || validation.non_finite_vertices > 0 {
        f64::INFINITY
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

    let sphere_area = 4.0 * std::f64::consts::PI * mesh.radius * mesh.radius;
    let total_cell_area: f64 = cell_areas.iter().sum();
    let relative_area_coverage_error = if sphere_area.is_finite() && sphere_area > 0.0 {
        (total_cell_area - sphere_area).abs() / sphere_area
    } else {
        f64::INFINITY
    };

    let mut overlapping_cell_pairs = 0;
    for i in 0..mesh.cells.len() {
        for j in i + 1..mesh.cells.len() {
            if spherical_cells_overlap(mesh, &mesh.cells[i], &mesh.cells[j]) {
                overlapping_cell_pairs += 1;
            }
        }
    }

    AuditReport {
        cell_count: mesh.cells.len(),
        vertex_count: mesh.vertices.len(),
        edge_count: edge_uses.len(),
        side_counts,
        boundary_edges,
        non_manifold_edges,
        degenerate_cells,
        duplicate_cell_ids,
        invalid_radius: validation.invalid_radius,
        non_finite_vertices: validation.non_finite_vertices,
        zero_length_vertices: validation.zero_length_vertices,
        off_radius_vertices: validation.off_radius_vertices,
        invalid_cell_indices: validation.invalid_cell_indices,
        repeated_vertex_ids: validation.repeated_vertex_ids,
        inconsistent_edge_orientations: topology_validation.inconsistent_edge_orientations,
        invalid_vertex_links: topology_validation.invalid_vertex_links,
        max_radius_error,
        min_edge_length,
        max_edge_length,
        mean_edge_length,
        min_cell_area,
        max_cell_area,
        mean_cell_area,
        cell_area_coefficient_of_variation,
        overlapping_cell_pairs,
        relative_area_coverage_error,
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
            println!("Invalid radius: {}", report.invalid_radius);
            println!("Non-finite vertices: {}", report.non_finite_vertices);
            println!("Zero-length vertices: {}", report.zero_length_vertices);
            println!("Off-radius vertices: {}", report.off_radius_vertices);
            println!("Invalid cell indices: {}", report.invalid_cell_indices);
            println!("Repeated vertex IDs: {}", report.repeated_vertex_ids);
            println!(
                "Inconsistent edge orientations: {}",
                report.inconsistent_edge_orientations
            );
            println!("Invalid vertex links: {}", report.invalid_vertex_links);
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
            println!(
                "Overlapping cell pairs detected: {}",
                report.overlapping_cell_pairs
            );
            println!(
                "Relative area coverage error: {:.8}",
                report.relative_area_coverage_error
            );
            let coverage = sample_spherical_coverage(&mesh, 4096);
            println!("Spherical coverage samples: {}", coverage.sample_count);
            println!(
                "Samples covered by exactly one cell: {}",
                coverage.covered_samples
            );
            println!("Sampled gaps: {}", coverage.gap_samples);
            println!(
                "Samples with multiple owners: {}",
                coverage.multiply_owned_samples
            );
            println!("Boundary samples excluded: {}", coverage.boundary_samples);
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

    fn tangent_point(x: f64, y: f64) -> Point3 {
        Point3 { x, y, z: 1.0 }.normalized()
    }

    fn tangent_quad(x0: f64, x1: f64, y0: f64, y1: f64) -> Vec<Point3> {
        vec![
            tangent_point(x0, y0),
            tangent_point(x1, y0),
            tangent_point(x1, y1),
            tangent_point(x0, y1),
        ]
    }

    fn separate_polygon_mesh(polygons: Vec<Vec<Point3>>) -> MeshInput {
        let mut vertices = Vec::new();
        let mut cells = Vec::new();
        for (id, polygon) in polygons.into_iter().enumerate() {
            let start = vertices.len();
            vertices.extend(polygon);
            cells.push(PolygonCell {
                id,
                vertices: (start..vertices.len()).collect(),
            });
        }
        MeshInput {
            radius: 1.0,
            vertices,
            cells,
        }
    }

    fn directed_edge_uses(mesh: &MeshInput) -> BTreeMap<(usize, usize), usize> {
        let mut uses = BTreeMap::new();

        for cell in &mesh.cells {
            for i in 0..cell.vertices.len() {
                let a = cell.vertices[i];
                let b = cell.vertices[(i + 1) % cell.vertices.len()];
                *uses.entry((a, b)).or_insert(0) += 1;
            }
        }

        uses
    }

    fn edge_cell_uses(mesh: &MeshInput) -> BTreeMap<(usize, usize), Vec<usize>> {
        let mut uses: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();

        for (cell_index, cell) in mesh.cells.iter().enumerate() {
            for i in 0..cell.vertices.len() {
                let a = cell.vertices[i];
                let b = cell.vertices[(i + 1) % cell.vertices.len()];

                if a != b {
                    uses.entry(edge_key(a, b)).or_default().push(cell_index);
                }
            }
        }

        uses
    }

    fn signed_spherical_polygon_area(mesh: &MeshInput, cell: &PolygonCell) -> f64 {
        if cell.vertices.len() < 3 {
            return 0.0;
        }

        let points: Vec<Point3> = cell
            .vertices
            .iter()
            .map(|&id| mesh.vertices[id].normalized())
            .collect();

        let anchor = points[0];
        let mut signed_area = 0.0;

        for i in 1..points.len() - 1 {
            let b = points[i];
            let c = points[i + 1];

            let numerator = anchor.dot(b.cross(c));
            let denominator = 1.0 + anchor.dot(b) + b.dot(c) + c.dot(anchor);

            signed_area += 2.0 * numerator.atan2(denominator);
        }

        signed_area * mesh.radius * mesh.radius
    }

    #[test]
    fn point_in_cell_uses_normalized_edge_sides_for_tiny_polygons() {
        let mesh = separate_polygon_mesh(vec![tangent_quad(-1e-5, 1e-5, -1e-5, 1e-5)]);
        let cell_center = mesh.cells[0]
            .vertices
            .iter()
            .map(|&id| mesh.vertices[id])
            .fold(
                Point3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                Point3::add,
            )
            .normalized();
        assert!(
            point_strictly_inside_spherical_cell(&mesh, &mesh.cells[0], cell_center),
            "the center of a tiny valid spherical polygon must be inside"
        );
    }

    #[test]
    fn duplicate_coordinate_matching_is_one_to_one() {
        let polygon_a = tangent_quad(-0.5, 0.5, -0.5, 0.5);
        // Three coordinates are deliberately clustered near one coordinate from A;
        // a non-bijective any-match comparison could reuse one match incorrectly.
        let polygon_b = vec![
            polygon_a[0],
            tangent_point(-0.5 + 5e-9, -0.5),
            tangent_point(-0.5 + 8e-9, -0.5),
            polygon_a[2],
        ];
        assert!(
            !coordinate_sets_match_one_to_one(&polygon_a, &polygon_b, 1e-8),
            "near-duplicate coordinate sets must not be called identical solely by reusing matches"
        );
        assert!(
            coordinate_sets_match_one_to_one(&polygon_a, &polygon_a, 1e-8),
            "identical coordinate sets should match one-to-one"
        );
    }

    #[test]
    fn malformed_coordinates_are_rejected_without_panicking() {
        let mut mesh = square_mesh();
        mesh.vertices[0] = Point3 {
            x: f64::NAN,
            y: 0.0,
            z: 1.0,
        };
        assert!(cell_points(&mesh, &mesh.cells[0]).is_none());
        let report = audit(&mesh);
        assert_eq!(report.non_finite_vertices, 1);
        assert!(!spherical_cells_overlap(
            &mesh,
            &mesh.cells[0],
            &mesh.cells[1]
        ));
    }

    #[test]
    fn runtime_topology_validation_detects_same_direction_shared_edge() {
        let mut mesh = square_mesh();
        // Both cells traverse edge 0-2 as 2 -> 0.
        mesh.cells[1].vertices = vec![0, 3, 2];
        let topology = validate_topology(&mesh);
        assert!(topology.inconsistent_edge_orientations > 0);
    }

    #[test]
    fn runtime_topology_validation_detects_disconnected_vertex_link() {
        let mut mesh = generate_icosphere_dual(2, 1.0).unwrap();
        // Add a second fan around vertex 0, disconnected from the original closed link.
        let start = mesh.vertices.len();
        mesh.vertices.extend([
            Point3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            Point3 {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
        ]);
        let cell_id = mesh.cells.len();
        mesh.cells.push(PolygonCell {
            id: cell_id,
            vertices: vec![0, start, start + 1],
        });
        let topology = validate_topology(&mesh);
        assert!(topology.invalid_vertex_links > 0);
    }

    #[test]
    fn generated_mesh_has_consistent_orientation_and_vertex_links() {
        for frequency in [2, 3, 4] {
            let mesh = generate_icosphere_dual(frequency, 100.0).unwrap();
            let topology = validate_topology(&mesh);
            assert_eq!(
                topology.inconsistent_edge_orientations, 0,
                "frequency {frequency}"
            );
            assert_eq!(topology.invalid_vertex_links, 0, "frequency {frequency}");
        }
    }

    #[test]
    fn validation_reports_invalid_radius_and_non_finite_vertices_without_panicking() {
        let mut mesh = square_mesh();
        mesh.radius = f64::NAN;
        mesh.vertices[0].x = f64::INFINITY;
        let validation = validate_mesh(&mesh);
        assert!(validation.invalid_radius);
        assert_eq!(validation.non_finite_vertices, 1);
        let report = audit(&mesh);
        assert!(report.invalid_radius);
        assert_eq!(report.non_finite_vertices, 1);
        assert!(report.max_radius_error.is_infinite());
    }

    #[test]
    fn validation_reports_zero_length_and_off_radius_vertices() {
        let mut mesh = square_mesh();
        mesh.vertices[0] = Point3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        mesh.vertices[1] = Point3 {
            x: 0.0,
            y: 2.0,
            z: 0.0,
        };
        let validation = validate_mesh(&mesh);
        assert_eq!(validation.zero_length_vertices, 1);
        assert_eq!(validation.off_radius_vertices, 1);
    }

    #[test]
    fn validation_reports_invalid_indices_and_repeated_vertex_ids() {
        let mut mesh = square_mesh();
        mesh.cells.push(PolygonCell {
            id: 12,
            vertices: vec![0, 1, 99, 1],
        });
        let validation = validate_mesh(&mesh);
        assert_eq!(validation.invalid_cell_indices, 1);
        assert_eq!(validation.repeated_vertex_ids, 1);
        let report = audit(&mesh);
        assert_eq!(report.invalid_cell_indices, 1);
        assert_eq!(report.repeated_vertex_ids, 1);
    }

    #[test]
    fn radius_tolerance_is_scale_aware() {
        let mut mesh = square_mesh();
        mesh.radius = 1_000_000.0;
        mesh.vertices = vec![Point3 {
            x: 1_000_000.001,
            y: 0.0,
            z: 0.0,
        }];
        mesh.cells.clear();
        let validation = validate_mesh(&mesh);
        assert_eq!(validation.off_radius_vertices, 0);
        mesh.vertices[0].x = 1_000_001.0;
        assert_eq!(validate_mesh(&mesh).off_radius_vertices, 1);
    }

    #[test]
    fn duplicate_polygon_with_distinct_vertex_ids_is_detected() {
        let polygon = tangent_quad(-0.5, 0.5, -0.5, 0.5);
        let mesh = separate_polygon_mesh(vec![polygon.clone(), polygon]);
        assert!(spherical_cells_overlap(
            &mesh,
            &mesh.cells[0],
            &mesh.cells[1]
        ));
    }

    #[test]
    fn partial_overlap_with_crossing_edges_is_detected() {
        let mesh = separate_polygon_mesh(vec![
            tangent_quad(-0.8, 0.2, -0.8, 0.2),
            tangent_quad(-0.2, 0.8, -0.2, 0.8),
        ]);
        assert!(spherical_cells_overlap(
            &mesh,
            &mesh.cells[0],
            &mesh.cells[1]
        ));
    }

    #[test]
    fn strict_containment_is_detected_as_overlap() {
        let mesh = separate_polygon_mesh(vec![
            tangent_quad(-0.8, 0.8, -0.8, 0.8),
            tangent_quad(-0.2, 0.2, -0.2, 0.2),
        ]);
        assert!(spherical_cells_overlap(
            &mesh,
            &mesh.cells[0],
            &mesh.cells[1]
        ));
    }

    #[test]
    fn shared_edge_without_interior_overlap_is_not_detected_as_overlap() {
        let vertices = vec![
            tangent_point(-0.7, -0.6),
            tangent_point(0.0, -0.6),
            tangent_point(0.0, 0.6),
            tangent_point(-0.7, 0.6),
            tangent_point(0.7, -0.6),
            tangent_point(0.7, 0.6),
        ];
        let mesh = MeshInput {
            radius: 1.0,
            vertices,
            cells: vec![
                PolygonCell {
                    id: 0,
                    vertices: vec![0, 1, 2, 3],
                },
                PolygonCell {
                    id: 1,
                    vertices: vec![1, 4, 5, 2],
                },
            ],
        };
        assert!(!spherical_cells_overlap(
            &mesh,
            &mesh.cells[0],
            &mesh.cells[1]
        ));
    }

    #[test]
    fn shared_vertex_without_interior_overlap_is_not_detected_as_overlap() {
        let mesh = separate_polygon_mesh(vec![
            tangent_quad(-0.8, 0.0, -0.8, 0.0),
            tangent_quad(0.0, 0.8, 0.0, 0.8),
        ]);
        assert!(!spherical_cells_overlap(
            &mesh,
            &mesh.cells[0],
            &mesh.cells[1]
        ));
    }

    #[test]
    fn disjoint_polygons_are_not_detected_as_overlapping() {
        let mesh = separate_polygon_mesh(vec![
            tangent_quad(-0.9, -0.5, -0.2, 0.2),
            tangent_quad(0.5, 0.9, -0.2, 0.2),
        ]);
        assert!(!spherical_cells_overlap(
            &mesh,
            &mesh.cells[0],
            &mesh.cells[1]
        ));
    }

    #[test]
    fn near_boundary_gap_is_deterministic_and_not_an_overlap() {
        let mesh = separate_polygon_mesh(vec![
            tangent_quad(-0.6, 0.0, -0.5, 0.5),
            tangent_quad(1e-5, 0.6, -0.5, 0.5),
        ]);
        let first = spherical_cells_overlap(&mesh, &mesh.cells[0], &mesh.cells[1]);
        let second = spherical_cells_overlap(&mesh, &mesh.cells[0], &mesh.cells[1]);
        assert_eq!(first, second);
        assert!(!first, "a positive gap should not be classified as overlap");
    }

    #[test]
    fn deterministic_spherical_sampling_validates_generated_mesh_coverage() {
        let mesh = generate_icosphere_dual(3, 100.0).expect("valid mesh");
        let a = sample_spherical_coverage(&mesh, 4096);
        let b = sample_spherical_coverage(&mesh, 4096);
        assert_eq!(a, b);
        assert_eq!(a.sample_count, 4096);
        assert_eq!(a.gap_samples, 0, "unexpected sampled gaps: {a:?}");
        assert_eq!(
            a.multiply_owned_samples, 0,
            "unexpected multiple ownership: {a:?}"
        );
        assert_eq!(a.covered_samples + a.boundary_samples, a.sample_count);
    }

    #[test]
    fn spherical_sampling_detects_gaps_after_removing_a_cell() {
        let mut mesh = generate_icosphere_dual(3, 100.0).expect("valid mesh");
        mesh.cells.remove(0);
        let coverage = sample_spherical_coverage(&mesh, 4096);
        assert!(
            coverage.gap_samples > 0,
            "expected sampled gaps: {coverage:?}"
        );
    }

    #[test]
    fn spherical_sampling_detects_multiple_ownership_after_duplicate_cell() {
        let mut mesh = generate_icosphere_dual(3, 100.0).expect("valid mesh");
        let mut duplicate = mesh.cells[0].clone();
        duplicate.id = mesh.cells.len();
        mesh.cells.push(duplicate);
        let coverage = sample_spherical_coverage(&mesh, 4096);
        assert!(
            coverage.multiply_owned_samples > 0,
            "expected sampled multiple ownership: {coverage:?}"
        );
    }

    #[test]
    fn generated_cells_have_consistent_outward_winding() {
        let mesh = generate_icosphere_dual(3, 100.0).expect("valid mesh");

        let signed_areas: Vec<f64> = mesh
            .cells
            .iter()
            .map(|cell| signed_spherical_polygon_area(&mesh, cell))
            .collect();

        assert!(
            signed_areas
                .iter()
                .all(|area| area.is_finite() && *area > 1e-8),
            "every generated cell should have positive, finite signed spherical area; \
             observed range: {:?}",
            (
                signed_areas.iter().copied().fold(f64::INFINITY, f64::min),
                signed_areas
                    .iter()
                    .copied()
                    .fold(f64::NEG_INFINITY, f64::max)
            )
        );
    }

    #[test]
    fn every_shared_edge_is_traversed_in_opposite_directions() {
        let mesh = generate_icosphere_dual(3, 100.0).expect("valid mesh");
        let directed = directed_edge_uses(&mesh);
        let mut undirected = BTreeSet::new();

        for &(a, b) in directed.keys() {
            undirected.insert(edge_key(a, b));
        }

        for (a, b) in undirected {
            let forward = directed.get(&(a, b)).copied().unwrap_or(0);
            let reverse = directed.get(&(b, a)).copied().unwrap_or(0);

            assert_eq!(
                (forward, reverse),
                (1, 1),
                "edge ({a}, {b}) should be used once in each direction"
            );
        }
    }

    #[test]
    fn cell_adjacency_matches_shared_edges_and_is_reciprocal() {
        let mesh = generate_icosphere_dual(3, 100.0).expect("valid mesh");
        let edge_uses = edge_cell_uses(&mesh);

        let mut neighbors = vec![BTreeSet::<usize>::new(); mesh.cells.len()];

        // Build the complete adjacency graph before validating connectivity.
        for (edge, owners) in &edge_uses {
            assert_eq!(
                owners.len(),
                2,
                "edge {edge:?} must belong to exactly two cells"
            );

            let a = owners[0];
            let b = owners[1];

            assert_ne!(a, b, "an edge cannot be shared by a cell with itself");

            assert!(
                neighbors[a].insert(b),
                "duplicate adjacency between cells {} and {} for edge {edge:?}",
                mesh.cells[a].id,
                mesh.cells[b].id
            );

            assert!(
                neighbors[b].insert(a),
                "duplicate reciprocal adjacency between cells {} and {} for edge {edge:?}",
                mesh.cells[b].id,
                mesh.cells[a].id
            );
        }

        // Every cell should have one distinct neighbor per boundary edge.
        for (cell_index, cell) in mesh.cells.iter().enumerate() {
            assert_eq!(
                neighbors[cell_index].len(),
                cell.vertices.len(),
                "cell {} should have one distinct neighbor per boundary edge",
                cell.id
            );

            for &neighbor_index in &neighbors[cell_index] {
                assert!(
                    neighbors[neighbor_index].contains(&cell_index),
                    "adjacency must be reciprocal between cells {} and {}",
                    cell.id,
                    mesh.cells[neighbor_index].id
                );
            }
        }

        // Every cell should be reachable through shared-edge adjacency.
        let mut visited = BTreeSet::<usize>::new();
        let mut stack = vec![0usize];

        while let Some(cell_index) = stack.pop() {
            if !visited.insert(cell_index) {
                continue;
            }

            for &neighbor_index in &neighbors[cell_index] {
                assert!(
                    neighbor_index < mesh.cells.len(),
                    "neighbor index {neighbor_index} is out of bounds"
                );

                stack.push(neighbor_index);
            }
        }

        assert_eq!(
            visited.len(),
            mesh.cells.len(),
            "every cell should be reachable through shared-edge adjacency"
        );
    }

    #[test]
    fn total_spherical_area_is_close_to_sphere_area() {
        let radius = 100.0;
        let mesh = generate_icosphere_dual(3, radius).expect("valid mesh");

        let total: f64 = mesh
            .cells
            .iter()
            .map(|cell| spherical_polygon_area(&mesh, cell))
            .sum();

        let sphere_area = 4.0 * std::f64::consts::PI * radius * radius;

        assert!(
            total.is_finite() && (total - sphere_area).abs() / sphere_area < 0.01,
            "total cell area {total} differs from sphere area {sphere_area}"
        );
    }

    #[test]
    fn generated_mesh_has_no_detectable_overlaps_and_covers_sphere_area() {
        let mesh = generate_icosphere_dual(3, 100.0).expect("valid mesh");
        let report = audit(&mesh);

        assert_eq!(report.overlapping_cell_pairs, 0);
        assert!(
            report.relative_area_coverage_error < 0.01,
            "relative area coverage error was {}",
            report.relative_area_coverage_error
        );
    }

    #[test]
    fn removing_a_cell_is_reported_as_incomplete_coverage() {
        let mut mesh = generate_icosphere_dual(3, 100.0).expect("valid mesh");
        mesh.cells.remove(0);
        let report = audit(&mesh);

        assert!(report.boundary_edges > 0);
        assert!(report.relative_area_coverage_error > 0.001);
        assert_eq!(report.overlapping_cell_pairs, 0);
    }

    #[test]
    fn duplicate_cell_is_reported_as_an_overlap() {
        let mut mesh = generate_icosphere_dual(2, 100.0).expect("valid mesh");
        let mut duplicate = mesh.cells[0].clone();
        duplicate.id = mesh.cells.len() + 1;
        mesh.cells.push(duplicate);

        let report = audit(&mesh);
        assert!(report.overlapping_cell_pairs > 0);
        assert!(report.relative_area_coverage_error > 0.01);
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
    fn frequency_sweep_has_no_sampled_gaps_or_multiple_ownership() {
        for frequency in [2, 3, 4] {
            let mesh = generate_icosphere_dual(frequency, 100.0).expect("valid frequency");
            let validation = validate_mesh(&mesh);
            assert!(!validation.invalid_radius, "frequency {frequency}");
            assert_eq!(validation.non_finite_vertices, 0, "frequency {frequency}");
            assert_eq!(validation.zero_length_vertices, 0, "frequency {frequency}");
            assert_eq!(validation.off_radius_vertices, 0, "frequency {frequency}");
            assert_eq!(validation.invalid_cell_indices, 0, "frequency {frequency}");
            assert_eq!(validation.repeated_vertex_ids, 0, "frequency {frequency}");

            let coverage = sample_spherical_coverage(&mesh, 1024);
            assert_eq!(
                coverage.gap_samples, 0,
                "frequency {frequency}: {coverage:?}"
            );
            assert_eq!(
                coverage.multiply_owned_samples, 0,
                "frequency {frequency}: {coverage:?}"
            );
            assert_eq!(
                coverage.covered_samples + coverage.boundary_samples,
                coverage.sample_count,
                "frequency {frequency}: {coverage:?}"
            );
        }
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
