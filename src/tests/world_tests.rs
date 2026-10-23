use super::*;

#[test]
fn chunk_coordinates_use_16_hexel_boundaries() {
    assert_eq!(ChunkCoord::from_hex(HexCoord::new(0, 0), 0).q, 0);
    assert_eq!(ChunkCoord::from_hex(HexCoord::new(15, 15), 0).q, 0);
    assert_eq!(ChunkCoord::from_hex(HexCoord::new(16, 16), 0).q, 1);
    assert_eq!(ChunkCoord::from_hex(HexCoord::new(-1, -1), 0).q, -1);
    assert_eq!(ChunkCoord::from_hex(HexCoord::new(-16, -16), 0).q, -1);
    assert_eq!(ChunkCoord::from_hex(HexCoord::new(-17, -17), 0).q, -2);
}

#[test]
fn vertical_chunk_boundary_marks_adjacent_chunks_dirty() {
    let coord = HexCoord::new(0, 0);

    let mut world = World::stress_test(1);

    world.mark_dirty(coord, 15);
    let below_chunk = ChunkCoord::from_hex(coord, 15);
    let above_chunk = ChunkCoord::from_hex(coord, 16);

    assert!(world.dirty_chunks.contains(&below_chunk));
    assert!(world.dirty_chunks.contains(&above_chunk));

    world.dirty_chunks.clear();

    world.mark_dirty(coord, 16);

    assert!(world.dirty_chunks.contains(&below_chunk));
    assert!(world.dirty_chunks.contains(&above_chunk));
}

#[test]
fn vertical_boundary_edit_rebuilds_adjacent_chunk_meshes() {
    let coord = HexCoord::new(0, 0);
    let mut world = World::stress_test(1);

    // Use a clean world with just two vertically adjacent hexels.
    for &(hex_coord, layer) in &world.hexels.clone() {
        world.remove_hexel_at_layer(hex_coord, layer);
    }

    world.add_hexel_at_layer(coord, 15);
    world.add_hexel_at_layer(coord, 16);
    world.rebuild_dirty_chunks();

    let below_chunk = ChunkCoord::from_hex(coord, 15);
    let above_chunk = ChunkCoord::from_hex(coord, 16);

    let below_mesh = world.chunk_mesh(below_chunk);
    let above_mesh = world.chunk_mesh(above_chunk);

    // Each hexel has 6 triangles for its top and 6 for its bottom.
    // With adjacent hexels, their shared faces should be omitted.
    assert_eq!(below_mesh.indices.len(), 3 * 6 * 3);
    assert_eq!(above_mesh.indices.len(), 3 * 6 * 3);

    // Removing the upper hexel should restore the lower hexel's top face.
    world.remove_hexel_at_layer(coord, 16);
    world.rebuild_dirty_chunks();

    let below_mesh = world.chunk_mesh(below_chunk);
    assert_eq!(below_mesh.indices.len(), 72);
}

#[test]
fn demo_world_has_many_columns() {
    let world = World::demo();
    assert!(world.hexels.len() > 150);
    assert!(world.height(HexCoord::new(0, 0)) > 0);
}

#[test]
fn editing_changes_column_height() {
    let mut world = World::demo();
    let coord = HexCoord::new(0, 0);
    let before = world.height(coord);
    world.add_hexel(coord);
    assert_eq!(world.height(coord), before + 1);
    world.remove_hexel_at_layer(coord, before);
    assert_eq!(world.height(coord), before);
}

#[test]
fn empty_columns_can_be_added_and_removed() {
    let mut world = World::demo();
    let coord = HexCoord::new(30, -30);
    assert_eq!(world.height(coord), 0);
    world.add_hexel(coord);
    assert_eq!(world.height(coord), 1);
    world.remove_hexel_at_layer(coord, 0);
    assert_eq!(world.height(coord), 0);
}

#[test]
fn all_six_neighbors_share_bit_identical_edges() {
    let origin = HexCoord::new(0, 0);
    let a = coord_corners(origin, 1.0);
    let edge_pairs = [
        (5, 0, 3, 2),
        (4, 5, 2, 1),
        (3, 4, 1, 0),
        (2, 3, 0, 5),
        (1, 2, 5, 4),
        (0, 1, 4, 3),
    ];

    for (direction, (a0, a1, b0, b1)) in edge_pairs.into_iter().enumerate() {
        let b = coord_corners(origin.neighbor(direction), 1.0);
        assert_eq!(a[a0], b[b0], "first shared corner in direction {direction}");
        assert_eq!(
            a[a1], b[b1],
            "second shared corner in direction {direction}"
        );
    }
}

#[test]
fn editing_a_column_preserves_neighbor_mesh_edges() {
    let mut world = World::demo();
    let coord = HexCoord::new(0, 0);
    world.add_hexel(coord);
    world.remove_hexel_at_layer(coord, world.height(coord) - 1);

    let a = coord_corners(coord, 1.0);
    for direction in 0..6 {
        let neighbor = coord.neighbor(direction);
        let b = coord_corners(neighbor, 1.0);
        let (a0, a1, b0, b1) = match direction {
            0 => (5, 0, 3, 2),
            1 => (4, 5, 2, 1),
            2 => (3, 4, 1, 0),
            3 => (2, 3, 0, 5),
            4 => (1, 2, 5, 4),
            _ => (0, 1, 4, 3),
        };
        assert_eq!(a[a0], b[b0]);
        assert_eq!(a[a1], b[b1]);
    }
}
