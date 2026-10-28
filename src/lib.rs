//! Core coordinate, storage, generation, and prism-mesh primitives.
//! Rendering and interactive controls are added in later implementation stages.

pub mod climate;
pub mod geology;
pub mod inventory;
pub mod planet;
pub mod planetary_frame;
pub mod planetary_world;
pub mod planetary_voxels;
pub mod planetary_chunks;
pub mod planetary_chunk_world;
pub mod soil;
pub mod surface_navigation;
pub mod surface_mesh;
pub mod save;
pub mod topology;
pub mod trees;
pub mod tree_blocks;
pub mod world_profile;

use glam::Vec3;
use hexx::Hex;
use noise::{NoiseFn, Perlin};
use std::collections::HashMap;

pub const CHUNK_Q: i32 = 16;
pub const CHUNK_R: i32 = 16;
pub const CHUNK_Y: i32 = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellPos {
    pub q: i32,
    pub r: i32,
    pub y: i32,
}

impl CellPos {
    pub const fn new(q: i32, r: i32, y: i32) -> Self {
        Self { q, r, y }
    }

    /// Six horizontal neighbors come from the shared `hexx` axial-grid
    /// implementation; the final two entries are the vertical prism neighbors.
    /// Keeping the crate's coordinate math here prevents a second hand-maintained
    /// hex-neighbor definition from drifting away from mesh and navigation code.
    pub fn neighbors(self) -> [Self; 8] {
        let horizontal = Hex::new(self.q, self.r).all_neighbors();
        // Preserve the engine's established clockwise ordering while using hexx
        // as the source of axial neighbor coordinates.
        [
            Self::new(horizontal[5].x, horizontal[5].y, self.y),
            Self::new(horizontal[0].x, horizontal[0].y, self.y),
            Self::new(horizontal[4].x, horizontal[4].y, self.y),
            Self::new(horizontal[3].x, horizontal[3].y, self.y),
            Self::new(horizontal[2].x, horizontal[2].y, self.y),
            Self::new(horizontal[1].x, horizontal[1].y, self.y),
            Self::new(self.q, self.r, self.y + 1),
            Self::new(self.q, self.r, self.y - 1),
        ]
    }

    pub fn chunk_and_local(self) -> (ChunkPos, [usize; 3]) {
        let cq = self.q.div_euclid(CHUNK_Q);
        let cr = self.r.div_euclid(CHUNK_R);
        let cy = self.y.div_euclid(CHUNK_Y);
        (
            ChunkPos { q: cq, r: cr, y: cy },
            [
                self.q.rem_euclid(CHUNK_Q) as usize,
                self.r.rem_euclid(CHUNK_R) as usize,
                self.y.rem_euclid(CHUNK_Y) as usize,
            ],
        )
    }
}


#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub q: i32,
    pub r: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockId {
    #[default]
    Air = 0,
    Grass = 1,
    Dirt = 2,
    Stone = 3,
    Foundation = 4,
}

impl BlockId {
    pub const fn is_solid(self) -> bool {
        !matches!(self, Self::Air)
    }
    pub const fn is_indestructible(self) -> bool {
        matches!(self, Self::Foundation)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WorldScale {
    pub player_height: f32,
    pub hexel_height: f32,
    pub hex_radius: f32,
}

impl Default for WorldScale {
    fn default() -> Self {
        Self {
            player_height: crate::world_profile::REFERENCE_PLAYER_HEIGHT_METRES,
            hexel_height: crate::world_profile::HEXEL_HEIGHT_METRES,
            hex_radius: crate::world_profile::HEXEL_WIDTH_METRES * 0.5,
        }
    }
}

impl WorldScale {
    pub fn prism_center(self, cell: CellPos) -> Vec3 {
        let x = self.hex_radius * 3.0_f32.sqrt() * (cell.q as f32 + cell.r as f32 * 0.5);
        let z = self.hex_radius * 1.5 * cell.r as f32;
        Vec3::new(x, (cell.y as f32 + 0.5) * self.hexel_height, z)
    }
}

#[derive(Clone, Debug)]
pub struct Chunk {
    cells: Vec<BlockId>,
}

impl Default for Chunk {
    fn default() -> Self {
        Self { cells: vec![BlockId::Air; (CHUNK_Q * CHUNK_R * CHUNK_Y) as usize] }
    }
}

impl Chunk {
    fn index(local: [usize; 3]) -> usize {
        (local[2] * CHUNK_R as usize + local[1]) * CHUNK_Q as usize + local[0]
    }
    pub fn get(&self, local: [usize; 3]) -> BlockId { self.cells[Self::index(local)] }
    pub fn set(&mut self, local: [usize; 3], id: BlockId) { self.cells[Self::index(local)] = id; }
    pub fn len(&self) -> usize { self.cells.len() }
    pub fn is_empty(&self) -> bool { self.cells.is_empty() }
}

#[derive(Default)]
pub struct World {
    chunks: HashMap<ChunkPos, Chunk>,
}

impl World {
    pub fn get(&self, pos: CellPos) -> BlockId {
        let (chunk_pos, local) = pos.chunk_and_local();
        self.chunks.get(&chunk_pos).map_or(BlockId::Air, |c| c.get(local))
    }
    pub fn set(&mut self, pos: CellPos, id: BlockId) {
        let (chunk_pos, local) = pos.chunk_and_local();
        self.chunks.entry(chunk_pos).or_default().set(local, id);
    }
    pub fn chunk_count(&self) -> usize { self.chunks.len() }
}

pub fn terrain_height(seed: u32, q: i32, r: i32) -> i32 {
    let noise = Perlin::new(seed);
    let n = noise.get([q as f64 / 28.0, r as f64 / 28.0]);
    (8.0 + n * 5.0).round() as i32
}

pub fn generated_block(seed: u32, pos: CellPos) -> BlockId {
    if pos.y < 0 {
        return BlockId::Foundation;
    }
    let height = terrain_height(seed, pos.q, pos.r);
    if pos.y > height {
        BlockId::Air
    } else if pos.y == height {
        BlockId::Grass
    } else if pos.y >= height - 3 {
        BlockId::Dirt
    } else {
        BlockId::Stone
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub cell: CellPos,
    pub face: usize,
    pub distance: f32,
}

pub fn raycast_prism(
    origin: Vec3,
    direction: Vec3,
    center: Vec3,
    radius: f32,
    height: f32,
    max_distance: f32,
) -> Option<(f32, usize)> {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let apothem = radius * 3.0_f32.sqrt() * 0.5;
    let half_h = height * 0.5;
    let planes = [
        (Vec3::new(1.0, 0.0, 0.0), apothem, 0usize),
        (Vec3::new(0.5, 0.0, 3.0_f32.sqrt() * 0.5), apothem, 1usize),
        (Vec3::new(-0.5, 0.0, 3.0_f32.sqrt() * 0.5), apothem, 2usize),
        (Vec3::new(-1.0, 0.0, 0.0), apothem, 3usize),
        (Vec3::new(-0.5, 0.0, -3.0_f32.sqrt() * 0.5), apothem, 4usize),
        (Vec3::new(0.5, 0.0, -3.0_f32.sqrt() * 0.5), apothem, 5usize),
        (Vec3::Y, half_h, 6usize),
        (-Vec3::Y, half_h, 7usize),
    ];
    let relative_origin = origin - center;
    let mut enter = 0.0_f32;
    let mut exit = max_distance;
    let mut enter_face = 0usize;
    for (normal, limit, face) in planes {
        let dist = normal.dot(relative_origin);
        let denom = normal.dot(dir);
        if denom.abs() < 1e-6 {
            if dist > limit { return None; }
            continue;
        }
        let t = (limit - dist) / denom;
        if denom < 0.0 {
            if t > enter { enter = t; enter_face = face; }
        } else {
            exit = exit.min(t);
        }
        if enter > exit { return None; }
    }
    if exit >= 0.0 && enter <= max_distance {
        Some((enter.max(0.0), enter_face))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_layout_has_4096_cells() { assert_eq!(Chunk::default().len(), 16 * 16 * 16); }

    #[test]
    fn negative_coordinates_use_euclidean_chunking() {
        let (chunk, local) = CellPos::new(-1, -16, -17).chunk_and_local();
        assert_eq!(chunk, ChunkPos { q: -1, r: -1, y: -2 });
        assert_eq!(local, [15, 0, 15]);
    }

    #[test]
    fn axial_neighbors_have_six_horizontal_and_two_vertical() {
        let n = CellPos::new(0, 0, 0).neighbors();
        assert_eq!(n.len(), 8);
        assert_eq!(n.iter().filter(|p| p.y == 0).count(), 6);
        assert_eq!(
            &n[..6],
            &[
                CellPos::new(1, 0, 0),
                CellPos::new(1, -1, 0),
                CellPos::new(0, -1, 0),
                CellPos::new(-1, 0, 0),
                CellPos::new(-1, 1, 0),
                CellPos::new(0, 1, 0),
            ],
        );
        assert_eq!(n[6], CellPos::new(0, 0, 1));
        assert_eq!(n[7], CellPos::new(0, 0, -1));
    }

    #[test]
    fn hexel_height_is_three_vertical_steps_per_player_height() {
        let s = WorldScale::default();
        assert!((s.hexel_height - 0.6).abs() < 1e-6);
        assert!((s.player_height / s.hexel_height - 3.0).abs() < 1e-6);
    }

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(generated_block(42, CellPos::new(3, -8, 5)), generated_block(42, CellPos::new(3, -8, 5)));
        assert_eq!(terrain_height(42, 3, -8), terrain_height(42, 3, -8));
    }

    #[test]
    fn world_edits_are_authoritative() {
        let mut world = World::default();
        let p = CellPos::new(-1, 2, 3);
        world.set(p, BlockId::Stone);
        assert_eq!(world.get(p), BlockId::Stone);
        world.set(p, BlockId::Air);
        assert_eq!(world.get(p), BlockId::Air);
    }

    #[test]
    fn ray_hits_prism_front_face() {
        let hit = raycast_prism(Vec3::new(0.0, 0.0, -3.0), Vec3::Z, Vec3::ZERO, 1.0, 1.0, 10.0);
        assert!(hit.is_some());
    }
}
