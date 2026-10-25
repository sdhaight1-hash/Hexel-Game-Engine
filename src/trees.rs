//! Seeded tree placement and harvestable tree state for planetary terrain.
//!
//! This module defines deterministic tree candidates from planetary samples. It is
//! renderer-independent: a tree's root and outward direction are expressed in planet
//! space, so callers orient its trunk to the local surface normal rather than assuming
//! global up. Persist harvested state separately from generated candidates.

use glam::DVec3;
use crate::planet::{Biome, PlanetSample};

pub type TreeId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeSpecies { Oak, Birch, Pine, Spruce, Jungle, Acacia, Scrub }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoliageStyle { Broadleaf, Conifer, Tropical, SparseCanopy }

/// Save this state by TreeId whenever changed; chunk loading must not reset it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeState { Alive, Felled, Stump, Regrowing }

/// A generated, harvestable tree in planet-relative coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeInstance {
    pub id: TreeId,
    pub species: TreeSpecies,
    pub foliage: FoliageStyle,
    /// Root location on the sampled planetary surface, in world units.
    pub root_position: DVec3,
    /// Outward surface normal at the root; the trunk's local up axis.
    pub local_up: DVec3,
    /// Trunk diameter as a multiple of one hexel's diameter, in [0.25, 1.0].
    pub trunk_diameter_hexels: f64,
    pub height: f64,
    /// Approximate usable trunk wood volume, in cubic world units.
    pub wood_volume: f64,
    pub state: TreeState,
}

impl TreeInstance {
    pub fn trunk_diameter_world(self, hexel_diameter: f64) -> f64 {
        self.trunk_diameter_hexels * hexel_diameter.max(0.0)
    }

    /// Return the one-time wood yield. Repeated harvest attempts cannot duplicate it.
    pub fn fell(&mut self) -> f64 {
        if self.state != TreeState::Alive { return 0.0; }
        self.state = TreeState::Felled;
        self.wood_volume
    }

    pub fn leave_stump(&mut self) {
        if self.state == TreeState::Felled { self.state = TreeState::Stump; }
    }
}

/// Deterministic tree candidates. The caller supplies a globally stable placement-slot
/// ID derived from world coordinates, never from chunk load order.
#[derive(Clone, Copy, Debug)]
pub struct TreeGenerator {
    seed: u64,
    density: f64,
    hexel_diameter: f64,
}

impl TreeGenerator {
    pub fn new(seed: u64, density: f64, hexel_diameter: f64) -> Self {
        Self {
            seed,
            density: density.clamp(0.0, 1.0),
            hexel_diameter: if hexel_diameter.is_finite() { hexel_diameter.max(0.001) } else { 1.0 },
        }
    }

    pub fn hexel_diameter(self) -> f64 { self.hexel_diameter }

    pub fn candidate(self, slot_id: TreeId, sample: PlanetSample) -> Option<TreeInstance> {
        let mut rng = splitmix64(self.seed ^ slot_id.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let occupancy = unit_f64(next_random(&mut rng));
        let suitability = suitability(sample.biome, sample.temperature_c, sample.moisture);
        if suitability <= 0.0 || occupancy >= self.density * suitability { return None; }

        let species = choose_species(sample.biome, sample.temperature_c, sample.moisture, next_random(&mut rng));
        let foliage = foliage_for(species);
        let diameter_fraction = 0.25 + unit_f64(next_random(&mut rng)) * 0.75;
        let height_hexels = match species {
            TreeSpecies::Scrub => 2.0 + unit_f64(next_random(&mut rng)) * 2.5,
            TreeSpecies::Acacia => 4.0 + unit_f64(next_random(&mut rng)) * 4.0,
            TreeSpecies::Pine | TreeSpecies::Spruce => 5.0 + unit_f64(next_random(&mut rng)) * 8.0,
            TreeSpecies::Jungle => 6.0 + unit_f64(next_random(&mut rng)) * 10.0,
            TreeSpecies::Oak | TreeSpecies::Birch => 4.0 + unit_f64(next_random(&mut rng)) * 7.0,
        };
        let height = height_hexels * self.hexel_diameter;
        let diameter = diameter_fraction * self.hexel_diameter;
        // Approximate hexagonal trunk cross-section area: sqrt(3)/2 * across-flats diameter².
        let trunk_area = 3.0_f64.sqrt() * 0.5 * diameter * diameter;
        Some(TreeInstance {
            id: slot_id,
            species,
            foliage,
            root_position: sample.surface_position,
            local_up: sample.direction.normalize_or_zero(),
            trunk_diameter_hexels: diameter_fraction,
            height,
            wood_volume: trunk_area * height,
            state: TreeState::Alive,
        })
    }
}

fn suitability(biome: Biome, temperature_c: f64, moisture: f64) -> f64 {
    let moisture = moisture.clamp(0.0, 1.0);
    let temperature_factor = (1.0 - ((temperature_c - 17.0).abs() / 48.0)).clamp(0.0, 1.0);
    let biome_factor = match biome {
        Biome::DeepOcean | Biome::ShallowOcean | Biome::PolarIce | Biome::Mountain => 0.0,
        Biome::Tundra => if temperature_c > -12.0 { 0.12 + moisture * 0.12 } else { 0.0 },
        Biome::BorealForest => 0.78 + moisture * 0.22,
        Biome::TemperateForest => 0.8 + moisture * 0.2,
        Biome::Grassland => 0.12 + moisture * 0.3,
        Biome::Desert => 0.02 + moisture * 0.08,
        Biome::Savanna => 0.3 + moisture * 0.35,
        Biome::TropicalForest => 0.88 + moisture * 0.12,
    };
    biome_factor * (0.65 + temperature_factor * 0.35)
}

fn choose_species(biome: Biome, temperature_c: f64, moisture: f64, random: u64) -> TreeSpecies {
    match biome {
        Biome::BorealForest => if random & 1 == 0 { TreeSpecies::Pine } else { TreeSpecies::Spruce },
        Biome::TemperateForest => match random % 3 {
            0 => TreeSpecies::Oak, 1 => TreeSpecies::Birch, _ => TreeSpecies::Pine,
        },
        Biome::Grassland => if moisture > 0.45 { TreeSpecies::Birch } else { TreeSpecies::Scrub },
        Biome::Desert => if moisture > 0.22 { TreeSpecies::Acacia } else { TreeSpecies::Scrub },
        Biome::Savanna => if random & 1 == 0 { TreeSpecies::Acacia } else { TreeSpecies::Scrub },
        Biome::TropicalForest => if temperature_c > 20.0 { TreeSpecies::Jungle } else { TreeSpecies::Oak },
        Biome::Tundra => TreeSpecies::Scrub,
        Biome::DeepOcean | Biome::ShallowOcean | Biome::PolarIce | Biome::Mountain => TreeSpecies::Scrub,
    }
}

fn foliage_for(species: TreeSpecies) -> FoliageStyle {
    match species {
        TreeSpecies::Oak | TreeSpecies::Birch => FoliageStyle::Broadleaf,
        TreeSpecies::Pine | TreeSpecies::Spruce => FoliageStyle::Conifer,
        TreeSpecies::Jungle => FoliageStyle::Tropical,
        TreeSpecies::Acacia | TreeSpecies::Scrub => FoliageStyle::SparseCanopy,
    }
}

fn unit_f64(value: u64) -> f64 { (value >> 11) as f64 / ((1_u64 << 53) as f64) }
fn next_random(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    splitmix64(*state)
}
fn splitmix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet::{GeoCoord, PlanetGenerator};

    fn sample_at(latitude: f64, longitude: f64, seed: u32) -> PlanetSample {
        PlanetGenerator::new(seed).sample(GeoCoord::from_degrees(latitude, longitude), 0.0)
    }

    #[test]
    fn candidate_is_deterministic_for_seed_and_slot() {
        let generator = TreeGenerator::new(77, 1.0, 1.0);
        let sample = sample_at(35.0, -90.0, 4);
        assert_eq!(generator.candidate(123, sample), generator.candidate(123, sample));
    }

    #[test]
    fn trunk_diameter_stays_within_quarter_to_one_hexel() {
        let generator = TreeGenerator::new(91, 1.0, 1.0);
        let sample = sample_at(35.0, -90.0, 4);
        for slot in 0..10_000 {
            if let Some(tree) = generator.candidate(slot, sample) {
                assert!((0.25..=1.0).contains(&tree.trunk_diameter_hexels));
            }
        }
    }

    #[test]
    fn trees_grow_outward_from_planet_surface() {
        let sample = sample_at(20.0, 110.0, 42);
        if let Some(tree) = TreeGenerator::new(3, 1.0, 1.0).candidate(9, sample) {
            assert!(tree.local_up.dot(sample.direction) > 0.999);
            assert_eq!(tree.root_position, sample.surface_position);
        }
    }

    #[test]
    fn felling_yields_wood_only_once() {
        let sample = sample_at(35.0, -90.0, 4);
        let generator = TreeGenerator::new(77, 1.0, 1.0);
        let mut tree = (0..1000).find_map(|id| generator.candidate(id, sample)).expect("candidate");
        let yield_once = tree.fell();
        assert!(yield_once > 0.0);
        assert_eq!(tree.fell(), 0.0);
        tree.leave_stump();
        assert_eq!(tree.state, TreeState::Stump);
    }
}
