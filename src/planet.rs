//! Deterministic, planet-wide terrain and first-pass climate sampling.
//!
//! This module deliberately generates samples from unit-sphere directions rather than
//! a latitude/longitude rectangle. The 3D noise domain has no longitude seam, so the
//! same physical location remains continuous across the date line and at the poles.
//!
//! This is a data-generation foundation, not yet a spherical prism mesher or renderer.
//! Surface topology and pure hexagonal-prism placement remain a separate engine layer.

use glam::DVec3;
use noise::{NoiseFn, Perlin};

const HALF_PI: f64 = std::f64::consts::FRAC_PI_2;
const TAU: f64 = std::f64::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetConfig {
    /// Planet radius in world units. Keep it much larger than ordinary local travel.
    pub radius: f64,
    /// Maximum broad elevation contribution above sea level, in world units.
    pub elevation_scale: f64,
    /// Axial tilt in degrees, used for the first-pass seasonal temperature estimate.
    pub axial_tilt_degrees: f64,
}

impl Default for PlanetConfig {
    fn default() -> Self {
        Self {
            radius: 6_371_000.0,
            elevation_scale: 4_500.0,
            axial_tilt_degrees: 23.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoCoord {
    /// Latitude in radians, clamped to [-pi/2, pi/2].
    pub latitude: f64,
    /// Longitude in radians. Any finite value is accepted and normalized by trig mapping.
    pub longitude: f64,
}

impl GeoCoord {
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude: latitude.clamp(-HALF_PI, HALF_PI),
            longitude: normalize_longitude(longitude),
        }
    }

    pub fn from_degrees(latitude: f64, longitude: f64) -> Self {
        Self::new(latitude.to_radians(), longitude.to_radians())
    }

    /// Unit-sphere direction. Longitude is naturally periodic, with no special seam.
    pub fn direction(self) -> DVec3 {
        let cos_lat = self.latitude.cos();
        DVec3::new(
            cos_lat * self.longitude.cos(),
            self.latitude.sin(),
            cos_lat * self.longitude.sin(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    DeepOcean,
    ShallowOcean,
    PolarIce,
    Tundra,
    BorealForest,
    TemperateForest,
    Grassland,
    Desert,
    Savanna,
    TropicalForest,
    Mountain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetSample {
    pub direction: DVec3,
    /// Radial distance from the planet center, including terrain elevation.
    pub surface_position: DVec3,
    /// Elevation relative to sea level. Negative values are ocean floor.
    pub elevation: f64,
    pub temperature_c: f64,
    /// Dimensionless first-pass moisture estimate in [0, 1].
    pub moisture: f64,
    pub biome: Biome,
}

#[derive(Clone, Debug)]
pub struct PlanetGenerator {
    seed: u32,
    config: PlanetConfig,
    continents: Perlin,
    detail: Perlin,
    mountains: Perlin,
    moisture_field: Perlin,
}

impl PlanetGenerator {
    pub fn new(seed: u32) -> Self {
        Self::with_config(seed, PlanetConfig::default())
    }

    pub fn with_config(seed: u32, config: PlanetConfig) -> Self {
        let config = PlanetConfig {
            radius: config.radius.max(1.0),
            elevation_scale: config.elevation_scale.max(1.0),
            axial_tilt_degrees: config.axial_tilt_degrees.clamp(0.0, 45.0),
        };
        Self {
            seed,
            config,
            continents: Perlin::new(seed),
            detail: Perlin::new(seed.wrapping_add(0x9E37_79B9)),
            mountains: Perlin::new(seed.wrapping_add(0x85EB_CA6B)),
            moisture_field: Perlin::new(seed.wrapping_add(0xC2B2_AE35)),
        }
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    pub fn config(&self) -> PlanetConfig {
        self.config
    }

    /// Sample terrain and a lightweight climate estimate at a geographic location.
    ///
    /// `season_phase` is a normalized year phase in [0, 1); 0 and 1 are equivalent.
    /// This is intentionally inexpensive and deterministic. It does not claim to be a
    /// full atmospheric, ocean-current, or hydrology simulation.
    pub fn sample(&self, coord: GeoCoord, season_phase: f64) -> PlanetSample {
        let direction = coord.direction();
        let continentalness = self.continents.get([
            direction.x * 2.15,
            direction.y * 2.15,
            direction.z * 2.15,
        ]);
        let detail = self.detail.get([
            direction.x * 17.0,
            direction.y * 17.0,
            direction.z * 17.0,
        ]);
        let ridge_noise = self.mountains.get([
            direction.x * 7.0,
            direction.y * 7.0,
            direction.z * 7.0,
        ]);
        let ridge = (1.0 - ridge_noise.abs()).powi(3);

        let elevation = if continentalness > 0.02 {
            let land_factor = smoothstep(0.02, 0.48, continentalness);
            let highlands = (continentalness - 0.02) * self.config.elevation_scale;
            let fine_relief = detail * self.config.elevation_scale * 0.045;
            let mountain_relief = ridge * land_factor * self.config.elevation_scale * 0.22;
            highlands + fine_relief + mountain_relief
        } else {
            // Ocean basins are broad and shallower near continental margins.
            (continentalness - 0.02) * self.config.elevation_scale * 0.30
                + detail * self.config.elevation_scale * 0.012
        };

        let season = (season_phase.rem_euclid(1.0) * TAU).sin();
        let axial_tilt = self.config.axial_tilt_degrees.to_radians();
        let subsolar_latitude = axial_tilt * season;
        let latitude_distance = (coord.latitude - subsolar_latitude).abs();
        let solar_temperature = 31.0 - latitude_distance.min(std::f64::consts::PI) * 39.0;
        let elevation_cooling = elevation.max(0.0) * 0.0065;
        let regional_heat = self.detail.get([
            direction.x * 3.5 + 19.1,
            direction.y * 3.5 - 7.7,
            direction.z * 3.5 + 2.3,
        ]) * 4.0;
        let temperature_c = solar_temperature - elevation_cooling + regional_heat;

        // A cheap, coherent proxy: circulation belts establish broad wet/dry bands;
        // proximity to oceanic terrain and a separate low-frequency field modify them.
        let circulation_belt = 0.5 + 0.5 * (coord.latitude * 3.0).cos();
        let ocean_influence = 1.0 - smoothstep(-0.28, 0.30, continentalness);
        let regional_moisture = 0.5
            + 0.5
                * self.moisture_field.get([
                    direction.x * 4.2,
                    direction.y * 4.2,
                    direction.z * 4.2,
                ]);
        let elevation_drying = 1.0 - (elevation.max(0.0) / (self.config.elevation_scale * 0.8)).clamp(0.0, 0.65);
        let moisture = (circulation_belt * 0.38
            + ocean_influence * 0.37
            + regional_moisture * 0.25)
            * elevation_drying;
        let moisture = moisture.clamp(0.0, 1.0);

        let biome = classify_biome(elevation, temperature_c, moisture, self.config.elevation_scale);
        PlanetSample {
            direction,
            surface_position: direction * (self.config.radius + elevation),
            elevation,
            temperature_c,
            moisture,
            biome,
        }
    }

    /// Great-circle distance between two locations, in the same units as planet radius.
    pub fn surface_distance(&self, a: GeoCoord, b: GeoCoord) -> f64 {
        let a = a.direction();
        let b = b.direction();
        let angle = a.dot(b).clamp(-1.0, 1.0).acos();
        self.config.radius * angle
    }
}

fn normalize_longitude(longitude: f64) -> f64 {
    if !longitude.is_finite() {
        return 0.0;
    }
    (longitude + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI
}

fn smoothstep(edge0: f64, edge1: f64, value: f64) -> f64 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn classify_biome(elevation: f64, temperature_c: f64, moisture: f64, elevation_scale: f64) -> Biome {
    if elevation < -elevation_scale * 0.12 {
        return Biome::DeepOcean;
    }
    if elevation <= 0.0 {
        return if temperature_c < -3.0 { Biome::PolarIce } else { Biome::ShallowOcean };
    }
    if elevation > elevation_scale * 0.68 {
        return Biome::Mountain;
    }
    if temperature_c < -8.0 {
        Biome::PolarIce
    } else if temperature_c < 2.0 {
        if moisture > 0.45 { Biome::Tundra } else { Biome::Tundra }
    } else if temperature_c < 10.0 {
        if moisture > 0.52 { Biome::BorealForest } else { Biome::Grassland }
    } else if temperature_c < 22.0 {
        if moisture > 0.58 { Biome::TemperateForest } else { Biome::Grassland }
    } else if moisture < 0.24 {
        Biome::Desert
    } else if temperature_c > 25.0 && moisture > 0.68 {
        Biome::TropicalForest
    } else if temperature_c > 22.0 {
        Biome::Savanna
    } else {
        Biome::Grassland
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn same_seed_and_coordinate_are_deterministic() {
        let generator = PlanetGenerator::new(42);
        let coord = GeoCoord::from_degrees(37.0, -122.0);
        assert_eq!(generator.sample(coord, 0.25), generator.sample(coord, 0.25));
    }

    #[test]
    fn longitude_wraps_without_a_seam() {
        let generator = PlanetGenerator::new(7);
        let a = generator.sample(GeoCoord::from_degrees(12.0, 180.0), 0.0);
        let b = generator.sample(GeoCoord::from_degrees(12.0, -180.0), 0.0);
        assert_abs_diff_eq!(a.elevation, b.elevation, epsilon = 1e-8);
        assert_abs_diff_eq!(a.temperature_c, b.temperature_c, epsilon = 1e-8);
    }

    #[test]
    fn latitude_is_clamped_at_the_poles() {
        let north = GeoCoord::from_degrees(120.0, 35.0);
        let pole = GeoCoord::from_degrees(90.0, 35.0);
        assert_abs_diff_eq!(north.direction().distance(pole.direction()), 0.0, epsilon = 1e-8);
    }

    #[test]
    fn surface_distance_uses_great_circle_geometry() {
        let generator = PlanetGenerator::new(1);
        let a = GeoCoord::from_degrees(0.0, 0.0);
        let b = GeoCoord::from_degrees(0.0, 90.0);
        assert_abs_diff_eq!(
            generator.surface_distance(a, b),
            generator.config().radius * std::f64::consts::FRAC_PI_2,
            epsilon = 0.01
        );
    }

    #[test]
    fn terrain_is_seeded_and_changes_with_location() {
        let a = PlanetGenerator::new(1);
        let b = PlanetGenerator::new(2);
        let coord = GeoCoord::from_degrees(15.0, 40.0);
        assert_ne!(a.sample(coord, 0.0).elevation, b.sample(coord, 0.0).elevation);
        assert_ne!(
            a.sample(coord, 0.0).elevation,
            a.sample(GeoCoord::from_degrees(-42.0, 120.0), 0.0).elevation
        );
    }

    #[test]
    fn generated_surface_stays_near_the_configured_radius() {
        let generator = PlanetGenerator::new(5);
        let sample = generator.sample(GeoCoord::from_degrees(0.0, 0.0), 0.4);
        assert!(sample.surface_position.length() > generator.config().radius * 0.98);
        assert!(sample.surface_position.length() < generator.config().radius * 1.02);
        assert!((0.0..=1.0).contains(&sample.moisture));
    }
}
