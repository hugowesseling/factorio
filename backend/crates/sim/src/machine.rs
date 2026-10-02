use crate::coords::TilePos;
use crate::entity::{AnyId, Stack};
use crate::fluid::{FluidTank, TANK_CAPACITY};

pub const MACHINE_BURNER_GENERATOR: u8 = 0;
pub const MACHINE_STONE_FURNACE: u8 = 1;
pub const MACHINE_ASSEMBLER: u8 = 2;
pub const MACHINE_OFFSHORE_PUMP: u8 = 3;
pub const MACHINE_LAB: u8 = 4;

#[derive(Clone, Copy, Debug)]
pub struct Machine {
    pub kind: u8,
    pub pos: TilePos,
    pub size: u8,
    pub recipe: u16,
    pub progress: u32,
    pub fuel: u32,
    pub power: u32,
    pub in_buf: [Stack; 4],
    pub out_buf: [Stack; 4],
    pub tank: FluidTank,
}

impl Machine {
    pub fn new(kind: u8, pos: TilePos, size: u8, recipe: u16) -> Self {
        Self {
            kind,
            pos,
            size,
            recipe,
            progress: 0,
            fuel: 0,
            power: 0,
            in_buf: [Stack::default(); 4],
            out_buf: [Stack::default(); 4],
            tank: FluidTank::new(TANK_CAPACITY),
        }
    }

    pub fn size_tuple(&self) -> (i32, i32) {
        (self.size as i32, self.size as i32)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Inserter {
    pub pos: TilePos,
    pub dir: u8,
    pub source: Option<AnyId>,
    pub target: Option<AnyId>,
    pub held: Option<Stack>,
    pub progress: u32,
}

impl Inserter {
    pub fn new(pos: TilePos, dir: u8) -> Self {
        Self {
            pos,
            dir,
            source: None,
            target: None,
            held: None,
            progress: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_new_defaults() {
        let machine = Machine::new(MACHINE_STONE_FURNACE, TilePos { x: 0, y: 0 }, 2, 0);
        assert_eq!(machine.size, 2);
        assert_eq!(machine.fuel, 0);
    }
}
