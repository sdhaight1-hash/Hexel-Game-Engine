use super::super::spherical::{Icosphere, SphericalWorld};

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

    assert!(world
        .cells
        .iter()
        .all(|cell| cell.corners.len() == 5 || cell.corners.len() == 6));
}

#[test]
fn every_cell_has_five_or_six_neighbors() {
    let sphere = Icosphere::new(2);
    let world = SphericalWorld::from_icosphere(&sphere, 10.0);

    assert!(world
        .cells
        .iter()
        .all(|cell| cell.neighbors.len() == 5 || cell.neighbors.len() == 6));
}

#[test]
fn cell_adjacency_is_reciprocal() {
    let sphere = Icosphere::new(2);
    let world = SphericalWorld::from_icosphere(&sphere, 10.0);

    for (cell_index, cell) in world.cells.iter().enumerate() {
        for &neighbor_index in &cell.neighbors {
            assert!(
                world.cells[neighbor_index].neighbors.contains(&cell_index),
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

#[test]
fn raising_a_cell_creates_exposed_wall_geometry() {
    let sphere = Icosphere::new(2);
    let mut world = SphericalWorld::from_icosphere(&sphere, 10.0);

    let cell_index = 0;
    let neighbor_index = world.cells[cell_index].neighbors[0];

    assert_eq!(world.cell_height(cell_index), Some(0));
    assert_eq!(world.cell_height(neighbor_index), Some(0));

    let flat_mesh = world.build_surface_mesh(&sphere);

    assert!(world.raise_cell(cell_index));
    let raised_mesh = world.build_surface_mesh(&sphere);

    assert!(raised_mesh.vertices.len() > flat_mesh.vertices.len());
    assert!(raised_mesh.indices.len() > flat_mesh.indices.len());
}

#[test]
fn equal_height_neighbors_do_not_add_wall_geometry() {
    let sphere = Icosphere::new(2);
    let mut world = SphericalWorld::from_icosphere(&sphere, 10.0);

    let flat_mesh = world.build_surface_mesh(&sphere);

    let cell_index = 0;
    let neighbor_index = world.cells[cell_index].neighbors[0];

    assert!(world.set_cell_height(cell_index, 1));
    assert!(world.set_cell_height(neighbor_index, 1));

    let equal_height_mesh = world.build_surface_mesh(&sphere);

    // Both raised cells have tops, but their shared edge needs no wall.
    assert!(
        equal_height_mesh.vertices.len() > flat_mesh.vertices.len(),
        "Raised cells should add top-surface geometry"
    );
}
