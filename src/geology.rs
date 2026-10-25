//! Data-driven crust geology, host-rock rules, and finite mineral deposits.
//!
//! Persist changed deposit quantities by DepositId; never recreate a depleted deposit
//! from its generation seed after a chunk reload.

pub type DepositId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RockCategory { Sedimentary, Metamorphic, IgneousExtrusive, IgneousIntrusive }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RockType {
    Chert, Claystone, Conglomerate, Limestone, Sandstone, Shale, Chalk,
    MetamorphosedClaystone, Marble, Phyllite, Quartzite, Schist, Slate, Gneiss,
    Andesite, Basalt, Obsidian, Peridotite, Rhyolite, Scoria, Tuff, Granite, Kimberlite,
}
impl RockType {
    pub const fn category(self) -> RockCategory {
        match self {
            Self::Chert | Self::Claystone | Self::Conglomerate | Self::Limestone | Self::Sandstone | Self::Shale | Self::Chalk => RockCategory::Sedimentary,
            Self::MetamorphosedClaystone | Self::Marble | Self::Phyllite | Self::Quartzite | Self::Schist | Self::Slate | Self::Gneiss => RockCategory::Metamorphic,
            Self::Andesite | Self::Basalt | Self::Obsidian | Self::Peridotite | Self::Rhyolite | Self::Scoria | Self::Tuff => RockCategory::IgneousExtrusive,
            Self::Granite | Self::Kimberlite => RockCategory::IgneousIntrusive,
        }
    }
    pub const fn is_lime_source(self) -> bool { matches!(self, Self::Limestone | Self::Chalk) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stratum { pub rock: RockType, pub depth_min_m: u32, pub depth_max_m: u32 }
impl Stratum {
    pub fn contains_depth(self, depth_m: u32) -> bool { depth_m >= self.depth_min_m && depth_m < self.depth_max_m }
}
/// Simple starter profile only. Region-specific geology should eventually supply these bands.
pub const DEFAULT_STRATIGRAPHY: &[Stratum] = &[
    Stratum { rock: RockType::Limestone, depth_min_m: 0, depth_max_m: 45 },
    Stratum { rock: RockType::Shale, depth_min_m: 45, depth_max_m: 180 },
    Stratum { rock: RockType::Granite, depth_min_m: 180, depth_max_m: 2_000 },
];
pub fn rock_at_depth(strata: &[Stratum], depth_m: u32) -> Option<RockType> {
    strata.iter().find(|s| s.contains_depth(depth_m)).map(|s| s.rock)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OreType { NativeCopper, Malachite, Cassiterite, Sphalerite, Bismuthinite, NativeSilver, NativeGold, Galena, Limonite, Hematite, Magnetite, MeteoricIron, Diamond }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiningTier { HandPickable, Stone, Copper, Bronze }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OreProfile { pub ore: OreType, pub tier: MiningTier, pub hosts: &'static [RockType], pub rarity_weight: f64, pub min_quantity: u32, pub max_quantity: u32, pub surface_nugget_allowed: bool }

const SED: &[RockType] = &[RockType::Chert, RockType::Claystone, RockType::Conglomerate, RockType::Limestone, RockType::Sandstone, RockType::Shale, RockType::Chalk];
const ALL: &[RockType] = &[RockType::Chert, RockType::Claystone, RockType::Conglomerate, RockType::Limestone, RockType::Sandstone, RockType::Shale, RockType::Chalk, RockType::MetamorphosedClaystone, RockType::Marble, RockType::Phyllite, RockType::Quartzite, RockType::Schist, RockType::Slate, RockType::Gneiss, RockType::Andesite, RockType::Basalt, RockType::Obsidian, RockType::Peridotite, RockType::Rhyolite, RockType::Scoria, RockType::Tuff, RockType::Granite, RockType::Kimberlite];
const SED_MET: &[RockType] = &[RockType::Chert, RockType::Claystone, RockType::Conglomerate, RockType::Limestone, RockType::Sandstone, RockType::Shale, RockType::Chalk, RockType::MetamorphosedClaystone, RockType::Marble, RockType::Phyllite, RockType::Quartzite, RockType::Schist, RockType::Slate, RockType::Gneiss];
const IGNEOUS_SED: &[RockType] = &[RockType::Andesite, RockType::Basalt, RockType::Obsidian, RockType::Peridotite, RockType::Rhyolite, RockType::Scoria, RockType::Tuff, RockType::Chert, RockType::Claystone, RockType::Conglomerate, RockType::Limestone, RockType::Sandstone, RockType::Shale, RockType::Chalk];
const GRANITE_GNEISS: &[RockType] = &[RockType::Granite, RockType::Gneiss];
const LIMESTONE: &[RockType] = &[RockType::Limestone];
const KIMBERLITE: &[RockType] = &[RockType::Kimberlite];
const NONE: &[RockType] = &[];

pub const ORE_PROFILES: &[OreProfile] = &[
 OreProfile { ore: OreType::NativeCopper, tier: MiningTier::Stone, hosts: ALL, rarity_weight: 0.8, min_quantity: 1, max_quantity: 6, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Malachite, tier: MiningTier::Stone, hosts: LIMESTONE, rarity_weight: 0.55, min_quantity: 8, max_quantity: 70, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Cassiterite, tier: MiningTier::Copper, hosts: GRANITE_GNEISS, rarity_weight: 0.35, min_quantity: 12, max_quantity: 120, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Sphalerite, tier: MiningTier::Copper, hosts: SED_MET, rarity_weight: 0.4, min_quantity: 10, max_quantity: 100, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Bismuthinite, tier: MiningTier::Copper, hosts: IGNEOUS_SED, rarity_weight: 0.28, min_quantity: 8, max_quantity: 90, surface_nugget_allowed: true },
 OreProfile { ore: OreType::NativeSilver, tier: MiningTier::Copper, hosts: ALL, rarity_weight: 0.12, min_quantity: 3, max_quantity: 45, surface_nugget_allowed: true },
 OreProfile { ore: OreType::NativeGold, tier: MiningTier::Copper, hosts: ALL, rarity_weight: 0.035, min_quantity: 1, max_quantity: 25, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Galena, tier: MiningTier::Copper, hosts: SED_MET, rarity_weight: 0.3, min_quantity: 12, max_quantity: 110, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Limonite, tier: MiningTier::Bronze, hosts: SED, rarity_weight: 0.45, min_quantity: 20, max_quantity: 180, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Hematite, tier: MiningTier::Bronze, hosts: IGNEOUS_SED, rarity_weight: 0.4, min_quantity: 20, max_quantity: 180, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Magnetite, tier: MiningTier::Bronze, hosts: IGNEOUS_SED, rarity_weight: 0.32, min_quantity: 20, max_quantity: 180, surface_nugget_allowed: true },
 OreProfile { ore: OreType::MeteoricIron, tier: MiningTier::Bronze, hosts: NONE, rarity_weight: 0.008, min_quantity: 20, max_quantity: 300, surface_nugget_allowed: true },
 OreProfile { ore: OreType::Diamond, tier: MiningTier::Bronze, hosts: KIMBERLITE, rarity_weight: 0.002, min_quantity: 1, max_quantity: 8, surface_nugget_allowed: false },
];
impl OreProfile {
 pub fn can_spawn_in(self, rock: RockType) -> bool { self.ore != OreType::MeteoricIron && self.hosts.contains(&rock) }
 pub fn by_type(ore: OreType) -> Option<&'static Self> { ORE_PROFILES.iter().find(|p| p.ore == ore) }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepositKind { SurfaceNugget, Vein, Meteorite }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OreDeposit { pub id: DepositId, pub ore: OreType, pub kind: DepositKind, pub remaining: u32 }
impl OreDeposit {
 pub fn extract(&mut self, requested: u32) -> u32 { let amount = requested.min(self.remaining); self.remaining -= amount; amount }
 pub fn depleted(&self) -> bool { self.remaining == 0 }
}
pub fn generate_deposit(seed: u64, slot: DepositId, ore: OreType, kind: DepositKind, host: RockType) -> Option<OreDeposit> {
 let p = OreProfile::by_type(ore)?;
 match kind {
  DepositKind::Meteorite if ore != OreType::MeteoricIron => return None,
  DepositKind::Meteorite => {},
  DepositKind::SurfaceNugget if !p.surface_nugget_allowed || ore == OreType::MeteoricIron => return None,
  DepositKind::SurfaceNugget => if !p.can_spawn_in(host) { return None; },
  DepositKind::Vein if !p.can_spawn_in(host) => return None,
  DepositKind::Vein => if ore == OreType::MeteoricIron { return None; },
 }
 let random = mix64(seed ^ slot.wrapping_mul(0x9E37_79B9_7F4A_7C15));
 let span = (p.max_quantity - p.min_quantity + 1) as u64;
 Some(OreDeposit { id: slot, ore, kind, remaining: p.min_quantity + (random % span) as u32 })
}
fn mix64(mut x: u64) -> u64 { x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9); x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB); x ^ (x >> 31) }

#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn diamond_only_spawns_in_kimberlite() { assert!(generate_deposit(1,2,OreType::Diamond,DepositKind::Vein,RockType::Kimberlite).is_some()); assert!(generate_deposit(1,2,OreType::Diamond,DepositKind::Vein,RockType::Granite).is_none()); }
 #[test] fn malachite_requires_limestone() { assert!(generate_deposit(1,2,OreType::Malachite,DepositKind::Vein,RockType::Limestone).is_some()); assert!(generate_deposit(1,2,OreType::Malachite,DepositKind::Vein,RockType::Granite).is_none()); }
 #[test] fn depletion_is_finite() { let mut d=generate_deposit(99,42,OreType::NativeGold,DepositKind::Vein,RockType::Granite).unwrap(); let n=d.remaining; assert_eq!(d.extract(n+100),n); assert!(d.depleted()); assert_eq!(d.extract(1),0); }
 #[test] fn meteorites_only_contain_meteoric_iron() { assert!(generate_deposit(2,4,OreType::MeteoricIron,DepositKind::Meteorite,RockType::Granite).is_some()); assert!(generate_deposit(2,4,OreType::NativeCopper,DepositKind::Meteorite,RockType::Granite).is_none()); }
}
