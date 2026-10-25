//! Deterministic planetary tree roster, placement, and harvest state.
//!
//! Generated candidates are renderer-independent and use planet-relative positions.
//! Persist changed TreeState and resource inventories by stable TreeId; never regenerate
//! harvested trees as alive when a chunk is loaded again.

use glam::DVec3;
use crate::planet::{Biome, PlanetSample};

pub type TreeId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TreeSpecies {
    Acacia, BaldCypress, Birch, CrimsonKingMaple, Ebony, MediterraneanCypress,
    Kapok, Larch, Maple, Oak, Pine, Purpleheart, Redwood, Walnut, Bamboo, Willow,
    // Transitional species retained for old callers/saves while the roster is adopted.
    Spruce, Jungle, Scrub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TreeVariant {
    Standard, PricklyMoses, Swamp, SilverBirch, HimalayanBirch, RiverBirch, DwarfBirch,
    NorwayMaple, MountainMaple, JapaneseMaple, SugarMaple, MountainPine, ScotsPine,
    Fir, Bristlecone, Massive, OldGrowth, Giant, Viney, Shrub, GreenBamboo, BrownBamboo,
    RiverWillow, BogWillow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoliageStyle { Broadleaf, Conifer, Tropical, SparseCanopy, Bamboo }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeState { Alive, Felled, Stump, Regrowing }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarvestProduct { Logs, Resin, FlexibleTwigs, MedicinalBark, BambooCulm }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpeciesProfile {
    pub species: TreeSpecies,
    pub min_temperature_c: f64,
    pub max_temperature_c: f64,
    /// Relative selection weight. Rare species have deliberately small weights.
    pub rarity_weight: f64,
    pub min_moisture: f64,
    pub max_moisture: f64,
    pub foliage: FoliageStyle,
    pub base_height_hexels: f64,
    pub height_variation_hexels: f64,
    pub products: &'static [HarvestProduct],
}

/// Canonical starting roster. River/bog/swamp and deep-forest constraints are
/// represented by moisture/elevation/biome proxies until hydrology exists.
pub const SPECIES_PROFILES: &[SpeciesProfile] = &[
    SpeciesProfile { species: TreeSpecies::Acacia, min_temperature_c: 28.0, max_temperature_c: 40.0, rarity_weight: 0.75, min_moisture: 0.05, max_moisture: 0.75, foliage: FoliageStyle::SparseCanopy, base_height_hexels: 5.0, height_variation_hexels: 4.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::BaldCypress, min_temperature_c: 15.0, max_temperature_c: 24.0, rarity_weight: 0.45, min_moisture: 0.68, max_moisture: 1.0, foliage: FoliageStyle::Broadleaf, base_height_hexels: 6.0, height_variation_hexels: 4.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Birch, min_temperature_c: 0.0, max_temperature_c: 22.0, rarity_weight: 1.0, min_moisture: 0.15, max_moisture: 1.0, foliage: FoliageStyle::Broadleaf, base_height_hexels: 5.0, height_variation_hexels: 5.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::CrimsonKingMaple, min_temperature_c: 8.0, max_temperature_c: 9.0, rarity_weight: 0.2, min_moisture: 0.42, max_moisture: 1.0, foliage: FoliageStyle::Broadleaf, base_height_hexels: 6.0, height_variation_hexels: 4.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Ebony, min_temperature_c: 28.0, max_temperature_c: 40.0, rarity_weight: 0.025, min_moisture: 0.35, max_moisture: 1.0, foliage: FoliageStyle::Tropical, base_height_hexels: 7.0, height_variation_hexels: 5.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::MediterraneanCypress, min_temperature_c: 8.0, max_temperature_c: 22.0, rarity_weight: 0.55, min_moisture: 0.0, max_moisture: 0.5, foliage: FoliageStyle::Conifer, base_height_hexels: 7.0, height_variation_hexels: 5.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Kapok, min_temperature_c: 25.0, max_temperature_c: 40.0, rarity_weight: 0.7, min_moisture: 0.58, max_moisture: 1.0, foliage: FoliageStyle::Tropical, base_height_hexels: 9.0, height_variation_hexels: 9.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Larch, min_temperature_c: -17.0, max_temperature_c: -3.0, rarity_weight: 0.65, min_moisture: 0.1, max_moisture: 0.8, foliage: FoliageStyle::Conifer, base_height_hexels: 5.0, height_variation_hexels: 4.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Maple, min_temperature_c: 0.0, max_temperature_c: 23.0, rarity_weight: 0.9, min_moisture: 0.25, max_moisture: 1.0, foliage: FoliageStyle::Broadleaf, base_height_hexels: 5.0, height_variation_hexels: 5.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Oak, min_temperature_c: 2.0, max_temperature_c: 22.0, rarity_weight: 1.0, min_moisture: 0.2, max_moisture: 0.95, foliage: FoliageStyle::Broadleaf, base_height_hexels: 5.0, height_variation_hexels: 6.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Pine, min_temperature_c: -18.0, max_temperature_c: 40.0, rarity_weight: 0.9, min_moisture: 0.0, max_moisture: 0.85, foliage: FoliageStyle::Conifer, base_height_hexels: 6.0, height_variation_hexels: 7.0, products: &[HarvestProduct::Logs, HarvestProduct::Resin] },
    SpeciesProfile { species: TreeSpecies::Purpleheart, min_temperature_c: 27.0, max_temperature_c: 40.0, rarity_weight: 0.008, min_moisture: 0.68, max_moisture: 1.0, foliage: FoliageStyle::Tropical, base_height_hexels: 7.0, height_variation_hexels: 5.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Redwood, min_temperature_c: 14.0, max_temperature_c: 18.0, rarity_weight: 0.16, min_moisture: 0.35, max_moisture: 1.0, foliage: FoliageStyle::Conifer, base_height_hexels: 16.0, height_variation_hexels: 18.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Walnut, min_temperature_c: 6.0, max_temperature_c: 22.0, rarity_weight: 0.55, min_moisture: 0.25, max_moisture: 0.85, foliage: FoliageStyle::Broadleaf, base_height_hexels: 5.0, height_variation_hexels: 5.0, products: &[HarvestProduct::Logs] },
    SpeciesProfile { species: TreeSpecies::Bamboo, min_temperature_c: 25.0, max_temperature_c: 40.0, rarity_weight: 0.5, min_moisture: 0.35, max_moisture: 1.0, foliage: FoliageStyle::Bamboo, base_height_hexels: 5.0, height_variation_hexels: 8.0, products: &[HarvestProduct::BambooCulm] },
    SpeciesProfile { species: TreeSpecies::Willow, min_temperature_c: 2.0, max_temperature_c: 20.0, rarity_weight: 0.6, min_moisture: 0.72, max_moisture: 1.0, foliage: FoliageStyle::Broadleaf, base_height_hexels: 4.0, height_variation_hexels: 4.0, products: &[HarvestProduct::Logs, HarvestProduct::FlexibleTwigs, HarvestProduct::MedicinalBark] },
];

impl TreeSpecies {
    pub fn profile(self) -> Option<&'static SpeciesProfile> {
        SPECIES_PROFILES.iter().find(|p| p.species == self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeInstance {
    pub id: TreeId,
    pub species: TreeSpecies,
    pub variant: TreeVariant,
    pub foliage: FoliageStyle,
    pub root_position: DVec3,
    pub local_up: DVec3,
    pub trunk_diameter_hexels: f64,
    pub height: f64,
    pub wood_volume: f64,
    pub state: TreeState,
    pub products: &'static [HarvestProduct],
}

impl TreeInstance {
    pub fn trunk_diameter_world(self, hexel_diameter: f64) -> f64 {
        self.trunk_diameter_hexels * hexel_diameter.max(0.0)
    }

    /// Yield the usable trunk volume once. Persist this state and awarded inventory.
    pub fn fell(&mut self) -> f64 {
        if self.state != TreeState::Alive { return 0.0; }
        self.state = TreeState::Felled;
        self.wood_volume
    }

    pub fn leave_stump(&mut self) {
        if self.state == TreeState::Felled { self.state = TreeState::Stump; }
    }
}

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
        let profile = choose_profile(sample, next_random(&mut rng))?;
        let suitability = suitability(sample.biome, sample.elevation, sample.moisture);
        if occupancy >= self.density * suitability * profile.rarity_weight { return None; }

        let variant = choose_variant(profile.species, next_random(&mut rng));
        let diameter_fraction = 0.25 + unit_f64(next_random(&mut rng)) * 0.75;
        let height_hexels = profile.base_height_hexels
            + unit_f64(next_random(&mut rng)) * profile.height_variation_hexels;
        let diameter = diameter_fraction * self.hexel_diameter;
        let height = height_hexels * self.hexel_diameter;
        // Regular hexagonal cross-section, using across-flats diameter.
        let trunk_area = 3.0_f64.sqrt() * 0.5 * diameter * diameter;
        Some(TreeInstance {
            id: slot_id,
            species: profile.species,
            variant,
            foliage: profile.foliage,
            root_position: sample.surface_position,
            local_up: sample.direction.normalize_or_zero(),
            trunk_diameter_hexels: diameter_fraction,
            height,
            wood_volume: trunk_area * height,
            state: TreeState::Alive,
            products: profile.products,
        })
    }
}

fn choose_profile(sample: PlanetSample, random: u64) -> Option<&'static SpeciesProfile> {
    if sample.elevation < 0.0 || matches!(sample.biome, Biome::DeepOcean | Biome::ShallowOcean | Biome::PolarIce | Biome::Mountain) {
        return None;
    }
    let eligible: Vec<_> = SPECIES_PROFILES.iter().filter(|p| {
        sample.temperature_c >= p.min_temperature_c
            && sample.temperature_c <= p.max_temperature_c
            && sample.moisture >= p.min_moisture
            && sample.moisture <= p.max_moisture
            && biome_supports(p.species, sample.biome)
    }).collect();
    if eligible.is_empty() { return None; }
    // Weighted deterministic choice; rare types are much less likely than common types.
    let total: f64 = eligible.iter().map(|p| p.rarity_weight).sum();
    let mut pick = unit_f64(random) * total;
    for profile in &eligible {
        pick -= profile.rarity_weight;
        if pick <= 0.0 { return Some(*profile); }
    }
    eligible.last().copied()
}

fn biome_supports(species: TreeSpecies, biome: Biome) -> bool {
    match species {
        TreeSpecies::Acacia => matches!(biome, Biome::Savanna | Biome::Desert | Biome::TropicalForest),
        TreeSpecies::Ebony | TreeSpecies::Kapok | TreeSpecies::Purpleheart | TreeSpecies::Bamboo => biome == Biome::TropicalForest,
        TreeSpecies::BaldCypress | TreeSpecies::Willow => matches!(biome, Biome::TemperateForest | Biome::Grassland | Biome::BorealForest),
        TreeSpecies::Larch => matches!(biome, Biome::BorealForest | Biome::Tundra),
        TreeSpecies::MediterraneanCypress => matches!(biome, Biome::Grassland | Biome::Savanna | Biome::TemperateForest),
        TreeSpecies::Redwood => matches!(biome, Biome::BorealForest | Biome::TemperateForest | Biome::Mountain),
        TreeSpecies::Oak | TreeSpecies::Birch | TreeSpecies::Maple | TreeSpecies::CrimsonKingMaple | TreeSpecies::Walnut => biome == Biome::TemperateForest,
        TreeSpecies::Pine => matches!(biome, Biome::BorealForest | Biome::TemperateForest | Biome::Grassland | Biome::Savanna | Biome::Desert),
        TreeSpecies::Spruce | TreeSpecies::Jungle | TreeSpecies::Scrub => false,
    }
}

fn suitability(biome: Biome, elevation: f64, moisture: f64) -> f64 {
    let base: f64 = match biome {
        Biome::DeepOcean | Biome::ShallowOcean | Biome::PolarIce | Biome::Mountain => 0.0,
        Biome::Tundra => 0.35,
        Biome::BorealForest | Biome::TemperateForest | Biome::TropicalForest => 1.0,
        Biome::Grassland | Biome::Savanna => 0.6,
        Biome::Desert => 0.25,
    };
    base * (0.35 + moisture.clamp(0.0, 1.0) * 0.65) * if elevation > 2500.0 { 0.35 } else { 1.0 }
}

fn choose_variant(species: TreeSpecies, random: u64) -> TreeVariant {
    match species {
        TreeSpecies::Acacia if random % 8 == 0 => TreeVariant::PricklyMoses,
        TreeSpecies::BaldCypress => TreeVariant::Swamp,
        TreeSpecies::Birch => match random % 5 { 0 => TreeVariant::SilverBirch, 1 => TreeVariant::HimalayanBirch, 2 => TreeVariant::RiverBirch, 3 => TreeVariant::DwarfBirch, _ => TreeVariant::Standard },
        TreeSpecies::Maple => match random % 5 { 0 => TreeVariant::NorwayMaple, 1 => TreeVariant::MountainMaple, 2 => TreeVariant::JapaneseMaple, 3 => TreeVariant::SugarMaple, _ => TreeVariant::Standard },
        TreeSpecies::Pine => match random % 5 { 0 => TreeVariant::MountainPine, 1 => TreeVariant::ScotsPine, 2 => TreeVariant::Fir, 3 => TreeVariant::Bristlecone, _ => TreeVariant::Standard },
        TreeSpecies::Kapok if random % 12 == 0 => TreeVariant::Viney,
        TreeSpecies::Kapok if random % 15 == 0 => TreeVariant::Shrub,
        TreeSpecies::Oak if random % 100 == 0 => TreeVariant::OldGrowth,
        TreeSpecies::Oak if random % 50 == 0 => TreeVariant::Massive,
        TreeSpecies::Redwood if random % 12 == 0 => TreeVariant::Giant,
        TreeSpecies::Bamboo if random & 1 == 0 => TreeVariant::GreenBamboo,
        TreeSpecies::Bamboo => TreeVariant::BrownBamboo,
        TreeSpecies::Willow if random & 1 == 0 => TreeVariant::RiverWillow,
        TreeSpecies::Willow => TreeVariant::BogWillow,
        _ => TreeVariant::Standard,
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
    fn canonical_roster_contains_all_starting_types() {
        for species in [
            TreeSpecies::Acacia, TreeSpecies::BaldCypress, TreeSpecies::Birch,
            TreeSpecies::CrimsonKingMaple, TreeSpecies::Ebony, TreeSpecies::MediterraneanCypress,
            TreeSpecies::Kapok, TreeSpecies::Larch, TreeSpecies::Maple, TreeSpecies::Oak,
            TreeSpecies::Pine, TreeSpecies::Purpleheart, TreeSpecies::Redwood, TreeSpecies::Walnut,
            TreeSpecies::Bamboo, TreeSpecies::Willow,
        ] { assert!(species.profile().is_some(), "missing profile for {species:?}"); }
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
