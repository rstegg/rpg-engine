use crate::entities::player::Stats;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSlot {
    Head,
    Body,
    Hands,
    Feet,
    Weapon,
    Addon,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub slot: ItemSlot,
    pub layer_name: String, // Exact string corresponding to the layer file in assets
    // pub stat_bonus: Stats, // To be used when stat calculation is expanded
}

impl Item {
    pub fn new(id: &str, name: &str, slot: ItemSlot, layer_name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            slot,
            layer_name: layer_name.to_string(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Equipment {
    pub head: Option<Item>,
    pub body: Option<Item>,
    pub hands: Option<Item>,
    pub feet: Option<Item>,
    pub weapon: Option<Item>,
    pub addon: Option<Item>,
}

impl Equipment {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the old item if there was one in that slot.
    pub fn equip(&mut self, item: Item) -> Option<Item> {
        match item.slot {
            ItemSlot::Head => std::mem::replace(&mut self.head, Some(item)),
            ItemSlot::Body => std::mem::replace(&mut self.body, Some(item)),
            ItemSlot::Hands => std::mem::replace(&mut self.hands, Some(item)),
            ItemSlot::Feet => std::mem::replace(&mut self.feet, Some(item)),
            ItemSlot::Weapon => std::mem::replace(&mut self.weapon, Some(item)),
            ItemSlot::Addon => std::mem::replace(&mut self.addon, Some(item)),
        }
    }

    pub fn unequip(&mut self, slot: ItemSlot) -> Option<Item> {
        match slot {
            ItemSlot::Head => self.head.take(),
            ItemSlot::Body => self.body.take(),
            ItemSlot::Hands => self.hands.take(),
            ItemSlot::Feet => self.feet.take(),
            ItemSlot::Weapon => self.weapon.take(),
            ItemSlot::Addon => self.addon.take(),
        }
    }
}
