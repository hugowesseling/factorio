use crate::coords::TilePos;

#[derive(Clone, Copy, Debug)]
pub struct Enemy {
    pub pos: TilePos,
    pub health: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Projectile {
    pub pos: TilePos,
    pub dx: i32,
    pub dy: i32,
    pub damage: u32,
}
