//! Small, deterministic inventory primitives for gameplay systems.
//! Rendering and input bindings are intentionally kept outside this module.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemId {
    OakWood,
    BirchWood,
    PineWood,
    GenericWood,
    Stick,
    GrassFiber,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: u16,
}

impl ItemStack {
    pub const MAX_STACK: u16 = 99;

    pub const fn new(item: ItemId, count: u16) -> Option<Self> {
        if count == 0 || count > Self::MAX_STACK {
            None
        } else {
            Some(Self { item, count })
        }
    }
}

/// Inventory with nine hotbar slots followed by 27 backpack slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    slots: [Option<ItemStack>; 36],
    selected_hotbar: usize,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: [None; 36],
            selected_hotbar: 0,
        }
    }
}

impl Inventory {
    pub const HOTBAR_SLOTS: usize = 9;
    pub const TOTAL_SLOTS: usize = 36;

    pub fn selected_hotbar(&self) -> usize {
        self.selected_hotbar
    }

    pub fn select_hotbar(&mut self, slot: usize) -> bool {
        if slot >= Self::HOTBAR_SLOTS {
            return false;
        }
        self.selected_hotbar = slot;
        true
    }

    pub fn selected_stack(&self) -> Option<ItemStack> {
        self.slots[self.selected_hotbar]
    }

    pub fn slot(&self, index: usize) -> Option<ItemStack> {
        self.slots.get(index).copied().flatten()
    }

    /// Adds as much as possible; returns the number of items that did not fit.
    pub fn add(&mut self, item: ItemId, mut count: u16) -> u16 {
        for stack in self
            .slots
            .iter_mut()
            .flatten()
            .filter(|stack| stack.item == item && stack.count < ItemStack::MAX_STACK)
        {
            let room = ItemStack::MAX_STACK - stack.count;
            let moved = count.min(room);
            stack.count += moved;
            count -= moved;
            if count == 0 {
                return 0;
            }
        }
        for slot in &mut self.slots {
            if slot.is_none() {
                let moved = count.min(ItemStack::MAX_STACK);
                *slot = ItemStack::new(item, moved);
                count -= moved;
                if count == 0 {
                    return 0;
                }
            }
        }
        count
    }

    pub fn remove_from_slot(&mut self, index: usize, count: u16) -> u16 {
        let Some(Some(stack)) = self.slots.get_mut(index) else {
            return 0;
        };
        let removed = count.min(stack.count);
        stack.count -= removed;
        if stack.count == 0 {
            self.slots[index] = None;
        }
        removed
    }

    /// Restore a saved slot without applying inventory pickup/stacking rules.
    pub fn set_slot(&mut self, index: usize, stack: Option<ItemStack>) -> bool {
        let Some(slot) = self.slots.get_mut(index) else { return false; };
        *slot = stack;
        true
    }

    pub fn slots(&self) -> &[Option<ItemStack>; 36] {
        &self.slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotbar_selection_is_limited_to_nine_slots() {
        let mut inventory = Inventory::default();
        assert!(inventory.select_hotbar(8));
        assert_eq!(inventory.selected_hotbar(), 8);
        assert!(!inventory.select_hotbar(9));
        assert_eq!(inventory.selected_hotbar(), 8);
    }

    #[test]
    fn items_stack_to_ninety_nine() {
        let mut inventory = Inventory::default();
        assert_eq!(inventory.add(ItemId::OakWood, 120), 0);
        assert_eq!(
            inventory.slot(0),
            Some(ItemStack {
                item: ItemId::OakWood,
                count: 99
            })
        );
        assert_eq!(
            inventory.slot(1),
            Some(ItemStack {
                item: ItemId::OakWood,
                count: 21
            })
        );
    }

    #[test]
    fn full_inventory_reports_items_that_do_not_fit() {
        let mut inventory = Inventory::default();
        assert_eq!(inventory.add(ItemId::OakWood, 36 * 99), 0);
        assert_eq!(inventory.add(ItemId::Stick, 1), 1);
    }

    #[test]
    fn removing_last_item_clears_slot() {
        let mut inventory = Inventory::default();
        inventory.add(ItemId::BirchWood, 3);
        assert_eq!(inventory.remove_from_slot(0, 3), 3);
        assert_eq!(inventory.slot(0), None);
    }
}
