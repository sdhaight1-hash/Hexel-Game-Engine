//! Shared deterministic gameplay model for procedural flora.
//!
//! Flora identity and gameplay state live here; renderer materials and textures do
//! not. Trees are one organism form among grasses, herbs, shrubs, crops, and other
//! plant life. This module is deliberately material-agnostic so procedural shading
//! can be introduced without changing flora gameplay data.

use std::collections::{BTreeMap, BTreeSet};

use crate::CellPos;

/// Broad ecological role of a flora organism.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloraForm {
    GroundCover,
    Herb,
    Fern,
    Shrub,
    Tree,
    Vine,
    Crop,
    Aquatic,
}

/// Species identity. Extend this catalog as the world flora expands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloraSpecies {
    Grass,
    Fern,
    Bracken,
    Clover,
    Wildflower,
    BerryShrub,
    Oak,
    Birch,
    Pine,
    Acacia,
    Kapok,
    Cypress,
    Maple,
    Crop,
    Generic,
}

/// Functional part of an organism. Not every flora form has block-like parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloraPartKind {
    Foliage,
    Stem,
    Trunk,
    Branch,
    Root,
    Flower,
    Fruit,
    Seed,
}

/// Gameplay representation of one harvestable flora hexel or plant part.
/// Visual materials are intentionally not stored here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloraBlock {
    pub position: CellPos,
    pub form: FloraForm,
    pub species: FloraSpecies,
    pub part: FloraPartKind,
    pub durability: u16,
    pub yield_units: u8,
}

impl FloraBlock {
    pub const fn new(
        position: CellPos,
        form: FloraForm,
        species: FloraSpecies,
        part: FloraPartKind,
        durability: u16,
        yield_units: u8,
    ) -> Self {
        Self { position, form, species, part, durability, yield_units }
    }
}

/// Mutable state for one deterministic flora organism.
/// Removed parts are tracked independently of renderer meshes, enabling reliable
/// targeting, harvesting, collision updates, and later save/load integration.
#[derive(Clone, Debug)]
pub struct FloraInstance {
    pub id: u64,
    pub form: FloraForm,
    pub species: FloraSpecies,
    blocks: BTreeMap<CellPos, FloraBlock>,
    removed: BTreeSet<CellPos>,
}

impl FloraInstance {
    pub fn new(
        id: u64,
        form: FloraForm,
        species: FloraSpecies,
        blocks: impl IntoIterator<Item = FloraBlock>,
    ) -> Self {
        let blocks = blocks
            .into_iter()
            .filter(|block| block.form == form && block.species == species)
            .map(|block| (block.position, block))
            .collect();
        Self { id, form, species, blocks, removed: BTreeSet::new() }
    }

    pub fn block_at(&self, position: CellPos) -> Option<&FloraBlock> {
        self.blocks.get(&position).filter(|_| !self.removed.contains(&position))
    }

    pub fn blocks(&self) -> impl Iterator<Item = &FloraBlock> {
        self.blocks.values().filter(|block| !self.removed.contains(&block.position))
    }

    pub fn remaining_block_count(&self) -> usize {
        self.blocks().count()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks().next().is_none()
    }

    /// Remove a part and return its yield once. Repeated harvesting yields None.
    pub fn harvest(&mut self, position: CellPos) -> Option<u8> {
        let block = self.blocks.get(&position)?;
        if !self.removed.insert(position) {
            return None;
        }
        Some(block.yield_units)
    }

    pub fn is_removed(&self, position: CellPos) -> bool {
        self.removed.contains(&position)
    }

    /// Expose stable removed positions for future persistence.
    pub fn removed_positions(&self) -> impl Iterator<Item = &CellPos> {
        self.removed.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oak() -> FloraInstance {
        FloraInstance::new(7, FloraForm::Tree, FloraSpecies::Oak, [
            FloraBlock::new(CellPos::new(2, 3, 4), FloraForm::Tree, FloraSpecies::Oak, FloraPartKind::Trunk, 10, 2),
            FloraBlock::new(CellPos::new(2, 3, 5), FloraForm::Tree, FloraSpecies::Oak, FloraPartKind::Trunk, 10, 2),
            FloraBlock::new(CellPos::new(3, 3, 6), FloraForm::Tree, FloraSpecies::Oak, FloraPartKind::Branch, 5, 1),
        ])
    }

    #[test]
    fn parts_are_addressable_by_world_cell() {
        let flora = oak();
        assert_eq!(flora.block_at(CellPos::new(2, 3, 4)).unwrap().part, FloraPartKind::Trunk);
        assert!(flora.block_at(CellPos::new(99, 99, 99)).is_none());
    }

    #[test]
    fn harvesting_removes_a_part_and_awards_yield_once() {
        let mut flora = oak();
        let position = CellPos::new(2, 3, 4);
        assert_eq!(flora.harvest(position), Some(2));
        assert!(flora.block_at(position).is_none());
        assert!(flora.is_removed(position));
        assert_eq!(flora.harvest(position), None);
        assert_eq!(flora.remaining_block_count(), 2);
    }

    #[test]
    fn removed_positions_are_available_for_persistence() {
        let mut flora = oak();
        let position = CellPos::new(3, 3, 6);
        flora.harvest(position);
        assert_eq!(flora.removed_positions().copied().collect::<Vec<_>>(), vec![position]);
    }

    #[test]
    fn organism_becomes_empty_after_all_parts_are_removed() {
        let mut flora = oak();
        let positions = flora.blocks().map(|block| block.position).collect::<Vec<_>>();
        for position in positions {
            assert!(flora.harvest(position).is_some());
        }
        assert!(flora.is_empty());
    }

    #[test]
    fn model_supports_non_tree_flora() {
        let grass = FloraInstance::new(8, FloraForm::GroundCover, FloraSpecies::Grass, [
            FloraBlock::new(CellPos::new(0, 0, 4), FloraForm::GroundCover, FloraSpecies::Grass, FloraPartKind::Foliage, 1, 1),
        ]);
        assert_eq!(grass.form, FloraForm::GroundCover);
        assert_eq!(grass.species, FloraSpecies::Grass);
    }
}
