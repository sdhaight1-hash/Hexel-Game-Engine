//! Double-precision planetary coordinates and camera-relative tangent frames.
//!
//! Earth-scale coordinates must remain in f64 until they are rebased around the
//! active player. GPU vertices and the current camera use f32, so converting raw
//! planet-centered positions (millions of metres from the origin) directly to f32
//! causes visible jitter. This module is the shared coordinate foundation for
//! planetary movement, terrain streaming, collision, and rendering.

use glam::{DVec3, Vec3};

/// An orthonormal local frame anchored to a point on a spherical planet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetaryFrame {
    /// Planet-centred position of the frame origin, including terrain elevation.
    pub origin: DVec3,
    /// Local east, north, and up axes expressed in planet-centred coordinates.
    pub east: DVec3,
    pub north: DVec3,
    pub up: DVec3,
}

impl PlanetaryFrame {
    /// Construct a stable local frame at a planet-centred position.
    ///
    /// At the poles, global +Z replaces +Y as the north reference to avoid a
    /// degenerate projection. The supplied position must be non-zero; invalid
    /// positions fall back to the positive X axis.
    pub fn at(position: DVec3) -> Self {
        let up = if position.is_finite() && position.length_squared() > f64::EPSILON {
            position.normalize()
        } else {
            DVec3::X
        };
        let reference = if up.dot(DVec3::Y).abs() < 0.99 {
            DVec3::Y
        } else {
            DVec3::Z
        };
        let north = (reference - up * reference.dot(up)).normalize_or_zero();
        let east = north.cross(up).normalize_or_zero();
        Self {
            origin: position,
            east,
            north,
            up,
        }
    }

    /// Convert a planet-centred position to a small, camera-relative coordinate.
    pub fn to_local(self, planet_position: DVec3) -> DVec3 {
        let delta = planet_position - self.origin;
        DVec3::new(
            delta.dot(self.east),
            delta.dot(self.up),
            delta.dot(self.north),
        )
    }

    /// Convert a planet-centred position directly to a camera-relative GPU coordinate.
    ///
    /// The subtraction and basis projection happen in f64; only the small local
    /// result is cast to f32. Callers must never cast the absolute planet position
    /// to f32 before rebasing, because metre-scale differences disappear at Earth radius.
    pub fn to_local_f32(self, planet_position: DVec3) -> Vec3 {
        self.to_local(planet_position).as_vec3()
    }

    /// Convert a local east/up/north coordinate back to planet-centred space.
    pub fn to_planet(self, local_position: DVec3) -> DVec3 {
        self.origin
            + self.east * local_position.x
            + self.up * local_position.y
            + self.north * local_position.z
    }

    /// Convert a planet-centred point to the local coordinates used by the renderer,
    /// retaining the subtraction and basis projection in f64 until the final cast.
    ///
    /// This is the required boundary for camera-relative GPU data. Call this before
    /// creating a Vertex; never call `as_vec3()` on an absolute planetary point.
    pub fn rebase_points_f32(self, points: impl IntoIterator<Item = DVec3>) -> Vec<Vec3> {
        points
            .into_iter()
            .map(|point| self.to_local_f32(point))
            .collect()
    }

    /// Return the squared distance from the frame origin without down-casting.
    /// Useful for deciding when a local render frame should be refreshed.
    pub fn distance_squared_to_origin(self, planet_position: DVec3) -> f64 {
        (planet_position - self.origin).length_squared()
    }

    /// Move a point along the spherical surface by a local east/north displacement.
    ///
    /// The input radius is preserved, so callers can apply the generated terrain
    /// elevation separately. Great-circle movement remains stable at the date line
    /// and across the poles.
    pub fn move_tangent(self, east_distance: f64, north_distance: f64) -> DVec3 {
        if !east_distance.is_finite() || !north_distance.is_finite() {
            return self.origin;
        }
        let radius = self.origin.length();
        if radius <= f64::EPSILON {
            return self.origin;
        }
        let displacement = self.east * east_distance + self.north * north_distance;
        let distance = displacement.length();
        if distance <= f64::EPSILON {
            return self.origin;
        }
        let angle = (distance / radius).min(std::f64::consts::PI);
        let tangent = displacement / distance;
        (self.up * angle.cos() + tangent * angle.sin()) * radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EARTH_RADIUS: f64 = 6_371_000.0;

    #[test]
    fn frame_axes_are_orthonormal_at_equator_and_poles() {
        for position in [
            DVec3::X * EARTH_RADIUS,
            DVec3::Y * EARTH_RADIUS,
            -DVec3::Y * EARTH_RADIUS,
            DVec3::new(1.0, 2.0, 3.0) * EARTH_RADIUS,
        ] {
            let frame = PlanetaryFrame::at(position);
            assert!((frame.east.length() - 1.0).abs() < 1e-12);
            assert!((frame.north.length() - 1.0).abs() < 1e-12);
            assert!((frame.up.length() - 1.0).abs() < 1e-12);
            assert!(frame.east.dot(frame.north).abs() < 1e-12);
            assert!(frame.east.dot(frame.up).abs() < 1e-12);
            assert!(frame.north.dot(frame.up).abs() < 1e-12);
        }
    }

    #[test]
    fn local_rebasing_round_trips_nearby_positions() {
        let frame = PlanetaryFrame::at(DVec3::X * EARTH_RADIUS);
        let local = DVec3::new(125.25, 8.0, -73.5);
        let planet = frame.to_planet(local);
        let round_trip = frame.to_local(planet);
        assert!((round_trip - local).length() < 1e-8);
        assert!((planet - frame.origin).length() < 200.0);
    }

    #[test]
    fn rebasing_preserves_metre_offsets_at_earth_radius() {
        let origin = DVec3::new(6_371_000.0, 0.0, 0.0);
        let frame = PlanetaryFrame::at(origin);
        let adjacent = origin + frame.east * 1.0 + frame.up * 2.0 + frame.north * 3.0;
        let local = frame.to_local_f32(adjacent);
        assert!((local - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
        // Casting the absolute coordinates first quantizes away metre-scale offsets
        // at Earth's radius. Keep this regression guard so rendering code must rebase
        // in f64 before converting to GPU f32 coordinates.
        let absolute_delta = adjacent.as_vec3() - origin.as_vec3();
        assert!((absolute_delta - Vec3::new(1.0, 2.0, 3.0)).length() > 0.1);
        assert!(local.is_finite());
    }

    #[test]
    fn batch_rebase_keeps_metre_scale_offsets_at_earth_radius() {
        let frame = PlanetaryFrame::at(DVec3::X * EARTH_RADIUS);
        let points = [
            frame.to_planet(DVec3::new(0.0, 0.0, 0.0)),
            frame.to_planet(DVec3::new(1.0, 0.0, 0.0)),
            frame.to_planet(DVec3::new(0.0, 0.0, 1.0)),
        ];
        let local = frame.rebase_points_f32(points);
        assert_eq!(local.len(), 3);
        assert!((local[1] - local[0] - Vec3::X).length() < 1e-5);
        assert!((local[2] - local[0] - Vec3::Z).length() < 1e-5);
        assert!((frame.distance_squared_to_origin(points[1]) - 1.0).abs() < 1e-7);
    }

    #[test]
    fn tangent_move_preserves_earth_radius_and_advances_expected_distance() {
        let frame = PlanetaryFrame::at(DVec3::X * EARTH_RADIUS);
        let moved = frame.move_tangent(1_000.0, 0.0);
        assert!((moved.length() - EARTH_RADIUS).abs() < 1e-8);
        let angle = frame.up.dot(moved.normalize()).clamp(-1.0, 1.0).acos();
        assert!((angle * EARTH_RADIUS - 1_000.0).abs() < 1e-5);
    }

    #[test]
    fn invalid_direction_falls_back_to_finite_frame() {
        let frame = PlanetaryFrame::at(DVec3::ZERO);
        assert_eq!(frame.up, DVec3::X);
        assert!(frame.to_local(DVec3::X * (EARTH_RADIUS + 1.0)).is_finite());
    }
}