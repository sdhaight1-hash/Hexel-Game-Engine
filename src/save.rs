//! Versioned, dependency-free text save format for player/world progress.
use crate::{BlockId, CellPos, inventory::{Inventory, ItemId, ItemStack}};

pub const SAVE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct GameSave {
    pub seed: u32,
    pub player_position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub planet_mode: bool,
    pub selected_hotbar: usize,
    pub inventory: [Option<ItemStack>; Inventory::TOTAL_SLOTS],
    pub flat_edits: Vec<(CellPos, BlockId)>,
    pub planet_edits: Vec<(u32, BlockId)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    UnsupportedVersion(u32),
    MissingField(&'static str),
    InvalidField(&'static str),
    InvalidBlock(u8),
    InvalidItem(u8),
    InvalidSlot(usize),
    InvalidStack(u16),
}

impl GameSave {
    pub fn fresh(seed: u32) -> Self {
        Self {
            seed,
            player_position: [0.0, 0.0, 0.0],
            yaw: 0.0,
            pitch: -0.15,
            planet_mode: false,
            selected_hotbar: 0,
            inventory: [None; Inventory::TOTAL_SLOTS],
            flat_edits: Vec::new(),
            planet_edits: Vec::new(),
        }
    }

    /// Encode as a stable, line-oriented format that is easy to inspect and diff.
    pub fn encode(&self) -> String {
        let mut out = format!(
            "HEXEL_SAVE {SAVE_VERSION}\nseed {}\nplayer {:.9} {:.9} {:.9}\nlook {:.9} {:.9}\nmode {}\nhotbar {}\n",
            self.seed,
            self.player_position[0], self.player_position[1], self.player_position[2],
            self.yaw, self.pitch,
            if self.planet_mode { "planet" } else { "flat" },
            self.selected_hotbar,
        );
        for (slot, stack) in self.inventory.iter().enumerate() {
            if let Some(stack) = stack {
                out.push_str(&format!("item {slot} {} {}\n", item_code(stack.item), stack.count));
            }
        }
        for (cell, block) in &self.flat_edits {
            out.push_str(&format!("flat {} {} {} {}\n", cell.q, cell.r, cell.y, block_code(*block)));
        }
        for (id, block) in &self.planet_edits {
            out.push_str(&format!("planet {id} {}\n", block_code(*block)));
        }
        out
    }

    pub fn decode(source: &str) -> Result<Self, SaveError> {
        let mut lines = source.lines();
        let header = lines.next().ok_or(SaveError::MissingField("header"))?;
        let mut header_parts = header.split_whitespace();
        if header_parts.next() != Some("HEXEL_SAVE") {
            return Err(SaveError::InvalidField("header"));
        }
        let version = parse(header_parts.next(), "version")?;
        if version != SAVE_VERSION {
            return Err(SaveError::UnsupportedVersion(version));
        }

        let mut save = Self::fresh(0);
        let mut saw_seed = false;
        let mut saw_player = false;
        let mut saw_look = false;
        let mut saw_mode = false;
        let mut saw_hotbar = false;
        for line in lines {
            let mut parts = line.split_whitespace();
            let Some(kind) = parts.next() else { continue };
            match kind {
                "seed" => { save.seed = parse(parts.next(), "seed")?; saw_seed = true; }
                "player" => {
                    save.player_position = [
                        parse(parts.next(), "player x")?,
                        parse(parts.next(), "player y")?,
                        parse(parts.next(), "player z")?,
                    ];
                    if !save.player_position.iter().all(|v: &f32| v.is_finite()) {
                        return Err(SaveError::InvalidField("player position"));
                    }
                    saw_player = true;
                }
                "look" => {
                    save.yaw = parse(parts.next(), "yaw")?;
                    save.pitch = parse(parts.next(), "pitch")?;
                    if !save.yaw.is_finite() || !save.pitch.is_finite() {
                        return Err(SaveError::InvalidField("look"));
                    }
                    saw_look = true;
                }
                "mode" => {
                    save.planet_mode = match parts.next() {
                        Some("planet") => true,
                        Some("flat") => false,
                        _ => return Err(SaveError::InvalidField("mode")),
                    };
                    saw_mode = true;
                }
                "hotbar" => {
                    save.selected_hotbar = parse(parts.next(), "hotbar")?;
                    if save.selected_hotbar >= Inventory::HOTBAR_SLOTS {
                        return Err(SaveError::InvalidField("hotbar"));
                    }
                    saw_hotbar = true;
                }
                "item" => {
                    let slot: usize = parse(parts.next(), "item slot")?;
                    if slot >= Inventory::TOTAL_SLOTS { return Err(SaveError::InvalidSlot(slot)); }
                    let item = item_from_code(parse(parts.next(), "item type")?)?;
                    let count: u16 = parse(parts.next(), "item count")?;
                    let stack = ItemStack::new(item, count).ok_or(SaveError::InvalidStack(count))?;
                    save.inventory[slot] = Some(stack);
                }
                "flat" => {
                    let cell = CellPos::new(
                        parse(parts.next(), "flat q")?,
                        parse(parts.next(), "flat r")?,
                        parse(parts.next(), "flat y")?,
                    );
                    let block = block_from_code(parse(parts.next(), "flat block")?)?;
                    save.flat_edits.push((cell, block));
                }
                "planet" => {
                    let id = parse(parts.next(), "planet cell")?;
                    let block = block_from_code(parse(parts.next(), "planet block")?)?;
                    save.planet_edits.push((id, block));
                }
                _ => return Err(SaveError::InvalidField("record type")),
            }
            if parts.next().is_some() {
                return Err(SaveError::InvalidField("extra data"));
            }
        }
        for (name, seen) in [
            ("seed", saw_seed), ("player", saw_player), ("look", saw_look),
            ("mode", saw_mode), ("hotbar", saw_hotbar),
        ] {
            if !seen { return Err(SaveError::MissingField(name)); }
        }
        Ok(save)
    }
}

fn parse<T: std::str::FromStr>(value: Option<&str>, field: &'static str) -> Result<T, SaveError> {
    value.ok_or(SaveError::MissingField(field))?
        .parse().map_err(|_| SaveError::InvalidField(field))
}

fn block_code(block: BlockId) -> u8 { block as u8 }
fn block_from_code(code: u8) -> Result<BlockId, SaveError> {
    match code {
        0 => Ok(BlockId::Air), 1 => Ok(BlockId::Grass), 2 => Ok(BlockId::Dirt),
        3 => Ok(BlockId::Stone), 4 => Ok(BlockId::Foundation),
        _ => Err(SaveError::InvalidBlock(code)),
    }
}
fn item_code(item: ItemId) -> u8 {
    match item {
        ItemId::OakWood => 0, ItemId::BirchWood => 1, ItemId::PineWood => 2,
        ItemId::GenericWood => 3, ItemId::Stick => 4, ItemId::GrassFiber => 5,
        ItemId::GrassBlock => 6, ItemId::DirtBlock => 7, ItemId::StoneBlock => 8,
    }
}
fn item_from_code(code: u8) -> Result<ItemId, SaveError> {
    match code {
        0 => Ok(ItemId::OakWood), 1 => Ok(ItemId::BirchWood), 2 => Ok(ItemId::PineWood),
        3 => Ok(ItemId::GenericWood), 4 => Ok(ItemId::Stick), 5 => Ok(ItemId::GrassFiber),
        6 => Ok(ItemId::GrassBlock), 7 => Ok(ItemId::DirtBlock), 8 => Ok(ItemId::StoneBlock),
        _ => Err(SaveError::InvalidItem(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_world_player_and_inventory() {
        let mut save = GameSave::fresh(123456);
        save.player_position = [10.5, -4.0, 9.25];
        save.yaw = 1.25;
        save.pitch = -0.3;
        save.planet_mode = true;
        save.selected_hotbar = 4;
        save.inventory[0] = ItemStack::new(ItemId::GrassFiber, 17);
        save.inventory[35] = ItemStack::new(ItemId::BirchWood, 99);
        save.inventory[34] = ItemStack::new(ItemId::StoneBlock, 7);
        save.flat_edits.push((CellPos::new(-3, 8, 2), BlockId::Air));
        save.planet_edits.push((42, BlockId::Stone));
        let decoded = GameSave::decode(&save.encode()).unwrap();
        assert_eq!(decoded, save);
    }

    #[test]
    fn rejects_unsupported_version_and_corrupt_blocks() {
        assert_eq!(GameSave::decode("HEXEL_SAVE 999\n"), Err(SaveError::UnsupportedVersion(999)));
        let corrupt = "HEXEL_SAVE 1\nseed 1\nplayer 0 0 0\nlook 0 0\nmode flat\nhotbar 0\nflat 0 0 0 99\n";
        assert_eq!(GameSave::decode(corrupt), Err(SaveError::InvalidBlock(99)));
    }

    #[test]
    fn rejects_non_finite_player_positions() {
        let corrupt = "HEXEL_SAVE 1\nseed 1\nplayer NaN 0 0\nlook 0 0\nmode flat\nhotbar 0\n";
        assert_eq!(GameSave::decode(corrupt), Err(SaveError::InvalidField("player position")));
    }
}
