//! Authoritative, deterministic data model for tree wood hexels.
//!
//! Rendering and player interaction will consume this model in the next integration
//! step. Placeholder textures deliberately do not live in this module: a tree block
//! describes gameplay material, not how the renderer shades it.

use std::collections::{BTreeMap, BTreeSet};

use crate::CellPos;

/// Gameplay identity for a harvestable tree hexel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WoodKind {
    Trunk,
    Branch,
    Root,
}

/// Species identity remains independent of rendering textures/materials.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TreeSpecies {
    Oak,
    Birch,
    Pine,
    Acacia,
    Kapok,
    Cypress,
    Maple,
    Generic,
}

/// A single wood hexel in a tree. Position is the stable world-space identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeBlock {
    pub position: CellPos,
    pub kind: WoodKind,
    pub species: TreeSpecies,
    pub durability: u16,
    pub wood_yield: u8,
}

impl TreeBlock {
    pub const fn new(
        position: CellPos,
        kind: WoodKind,
        species: TreeSpecies,
        durability: u16,
        wood_yield: u8,
    ) -> Self {
        Self {
            position,
            kind,
            species,
            durability,
            wood_yield,
        }
    }
}

/// Mutable gameplay state for one deterministically generated tree.
///
/// The block map is authoritative: removing a block updates the state once and
/// returns its yield once. Persistence can serialize the remaining map (or the
/// removed-position set) without depending on renderer meshes or textures.
#[derive(Clone, Debug)]
pub struct TreeInstance {
    pub id: u64,
    pub species: TreeSpecies,
    blocks: BTreeMap<CellPos, TreeBlock>,
    removed: BTreeSet<CellPos>,
}

impl TreeInstance {
    pub fn new(id: u64, species: TreeSpecies, blocks: impl IntoIterator<Item = TreeBlock>) -> Self {
        let blocks = blocks
            .into_iter()
            .filter(|block| block.species == species)
            .map(|block| (block.position, block))
            .collect();
        Self {
            id,
            species,
            blocks,
            removed: BTreeSet::new(),
        }
    }

    pub fn block_at(&self, position: CellPos) -> Option<&TreeBlock> {
        self.blocks
            .get(&position)
            .filter(|_| !self.removed.contains(&position))
    }

    pub fn blocks(&self) -> impl Iterator<Item = &TreeBlock> {
        self.blocks
            .values()
            .filter(|block| !self.removed.contains(&block.position))
    }

    pub fn is_empty(&self) -> bool {
        self.blocks().next().is_none()
    }

    pub fn remaining_block_count(&self) -> usize {
        self.blocks()
            .filter(|block| !self.removed.contains(&block.position))
            .count()
    }

    /// Harvest a wood hexel exactly once. Repeated calls return None.
    pub fn harvest(&mut self, position: CellPos) -> Option<u8> {
        let block = self.blocks.get(&position)?;
        if !self.removed.insert(position) {
            return None;
        }
        Some(block.wood_yield)
    }

    pub fn is_removed(&self, position: CellPos) -> bool {
        self.removed.contains(&position)
    }

    /// Positions are exposed for a future save format and deterministic tests.
    pub fn removed_positions(&self) -> impl Iterator<Item = &CellPos> {
        self.removed.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> TreeInstance {
        TreeInstance::new(
            7,
            TreeSpecies::Oak,
            [
                TreeBlock::new(
                    CellPos::new(2, 3, 4),
                    WoodKind::Trunk,
                    TreeSpecies::Oak,
                    10,
                    2,
                ),
                TreeBlock::new(
                    CellPos::new(2, 3, 5),
                    WoodKind::Trunk,
                    TreeSpecies::Oak,
                    10,
                    2,
                ),
                TreeBlock::new(
                    CellPos::new(3, 3, 6),
                    WoodKind::Branch,
                    TreeSpecies::Oak,
                    5,
                    1,
                ),
            ],
        )
    }

    #[test]
    fn blocks_are_addressable_by_world_cell() {
        let tree = sample_tree();
        assert_eq!(
            tree.block_at(CellPos::new(2, 3, 4)).unwrap().kind,
            WoodKind::Trunk
        );
        assert!(tree.block_at(CellPos::new(99, 99, 99)).is_none());
    }

    #[test]
    fn harvesting_removes_a_block_and_awards_yield_once() {
        let mut tree = sample_tree();
        let position = CellPos::new(2, 3, 4);
        assert_eq!(tree.harvest(position), Some(2));
        assert!(tree.block_at(position).is_none());
        assert!(tree.is_removed(position));
        assert_eq!(tree.harvest(position), None);
        assert_eq!(tree.remaining_block_count(), 2);
    }

    #[test]
    fn removed_positions_are_available_for_persistence() {
        let mut tree = sample_tree();
        let position = CellPos::new(3, 3, 6);
        tree.harvest(position);
        assert_eq!(
            tree.removed_positions().copied().collect::<Vec<_>>(),
            vec![position]
        );
    }

    #[test]
    fn tree_becomes_empty_after_all_blocks_are_removed() {
        let mut tree = sample_tree();
        let positions = tree
            .blocks()
            .map(|block| block.position)
            .collect::<Vec<_>>();
        for position in positions {
            assert!(tree.harvest(position).is_some());
        }
        assert!(tree.is_empty());
    }
}
