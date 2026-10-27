//! Sparse, deterministic voxel volume indexed over geodesic surface cells.
//!
//! Layer 0 is the generated surface; negative layers extend radially inward and
//! positive layers outward. This gives gameplay a real 3D block address without
//! pretending a surface-cell edit is a physical hole. Mesh generation, ray traversal,
//! collision, and persistence must all consume this same address model.

use std::collections::BTreeMap;

use glam::DVec3;

use crate::{
    BlockId,
    planet::{Biome, PlanetSample},
    planetary_world::PlanetaryWorld,
    topology::SurfaceCellId,
};

/// A voxel in the local radial column above a geodesic surface cell.
/// Layer 0 is the generated surface, -1 is the first subsurface block, +1 is above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanetVoxelPos {
    pub cell: SurfaceCellId,
    pub layer: i32,
}

impl PlanetVoxelPos {
    pub const fn new(cell: SurfaceCellId, layer: i32) -> Self {
        Self { cell, layer }
    }

    pub const fn above(self) -> Self {
        Self { cell: self.cell, layer: self.layer + 1 }
    }

    pub const fn below(self) -> Self {
        Self { cell: self.cell, layer: self.layer - 1 }
    }
}

/// Versioned, deterministic sparse voxel overrides. Procedural layers are generated
/// from the planet's biome; only differences are stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanetVoxelSave {
    pub schema_version: u32,
    pub edits: Vec<(PlanetVoxelPos, BlockId)>,
}

#[derive(Clone, Debug, Default)]
pub struct PlanetVoxelVolume {
    edits: BTreeMap<PlanetVoxelPos, BlockId>,
}

impl PlanetVoxelVolume {
    pub const SAVE_SCHEMA_VERSION: u32 = 1;
    /// A gameplay block's radial thickness in world units.
    pub const DEFAULT_BLOCK_HEIGHT: f64 = 2.4;
    /// A non-removable bedrock layer bounds downward digging.
    pub const BEDROCK_LAYER: i32 = -64;
    /// Bound vertical construction above the procedural surface.
    pub const MAX_BUILD_HEIGHT: i32 = 64;

    pub fn new() -> Self { Self::default() }

    pub fn edited_voxels(&self) -> &BTreeMap<PlanetVoxelPos, BlockId> {
        &self.edits
    }

    /// Query an edited voxel first, then the deterministic generated radial column.
    pub fn block_at(
        &self,
        world: &PlanetaryWorld,
        pos: PlanetVoxelPos,
    ) -> Option<BlockId> {
        world.topology().cell(pos.cell)?;
        Some(self.edits.get(&pos).copied().unwrap_or_else(|| {
            generated_layer(world.sample_cell(pos.cell, 0.0).expect("cell validated"), pos.layer)
        }))
    }

    /// Edit a voxel. Reapplying its generated value removes the sparse override.
    /// Foundation is immutable, including the generated bedrock boundary.
    pub fn set_block(
        &mut self,
        world: &PlanetaryWorld,
        pos: PlanetVoxelPos,
        block: BlockId,
    ) -> bool {
        let Some(sample) = world.sample_cell(pos.cell, 0.0).ok() else {
            return false;
        };
        let generated = generated_layer(sample, pos.layer);
        if generated == BlockId::Foundation && block != BlockId::Foundation {
            return false;
        }
        if block == generated {
            self.edits.remove(&pos);
        } else {
            self.edits.insert(pos, block);
        }
        true
    }

    /// Planet-centred radial voxel centre, retaining f64 precision until render rebasing.
    pub fn voxel_center(
        &self,
        world: &PlanetaryWorld,
        pos: PlanetVoxelPos,
        block_height: f64,
    ) -> Option<DVec3> {
        if !block_height.is_finite() || block_height <= 0.0 {
            return None;
        }
        let cell = world.topology().cell(pos.cell)?;
        let surface = world.terrain().sample_direction(cell.center, 0.0).surface_position;
        let radial = cell.center.normalize();
        let offset = (pos.layer as f64 - 0.5) * block_height;
        Some(surface + radial * offset)
    }

    pub fn save(&self) -> PlanetVoxelSave {
        PlanetVoxelSave {
            schema_version: Self::SAVE_SCHEMA_VERSION,
            edits: self.edits.iter().map(|(pos, block)| (*pos, *block)).collect(),
        }
    }

    /// Validate the entire payload before replacing live edits, so malformed saves
    /// cannot partially mutate a running world.
    pub fn load_save(
        &mut self,
        world: &PlanetaryWorld,
        save: &PlanetVoxelSave,
    ) -> Result<(), &'static str> {
        if save.schema_version != Self::SAVE_SCHEMA_VERSION {
            return Err("unsupported planetary voxel save schema");
        }
        let mut restored = BTreeMap::new();
        for (pos, block) in &save.edits {
            let Some(sample) = world.sample_cell(pos.cell, 0.0).ok() else {
                return Err("planetary voxel save references an invalid surface cell");
            };
            if generated_layer(sample, pos.layer) == BlockId::Foundation
                && *block != BlockId::Foundation
            {
                return Err("planetary voxel save attempts to remove foundation");
            }
            if generated_layer(sample, pos.layer) != *block {
                restored.insert(*pos, *block);
            }
        }
        self.edits = restored;
        Ok(())
    }
}

fn generated_layer(sample: PlanetSample, layer: i32) -> BlockId {
    if layer > 0 {
        return BlockId::Air;
    }
    if layer <= PlanetVoxelVolume::BEDROCK_LAYER {
        return BlockId::Foundation;
    }
    if layer == 0 {
        return match sample.biome {
            Biome::DeepOcean | Biome::ShallowOcean => BlockId::Stone,
            Biome::Mountain | Biome::PolarIce | Biome::Tundra => BlockId::Stone,
            Biome::Desert | Biome::Savanna => BlockId::Dirt,
            Biome::BorealForest | Biome::TemperateForest
            | Biome::Grassland | Biome::TropicalForest => BlockId::Grass,
        };
    }
    if layer >= -3 && !matches!(
        sample.biome,
        Biome::DeepOcean | Biome::ShallowOcean | Biome::Mountain | Biome::PolarIce | Biome::Tundra
    ) {
        BlockId::Dirt
    } else {
        BlockId::Stone
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet::PlanetConfig;

    fn world() -> PlanetaryWorld {
        PlanetaryWorld::new(42, 4, PlanetConfig {
            radius: 100.0,
            elevation_scale: 4.0,
            axial_tilt_degrees: 23.5,
        }).unwrap()
    }

    #[test]
    fn volume_has_surface_air_subsurface_and_bedrock_layers() {
        let world = world();
        let volume = PlanetVoxelVolume::new();
        let cell = world.topology().cells()[0].id;
        assert!(volume.block_at(&world, PlanetVoxelPos::new(cell, 1)).unwrap() == BlockId::Air);
        assert!(volume.block_at(&world, PlanetVoxelPos::new(cell, -1)).unwrap().is_solid());
        assert_eq!(volume.block_at(&world, PlanetVoxelPos::new(cell, -64)), Some(BlockId::Foundation));
    }

    #[test]
    fn voxel_edits_are_sparse_and_reverting_restores_generation() {
        let world = world();
        let mut volume = PlanetVoxelVolume::new();
        let pos = PlanetVoxelPos::new(world.topology().cells()[0].id, 0);
        let original = volume.block_at(&world, pos).unwrap();
        let replacement = if original == BlockId::Air { BlockId::Stone } else { BlockId::Air };
        assert!(volume.set_block(&world, pos, replacement));
        assert_eq!(volume.block_at(&world, pos), Some(replacement));
        assert_eq!(volume.edited_voxels().len(), 1);
        assert!(volume.set_block(&world, pos, original));
        assert!(volume.edited_voxels().is_empty());
    }

    #[test]
    fn foundation_cannot_be_removed() {
        let world = world();
        let mut volume = PlanetVoxelVolume::new();
        let pos = PlanetVoxelPos::new(world.topology().cells()[0].id, PlanetVoxelVolume::BEDROCK_LAYER);
        assert!(!volume.set_block(&world, pos, BlockId::Air));
        assert_eq!(volume.block_at(&world, pos), Some(BlockId::Foundation));
    }

    #[test]
    fn save_round_trip_is_sorted_and_invalid_load_is_atomic() {
        let world = world();
        let mut volume = PlanetVoxelVolume::new();
        let a = PlanetVoxelPos::new(world.topology().cells()[0].id, -2);
        let b = PlanetVoxelPos::new(world.topology().cells()[1].id, 0);
        volume.set_block(&world, a, BlockId::Air);
        volume.set_block(&world, b, BlockId::Stone);
        let save = volume.save();
        assert!(save.edits.windows(2).all(|w| w[0].0 < w[1].0));
        let mut restored = PlanetVoxelVolume::new();
        restored.load_save(&world, &save).unwrap();
        assert_eq!(restored.save(), save);
        let mut invalid = save.clone();
        invalid.edits.push((PlanetVoxelPos::new(SurfaceCellId(u32::MAX), 0), BlockId::Air));
        assert!(restored.load_save(&world, &invalid).is_err());
        assert_eq!(restored.save(), save);
    }

    #[test]
    fn voxel_centres_remain_on_radial_column() {
        let world = world();
        let volume = PlanetVoxelVolume::new();
        let pos = PlanetVoxelPos::new(world.topology().cells()[0].id, 2);
        let center = volume.voxel_center(&world, pos, 1.0).unwrap();
        assert!(center.is_finite());
        assert!(center.length() > world.descriptor().radius);
    }
}
