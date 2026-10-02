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

fn floor_div(a: i32, b: i32) -> i32 {
    let q = a / b;
    if a % b != 0 && (a < 0) != (b < 0) {
        q - 1
    } else {
        q
    }
}

fn hash_corner(seed: u32, x: i32, y: i32) -> u32 {
    let mut h = seed;
    h ^= (x as u32).wrapping_mul(0x9e37_79b1);
    h = h.rotate_left(13);
    h ^= (y as u32).wrapping_mul(0x85eb_ca77);
    h = h.rotate_left(17);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    h
}

fn smoothstep(value: u32) -> u32 {
    let t = value as u64;
    let t2 = (t * t) >> 16;
    let t3 = (t2 * t) >> 16;
    let blended = 3 * t2 as i64 - 2 * t3 as i64;
    blended.clamp(0, 65535) as u32
}

fn lerp16(a: u32, b: u32, t: u32) -> u32 {
    let delta = b as i64 - a as i64;
    (a as i64 + ((delta * t as i64) >> 16)) as u32
}

pub fn value_noise(seed: u32, x: i32, y: i32, cell: i32) -> u32 {
    let cx = floor_div(x, cell);
    let cy = floor_div(y, cell);
    let fx = (x - cx * cell) as u32;
    let fy = (y - cy * cell) as u32;
    let tx = smoothstep((fx * 65536) / cell as u32);
    let ty = smoothstep((fy * 65536) / cell as u32);
    let v00 = hash_corner(seed, cx, cy) >> 16;
    let v10 = hash_corner(seed, cx + 1, cy) >> 16;
    let v01 = hash_corner(seed, cx, cy + 1) >> 16;
    let v11 = hash_corner(seed, cx + 1, cy + 1) >> 16;
    let top = lerp16(v00, v10, tx);
    let bottom = lerp16(v01, v11, tx);
    lerp16(top, bottom, ty)
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
