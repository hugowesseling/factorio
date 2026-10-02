use std::collections::HashMap;
use std::io;
use std::path::Path;

use factorio_content::Content;
use factorio_sim::{
    ChunkPos, EntityKind, Fluid, FluidTank, Inserter, Inventory, Machine, OccupancyIndex, Occupant,
    Pcg32, Player, Pool, Stack, TerrainChunk, TilePos, TransportBelt, World, CHUNK_AREA,
};

pub const SAVE_VERSION: u32 = 1;
pub const SAVE_MAGIC: [u8; 4] = *b"FSV1";

#[derive(Clone, Debug)]
pub struct ChunkSave {
    pub chunk: ChunkPos,
    pub resource: Box<[u8; CHUNK_AREA]>,
    pub ore: Box<[u16; CHUNK_AREA]>,
    pub flags: Box<[u16; CHUNK_AREA]>,
}

#[derive(Clone, Debug)]
pub struct PlayerSave {
    pub pos: TilePos,
    pub dir: u8,
    pub health: u32,
    pub target: Option<TilePos>,
    pub items: Vec<(u16, u16)>,
}

#[derive(Clone, Debug)]
pub struct SaveFile {
    pub version: u32,
    pub tick: u64,
    pub seed: u64,
    pub rng_state: u64,
    pub rng_inc: u64,
    pub chunks: Vec<ChunkSave>,
    pub belts: Vec<TransportBelt>,
    pub machines: Vec<Machine>,
    pub inserters: Vec<Inserter>,
    pub players: Vec<PlayerSave>,
    pub research: Vec<(u16, u32)>,
    pub power_produced: u32,
    pub power_consumed: u32,
}

impl SaveFile {
    pub fn capture(world: &World) -> Self {
        let mut chunks = Vec::with_capacity(world.terrain.len());
        for chunk in world.chunk_keys() {
            if let Some(data) = world.terrain.get(&chunk) {
                chunks.push(ChunkSave {
                    chunk,
                    resource: data.resource.clone(),
                    ore: data.ore_amount.clone(),
                    flags: data.flags.clone(),
                });
            }
        }
        SaveFile {
            version: SAVE_VERSION,
            tick: world.tick,
            seed: world.seed,
            rng_state: world.rng.state,
            rng_inc: world.rng.inc,
            chunks,
            belts: world.belts.iter().map(|(_, belt)| *belt).collect(),
            machines: world.machines.iter().map(|(_, machine)| *machine).collect(),
            inserters: world.inserters.iter().map(|(_, inserter)| *inserter).collect(),
            players: world
                .players
                .iter()
                .map(|(_, player)| PlayerSave {
                    pos: player.pos,
                    dir: player.dir,
                    health: player.health,
                    target: player.target,
                    items: player.inventory.iter().collect(),
                })
                .collect(),
            research: world.research.clone(),
            power_produced: world.power_produced,
            power_consumed: world.power_consumed,
        }
    }

    pub fn restore(self) -> World {
        let content = Content::load();
        let mut terrain = HashMap::new();
        for chunk in self.chunks {
            terrain.insert(
                chunk.chunk,
                TerrainChunk { resource: chunk.resource, ore_amount: chunk.ore, flags: chunk.flags },
            );
        }
        let mut occupancy = OccupancyIndex::default();
        let mut belts: Pool<TransportBelt> = Pool::new();
        for belt in self.belts {
            let pos = belt.pos;
            let index = belts.alloc(belt);
            occupancy.register(pos, (1, 1), Occupant { kind: EntityKind::Belt, index });
        }
        let mut machines: Pool<Machine> = Pool::new();
        for machine in self.machines {
            let pos = machine.pos;
            let size = machine.size_tuple();
            let index = machines.alloc(machine);
            occupancy.register(pos, size, Occupant { kind: EntityKind::Machine, index });
        }
        let mut inserters: Pool<Inserter> = Pool::new();
        for inserter in self.inserters {
            let pos = inserter.pos;
            let index = inserters.alloc(inserter);
            occupancy.register(pos, (1, 1), Occupant { kind: EntityKind::Inserter, index });
        }
        let mut players: Pool<Player> = Pool::new();
        for save in self.players {
            let mut player = Player::new(save.pos);
            player.dir = save.dir;
            player.health = save.health;
            player.target = save.target;
            let mut inventory = Inventory::new();
            for (item, count) in save.items {
                inventory.add(item, count);
            }
            player.inventory = inventory;
            let index = players.alloc(player);
            occupancy.register(save.pos, (1, 1), Occupant { kind: EntityKind::Player, index });
        }
        let mut world = World {
            content,
            tick: self.tick,
            seed: self.seed,
            rng: Pcg32 { state: self.rng_state, inc: self.rng_inc },
            terrain,
            occupancy,
            belts,
            machines,
            inserters,
            players,
            enemies: Pool::new(),
            projectiles: Pool::new(),
            research: self.research,
            commands: Vec::new(),
            events: Vec::new(),
            power_produced: self.power_produced,
            power_consumed: self.power_consumed,
            state_hash: 0,
        };
        world.recompute_state_hash();
        world
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Buf::new();
        writer.raw(&SAVE_MAGIC);
        writer.u32(self.version);
        writer.u64(self.tick);
        writer.u64(self.seed);
        writer.u64(self.rng_state);
        writer.u64(self.rng_inc);
        writer.u32(self.chunks.len() as u32);
        for chunk in self.chunks.iter() {
            writer.i32(chunk.chunk.x);
            writer.i32(chunk.chunk.y);
            writer.bytes_of(&chunk.resource[..]);
            writer.u16s(&chunk.ore);
            writer.u16s(&chunk.flags);
        }
        writer.u32(self.belts.len() as u32);
        for belt in self.belts.iter() {
            writer.i32(belt.pos.x);
            writer.i32(belt.pos.y);
            writer.u8(belt.flags);
            writer.u32(belt.next);
            writer.u32(belt.prev);
            writer.u32(belt.progress);
            writer.stacks(&belt.items);
        }
        writer.u32(self.machines.len() as u32);
        for machine in self.machines.iter() {
            writer.u8(machine.kind);
            writer.i32(machine.pos.x);
            writer.i32(machine.pos.y);
            writer.u8(machine.size);
            writer.u16(machine.recipe);
            writer.u32(machine.progress);
            writer.u32(machine.fuel);
            writer.u32(machine.power);
            writer.stacks(&machine.in_buf);
            writer.stacks(&machine.out_buf);
            writer.u32(machine.tank.capacity);
            match machine.tank.fluid {
                Some(fluid) => {
                    writer.u8(1);
                    writer.u8(fluid.kind);
                    writer.u32(fluid.amount);
                }
                None => writer.u8(0),
            }
        }
        writer.u32(self.inserters.len() as u32);
        for inserter in self.inserters.iter() {
            writer.i32(inserter.pos.x);
            writer.i32(inserter.pos.y);
            writer.u8(inserter.dir);
            writer.any_id(inserter.source);
            writer.any_id(inserter.target);
            writer.stack(inserter.held);
            writer.u32(inserter.progress);
        }
        writer.u32(self.players.len() as u32);
        for player in self.players.iter() {
            writer.i32(player.pos.x);
            writer.i32(player.pos.y);
            writer.u8(player.dir);
            writer.u32(player.health);
            writer.tile(player.target);
            writer.u32(player.items.len() as u32);
            for (item, count) in player.items.iter() {
                writer.u16(*item);
                writer.u16(*count);
            }
        }
        writer.u32(self.research.len() as u32);
        for (tech, progress) in self.research.iter() {
            writer.u16(*tech);
            writer.u32(*progress);
        }
        writer.u32(self.power_produced);
        writer.u32(self.power_consumed);
        writer.into_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Cursor::new(bytes);
        if reader.take(4)? != SAVE_MAGIC {
            return Err("bad save magic".to_string());
        }
        let version = reader.u32()?;
        if version != SAVE_VERSION {
            return Err(format!("unsupported save version {version}"));
        }
        let tick = reader.u64()?;
        let seed = reader.u64()?;
        let rng_state = reader.u64()?;
        let rng_inc = reader.u64()?;
        let chunk_count = reader.u32()? as usize;
        let mut chunks = Vec::with_capacity(chunk_count);
        for _ in 0..chunk_count {
            let x = reader.i32()?;
            let y = reader.i32()?;
            let mut resource = Box::new([0u8; CHUNK_AREA]);
            resource.copy_from_slice(reader.take(CHUNK_AREA)?);
            chunks.push(ChunkSave {
                chunk: ChunkPos { x, y },
                resource,
                ore: reader.u16_array()?,
                flags: reader.u16_array()?,
            });
        }
        let belt_count = reader.u32()? as usize;
        let mut belts = Vec::with_capacity(belt_count);
        for _ in 0..belt_count {
            let pos = TilePos { x: reader.i32()?, y: reader.i32()? };
            let flags = reader.u8()?;
            let next = reader.u32()?;
            let prev = reader.u32()?;
            let progress = reader.u32()?;
            let items = reader.stacks()?;
            let mut belt = TransportBelt::new(pos, flags);
            belt.next = next;
            belt.prev = prev;
            belt.progress = progress;
            belt.items = items;
            belts.push(belt);
        }
        let machine_count = reader.u32()? as usize;
        let mut machines = Vec::with_capacity(machine_count);
        for _ in 0..machine_count {
            let kind = reader.u8()?;
            let pos = TilePos { x: reader.i32()?, y: reader.i32()? };
            let size = reader.u8()?;
            let recipe = reader.u16()?;
            let progress = reader.u32()?;
            let fuel = reader.u32()?;
            let power = reader.u32()?;
            let in_buf = reader.stacks()?;
            let out_buf = reader.stacks()?;
            let capacity = reader.u32()?;
            let has_fluid = reader.u8()?;
            let tank = if has_fluid == 1 {
                let kind = reader.u8()?;
                let amount = reader.u32()?;
                FluidTank { fluid: Some(Fluid::new(kind, amount)), capacity }
            } else {
                FluidTank { fluid: None, capacity }
            };
            let mut machine = Machine::new(kind, pos, size, recipe);
            machine.progress = progress;
            machine.fuel = fuel;
            machine.power = power;
            machine.in_buf = in_buf;
            machine.out_buf = out_buf;
            machine.tank = tank;
            machines.push(machine);
        }
        let inserter_count = reader.u32()? as usize;
        let mut inserters = Vec::with_capacity(inserter_count);
        for _ in 0..inserter_count {
            let pos = TilePos { x: reader.i32()?, y: reader.i32()? };
            let dir = reader.u8()?;
            let source = reader.any_id()?;
            let target = reader.any_id()?;
            let held = reader.stack()?;
            let progress = reader.u32()?;
            let mut inserter = Inserter::new(pos, dir);
            inserter.source = source;
            inserter.target = target;
            inserter.held = held;
            inserter.progress = progress;
            inserters.push(inserter);
        }
        let player_count = reader.u32()? as usize;
        let mut players = Vec::with_capacity(player_count);
        for _ in 0..player_count {
            let pos = TilePos { x: reader.i32()?, y: reader.i32()? };
            let dir = reader.u8()?;
            let health = reader.u32()?;
            let target = reader.tile()?;
            let item_count = reader.u32()? as usize;
            let mut items = Vec::with_capacity(item_count);
            for _ in 0..item_count {
                let item = reader.u16()?;
                let count = reader.u16()?;
                items.push((item, count));
            }
            players.push(PlayerSave { pos, dir, health, target, items });
        }
        let research_count = reader.u32()? as usize;
        let mut research = Vec::with_capacity(research_count);
        for _ in 0..research_count {
            let tech = reader.u16()?;
            let progress = reader.u32()?;
            research.push((tech, progress));
        }
        let power_produced = reader.u32()?;
        let power_consumed = reader.u32()?;
        Ok(SaveFile {
            version,
            tick,
            seed,
            rng_state,
            rng_inc,
            chunks,
            belts,
            machines,
            inserters,
            players,
            research,
            power_produced,
            power_consumed,
        })
    }

    pub fn write_to(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }

    pub fn read_from(path: &Path) -> io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes).map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))
    }
}

struct Buf {
    bytes: Vec<u8>,
}

impl Buf {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn raw(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn bytes_of(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
    }

    fn u16s(&mut self, values: &[u16; CHUNK_AREA]) {
        for value in values.iter() {
            self.u16(*value);
        }
    }

    fn stack(&mut self, stack: Option<Stack>) {
        match stack {
            Some(stack) => {
                self.u8(1);
                self.u16(stack.item);
                self.u16(stack.count);
            }
            None => self.u8(0),
        }
    }

    fn stacks(&mut self, stacks: &[Stack; 4]) {
        for stack in stacks.iter() {
            self.u16(stack.item);
            self.u16(stack.count);
        }
    }

    fn any_id(&mut self, id: Option<factorio_sim::AnyId>) {
        match id {
            Some(id) => {
                self.u8(1);
                self.u8(id.kind());
                self.u32(id.index());
            }
            None => self.u8(0),
        }
    }

    fn tile(&mut self, pos: Option<TilePos>) {
        match pos {
            Some(pos) => {
                self.u8(1);
                self.i32(pos.x);
                self.i32(pos.y);
            }
            None => self.u8(0),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        if self.pos + length > self.bytes.len() {
            return Err("unexpected end of save".to_string());
        }
        let slice = &self.bytes[self.pos..self.pos + length];
        self.pos += length;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let bytes = self.take(8)?;
        let mut value = [0u8; 8];
        value.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(value))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }

    fn u16_array(&mut self) -> Result<Box<[u16; CHUNK_AREA]>, String> {
        let bytes = self.take(CHUNK_AREA * 2)?;
        let mut out = Box::new([0u16; CHUNK_AREA]);
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]]);
        }
        Ok(out)
    }

    fn stacks(&mut self) -> Result<[Stack; 4], String> {
        let mut out = [Stack::default(); 4];
        for slot in out.iter_mut() {
            slot.item = self.u16()?;
            slot.count = self.u16()?;
        }
        Ok(out)
    }

    fn stack(&mut self) -> Result<Option<Stack>, String> {
        if self.u8()? == 0 {
            return Ok(None);
        }
        let item = self.u16()?;
        let count = self.u16()?;
        Ok(Some(Stack::new(item, count)))
    }

    fn any_id(&mut self) -> Result<Option<factorio_sim::AnyId>, String> {
        if self.u8()? == 0 {
            return Ok(None);
        }
        let kind = self.u8()?;
        let index = self.u32()?;
        Ok(Some(match kind {
            0 => factorio_sim::AnyId::Belt(factorio_sim::BeltId(index)),
            1 => factorio_sim::AnyId::Machine(factorio_sim::MachineId(index)),
            2 => factorio_sim::AnyId::Inserter(factorio_sim::InserterId(index)),
            _ => factorio_sim::AnyId::Player(factorio_sim::PlayerId(index)),
        }))
    }

    fn tile(&mut self) -> Result<Option<TilePos>, String> {
        if self.u8()? == 0 {
            return Ok(None);
        }
        let x = self.i32()?;
        let y = self.i32()?;
        Ok(Some(TilePos { x, y }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factorio_sim::{Command, DIR_EAST, MACHINE_ASSEMBLER, MACHINE_BURNER_GENERATOR, MACHINE_STONE_FURNACE};

    fn scripted_world(seed: u64) -> World {
        let mut world = World::new(seed);
        world.queue(Command::PlaceBelt { player: 0, x: 4, y: 4, dir: DIR_EAST });
        world.queue(Command::PlaceBelt { player: 0, x: 5, y: 4, dir: DIR_EAST });
        world.queue(Command::PlaceInserter { player: 0, x: 6, y: 4, dir: DIR_EAST });
        world.queue(Command::PlaceMachine { player: 0, x: 7, y: 4, kind: MACHINE_STONE_FURNACE });
        world.queue(Command::PlaceMachine { player: 0, x: 12, y: 4, kind: MACHINE_BURNER_GENERATOR });
        world.place_machine(TilePos { x: 20, y: 4 }, MACHINE_ASSEMBLER);
        let coal = world.item_id("coal");
        world.player_mut(0).unwrap().inventory.add(coal, 17);
        for _ in 0..90 {
            world.tick();
        }
        world
    }

    #[test]
    fn round_trip_preserves_state_hash() {
        let world = scripted_world(555);
        let save = SaveFile::capture(&world);
        let restored = save.restore();
        assert_eq!(restored.tick, world.tick);
        assert_eq!(restored.state_hash, world.state_hash);
        assert_eq!(restored.seed, world.seed);
        assert_eq!(restored.power_produced, world.power_produced);
    }

    #[test]
    fn byte_round_trip_preserves_state_hash() {
        let world = scripted_world(556);
        let bytes = SaveFile::capture(&world).to_bytes();
        let parsed = SaveFile::from_bytes(&bytes).unwrap();
        let restored = parsed.restore();
        assert_eq!(restored.state_hash, world.state_hash);
        assert_eq!(restored.terrain.len(), world.terrain.len());
        assert_eq!(restored.belts.len(), world.belts.len());
        assert_eq!(restored.inserters.len(), world.inserters.len());
    }

    #[test]
    fn restored_world_keeps_ticking_identically() {
        let world = scripted_world(557);
        let mut restored = SaveFile::capture(&world).restore();
        let mut original = scripted_world(557);
        for _ in 0..45 {
            restored.tick();
            original.tick();
        }
        assert_eq!(restored.state_hash, original.state_hash);
    }

    #[test]
    fn occupancy_is_rebuilt_on_restore() {
        let world = scripted_world(558);
        let restored = SaveFile::capture(&world).restore();
        assert_eq!(restored.occupancy.len(), world.occupancy.len());
        for (index, machine) in restored.machines.iter() {
            let occupant = restored.occupancy.get(machine.pos).unwrap();
            assert_eq!(occupant.index, index);
            assert_eq!(occupant.kind, EntityKind::Machine);
        }
    }

    #[test]
    fn bad_saves_are_rejected() {
        assert!(SaveFile::from_bytes(&[]).is_err());
        assert!(SaveFile::from_bytes(b"NOPE").is_err());
        let mut bytes = SaveFile::capture(&World::new(1)).to_bytes();
        bytes[4] = 9;
        assert!(SaveFile::from_bytes(&bytes).is_err());
    }

    #[test]
    fn truncated_saves_are_rejected() {
        let world = scripted_world(559);
        let bytes = SaveFile::capture(&world).to_bytes();
        assert!(SaveFile::from_bytes(&bytes[..bytes.len() / 2]).is_err());
    }
}