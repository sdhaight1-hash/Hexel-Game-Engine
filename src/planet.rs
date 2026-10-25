//! Deterministic, planet-wide terrain and first-pass climate sampling.
//!
//! This module deliberately generates samples from unit-sphere directions rather than
//! a latitude/longitude rectangle. The 3D noise domain has no longitude seam, so the
//! same physical location remains continuous across the date line and at the poles.
//!
//! This is a data-generation foundation, not yet a spherical prism mesher or renderer.
//! Surface topology and pure hexagonal-prism placement remain a separate engine layer.
//!
//! Scale invariant: the default radius is Earth-sized (one world unit represents one
//! metre). At that radius, the surface departs from a local tangent plane by only about
//! 0.8 mm over 100 m and 7.8 cm over 1 km. Normal play should therefore use a local
//! tangent frame; planetary curvature becomes relevant over long distances, not during
//! ordinary building, digging, or walking. Keep large world positions in f64 and render
//! nearby geometry relative to the player to avoid floating-point jitter.

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
            latitude: if latitude.is_finite() {
                latitude.clamp(-HALF_PI, HALF_PI)
            } else {
                0.0
            },
            longitude: normalize_longitude(longitude),
        }
    }

    pub fn from_degrees(latitude: f64, longitude: f64) -> Self {
        Self::new(latitude.to_radians(), longitude.to_radians())
    }

    /// Unit-sphere direction. Longitude is naturally periodic, with no special seam.
    pub fn direction(self) -> DVec3 {
        // Public fields allow direct construction, so normalize again here to keep
        // malformed coordinates from poisoning terrain samples with NaNs.
        let latitude = if self.latitude.is_finite() {
            self.latitude.clamp(-HALF_PI, HALF_PI)
        } else {
            0.0
        };
        let longitude = normalize_longitude(self.longitude);
        let cos_lat = latitude.cos();
        DVec3::new(
            cos_lat * longitude.cos(),
            latitude.sin(),
            cos_lat * longitude.sin(),
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
    /// Köppen-Geiger subtype estimated from procedural monthly climate normals.
    pub koppen: crate::climate::KoppenClass,
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
        // Sanitize the coordinate once because latitude is also used by the climate
        // model, not only by the sphere-direction conversion.
        let coord = GeoCoord::new(coord.latitude, coord.longitude);
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

        let normalized_season = if season_phase.is_finite() {
            season_phase.rem_euclid(1.0)
        } else {
            0.0
        };
        let season = (normalized_season * TAU).sin();
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

        // Build twelve deterministic monthly normals. Temperature uses axial-tilt seasonality;
        // precipitation is a broad circulation/ocean proxy with a seasonal rainfall cycle.
        // These are explicit estimates, not a calibrated weather/climate simulation.
        let annual_mean_temperature = 31.0 - coord.latitude.abs() * 39.0
            - elevation.max(0.0) * 0.0065
            + self.detail.get([direction.x * 3.5 + 19.1, direction.y * 3.5 - 7.7, direction.z * 3.5 + 2.3]) * 4.0;
        // Seasonal contrast increases toward the poles; retain a smaller tropical
        // seasonal cycle rather than incorrectly making equatorial temperatures static.
        let seasonal_amplitude = self.config.axial_tilt_degrees.to_radians().sin()
            * (0.35 + 0.65 * coord.latitude.sin().abs()) * 30.0;
        let mut monthly_temperature = [0.0; 12];
        let mut monthly_precipitation = [0.0; 12];
        for month in 0..12 {
            let phase = (month as f64 + 0.5) / 12.0;
            let seasonal_wave = (phase * TAU - std::f64::consts::FRAC_PI_2).sin()
                * if coord.latitude >= 0.0 { 1.0 } else { -1.0 };
            monthly_temperature[month] = annual_mean_temperature + seasonal_amplitude * seasonal_wave;
            // Separate rainfall field adds regional variation; seasonal amplitude grows away from the equator.
            let rainfall_detail = self.moisture_field.get([
                direction.x * 8.1 + 13.7,
                direction.y * 8.1 - 4.3,
                direction.z * 8.1 + 21.9,
            ]);
            let continental_drying = 1.0 - smoothstep(-0.18, 0.58, continentalness) * 0.38;
            let lee_side_penalty = if ridge_noise > 0.55 { 0.78 } else { 1.0 };
            let precipitation_capacity = (moisture * 720.0
                * (0.78 + rainfall_detail * 0.22)
                * continental_drying * lee_side_penalty).max(0.0);
            let seasonal_strength = 0.18 + 0.42 * coord.latitude.sin().abs();
            let rainfall_wave = (1.0 + seasonal_strength * seasonal_wave
                + rainfall_detail * 0.16).clamp(0.08, 1.55);
            monthly_precipitation[month] = precipitation_capacity * rainfall_wave;
        }
        let koppen = crate::climate::MonthlyClimateNormals {
            temperature_c: monthly_temperature,
            precipitation_mm: monthly_precipitation,
        }.classify(elevation.max(0.0));
        // Ocean depth and extreme relief remain geographic overrides. On land, the
        // climate subtype now drives the broad biome used by downstream systems.
        let biome = if elevation < -self.config.elevation_scale * 0.12 {
            Biome::DeepOcean
        } else if elevation <= 0.0 {
            if temperature_c < -3.0 { Biome::PolarIce } else { Biome::ShallowOcean }
        } else if matches!(koppen, crate::climate::KoppenClass::Highland)
            || elevation > self.config.elevation_scale * 0.68 {
            Biome::Mountain
        } else {
            use crate::climate::KoppenClass as K;
            match koppen {
                K::EF => Biome::PolarIce,
                K::ET => Biome::Tundra,
                K::Dsa | K::Dsb | K::Dsc | K::Dsd | K::Dwa | K::Dwb | K::Dwc | K::Dwd
                | K::Dfa | K::Dfb | K::Dfc | K::Dfd => Biome::BorealForest,
                K::Af | K::Am => Biome::TropicalForest,
                K::Aw | K::As => Biome::Savanna,
                K::BWh | K::BWk => Biome::Desert,
                K::BSh | K::BSk => Biome::Grassland,
                K::Csa | K::Csb | K::Csc if moisture < 0.45 => Biome::Grassland,
                K::Csa | K::Csb | K::Csc | K::Cwa | K::Cwb | K::Cwc
                | K::Cfa | K::Cfb | K::Cfc =>
                    if moisture > 0.52 { Biome::TemperateForest } else { Biome::Grassland },
                K::Highland => Biome::Mountain,
            }
        };
        PlanetSample {
            direction,
            surface_position: direction * (self.config.radius + elevation),
            elevation,
            temperature_c,
            moisture,
            biome,
            koppen,
        }
    }

    /// Approximate height of the curved surface above its local tangent plane after
    /// travelling this far along the surface, in world units.
    ///
    /// This makes the "locally flat, globally spherical" scale explicit and testable.
    /// Distances must be non-negative; values are clamped to a half-circumference.
    pub fn curvature_drop(&self, surface_distance: f64) -> f64 {
        if !surface_distance.is_finite() {
            return 0.0;
        }
        let distance = surface_distance
            .abs()
            .min(std::f64::consts::PI * self.config.radius);
        self.config.radius * (1.0 - (distance / self.config.radius).cos())
    }

    /// Move over the spherical surface by local east/north distances, in world units.
    ///
    /// Uses a great-circle step rather than adding directly to latitude/longitude, so
    /// movement remains well behaved across the date line and near either pole.
    pub fn move_on_surface(&self, start: GeoCoord, east: f64, north: f64) -> GeoCoord {
        if !east.is_finite() || !north.is_finite() {
            return GeoCoord::new(start.latitude, start.longitude);
        }
        let start = GeoCoord::new(start.latitude, start.longitude);
        let up = start.direction();
        let east_axis = DVec3::new(-start.longitude.sin(), 0.0, start.longitude.cos())
            .normalize_or_zero();
        let north_axis = east_axis.cross(up).normalize_or_zero();
        let displacement = east_axis * east + north_axis * north;
        let distance = displacement.length();
        if distance <= f64::EPSILON {
            return start;
        }
        let angle = (distance / self.config.radius).min(std::f64::consts::PI);
        let tangent = displacement / distance;
        let destination = (up * angle.cos() + tangent * angle.sin()).normalize_or_zero();
        GeoCoord::new(destination.y.clamp(-1.0, 1.0).asin(), destination.z.atan2(destination.x))
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

/// Player-centred east/north/up frame for stable local simulation and rendering.
///
/// Keep planetary positions in f64 and convert nearby points into this frame before
/// narrowing to f32 for GPU buffers. Source positions retain the planet's curvature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalTangentFrame {
    origin: DVec3,
    east: DVec3,
    north: DVec3,
    up: DVec3,
}

impl LocalTangentFrame {
    /// Create a local frame at a geographic position and radial elevation.
    pub fn at(coord: GeoCoord, planet_radius: f64, elevation: f64) -> Self {
        let coord = GeoCoord::new(coord.latitude, coord.longitude);
        let up = coord.direction().normalize_or_zero();
        let east = DVec3::new(-coord.longitude.sin(), 0.0, coord.longitude.cos())
            .normalize_or_zero();
        let north = east.cross(up).normalize_or_zero();
        let radius = if planet_radius.is_finite() { planet_radius.max(1.0) } else { 1.0 };
        let elevation = if elevation.is_finite() { elevation } else { 0.0 };
        Self { origin: up * (radius + elevation), east, north, up }
    }

    /// Convert a planet-centred f64 point to local east/north/up coordinates.
    pub fn to_local(self, planet_position: DVec3) -> DVec3 {
        let relative = planet_position - self.origin;
        DVec3::new(relative.dot(self.east), relative.dot(self.north), relative.dot(self.up))
    }

    /// Convert local east/north/up coordinates back to planet-centred f64 coordinates.
    pub fn to_planet(self, local_position: DVec3) -> DVec3 {
        self.origin + self.east * local_position.x
            + self.north * local_position.y + self.up * local_position.z
    }

    pub fn origin(self) -> DVec3 { self.origin }

    /// Radial up direction, used for planet-relative gravity and player orientation.
    pub fn up(self) -> DVec3 { self.up }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn tangent_frame_round_trips_planet_positions() {
        let coord = GeoCoord::from_degrees(37.0, -122.0);
        let frame = LocalTangentFrame::at(coord, 6_371_000.0, 120.0);
        let planet_point = frame.to_planet(DVec3::new(12.0, -4.0, 2.0));
        let local = frame.to_local(planet_point);
        let reconstructed = frame.to_planet(local);
        assert!(reconstructed.distance(planet_point) < 1e-8);
        assert_abs_diff_eq!(local.x, 12.0, epsilon = 1e-8);
        assert_abs_diff_eq!(local.y, -4.0, epsilon = 1e-8);
        assert_abs_diff_eq!(local.z, 2.0, epsilon = 1e-8);
    }

    #[test]
    fn tangent_frame_north_points_toward_increasing_latitude() {
        let coord = GeoCoord::from_degrees(0.0, 0.0);
        let frame = LocalTangentFrame::at(coord, 6_371_000.0, 0.0);
        let slightly_north = GeoCoord::from_degrees(0.001, 0.0).direction()
            * 6_371_000.0;
        assert!(frame.to_local(slightly_north).y > 0.0);
    }

    #[test]
    fn tangent_frame_up_matches_planet_radial_direction() {
        let coord = GeoCoord::from_degrees(-28.0, 151.0);
        let frame = LocalTangentFrame::at(coord, 6_371_000.0, 0.0);
        assert_abs_diff_eq!(frame.up().length(), 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(frame.up().dot(coord.direction()), 1.0, epsilon = 1e-12);
    }

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
    fn surface_movement_crosses_date_line_without_discontinuity() {
        let generator = PlanetGenerator::new(1);
        let start = GeoCoord::from_degrees(0.0, 179.999);
        let end = generator.move_on_surface(start, 500.0, 0.0);
        assert!(end.longitude < 0.0);
        assert_abs_diff_eq!(
            generator.surface_distance(start, end),
            500.0,
            epsilon = 0.01
        );
    }

    #[test]
    fn surface_movement_toward_north_increases_latitude() {
        let generator = PlanetGenerator::new(1);
        let start = GeoCoord::from_degrees(0.0, 0.0);
        let end = generator.move_on_surface(start, 0.0, 1_000.0);
        assert!(end.latitude > start.latitude);
        assert_abs_diff_eq!(
            generator.surface_distance(start, end),
            1_000.0,
            epsilon = 0.01
        );
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

    #[test]
    fn pole_samples_are_independent_of_longitude() {
        let generator = PlanetGenerator::new(13);
        let a = generator.sample(GeoCoord::from_degrees(90.0, -170.0), 0.2);
        let b = generator.sample(GeoCoord::from_degrees(90.0, 75.0), 0.2);
        assert_abs_diff_eq!(a.elevation, b.elevation, epsilon = 1e-8);
        assert_abs_diff_eq!(a.moisture, b.moisture, epsilon = 1e-8);
    }

    #[test]
    fn non_finite_inputs_fall_back_to_finite_samples() {
        let generator = PlanetGenerator::new(19);
        let coord = GeoCoord::new(f64::NAN, f64::INFINITY);
        let sample = generator.sample(coord, f64::NAN);
        assert!(sample.direction.is_finite());
        assert!(sample.surface_position.is_finite());
        assert!(sample.elevation.is_finite());
        assert!(sample.temperature_c.is_finite());
        assert!(sample.moisture.is_finite());
    }

    #[test]
    fn season_phase_wraps_across_year_boundary() {
        let generator = PlanetGenerator::new(23);
        let coord = GeoCoord::from_degrees(48.0, 9.0);
        let a = generator.sample(coord, 0.15);
        let b = generator.sample(coord, 1.15);
        assert_abs_diff_eq!(a.temperature_c, b.temperature_c, epsilon = 1e-8);
        assert_abs_diff_eq!(a.moisture, b.moisture, epsilon = 1e-8);
    }

    #[test]
    fn default_planet_is_large_enough_for_locally_flat_gameplay() {
        let generator = PlanetGenerator::new(1);
        let radius = generator.config().radius;
        assert_abs_diff_eq!(radius, 6_371_000.0, epsilon = 0.01);
        // Earth-scale curvature is sub-millimetre across a typical 100 m local area.
        assert!(generator.curvature_drop(100.0) < 0.001);
        // Even over 1 km, curvature is only about 8 cm.
        assert!(generator.curvature_drop(1_000.0) < 0.1);
        // But it is measurable over long-distance travel.
        assert!(generator.curvature_drop(10_000.0) > 7.0);
    }
}
