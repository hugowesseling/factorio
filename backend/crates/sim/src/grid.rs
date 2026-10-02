use std::collections::HashMap;

use crate::coords::TilePos;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityKind {
    Belt,
    Machine,
    Inserter,
    Player,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Occupant {
    pub kind: EntityKind,
    pub index: u32,
}

#[derive(Debug, Default)]
pub struct OccupancyIndex {
    by_tile: HashMap<u64, Occupant>,
}

impl OccupancyIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, pos: TilePos) -> Option<Occupant> {
        self.by_tile.get(&pos.key()).copied()
    }

    pub fn set(&mut self, pos: TilePos, occ: Occupant) {
        self.by_tile.insert(pos.key(), occ);
    }

    pub fn remove(&mut self, pos: TilePos) {
        self.by_tile.remove(&pos.key());
    }

    pub fn is_free(&self, pos: TilePos, size: (i32, i32)) -> bool {
        for dy in 0..size.1 {
            for dx in 0..size.0 {
                if self.by_tile.contains_key(&pos.add(dx, dy).key()) {
                    return false;
                }
            }
        }
        true
    }

    pub fn register(&mut self, pos: TilePos, size: (i32, i32), occ: Occupant) {
        for dy in 0..size.1 {
            for dx in 0..size.0 {
                self.set(pos.add(dx, dy), occ);
            }
        }
    }

    pub fn unregister(&mut self, pos: TilePos, size: (i32, i32)) {
        for dy in 0..size.1 {
            for dx in 0..size.0 {
                self.remove(pos.add(dx, dy));
            }
        }
    }

    pub fn iter_sorted(&self) -> Vec<(u64, Occupant)> {
        let mut out: Vec<(u64, Occupant)> = self.by_tile.iter().map(|(k, v)| (*k, *v)).collect();
        out.sort_by_key(|(k, _)| *k);
        out
    }

    pub fn len(&self) -> usize {
        self.by_tile.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_tile.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_tile_registration() {
        let mut index = OccupancyIndex::new();
        index.register(TilePos { x: 0, y: 0 }, (2, 2), Occupant { kind: EntityKind::Machine, index: 1 });
        assert!(!index.is_free(TilePos { x: 1, y: 1 }, (1, 1)));
        assert!(index.is_free(TilePos { x: 2, y: 2 }, (1, 1)));
        index.unregister(TilePos { x: 0, y: 0 }, (2, 2));
        assert!(index.is_free(TilePos { x: 1, y: 1 }, (1, 1)));
    }
}
