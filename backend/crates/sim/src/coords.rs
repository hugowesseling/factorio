#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct TilePos {
    pub x: i32,
    pub y: i32,
}

impl TilePos {
    #[inline]
    pub fn key(self) -> u64 {
        ((self.x as u32 as u64) << 32) | (self.y as u32 as u64)
    }

    #[inline]
    pub fn add(self, dx: i32, dy: i32) -> Self {
        Self { x: self.x + dx, y: self.y + dy }
    }

    pub fn rect(start: TilePos, w: i32, h: i32) -> Vec<TilePos> {
        let mut out = Vec::with_capacity(w as usize * h as usize);
        for dy in 0..h {
            for dx in 0..w {
                out.push(start.add(dx, dy));
            }
        }
        out
    }
}

pub const CHUNK_SIZE: i32 = 32;
pub const CHUNK_AREA: usize = 32 * 32;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
}

impl From<TilePos> for ChunkPos {
    fn from(p: TilePos) -> Self {
        ChunkPos {
            x: p.x.div_euclid(CHUNK_SIZE),
            y: p.y.div_euclid(CHUNK_SIZE),
        }
    }
}

impl ChunkPos {
    pub fn key(self) -> u64 {
        ((self.x as u32 as u64) << 32) | (self.y as u32 as u64)
    }
}

#[inline]
pub fn chunk_origin(c: ChunkPos) -> TilePos {
    TilePos {
        x: c.x * CHUNK_SIZE,
        y: c.y * CHUNK_SIZE,
    }
}

#[inline]
pub fn chunk_index(c: ChunkPos, pos: TilePos) -> Option<usize> {
    let ox = c.x * CHUNK_SIZE;
    let oy = c.y * CHUNK_SIZE;
    if pos.x < ox || pos.x >= ox + CHUNK_SIZE || pos.y < oy || pos.y >= oy + CHUNK_SIZE {
        return None;
    }
    Some(((pos.x - ox) * CHUNK_SIZE + (pos.y - oy)) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_index_handles_negative_tiles() {
        let chunk = ChunkPos::from(TilePos { x: -1, y: -1 });
        assert_eq!(chunk, ChunkPos { x: -1, y: -1 });
        assert_eq!(chunk_index(chunk, TilePos { x: -32, y: -32 }), Some(0));
        assert_eq!(chunk_index(chunk, TilePos { x: -31, y: -32 }), Some(32));
        assert_eq!(chunk_index(chunk, TilePos { x: -32, y: -31 }), Some(1));
        assert_eq!(chunk_index(chunk, TilePos { x: -1, y: -1 }), Some(1023));
        assert_eq!(chunk_index(chunk, TilePos { x: 0, y: 0 }), None);
        assert_eq!(chunk_index(ChunkPos { x: 0, y: 0 }, TilePos { x: 0, y: 0 }), Some(0));
        assert_eq!(chunk_index(ChunkPos { x: 0, y: 0 }, TilePos { x: 30, y: 30 }), Some(990));
    }
}
