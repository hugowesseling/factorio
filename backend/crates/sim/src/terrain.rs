use crate::coords::{chunk_index, ChunkPos, TilePos};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Resource {
    Metal,
    Copper,
    Stone,
    Coal,
}

impl Resource {
    pub const COUNT: usize = 4;

    pub fn from_index(i: u8) -> Option<Self> {
        Some(match i {
            0 => Resource::Metal,
            1 => Resource::Copper,
            2 => Resource::Stone,
            3 => Resource::Coal,
            _ => return None,
        })
    }

    pub fn index(self) -> u8 {
        match self {
            Resource::Metal => 0,
            Resource::Copper => 1,
            Resource::Stone => 2,
            Resource::Coal => 3,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(transparent)]
pub struct TileFlag(pub u16);

impl TileFlag {
    pub const NONE: TileFlag = TileFlag(0);
    pub const WATER: TileFlag = TileFlag(1 << 0);
    pub const CLIFF: TileFlag = TileFlag(1 << 1);

    #[inline]
    pub fn is_set(self, flag: TileFlag) -> bool {
        self.0 & flag.0 != 0
    }

    #[inline]
    pub fn set(&mut self, flag: TileFlag) {
        self.0 |= flag.0;
    }

    #[inline]
    pub fn clear(&mut self, flag: TileFlag) {
        self.0 &= !flag.0;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TileData {
    pub resource: u8,
    pub ore: u16,
    pub water: bool,
}

#[derive(Debug)]
pub struct TerrainChunk {
    pub resource: Box<[u8; crate::coords::CHUNK_AREA]>,
    pub ore_amount: Box<[u16; crate::coords::CHUNK_AREA]>,
    pub flags: Box<[u16; crate::coords::CHUNK_AREA]>,
}

impl TerrainChunk {
    pub fn new() -> Self {
        Self {
            resource: Box::new([0u8; crate::coords::CHUNK_AREA]),
            ore_amount: Box::new([0u16; crate::coords::CHUNK_AREA]),
            flags: Box::new([0u16; crate::coords::CHUNK_AREA]),
        }
    }

    pub fn get(&self, chunk: ChunkPos, pos: TilePos) -> Option<TileData> {
        let i = chunk_index(chunk, pos)?;
        Some(TileData {
            resource: self.resource[i],
            ore: self.ore_amount[i],
            water: self.flags[i] & TileFlag::WATER.0 != 0,
        })
    }

    pub fn set_water(&mut self, chunk: ChunkPos, pos: TilePos) -> Option<()> {
        let i = chunk_index(chunk, pos)?;
        self.flags[i] |= TileFlag::WATER.0;
        self.resource[i] = 0;
        self.ore_amount[i] = 0;
        Some(())
    }

    pub fn clear_water(&mut self, chunk: ChunkPos, pos: TilePos) -> Option<()> {
        let i = chunk_index(chunk, pos)?;
        self.flags[i] &= !TileFlag::WATER.0;
        Some(())
    }

    pub fn set_ore(&mut self, chunk: ChunkPos, pos: TilePos, resource: u8, amount: u16) -> Option<()> {
        let i = chunk_index(chunk, pos)?;
        self.resource[i] = resource;
        self.ore_amount[i] = amount;
        self.flags[i] &= !TileFlag::WATER.0;
        Some(())
    }

    pub fn decrease_ore(&mut self, chunk: ChunkPos, pos: TilePos, amount: u16) -> Option<u16> {
        let i = chunk_index(chunk, pos)?;
        let old = self.ore_amount[i];
        let next = old.saturating_sub(amount);
        self.ore_amount[i] = next;
        if next == 0 {
            self.resource[i] = 0;
        }
        Some(old)
    }
}

impl Default for TerrainChunk {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_and_ore_do_not_overlap() {
        let mut chunk = TerrainChunk::new();
        let c = ChunkPos { x: 0, y: 0 };
        chunk.set_water(c, TilePos { x: 1, y: 1 });
        chunk.set_ore(c, TilePos { x: 1, y: 1 }, 0, 10);
        let tile = chunk.get(c, TilePos { x: 1, y: 1 }).unwrap();
        assert!(!tile.water);
        assert_eq!(tile.ore, 10);
    }
}
