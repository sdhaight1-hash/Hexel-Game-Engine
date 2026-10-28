//! Logical hierarchy for the unified planetary Hexel world.
//!
//! These are stable parent addresses, not extra terrain stores: Hexels remain
//! the editable source of truth, grouped into 16³ chunks, regions, and
//! hyper-regions for streaming, persistence, and LOD scheduling.
//!
//! Region sizes are policy constants and can be tuned without changing the
//! address semantics. Chunk coordinates use Euclidean division so negative
//! tangent-space coordinates group consistently across the anchor origin.

use crate::planetary_chunks::PlanetHexChunkKey;

/// Number of chunks along each axis of one region.
pub const CHUNKS_PER_REGION: i32 = 8;
/// Number of regions along each axis of one hyper-region.
pub const REGIONS_PER_HYPER_REGION: i32 = 8;
/// Number of chunks along each axis of one hyper-region.
pub const CHUNKS_PER_HYPER_REGION: i32 =
    CHUNKS_PER_REGION * REGIONS_PER_HYPER_REGION;

/// A logical group of 8×8×8 chunks, scoped to a stable planetary anchor cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanetRegionKey {
    pub anchor_cell: u32,
    pub q: i32,
    pub r: i32,
    pub y: i32,
}

impl PlanetRegionKey {
    pub const fn hyper_region(self) -> PlanetHyperRegionKey {
        PlanetHyperRegionKey {
            anchor_cell: self.anchor_cell,
            q: self.q.div_euclid(REGIONS_PER_HYPER_REGION),
            r: self.r.div_euclid(REGIONS_PER_HYPER_REGION),
            y: self.y.div_euclid(REGIONS_PER_HYPER_REGION),
        }
    }
}

/// A logical group of 8×8×8 regions, used for coarse streaming and LOD policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanetHyperRegionKey {
    pub anchor_cell: u32,
    pub q: i32,
    pub r: i32,
    pub y: i32,
}

/// Derive the region containing a chunk. This is address math only; it does not
/// load data or allocate a second copy of the chunk's Hexels.
pub const fn region_for_chunk(chunk: PlanetHexChunkKey) -> PlanetRegionKey {
    PlanetRegionKey {
        anchor_cell: chunk.anchor_cell,
        q: chunk.q.div_euclid(CHUNKS_PER_REGION),
        r: chunk.r.div_euclid(CHUNKS_PER_REGION),
        y: chunk.y.div_euclid(CHUNKS_PER_REGION),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planetary_chunks::PlanetHexChunkKey;

    #[test]
    fn chunk_region_and_hyper_region_parentage_is_stable() {
        let chunk = PlanetHexChunkKey { anchor_cell: 17, q: 9, r: 16, y: -1 };
        let region = region_for_chunk(chunk);
        assert_eq!(region, PlanetRegionKey { anchor_cell: 17, q: 1, r: 2, y: -1 });
        assert_eq!(region.hyper_region(), PlanetHyperRegionKey {
            anchor_cell: 17, q: 0, r: 0, y: -1,
        });
    }

    #[test]
    fn negative_chunks_use_euclidean_parent_coordinates() {
        let chunk = PlanetHexChunkKey { anchor_cell: 2, q: -1, r: -8, y: -9 };
        assert_eq!(region_for_chunk(chunk), PlanetRegionKey {
            anchor_cell: 2, q: -1, r: -1, y: -2,
        });
    }

    #[test]
    fn hierarchy_sizes_are_consistent() {
        assert_eq!(CHUNKS_PER_HYPER_REGION, 64);
        assert_eq!(CHUNKS_PER_REGION.pow(3), 512);
        assert_eq!(REGIONS_PER_HYPER_REGION.pow(3), 512);
    }
}
