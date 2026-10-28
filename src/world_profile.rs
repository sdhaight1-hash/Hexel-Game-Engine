//! Shared design profile for the unified planetary Hexel world.
//!
//! Keep world-scale constants here rather than duplicating magic numbers across
//! terrain generation, chunk streaming, navigation, rendering, and persistence.

/// Nominal radius of the Earth-sized playable planet, in metres.
pub const DEFAULT_PLANET_RADIUS_METRES: f64 = 6_371_000.0;
/// Nominal horizontal width of one Hexel, in metres.
pub const HEXEL_WIDTH_METRES: f32 = 1.0;
/// Vertical increment represented by one Hexel, in metres.
pub const HEXEL_HEIGHT_METRES: f32 = 0.6;
/// Reference player height, in metres (three vertical Hexel steps).
pub const REFERENCE_PLAYER_HEIGHT_METRES: f32 = 1.8;
/// Initial total playable vertical budget, in metres.
pub const DEFAULT_VERTICAL_BUDGET_METRES: f32 = 256.0;

/// Authoritative chunk dimensions for editable terrain.
pub const HEX_CHUNK_Q: usize = 16;
pub const HEX_CHUNK_R: usize = 16;
pub const HEX_CHUNK_Y: usize = 16;
pub const HEXELS_PER_CHUNK: usize = HEX_CHUNK_Q * HEX_CHUNK_R * HEX_CHUNK_Y;
pub const CHUNK_VERTICAL_SPAN_METRES: f32 = HEX_CHUNK_Y as f32 * HEXEL_HEIGHT_METRES;

/// Initial vertical-world profile. The sea-level datum is local radial elevation zero.
/// Lower/upper extents sum to the total budget; they can be tuned without changing
/// cell dimensions or chunk identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetaryWorldProfile {
    pub planet_radius_metres: f64,
    pub vertical_budget_metres: f32,
    pub below_datum_metres: f32,
    pub above_datum_metres: f32,
    pub sea_level_datum_metres: f32,
}

impl Default for PlanetaryWorldProfile {
    fn default() -> Self {
        Self {
            planet_radius_metres: DEFAULT_PLANET_RADIUS_METRES,
            vertical_budget_metres: DEFAULT_VERTICAL_BUDGET_METRES,
            below_datum_metres: DEFAULT_VERTICAL_BUDGET_METRES * 0.5,
            above_datum_metres: DEFAULT_VERTICAL_BUDGET_METRES * 0.5,
            sea_level_datum_metres: 0.0,
        }
    }
}

impl PlanetaryWorldProfile {
    /// Validate configuration before generation or loading. The split is explicit:
    /// 256 m total does not imply 256 m above or below sea level.
    pub fn is_valid(self) -> bool {
        self.planet_radius_metres.is_finite()
            && self.planet_radius_metres > 0.0
            && self.vertical_budget_metres.is_finite()
            && self.vertical_budget_metres > 0.0
            && self.below_datum_metres.is_finite()
            && self.below_datum_metres >= 0.0
            && self.above_datum_metres.is_finite()
            && self.above_datum_metres >= 0.0
            && (self.below_datum_metres + self.above_datum_metres
                - self.vertical_budget_metres)
                .abs()
                <= 0.001
            && self.sea_level_datum_metres.is_finite()
    }

    /// Lowest permitted radial elevation relative to the sea-level datum.
    pub fn lower_elevation_metres(self) -> f32 {
        self.sea_level_datum_metres - self.below_datum_metres
    }

    /// Highest permitted radial elevation relative to the sea-level datum.
    pub fn upper_elevation_metres(self) -> f32 {
        self.sea_level_datum_metres + self.above_datum_metres
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_matches_the_initial_world_contract() {
        let profile = PlanetaryWorldProfile::default();
        assert!(profile.is_valid());
        assert_eq!(profile.planet_radius_metres, 6_371_000.0);
        assert_eq!(profile.vertical_budget_metres, 256.0);
        assert_eq!(profile.lower_elevation_metres(), -128.0);
        assert_eq!(profile.upper_elevation_metres(), 128.0);
        assert_eq!(HEXELS_PER_CHUNK, 4096);
        assert_eq!(CHUNK_VERTICAL_SPAN_METRES, 9.6);
        assert_eq!(REFERENCE_PLAYER_HEIGHT_METRES / HEXEL_HEIGHT_METRES, 3.0);
    }

    #[test]
    fn vertical_budget_requires_an_explicit_consistent_split() {
        let valid = PlanetaryWorldProfile {
            below_datum_metres: 80.0,
            above_datum_metres: 176.0,
            ..PlanetaryWorldProfile::default()
        };
        assert!(valid.is_valid());

        let invalid = PlanetaryWorldProfile {
            above_datum_metres: 200.0,
            ..PlanetaryWorldProfile::default()
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn profile_rejects_non_finite_or_non_positive_planet_radius() {
        assert!(!PlanetaryWorldProfile {
            planet_radius_metres: f64::NAN,
            ..PlanetaryWorldProfile::default()
        }.is_valid());
        assert!(!PlanetaryWorldProfile {
            planet_radius_metres: 0.0,
            ..PlanetaryWorldProfile::default()
        }.is_valid());
    }
}
