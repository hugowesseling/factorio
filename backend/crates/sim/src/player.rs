use crate::coords::TilePos;
use crate::inventory::Inventory;

#[derive(Clone, Debug)]
pub struct Player {
    pub pos: TilePos,
    pub dir: u8,
    pub health: u32,
    pub target: Option<TilePos>,
    pub inventory: Inventory,
}

impl Player {
    pub fn new(pos: TilePos) -> Self {
        Self {
            pos,
            dir: 0,
            health: 100,
            target: None,
            inventory: Inventory::new(),
        }
    }
}
