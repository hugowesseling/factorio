use std::collections::BTreeSet;

use factorio_sim::{ChunkPos, TilePos, World, CHUNK_SIZE};

pub const DEFAULT_VIEW_RADIUS: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StreamStats {
    pub generated: u32,
    pub released: u32,
    pub resident: u32,
}

#[derive(Clone, Debug)]
pub struct ChunkStreamer {
    view_radius: i32,
    center: ChunkPos,
    known: BTreeSet<(i32, i32)>,
}

impl ChunkStreamer {
    pub fn new(view_radius: i32) -> Self {
        let radius = view_radius.max(0);
        Self { view_radius: radius, center: ChunkPos { x: 0, y: 0 }, known: BTreeSet::new() }
    }

    pub fn view_radius(&self) -> i32 {
        self.view_radius
    }

    pub fn center(&self) -> ChunkPos {
        self.center
    }

    pub fn resident(&self) -> u32 {
        self.known.len() as u32
    }

    pub fn desired(&self, center: ChunkPos) -> BTreeSet<(i32, i32)> {
        let mut wanted = BTreeSet::new();
        for dy in -self.view_radius..=self.view_radius {
            for dx in -self.view_radius..=self.view_radius {
                if dx.abs() + dy.abs() > self.view_radius * 2 {
                    continue;
                }
                wanted.insert((center.x + dx, center.y + dy));
            }
        }
        wanted
    }

    pub fn update(&mut self, world: &mut World, focus: TilePos) -> StreamStats {
        let center = ChunkPos::from(focus);
        self.center = center;
        let wanted = self.desired(center);
        let mut stats = StreamStats::default();
        for (x, y) in wanted.iter() {
            let chunk = ChunkPos { x: *x, y: *y };
            if world.ensure_chunk(chunk) {
                stats.generated += 1;
            }
            self.known.insert((*x, *y));
        }
        let stale: Vec<(i32, i32)> = self
            .known
            .iter()
            .filter(|key| !wanted.contains(*key))
            .copied()
            .collect();
        for (x, y) in stale.iter() {
            let chunk = ChunkPos { x: *x, y: *y };
            if chunk_has_entities(world, chunk) {
                continue;
            }
            world.terrain.remove(&chunk);
            self.known.remove(&(*x, *y));
            stats.released += 1;
        }
        stats.resident = self.known.len() as u32;
        stats
    }
}

pub fn chunk_has_entities(world: &World, chunk: ChunkPos) -> bool {
    let origin = TilePos { x: chunk.x * CHUNK_SIZE, y: chunk.y * CHUNK_SIZE };
    for dy in 0..CHUNK_SIZE {
        for dx in 0..CHUNK_SIZE {
            if world.occupancy.get(origin.add(dx, dy)).is_some() {
                return true;
            }
        }
    }
    false
}

pub fn distance_in_chunks(a: ChunkPos, b: ChunkPos) -> i32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx.abs() + dy.abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desired_set_is_bounded_and_sorted() {
        let streamer = ChunkStreamer::new(2);
        let wanted = streamer.desired(ChunkPos { x: 1, y: 1 });
        assert!(wanted.len() <= 25);
        assert!(wanted.contains(&(1, 1)));
        let keys: Vec<(i32, i32)> = wanted.iter().copied().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn spawn_chunks_are_resident_before_streaming() {
        let world = World::new(3);
        for chunk in [
            ChunkPos { x: 0, y: 0 },
            ChunkPos { x: -1, y: -1 },
            ChunkPos { x: 1, y: 1 },
        ] {
            assert!(world.chunk(chunk).is_some());
        }
        assert!(world.chunk(ChunkPos { x: 2, y: 0 }).is_none());
    }

    #[test]
    fn update_generates_chunks_once() {
        let mut world = World::new(3);
        let mut streamer = ChunkStreamer::new(1);
        let focus = TilePos { x: 96, y: -96 };
        let stats = streamer.update(&mut world, focus);
        assert_eq!(stats.generated, 9);
        assert_eq!(stats.released, 0);
        assert_eq!(stats.resident, 9);
        let again = streamer.update(&mut world, focus);
        assert_eq!(again.generated, 0);
        assert_eq!(again.released, 0);
        assert_eq!(again.resident, 9);
    }

    #[test]
    fn chunks_with_entities_are_not_released() {
        let mut world = World::new(3);
        let mut streamer = ChunkStreamer::new(1);
        streamer.update(&mut world, TilePos { x: 0, y: 0 });
        let mut belt_pos = None;
        for y in 0..CHUNK_SIZE {
            for x in CHUNK_SIZE..CHUNK_SIZE * 2 {
                let pos = TilePos { x, y };
                if !world.terrain_tile(pos).water {
                    belt_pos = Some(pos);
                    break;
                }
            }
            if belt_pos.is_some() {
                break;
            }
        }
        let belt_pos = belt_pos.expect("dry tile in chunk (1, 0)");
        let belt = world.place_belt(belt_pos, 0).unwrap();
        let stats = streamer.update(&mut world, TilePos { x: 400, y: 0 });
        assert_eq!(stats.generated, 9);
        assert_eq!(stats.released, 7);
        assert_eq!(stats.resident, 11);
        assert!(world.chunk(ChunkPos { x: 1, y: 0 }).is_some());
        assert!(world.chunk(ChunkPos { x: 0, y: 0 }).is_some());
        assert_eq!(world.belt(belt).map(|belt| belt.pos), Some(belt_pos));
        assert!(world.chunk(ChunkPos { x: -1, y: 0 }).is_none());
        assert!(world.chunk(ChunkPos { x: 0, y: -1 }).is_none());
    }

    #[test]
    fn streaming_is_deterministic() {
        let mut left = World::new(17);
        let mut right = World::new(17);
        let mut a = ChunkStreamer::new(2);
        let mut b = ChunkStreamer::new(2);
        for step in 0..5 {
            let focus = TilePos { x: step * 40, y: step * -20 };
            a.update(&mut left, focus);
            b.update(&mut right, focus);
        }
        let mut keys_a = a.known.iter().copied().collect::<Vec<_>>();
        let mut keys_b = b.known.iter().copied().collect::<Vec<_>>();
        keys_a.sort();
        keys_b.sort();
        assert_eq!(keys_a, keys_b);
        for key in keys_a {
            let chunk = ChunkPos { x: key.0, y: key.1 };
            assert_eq!(left.chunk(chunk).unwrap().ore_amount, right.chunk(chunk).unwrap().ore_amount);
        }
    }
}