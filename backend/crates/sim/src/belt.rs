use crate::coords::TilePos;
use crate::entity::{Stack, SENTINEL};

pub const DIR_EAST: u8 = 0;
pub const DIR_NORTH: u8 = 1;
pub const DIR_WEST: u8 = 2;
pub const DIR_SOUTH: u8 = 3;
pub const DIR_MASK: u8 = 0b0000_0011;
pub const CURVE_BIT: u8 = 0b0000_0100;
pub const SIDE_BIT: u8 = 0b0000_1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TransportBelt {
    pub pos: TilePos,
    pub flags: u8,
    pub next: u32,
    pub prev: u32,
    pub progress: u32,
    pub items: [Stack; 4],
}

impl TransportBelt {
    pub fn new(pos: TilePos, dir: u8) -> Self {
        Self {
            pos,
            flags: dir & DIR_MASK,
            next: SENTINEL,
            prev: SENTINEL,
            progress: 0,
            items: [Stack::default(); 4],
        }
    }

    pub fn direction(&self) -> u8 {
        self.flags & DIR_MASK
    }

    pub fn set_direction(&mut self, dir: u8) {
        self.flags = (self.flags & !DIR_MASK) | (dir & DIR_MASK);
    }

    pub fn step(&self) -> TilePos {
        match self.direction() {
            DIR_EAST => self.pos.add(1, 0),
            DIR_NORTH => self.pos.add(0, -1),
            DIR_WEST => self.pos.add(-1, 0),
            _ => self.pos.add(0, 1),
        }
    }

    pub fn next_index(&self) -> Option<u32> {
        if self.next == SENTINEL {
            None
        } else {
            Some(self.next)
        }
    }
}

#[derive(Debug, Default)]
pub struct BeltLanes {
    pub halves: Vec<[Stack; 4]>,
}

impl BeltLanes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ensure(&mut self, len: usize) {
        while self.halves.len() < len {
            self.halves.push([Stack::default(); 4]);
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.halves.len() {
            self.halves.swap_remove(index);
        }
    }

    pub fn len(&self) -> usize {
        self.halves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.halves.is_empty()
    }

    pub fn add_first(&mut self, index: usize, item: u16, count: u16) -> u16 {
        let buf = &mut self.halves[index];
        crate::entity::stack_add(buf, item, count)
    }

    pub fn take_first(&mut self, index: usize) -> Option<Stack> {
        let buf = &mut self.halves[index];
        crate::entity::stack_any(buf)
    }

    pub fn is_full(&self, index: usize) -> bool {
        self.halves.get(index).map(|buf| buf.iter().all(|s| s.count == 65535)).unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn belt_direction_steps() {
        let mut belt = TransportBelt::new(TilePos { x: 0, y: 0 }, DIR_EAST);
        assert_eq!(belt.step(), TilePos { x: 1, y: 0 });
        belt.set_direction(DIR_SOUTH);
        assert_eq!(belt.step(), TilePos { x: 0, y: 1 });
    }
}
