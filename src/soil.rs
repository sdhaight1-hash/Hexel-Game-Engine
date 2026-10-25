//! Soil fertility, nutrient depletion, and finite peat deposits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoilType {
    Barren,
    LowFertility,
    MediumFertility,
    HighFertility,
    TerraPreta,
    Peat,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoilProfile {
    pub soil: SoilType,
    pub starting_fertility: f32,
    pub nutrient_capacity: f32,
    pub color_rgb: [u8; 3],
    pub crops_supported: bool,
    pub renewable: bool,
}
pub const SOIL_PROFILES: &[SoilProfile] = &[
    SoilProfile {
        soil: SoilType::Barren,
        starting_fertility: 0.025,
        nutrient_capacity: 2.0,
        color_rgb: [160, 133, 82],
        crops_supported: true,
        renewable: true,
    },
    SoilProfile {
        soil: SoilType::LowFertility,
        starting_fertility: 0.25,
        nutrient_capacity: 15.0,
        color_rgb: [151, 111, 70],
        crops_supported: true,
        renewable: true,
    },
    SoilProfile {
        soil: SoilType::MediumFertility,
        starting_fertility: 0.50,
        nutrient_capacity: 35.0,
        color_rgb: [112, 76, 48],
        crops_supported: true,
        renewable: true,
    },
    SoilProfile {
        soil: SoilType::HighFertility,
        starting_fertility: 0.65,
        nutrient_capacity: 55.0,
        color_rgb: [76, 48, 31],
        crops_supported: true,
        renewable: true,
    },
    SoilProfile {
        soil: SoilType::TerraPreta,
        starting_fertility: 0.875,
        nutrient_capacity: 100.0,
        color_rgb: [28, 25, 22],
        crops_supported: true,
        renewable: false,
    },
    SoilProfile {
        soil: SoilType::Peat,
        starting_fertility: 0.0,
        nutrient_capacity: 0.0,
        color_rgb: [54, 43, 35],
        crops_supported: false,
        renewable: false,
    },
];
impl SoilType {
    pub fn profile(self) -> &'static SoilProfile {
        SOIL_PROFILES
            .iter()
            .find(|p| p.soil == self)
            .expect("each SoilType has a profile")
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoilState {
    pub soil: SoilType,
    pub nutrients: f32,
}
impl SoilState {
    pub fn new(soil: SoilType) -> Self {
        let p = soil.profile();
        Self {
            soil,
            nutrients: p.nutrient_capacity * p.starting_fertility,
        }
    }
    pub fn fertility(self) -> f32 {
        let cap = self.soil.profile().nutrient_capacity;
        if cap <= 0.0 {
            0.0
        } else {
            (self.nutrients / cap).clamp(0.0, 1.0)
        }
    }
    /// Farming consumes nutrient stock; output is the fraction of nominal crop growth supported.
    pub fn farm(&mut self, nutrient_cost: f32) -> f32 {
        if !self.soil.profile().crops_supported {
            return 0.0;
        }
        let cap = self.soil.profile().nutrient_capacity;
        if cap <= 0.0 {
            return 0.0;
        }
        let growth = self.fertility();
        self.nutrients = (self.nutrients - nutrient_cost.max(0.0)).clamp(0.0, cap);
        growth
    }
    /// Fallow recovery is explicit and capped. Non-renewable Terra Preta does not passively refill.
    pub fn fallow(&mut self, recovered: f32) {
        if self.soil.profile().renewable {
            self.nutrients =
                (self.nutrients + recovered.max(0.0)).min(self.soil.profile().nutrient_capacity);
        }
    }
    /// Fertilizer adds nutrients but cannot exceed this soil's capacity.
    pub fn fertilize(&mut self, nutrients: f32) {
        self.nutrients =
            (self.nutrients + nutrients.max(0.0)).min(self.soil.profile().nutrient_capacity);
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeatDeposit {
    pub id: u64,
    pub remaining_units: u32,
}
impl PeatDeposit {
    pub fn harvest(&mut self, requested: u32) -> u32 {
        let n = requested.min(self.remaining_units);
        self.remaining_units -= n;
        n
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn farming_depletes_and_fallow_recovers_renewable_soil() {
        let mut s = SoilState::new(SoilType::MediumFertility);
        let before = s.nutrients;
        s.farm(5.0);
        assert!(s.nutrients < before);
        s.fallow(2.0);
        assert!(s.nutrients > before - 5.0);
    }
    #[test]
    fn terra_preta_does_not_passively_regenerate() {
        let mut s = SoilState::new(SoilType::TerraPreta);
        s.farm(10.0);
        let after = s.nutrients;
        s.fallow(100.0);
        assert_eq!(s.nutrients, after);
    }
    #[test]
    fn peat_is_finite() {
        let mut p = PeatDeposit {
            id: 4,
            remaining_units: 3,
        };
        assert_eq!(p.harvest(9), 3);
        assert_eq!(p.harvest(1), 0);
    }
}
