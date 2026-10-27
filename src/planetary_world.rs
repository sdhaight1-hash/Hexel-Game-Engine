//! Authoritative geodesic planet state, separate from rendering.
//!
//! The topology defines surface identity, the terrain generator defines untouched
//! terrain, and this module owns player edits. Rendering and gameplay should query
//! this model rather than keeping competing copies of planetary block state.

use std::collections::BTreeMap;

use glam::DVec3;

use crate::{
    BlockId,
    planet::{Biome, PlanetConfig, PlanetGenerator, PlanetSample},
    topology::{SurfaceCellId, SurfaceTopology, TopologyError},
};

/// Bump when the meaning or encoding of a saved planetary world changes.
pub const PLANET_SAVE_SCHEMA_VERSION: u32 = 1;
/// Bump when terrain generation changes in a save-incompatible way.
pub const PLANET_GENERATOR_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetWorldError {
    Topology(TopologyError),
    UnsupportedSaveSchema(u32),
    GeneratorVersionMismatch { saved: u32, current: u32 },
    WorldDescriptorMismatch,
    InvalidCell(SurfaceCellId),
}

impl From<TopologyError> for PlanetWorldError {
    fn from(value: TopologyError) -> Self {
        Self::Topology(value)
    }
}

/// Explicit identity of the procedural world a save belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetDescriptor {
    pub schema_version: u32,
    pub generator_version: u32,
    pub seed: u32,
    pub frequency: u32,
    pub radius: f64,
    pub elevation_scale: f64,
    pub axial_tilt_degrees: f64,
}

impl PlanetDescriptor {
    fn new(seed: u32, frequency: u32, config: PlanetConfig) -> Self {
        Self {
            schema_version: PLANET_SAVE_SCHEMA_VERSION,
            generator_version: PLANET_GENERATOR_VERSION,
            seed,
            frequency,
            radius: config.radius,
            elevation_scale: config.elevation_scale,
            axial_tilt_degrees: config.axial_tilt_degrees,
        }
    }
}

/// Portable save payload. Cell IDs are only meaningful with this full descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanetSave {
    pub descriptor: PlanetDescriptor,
    /// Sorted by cell ID for deterministic saves and diffs.
    pub edits: Vec<(SurfaceCellId, BlockId)>,
}

/// Authoritative data model for a geodesic planet.
///
/// This first version stores edits at surface-cell granularity. It deliberately does
/// not claim to implement vertical block layers or physical holes; those require a
/// volume/chunk representation and a corresponding mesher.
#[derive(Clone, Debug)]
pub struct PlanetaryWorld {
    descriptor: PlanetDescriptor,
    topology: SurfaceTopology,
    terrain: PlanetGenerator,
    edits: BTreeMap<SurfaceCellId, BlockId>,
}

impl PlanetaryWorld {
    pub fn new(seed: u32, frequency: u32, config: PlanetConfig) -> Result<Self, PlanetWorldError> {
        let terrain = PlanetGenerator::with_config(seed, config);
        let effective_config = terrain.config();
        let topology = SurfaceTopology::generate(frequency, effective_config.radius)?;
        Ok(Self {
            descriptor: PlanetDescriptor::new(seed, frequency, effective_config),
            topology,
            terrain,
            edits: BTreeMap::new(),
        })
    }

    pub fn descriptor(&self) -> PlanetDescriptor {
        self.descriptor
    }

    pub fn topology(&self) -> &SurfaceTopology {
        &self.topology
    }

    /// Procedural terrain source used by rendering, collision, and sampling.
    pub fn terrain(&self) -> &PlanetGenerator {
        &self.terrain
    }

    pub fn edited_cells(&self) -> &BTreeMap<SurfaceCellId, BlockId> {
        &self.edits
    }

    pub fn sample_cell(
        &self,
        id: SurfaceCellId,
        season_phase: f64,
    ) -> Result<PlanetSample, PlanetWorldError> {
        let cell = self.topology.cell(id).ok_or(PlanetWorldError::InvalidCell(id))?;
        Ok(self.terrain.sample_direction(cell.center, season_phase))
    }

    /// Resolve a block using edits first, then deterministic biome defaults.
    pub fn block_at(&self, id: SurfaceCellId) -> Result<BlockId, PlanetWorldError> {
        let sample = self.sample_cell(id, 0.0)?;
        Ok(self.edits.get(&id).copied().unwrap_or_else(|| default_block(sample)))
    }

    /// Set or clear a cell override. Passing its generated default removes the override.
    pub fn set_block(&mut self, id: SurfaceCellId, block: BlockId) -> Result<(), PlanetWorldError> {
        let sample = self.sample_cell(id, 0.0)?;
        if block == default_block(sample) {
            self.edits.remove(&id);
        } else {
            self.edits.insert(id, block);
        }
        Ok(())
    }

    pub fn cell_id_at_direction(&self, direction: DVec3) -> Option<SurfaceCellId> {
        self.topology.cell_id_for_direction(direction)
    }

    pub fn save(&self) -> PlanetSave {
        PlanetSave {
            descriptor: self.descriptor,
            edits: self.edits.iter().map(|(id, block)| (*id, *block)).collect(),
        }
    }

    /// Restore only into an exactly matching procedural world. This avoids silently
    /// applying old numeric cell IDs to a different topology or generator version.
    pub fn load_save(&mut self, save: &PlanetSave) -> Result<(), PlanetWorldError> {
        if save.descriptor.schema_version != PLANET_SAVE_SCHEMA_VERSION {
            return Err(PlanetWorldError::UnsupportedSaveSchema(
                save.descriptor.schema_version,
            ));
        }
        if save.descriptor.generator_version != PLANET_GENERATOR_VERSION {
            return Err(PlanetWorldError::GeneratorVersionMismatch {
                saved: save.descriptor.generator_version,
                current: PLANET_GENERATOR_VERSION,
            });
        }
        if save.descriptor != self.descriptor {
            return Err(PlanetWorldError::WorldDescriptorMismatch);
        }

        let mut restored = BTreeMap::new();
        for (id, block) in &save.edits {
            if self.topology.cell(*id).is_none() {
                return Err(PlanetWorldError::InvalidCell(*id));
            }
            restored.insert(*id, *block);
        }
        self.edits = restored;
        Ok(())
    }
}

fn default_block(sample: PlanetSample) -> BlockId {
    match sample.biome {
        Biome::DeepOcean | Biome::ShallowOcean => BlockId::Stone,
        Biome::Mountain | Biome::PolarIce | Biome::Tundra => BlockId::Stone,
        Biome::Desert | Biome::Savanna => BlockId::Dirt,
        Biome::BorealForest | Biome::TemperateForest | Biome::Grassland | Biome::TropicalForest => {
            BlockId::Grass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> PlanetaryWorld {
        PlanetaryWorld::new(
            42,
            4,
            PlanetConfig {
                radius: 6_371_000.0,
                elevation_scale: 4_500.0,
                axial_tilt_degrees: 23.5,
            },
        )
        .unwrap()
    }

    #[test]
    fn world_uses_planet_scale_and_expected_geodesic_cell_count() {
        let world = world();
        assert_eq!(world.descriptor().radius, 6_371_000.0);
        assert_eq!(world.topology().cell_count(), 162);
    }

    #[test]
    fn generated_terrain_is_deterministic_for_same_seed_and_cell() {
        let a = world();
        let b = world();
        for cell in a.topology().cells() {
            assert_eq!(a.sample_cell(cell.id, 0.0).unwrap(), b.sample_cell(cell.id, 0.0).unwrap());
            assert_eq!(a.block_at(cell.id).unwrap(), b.block_at(cell.id).unwrap());
        }
    }

    #[test]
    fn edits_override_generated_blocks_and_default_removes_override() {
        let mut world = world();
        let id = world.topology().cells()[0].id;
        let generated = world.block_at(id).unwrap();
        let replacement = if generated == BlockId::Air { BlockId::Grass } else { BlockId::Air };
        world.set_block(id, replacement).unwrap();
        assert_eq!(world.block_at(id).unwrap(), replacement);
        assert_eq!(world.edited_cells().len(), 1);
        world.set_block(id, generated).unwrap();
        assert_eq!(world.block_at(id).unwrap(), generated);
        assert!(world.edited_cells().is_empty());
    }

    #[test]
    fn save_round_trip_preserves_edits_and_is_deterministically_sorted() {
        let mut source = world();
        for id in [SurfaceCellId(40), SurfaceCellId(3), SurfaceCellId(20)] {
            source.set_block(id, BlockId::Air).unwrap();
        }
        let save = source.save();
        assert!(save.edits.windows(2).all(|pair| pair[0].0 < pair[1].0));

        let mut restored = world();
        restored.load_save(&save).unwrap();
        assert_eq!(restored.save(), save);
    }

    #[test]
    fn save_rejects_a_different_seed_or_resolution_without_mutating_world() {
        let source = world();
        let save = source.save();
        let mut incompatible = PlanetaryWorld::new(
            99,
            4,
            PlanetConfig { radius: 6_371_000.0, elevation_scale: 4_500.0, axial_tilt_degrees: 23.5 },
        ).unwrap();
        assert_eq!(incompatible.load_save(&save), Err(PlanetWorldError::WorldDescriptorMismatch));
        assert!(incompatible.edited_cells().is_empty());
    }

    #[test]
    fn invalid_cell_edits_fail_cleanly() {
        let mut world = world();
        let invalid = SurfaceCellId(u32::MAX);
        assert_eq!(world.block_at(invalid), Err(PlanetWorldError::InvalidCell(invalid)));
        assert_eq!(world.set_block(invalid, BlockId::Grass), Err(PlanetWorldError::InvalidCell(invalid)));
    }
}
