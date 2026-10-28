//! Chunk-backed editable terrain addressed on a spherical planet.
//!
//! Each loaded entry owns a real 16×16×16 Hexel prism chunk. Surface sampling
//! determines the biome and crust material; edits live in the chunk itself rather
//! than in a parallel surface-cell or radial-column store.

use std::collections::{HashMap, HashSet};

use glam::DVec3;

use crate::{
    BlockId, CellPos, Chunk, ChunkPos, WorldScale,
    planetary_chunks::{PlanetHexChunkKey, PlanetHexChunkPlacement},
    planetary_frame::PlanetaryFrame,
    planet::{Biome, PlanetGenerator},
    world_profile::{HEXEL_HEIGHT_METRES, HEXELS_PER_CHUNK, PlanetaryWorldProfile},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetaryChunkError {
    InvalidAnchor,
}

#[derive(Clone, Debug)]
pub struct LoadedPlanetHexChunk {
    pub placement: PlanetHexChunkPlacement,
    pub data: Chunk,
}

/// Editable source of truth for loaded 16³ volumes. Chunks are generated lazily
/// and deterministically; persistence can unload them without changing their keys.
#[derive(Clone, Debug)]
pub struct PlanetaryChunkWorld {
    profile: PlanetaryWorldProfile,
    terrain: PlanetGenerator,
    scale: WorldScale,
    chunks: HashMap<PlanetHexChunkKey, LoadedPlanetHexChunk>,
}

impl PlanetaryChunkWorld {
    pub fn new(
        terrain: PlanetGenerator,
        profile: PlanetaryWorldProfile,
    ) -> Result<Self, PlanetaryChunkError> {
        if !profile.is_valid() {
            return Err(PlanetaryChunkError::InvalidAnchor);
        }
        Ok(Self {
            profile,
            terrain,
            scale: WorldScale {
                player_height: crate::world_profile::REFERENCE_PLAYER_HEIGHT_METRES,
                hexel_height: HEXEL_HEIGHT_METRES,
                hex_radius: 0.5,
            },
            chunks: HashMap::new(),
        })
    }

    pub fn profile(&self) -> PlanetaryWorldProfile { self.profile }
    pub fn loaded_chunk_count(&self) -> usize { self.chunks.len() }

    pub fn loaded_chunk(&self, key: PlanetHexChunkKey) -> Option<&LoadedPlanetHexChunk> {
        self.chunks.get(&key)
    }

    pub fn loaded_chunk_mut(&mut self, key: PlanetHexChunkKey) -> Option<&mut LoadedPlanetHexChunk> {
        self.chunks.get_mut(&key)
    }

    /// Drop resident chunk data outside the active streaming window. Chunk keys and
    /// deterministic generation remain stable, so persistence can restore them later.
    pub fn retain_chunks(&mut self, keep: &HashSet<PlanetHexChunkKey>) {
        self.chunks.retain(|key, _| keep.contains(key));
    }

    /// Load or generate a chunk addressed relative to a stable planetary anchor.
    /// Vertical chunk layer zero starts at the surface; negative layers extend
    /// underground and positive layers extend into open air.
    pub fn ensure_chunk(
        &mut self,
        anchor_cell: u32,
        anchor_direction: DVec3,
        chunk: ChunkPos,
    ) -> Result<&LoadedPlanetHexChunk, PlanetaryChunkError> {
        if !anchor_direction.is_finite() || anchor_direction.length_squared() <= f64::EPSILON {
            return Err(PlanetaryChunkError::InvalidAnchor);
        }
        let key = PlanetHexChunkKey { anchor_cell, q: chunk.q, r: chunk.r, y: chunk.y };
        if !self.chunks.contains_key(&key) {
            let anchor_sample = self.terrain.sample_direction(anchor_direction.normalize(), 0.0);
            let anchor_frame = PlanetaryFrame::at(anchor_sample.surface_position);
            let horizontal_origin = crate::planetary_chunks::hex_chunk_horizontal_origin(
                chunk, self.scale.hex_radius as f64,
            );
            let chunk_surface = if chunk.q == 0 && chunk.r == 0 {
                anchor_sample
            } else {
                let approximate = anchor_frame.origin
                    + anchor_frame.east * horizontal_origin.x
                    + anchor_frame.north * horizontal_origin.y;
                self.terrain.sample_direction(approximate, 0.0)
            };
            let frame = PlanetaryFrame::at(chunk_surface.surface_position);
            let placement = PlanetHexChunkPlacement::new(anchor_cell, frame, chunk);
            let mut data = Chunk::default();
            // All horizontal chunks sharing this anchor use the same vertical datum.
            // Using each chunk's own sampled elevation here made neighbouring chunks
            // disagree about which y-layer a surface cell occupies, creating seams.
            self.generate_chunk_cells(&mut data, anchor_frame, anchor_sample.elevation, chunk);
            self.chunks.insert(key, LoadedPlanetHexChunk { placement, data });
        }
        Ok(self.chunks.get(&key).expect("chunk inserted above"))
    }

    /// Unloaded chunks read as air; ensure the target chunk before gameplay queries.
    pub fn get_loaded(&self, anchor_cell: u32, pos: CellPos) -> BlockId {
        let (chunk, local) = pos.chunk_and_local();
        let key = PlanetHexChunkKey { anchor_cell, q: chunk.q, r: chunk.r, y: chunk.y };
        self.chunks.get(&key).map_or(BlockId::Air, |loaded| loaded.data.get(local))
    }

    /// Edit a loaded chunk. Returns false when its chunk has not been loaded.
    pub fn set_loaded(&mut self, anchor_cell: u32, pos: CellPos, block: BlockId) -> bool {
        let (chunk, local) = pos.chunk_and_local();
        let key = PlanetHexChunkKey { anchor_cell, q: chunk.q, r: chunk.r, y: chunk.y };
        if let Some(loaded) = self.chunks.get_mut(&key) {
            loaded.data.set(local, block);
            true
        } else {
            false
        }
    }

    pub fn scale(&self) -> WorldScale { self.scale }

    fn generate_chunk_cells(
        &self,
        data: &mut Chunk,
        anchor_frame: PlanetaryFrame,
        chunk_base_elevation: f64,
        chunk: ChunkPos,
    ) {
        // Keep a single vertical datum for all chunks around this anchor while
        // sampling terrain independently for each horizontal Hexel. Elevation
        // differences become whole 0.6 m layers; the same seed/direction always
        // yields the same surface and biome.
        for local_r in 0..16usize {
            for local_q in 0..16usize {
                let q = (chunk.q * 16 + local_q as i32) as f64;
                let r = (chunk.r * 16 + local_r as i32) as f64;
                let x = self.scale.hex_radius as f64 * 3.0_f64.sqrt() * (q + r * 0.5);
                let z = self.scale.hex_radius as f64 * 1.5 * r;
                let approximate = anchor_frame.origin
                    + anchor_frame.east * x
                    + anchor_frame.north * z;
                let sample = self.terrain.sample_direction(approximate, 0.0);
                let surface_layer = ((sample.elevation - chunk_base_elevation)
                    / self.scale.hexel_height as f64).round() as i32;
                for local_y in 0..16usize {
                    let world_y = chunk.y * 16 + local_y as i32;
                    let depth_below_surface = surface_layer - world_y;
                    let block = if depth_below_surface < 0 {
                        BlockId::Air
                    } else if depth_below_surface == 0 {
                        surface_block(sample.biome)
                    } else if depth_below_surface <= 3 {
                        BlockId::Dirt
                    } else {
                        BlockId::Stone
                    };
                    data.set([local_q, local_r, local_y], block);
                }
            }
        }
        debug_assert_eq!(data.len(), HEXELS_PER_CHUNK);
    }
}

fn surface_block(biome: Biome) -> BlockId {
    match biome {
        Biome::DeepOcean | Biome::ShallowOcean | Biome::Mountain | Biome::PolarIce | Biome::Tundra => BlockId::Stone,
        Biome::Desert | Biome::Savanna => BlockId::Dirt,
        Biome::BorealForest | Biome::TemperateForest | Biome::Grassland | Biome::TropicalForest => BlockId::Grass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{planet::PlanetConfig, world_profile::PlanetaryWorldProfile};

    fn store() -> PlanetaryChunkWorld {
        PlanetaryChunkWorld::new(
            PlanetGenerator::with_config(7, PlanetConfig {
                radius: 6_371_000.0,
                elevation_scale: 4_500.0,
                axial_tilt_degrees: 23.5,
            }),
            PlanetaryWorldProfile::default(),
        ).unwrap()
    }

    #[test]
    fn loading_a_planet_chunk_allocates_exactly_4096_real_hexels() {
        let mut world = store();
        let (cell_count, origin_is_finite) = {
            let chunk = world.ensure_chunk(5, DVec3::X, ChunkPos::default()).unwrap();
            (chunk.data.len(), chunk.placement.frame.origin.is_finite())
        };
        assert_eq!(cell_count, HEXELS_PER_CHUNK);
        assert_eq!(world.loaded_chunk_count(), 1);
        assert!(origin_is_finite);
    }

    #[test]
    fn generated_chunk_has_surface_dirt_and_stone_layers() {
        let mut world = store();
        let surface = world.ensure_chunk(5, DVec3::X, ChunkPos::default()).unwrap();
        assert!(matches!(surface.data.get([0, 0, 0]), BlockId::Grass | BlockId::Dirt | BlockId::Stone));
        let underground = world.ensure_chunk(5, DVec3::X, ChunkPos { q: 0, r: 0, y: -1 }).unwrap();
        assert_eq!(underground.data.get([0, 0, 15]), BlockId::Dirt);
        assert_eq!(underground.data.get([0, 0, 0]), BlockId::Stone);
        let sky = world.ensure_chunk(5, DVec3::X, ChunkPos { q: 0, r: 0, y: 1 }).unwrap();
        assert_eq!(sky.data.get([0, 0, 0]), BlockId::Air);
    }

    #[test]
    fn loaded_chunk_edits_change_the_same_authoritative_cells() {
        let mut world = store();
        world.ensure_chunk(12, DVec3::Z, ChunkPos::default()).unwrap();
        let pos = CellPos::new(1, 2, 0);
        assert!(world.set_loaded(12, pos, BlockId::Air));
        assert_eq!(world.get_loaded(12, pos), BlockId::Air);
        assert!(!world.set_loaded(12, CellPos::new(1000, 1000, 0), BlockId::Stone));
    }

    #[test]
    fn streaming_window_can_release_chunks_without_changing_their_addresses() {
        let mut world = store();
        world.ensure_chunk(5, DVec3::X, ChunkPos::default()).unwrap();
        world.ensure_chunk(5, DVec3::X, ChunkPos { q: 1, r: 0, y: 0 }).unwrap();
        let keep = [PlanetHexChunkKey { anchor_cell: 5, q: 1, r: 0, y: 0 }]
            .into_iter().collect();
        world.retain_chunks(&keep);
        assert_eq!(world.loaded_chunk_count(), 1);
        assert!(world.loaded_chunk(PlanetHexChunkKey { anchor_cell: 5, q: 1, r: 0, y: 0 }).is_some());
        assert!(world.loaded_chunk(PlanetHexChunkKey { anchor_cell: 5, q: 0, r: 0, y: 0 }).is_none());
    }

    #[test]
    fn invalid_anchor_is_rejected() {
        let mut world = store();
        assert_eq!(
            world.ensure_chunk(0, DVec3::ZERO, ChunkPos::default()).unwrap_err(),
            PlanetaryChunkError::InvalidAnchor
        );
    }
}
