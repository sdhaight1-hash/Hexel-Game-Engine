//! Opt-in gameplay-facing navigation over geodesic surface cells.
//!
//! This adapter deliberately does not replace the existing flat-world movement,
//! collision, raycast, or edit systems. It provides the cell-level operations a
//! future spherical movement controller can use while those systems remain intact.

use glam::DVec3;

use crate::topology::{SurfaceCellId, SurfaceTopology, TopologyError};

/// Small navigation adapter for a generated spherical surface.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceNavigator {
    topology: SurfaceTopology,
}

impl SurfaceNavigator {
    /// Generate a navigation surface at the requested topology frequency and radius.
    pub fn new(frequency: u32, radius: f64) -> Result<Self, TopologyError> {
        Ok(Self {
            topology: SurfaceTopology::generate(frequency, radius)?,
        })
    }

    /// Access the underlying topology for cell geometry and metadata.
    pub fn topology(&self) -> &SurfaceTopology {
        &self.topology
    }

    /// Find the surface cell containing the closest center direction.
    ///
    /// The position is planet-centered; its magnitude is ignored. Invalid or zero
    /// positions return `None`.
    pub fn cell_at(&self, position: DVec3) -> Option<SurfaceCellId> {
        self.topology.cell_id_for_direction(position)
    }

    /// Return the sorted neighbors of a valid cell ID.
    pub fn neighbors(&self, current: SurfaceCellId) -> Option<&[SurfaceCellId]> {
        self.topology.neighbor_ids(current)
    }

    /// Choose the adjacent cell most aligned with a requested world-space travel
    /// direction projected onto the tangent plane at `current`.
    ///
    /// The requested direction may include a radial component; only its tangent
    /// component affects the result. Zero, non-finite, or purely radial directions,
    /// and invalid current IDs return `None`. Equal alignment scores choose the
    /// lowest neighbor ID.
    pub fn next_cell_toward(
        &self,
        current: SurfaceCellId,
        travel_direction: DVec3,
    ) -> Option<SurfaceCellId> {
        if !travel_direction.x.is_finite()
            || !travel_direction.y.is_finite()
            || !travel_direction.z.is_finite()
        {
            return None;
        }

        let cell = self.topology.cell(current)?;
        let radial = cell.center.normalize();
        let tangent = travel_direction - radial * travel_direction.dot(radial);
        if tangent.length_squared() <= f64::EPSILON {
            return None;
        }
        let tangent = tangent.normalize();

        self.topology
            .neighbor_ids(current)?
            .iter()
            .filter_map(|neighbor_id| {
                let neighbor = self.topology.cell(*neighbor_id)?;
                let neighbor_radial = neighbor.center.normalize();
                let neighbor_tangent =
                    (neighbor_radial - radial * neighbor_radial.dot(radial)).normalize();
                Some((*neighbor_id, neighbor_tangent.dot(tangent)))
            })
            .max_by(|(a_id, a_score), (b_id, b_score)| {
                a_score.total_cmp(b_score).then_with(|| b_id.cmp(a_id))
            })
            .map(|(id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_lookup_uses_planet_centered_positions() {
        let navigator = SurfaceNavigator::new(3, 100.0).unwrap();
        for cell in navigator.topology().cells() {
            assert_eq!(navigator.cell_at(cell.center), Some(cell.id));
            assert_eq!(navigator.cell_at(cell.center * 3.0), Some(cell.id));
        }
        assert_eq!(navigator.cell_at(DVec3::ZERO), None);
        assert_eq!(navigator.cell_at(DVec3::splat(f64::NAN)), None);
    }

    #[test]
    fn movement_selects_the_neighbor_aligned_with_tangent_direction() {
        let navigator = SurfaceNavigator::new(3, 100.0).unwrap();
        let current = navigator.topology().cells()[0].id;
        let cell = navigator.topology().cell(current).unwrap();
        let radial = cell.center.normalize();
        let expected = navigator.neighbors(current).unwrap()[0];
        let neighbor_radial = navigator
            .topology()
            .cell(expected)
            .unwrap()
            .center
            .normalize();
        let toward_neighbor = neighbor_radial - radial * neighbor_radial.dot(radial);

        assert_eq!(
            navigator.next_cell_toward(current, toward_neighbor),
            Some(expected)
        );
    }

    #[test]
    fn movement_rejects_invalid_ids_and_non_tangent_directions() {
        let navigator = SurfaceNavigator::new(2, 100.0).unwrap();
        let current = navigator.topology().cells()[0].id;
        let radial = navigator
            .topology()
            .cell(current)
            .unwrap()
            .center
            .normalize();

        assert_eq!(navigator.next_cell_toward(current, DVec3::ZERO), None);
        assert_eq!(navigator.next_cell_toward(current, radial * 5.0), None);
        assert_eq!(
            navigator.next_cell_toward(current, DVec3::splat(f64::INFINITY)),
            None
        );
        assert_eq!(
            navigator.next_cell_toward(SurfaceCellId(u32::MAX), DVec3::X),
            None
        );
        assert_eq!(navigator.neighbors(SurfaceCellId(u32::MAX)), None);
    }

    #[test]
    fn movement_always_returns_an_adjacent_cell() {
        let navigator = SurfaceNavigator::new(4, 100.0).unwrap();
        for cell in navigator.topology().cells() {
            for neighbor in navigator.neighbors(cell.id).unwrap() {
                let neighbor_center = navigator.topology().cell(*neighbor).unwrap().center;
                let direction = neighbor_center - cell.center;
                let next = navigator.next_cell_toward(cell.id, direction).unwrap();
                assert!(navigator.topology().are_neighbors(cell.id, next).unwrap());
            }
        }
    }
}
