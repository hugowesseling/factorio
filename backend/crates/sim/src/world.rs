use std::collections::HashMap;

use factorio_content::Content;

use crate::belt::{TransportBelt, DIR_MASK};
use crate::combat::{Enemy, Projectile};
use crate::commands::Command;
use crate::coords::{chunk_origin, ChunkPos, TilePos, CHUNK_AREA, CHUNK_SIZE};
use crate::entity::{
    stack_add, stack_any, stack_contains, stack_take, AnyId, BeltId, InserterId, MachineId, PlayerId,
    Pool, Stack, SENTINEL,
};
use crate::events::Event;
use crate::fluid::{Fluid, FLUID_WATER};
use crate::grid::{EntityKind, OccupancyIndex, Occupant};
use crate::machine::{
    Machine, MACHINE_ASSEMBLER, MACHINE_BURNER_GENERATOR, MACHINE_LAB, MACHINE_OFFSHORE_PUMP,
    MACHINE_STONE_FURNACE,
};
use crate::player::Player;
use crate::power::{
    machine_is_consumer, machine_is_producer, machine_power_required, machine_produced,
    satisfaction, POWER_RATIO_FULL,
};
use crate::rng::{fnv_update, Pcg32, FNV_OFFSET};
use crate::terrain::{Resource, TerrainChunk, TileData};
use crate::tick::Phase;
use crate::{Inserter, DIR_EAST, DIR_NORTH, DIR_SOUTH, DIR_WEST};

pub const TICKS_PER_SECOND: u64 = 60;
pub const FUEL_TICKS: u32 = 50;
pub const WATER_PER_EXTRACT: u32 = 100;
pub const WATER_PER_ITEM: u32 = 100;
pub const LAB_TICKS_PER_PACK: u32 = 30;
pub const RESEARCH_PER_PACK: u32 = 10;
pub const DEFAULT_BELT: u16 = 0;
pub const DEFAULT_INSERTER: u16 = 0;

#[derive(Clone, Debug)]
pub struct TickOutcome {
    pub tick: u64,
    pub state_hash: u64,
    pub events: Vec<Event>,
}

pub struct World {
    pub content: Content,
    pub tick: u64,
    pub seed: u64,
    pub rng: Pcg32,
    pub terrain: HashMap<ChunkPos, TerrainChunk>,
    pub occupancy: OccupancyIndex,
    pub belts: Pool<TransportBelt>,
    pub machines: Pool<Machine>,
    pub inserters: Pool<Inserter>,
    pub players: Pool<Player>,
    pub enemies: Pool<Enemy>,
    pub projectiles: Pool<Projectile>,
    pub research: Vec<(u16, u32)>,
    pub commands: Vec<Command>,
    pub events: Vec<Event>,
    pub power_produced: u32,
    pub power_consumed: u32,
    pub state_hash: u64,
}

impl World {
    pub fn new(seed: u64) -> Self {
        let content = Content::load();
        let mut terrain = HashMap::new();
        for chunk_x in -1..=1 {
            for chunk_y in -1..=1 {
                let chunk = ChunkPos { x: chunk_x, y: chunk_y };
                terrain.insert(chunk, generate_chunk(seed, chunk));
            }
        }
        let spawn = TilePos { x: 0, y: 0 };
        clear_area(&mut terrain, spawn, 3);
        let mut occupancy = OccupancyIndex::default();
        let mut players = Pool::new();
        let player = players.alloc(Player::new(spawn));
        occupancy.register(
            spawn,
            (1, 1),
            Occupant { kind: EntityKind::Player, index: player },
        );
        let mut world = World {
            content,
            tick: 0,
            seed,
            rng: Pcg32::new(seed),
            terrain,
            occupancy,
            belts: Pool::new(),
            machines: Pool::new(),
            inserters: Pool::new(),
            players,
            enemies: Pool::new(),
            projectiles: Pool::new(),
            research: Vec::new(),
            commands: Vec::new(),
            events: Vec::new(),
            power_produced: 0,
            power_consumed: 0,
            state_hash: FNV_OFFSET,
        };
        world.recompute_state_hash();
        world
    }

    pub fn ensure_chunk(&mut self, chunk: ChunkPos) -> bool {
        if self.terrain.contains_key(&chunk) {
            return false;
        }
        self.terrain.insert(chunk, generate_chunk(self.seed, chunk));
        true
    }

    pub fn chunk(&self, chunk: ChunkPos) -> Option<&TerrainChunk> {
        self.terrain.get(&chunk)
    }

    pub fn chunk_keys(&self) -> Vec<ChunkPos> {
        let mut keys: Vec<ChunkPos> = self.terrain.keys().copied().collect();
        keys.sort();
        keys
    }

    pub fn terrain_tile(&self, pos: TilePos) -> TileData {
        let chunk = ChunkPos::from(pos);
        self.terrain
            .get(&chunk)
            .and_then(|data| data.get(chunk, pos))
            .unwrap_or(TileData { resource: 0, ore: 0, water: false })
    }

    pub fn edit_tile<F: FnOnce(&mut TerrainChunk, ChunkPos)>(&mut self, pos: TilePos, edit: F) {
        let chunk = ChunkPos::from(pos);
        if let Some(data) = self.terrain.get_mut(&chunk) {
            edit(data, chunk);
        }
    }

    pub fn is_passable(&self, pos: TilePos) -> bool {
        self.occupancy.get(pos).is_none() && !self.terrain_tile(pos).water
    }

    pub fn can_place(&self, pos: TilePos, size: (i32, i32)) -> bool {
        if !self.occupancy.is_free(pos, size) {
            return false;
        }
        for dy in 0..size.1 {
            for dx in 0..size.0 {
                if self.terrain_tile(pos.add(dx, dy)).water {
                    return false;
                }
            }
        }
        true
    }

    pub fn can_place_machine(&self, pos: TilePos, kind: u8, size: (i32, i32)) -> bool {
        if !self.occupancy.is_free(pos, size) {
            return false;
        }
        if kind == MACHINE_OFFSHORE_PUMP {
            return true;
        }
        self.can_place(pos, size)
    }

    pub fn tick(&mut self) {
        for phase in crate::tick::PHASES {
            self.run_phase(phase);
        }
    }

    fn run_phase(&mut self, phase: Phase) {
        match phase {
            Phase::InputDrain => self.phase_input_drain(),
            Phase::Movement => self.phase_movement(),
            Phase::Belts => self.phase_belts(),
            Phase::Inserters => self.phase_inserters(),
            Phase::Machines => self.phase_machines(),
            Phase::Fluids => self.phase_fluids(),
            Phase::Power => self.phase_power(),
            Phase::Combat => self.phase_combat(),
            Phase::TickEvents => self.phase_tick_events(),
        }
    }

    fn phase_input_drain(&mut self) {
        let queued = std::mem::take(&mut self.commands);
        for command in queued {
            self.execute(command);
        }
    }

    fn phase_movement(&mut self) {
        let count = self.players.len();
        for index in 0..count {
            let (pos, target) = match self.players.get(index) {
                Some(player) => (player.pos, player.target),
                None => continue,
            };
            let target = match target {
                Some(target) => target,
                None => continue,
            };
            if pos == target {
                if let Some(player) = self.players.get_mut(index) {
                    player.target = None;
                }
                continue;
            }
            let step_x = sign_to(target.x - pos.x);
            let step_y = sign_to(target.y - pos.y);
            let next = pos.add(step_x, step_y);
            if !self.is_passable(next) {
                if let Some(player) = self.players.get_mut(index) {
                    player.target = None;
                }
                continue;
            }
            if let Some(player) = self.players.get_mut(index) {
                player.pos = next;
                player.dir = direction_of(step_x, step_y);
                player.target = if next == target { None } else { Some(target) };
            }
            self.occupancy.unregister(pos, (1, 1));
            self.occupancy.register(
                next,
                (1, 1),
                Occupant { kind: EntityKind::Player, index },
            );
            self.events.push(Event::PlayerMoved { player: index, x: next.x, y: next.y });
        }
        let count = self.projectiles.len();
        for index in 0..count {
            if let Some(projectile) = self.projectiles.get_mut(index) {
                projectile.pos = projectile.pos.add(projectile.dx, projectile.dy);
            }
        }
    }

    fn phase_belts(&mut self) {
        let speed = self.content.belt_speed().max(1);
        let count = self.belts.len();
        for index in 0..count {
            let ready = match self.belts.get_mut(index) {
                Some(belt) => {
                    belt.progress = belt.progress.saturating_add(1);
                    if belt.progress >= speed {
                        belt.progress = 0;
                        true
                    } else {
                        false
                    }
                }
                None => continue,
            };
            if !ready {
                continue;
            }
            let next = match self.belts.get(index) {
                Some(belt) => belt.next,
                None => continue,
            };
            if next == SENTINEL {
                continue;
            }
            self.transfer_belt_item(index, next);
        }
    }

    fn transfer_belt_item(&mut self, from: u32, to: u32) {
        let taken = match self.belts.get_mut(from) {
            Some(belt) => stack_any(&mut belt.items),
            None => None,
        };
        let stack = match taken {
            Some(stack) => stack,
            None => return,
        };
        let added = match self.belts.get_mut(to) {
            Some(belt) => stack_add(&mut belt.items, stack.item, stack.count),
            None => 0,
        };
        if added == 0 {
            if let Some(belt) = self.belts.get_mut(from) {
                stack_add(&mut belt.items, stack.item, stack.count);
            }
        }
    }

    fn phase_inserters(&mut self) {
        let count = self.inserters.len();
        for index in 0..count {
            self.tick_inserter(index);
        }
    }

    fn tick_inserter(&mut self, index: u32) {
        let (pos, dir, swing) = match self.inserters.get(index) {
            Some(inserter) => (inserter.pos, inserter.dir & DIR_MASK, self.content.inserter_swing(DEFAULT_INSERTER).max(1)),
            None => return,
        };
        let (step_x, step_y) = dir_vector(dir);
        let source = self
            .occupancy
            .get(pos.add(-step_x, -step_y))
            .and_then(|occ| self.any_id(occ));
        let target = self
            .occupancy
            .get(pos.add(step_x, step_y))
            .and_then(|occ| self.any_id(occ));
        let ready = match self.inserters.get_mut(index) {
            Some(inserter) => {
                inserter.source = source;
                inserter.target = target;
                inserter.progress = inserter.progress.saturating_add(1);
                if inserter.progress >= swing {
                    inserter.progress = 0;
                    true
                } else {
                    false
                }
            }
            None => return,
        };
        if !ready {
            return;
        }
        let held = self.inserters.get(index).and_then(|inserter| inserter.held);
        match held {
            None => {
                let source = match source {
                    Some(source) => source,
                    None => return,
                };
                let stack = take_from_source(source, &mut self.belts, &mut self.machines, &mut self.players);
                if let Some(stack) = stack {
                    if let Some(inserter) = self.inserters.get_mut(index) {
                        inserter.held = Some(stack);
                    }
                }
            }
            Some(stack) => {
                let target = match target {
                    Some(target) => target,
                    None => return,
                };
                if put_into_target(target, stack, &mut self.belts, &mut self.machines, &mut self.players) {
                    if let Some(inserter) = self.inserters.get_mut(index) {
                        inserter.held = None;
                    }
                }
            }
        }
    }

    fn phase_machines(&mut self) {
        let count = self.machines.len();
        for index in 0..count {
            let kind = match self.machines.get(index) {
                Some(machine) => machine.kind,
                None => continue,
            };
            match kind {
                MACHINE_BURNER_GENERATOR => self.tick_burner_generator(index),
                MACHINE_STONE_FURNACE | MACHINE_ASSEMBLER => self.tick_crafting_machine(index),
                MACHINE_OFFSHORE_PUMP => self.tick_offshore_pump(index),
                MACHINE_LAB => self.tick_lab(index),
                _ => {}
            }
        }
    }

    fn tick_burner_generator(&mut self, index: u32) {
        let fueled = self.consume_fuel(index);
        let produced = machine_produced(&self.content, MACHINE_BURNER_GENERATOR);
        if let Some(machine) = self.machines.get_mut(index) {
            machine.power = if fueled { produced } else { 0 };
        }
    }

    fn consume_fuel(&mut self, index: u32) -> bool {
        let coal = self.content.resource_items[3];
        let machine = match self.machines.get_mut(index) {
            Some(machine) => machine,
            None => return false,
        };
        if machine.fuel > 0 {
            machine.fuel -= 1;
            return true;
        }
        if stack_take(&mut machine.in_buf, coal, 1) > 0 {
            machine.fuel = FUEL_TICKS;
            true
        } else {
            false
        }
    }

    fn tick_crafting_machine(&mut self, index: u32) {
        let (kind, recipe_index, powered) = match self.machines.get(index) {
            Some(machine) => (machine.kind, machine.recipe, machine.power >= POWER_RATIO_FULL),
            None => return,
        };
        if kind == MACHINE_ASSEMBLER && !powered {
            return;
        }
        if kind == MACHINE_STONE_FURNACE && !self.consume_fuel(index) {
            return;
        }
        if recipe_index == u16::MAX {
            return;
        }
        let recipe = match self.content.recipe(recipe_index) {
            Some(recipe) => recipe,
            None => return,
        };
        let machine = match self.machines.get_mut(index) {
            Some(machine) => machine,
            None => return,
        };
        let time = recipe.time_ticks.max(1);
        if machine.progress == 0 {
            for &(item, count) in recipe.inputs.iter() {
                if stack_contains(&machine.in_buf, item) < count {
                    return;
                }
            }
            for &(item, count) in recipe.inputs.iter() {
                stack_take(&mut machine.in_buf, item, count);
            }
            machine.progress = 1;
            return;
        }
        machine.progress = machine.progress.saturating_add(1);
        if machine.progress < time {
            return;
        }
        for &(item, count) in recipe.outputs.iter() {
            if !stack_has_space(&machine.out_buf, item, count) {
                return;
            }
        }
        for &(item, count) in recipe.outputs.iter() {
            stack_add(&mut machine.out_buf, item, count);
            self.events.push(Event::MachineProduced { machine: index, item, count });
        }
        machine.progress = 0;
    }

    fn tick_offshore_pump(&mut self, index: u32) {
        let pos = match self.machines.get(index) {
            Some(machine) => machine.pos,
            None => return,
        };
        if !self.terrain_tile(pos).water {
            return;
        }
        let mut cleared = false;
        self.edit_tile(pos, |data, chunk| {
            let water = data.get(chunk, pos).map(|tile| tile.water).unwrap_or(false);
            if water {
                data.clear_water(chunk, pos);
                cleared = true;
            }
        });
        if !cleared {
            return;
        }
        if let Some(machine) = self.machines.get_mut(index) {
            machine.tank.add(Fluid::new(FLUID_WATER, WATER_PER_EXTRACT));
        }
        self.events.push(Event::WaterExtracted { machine: index });
    }

    fn tick_lab(&mut self, index: u32) {
        let powered = self
            .machines
            .get(index)
            .map(|machine| machine.power >= POWER_RATIO_FULL)
            .unwrap_or(false);
        if !powered {
            return;
        }
        let ready = match self.machines.get_mut(index) {
            Some(machine) => {
                machine.progress = machine.progress.saturating_add(1);
                if machine.progress >= LAB_TICKS_PER_PACK {
                    machine.progress = 0;
                    true
                } else {
                    false
                }
            }
            None => return,
        };
        if !ready {
            return;
        }
        let packs = self.item_id("science_pack");
        let consumed = match self.machines.get_mut(index) {
            Some(machine) => stack_take(&mut machine.in_buf, packs, 1),
            None => return,
        };
        if consumed == 0 {
            return;
        }
        self.events.push(Event::MachineProduced { machine: index, item: packs, count: consumed });
        self.advance_research(RESEARCH_PER_PACK);
    }

    fn phase_fluids(&mut self) {
        let water = self.item_id("water");
        let count = self.machines.len();
        for index in 0..count {
            let is_pump = self
                .machines
                .get(index)
                .map(|machine| machine.kind == MACHINE_OFFSHORE_PUMP)
                .unwrap_or(false);
            if !is_pump {
                continue;
            }
            let drained = match self.machines.get_mut(index) {
                Some(machine) => machine.tank.take(WATER_PER_ITEM),
                None => None,
            };
            let fluid = match drained {
                Some(fluid) => fluid,
                None => continue,
            };
            if fluid.kind != FLUID_WATER {
                continue;
            }
            let items = (fluid.amount / WATER_PER_ITEM).max(1);
            let added = match self.machines.get_mut(index) {
                Some(machine) => stack_add(&mut machine.out_buf, water, items as u16),
                None => 0,
            };
            let refused = items.saturating_sub(added as u32);
            if refused > 0 {
                if let Some(machine) = self.machines.get_mut(index) {
                    machine.tank.add(Fluid::new(FLUID_WATER, refused * WATER_PER_ITEM));
                }
            }
        }
    }

    fn phase_power(&mut self) {
        let mut produced = 0u32;
        let mut consumed = 0u32;
        let count = self.machines.len();
        for index in 0..count {
            let kind = match self.machines.get(index) {
                Some(machine) => machine.kind,
                None => continue,
            };
            let is_producer = machine_is_producer(&self.content, kind);
            let is_consumer = machine_is_consumer(&self.content, kind);
            if is_producer {
                let output = self.machines.get(index).map(|machine| machine.power).unwrap_or(0);
                produced = produced.saturating_add(output);
            } else if is_consumer {
                consumed = consumed.saturating_add(machine_power_required(&self.content, kind));
            }
        }
        self.power_produced = produced;
        self.power_consumed = consumed;
        let ratio = satisfaction(produced, consumed);
        for index in 0..count {
            let kind = match self.machines.get(index) {
                Some(machine) => machine.kind,
                None => continue,
            };
            if machine_is_producer(&self.content, kind) {
                continue;
            }
            if let Some(machine) = self.machines.get_mut(index) {
                machine.power = ratio;
            }
        }
    }

    fn phase_combat(&mut self) {
        let mut index = 0;
        while index < self.enemies.len() {
            let alive = self
                .enemies
                .get(index)
                .map(|enemy| enemy.health > 0)
                .unwrap_or(false);
            if alive {
                index += 1;
                continue;
            }
            let last = self.enemies.len().saturating_sub(1);
            if index != last {
                let moved = self.enemies.get(last).copied();
                if let Some(moved) = moved {
                    if let Some(enemy) = self.enemies.get_mut(index) {
                        enemy.pos = moved.pos;
                        enemy.health = moved.health;
                    }
                }
            }
            self.enemies.dealloc(index);
        }
    }

    fn phase_tick_events(&mut self) {
        self.tick = self.tick.saturating_add(1);
        self.recompute_state_hash();
        self.events.push(Event::TickCompleted { tick: self.tick });
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            Command::Move { player, dx, dy } => {
                let pos = match self.players.get(player) {
                    Some(player) => player.pos,
                    None => return,
                };
                if let Some(player) = self.players.get_mut(player) {
                    player.target = Some(pos.add(dx, dy));
                }
            }
            Command::PlaceBelt { player: _, x, y, dir } => {
                self.place_belt(TilePos { x, y }, dir);
            }
            Command::PlaceInserter { player: _, x, y, dir } => {
                self.place_inserter(TilePos { x, y }, dir);
            }
            Command::PlaceMachine { player: _, x, y, kind } => {
                self.place_machine(TilePos { x, y }, kind);
            }
            Command::RemoveEntity { player: _, x, y } => {
                self.remove_at(TilePos { x, y });
            }
            Command::MineTile { player, x, y } => {
                self.mine_tile(player, TilePos { x, y }, 1);
            }
            Command::CollectMachine { player, x, y } => {
                self.collect_machine(player, TilePos { x, y });
            }
            Command::ResearchTech { player, tech } => {
                self.research_tech(player, tech);
            }
        }
    }

    pub fn queue(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn place_belt(&mut self, pos: TilePos, dir: u8) -> Option<u32> {
        if !self.can_place(pos, (1, 1)) {
            return None;
        }
        let index = self.belts.alloc(TransportBelt::new(pos, dir));
        self.occupancy.register(
            pos,
            (1, 1),
            Occupant { kind: EntityKind::Belt, index },
        );
        self.rebuild_belt_links();
        self.events
            .push(Event::EntityPlaced { kind: EntityKind::Belt, index, x: pos.x, y: pos.y });
        Some(index)
    }

    pub fn place_inserter(&mut self, pos: TilePos, dir: u8) -> Option<u32> {
        if !self.can_place(pos, (1, 1)) {
            return None;
        }
        let index = self.inserters.alloc(Inserter::new(pos, dir));
        self.occupancy.register(
            pos,
            (1, 1),
            Occupant { kind: EntityKind::Inserter, index },
        );
        self.events
            .push(Event::EntityPlaced { kind: EntityKind::Inserter, index, x: pos.x, y: pos.y });
        Some(index)
    }

    pub fn place_machine(&mut self, pos: TilePos, kind: u8) -> Option<u32> {
        let size = self.content.machine(kind).map(|def| def.size)?;
        if !self.can_place_machine(pos, kind, size) {
            return None;
        }
        let recipe = self.content.machine(kind).map(|def| def.recipe).unwrap_or(u16::MAX);
        let index = self.machines.alloc(Machine::new(kind, pos, size.0 as u8, recipe));
        self.occupancy
            .register(pos, size, Occupant { kind: EntityKind::Machine, index });
        self.events
            .push(Event::EntityPlaced { kind: EntityKind::Machine, index, x: pos.x, y: pos.y });
        Some(index)
    }

    pub fn remove_at(&mut self, pos: TilePos) -> Option<EntityKind> {
        let occupant = self.occupancy.get(pos)?;
        match occupant.kind {
            EntityKind::Belt => {
                self.remove_belt(occupant.index);
                Some(EntityKind::Belt)
            }
            EntityKind::Machine => {
                self.remove_machine(occupant.index);
                Some(EntityKind::Machine)
            }
            EntityKind::Inserter => {
                self.remove_inserter(occupant.index);
                Some(EntityKind::Inserter)
            }
            EntityKind::Player => None,
        }
    }

    fn remove_belt(&mut self, index: u32) {
        let pos = match self.belts.get(index) {
            Some(belt) => belt.pos,
            None => return,
        };
        self.occupancy.unregister(pos, (1, 1));
        let last = self.belts.len().saturating_sub(1);
        if index != last {
            if let Some(moved) = self.belts.get(last) {
                let moved_pos = moved.pos;
                self.occupancy.set(
                    moved_pos,
                    Occupant { kind: EntityKind::Belt, index },
                );
            }
        }
        self.belts.dealloc(index);
        self.rebuild_belt_links();
        self.events.push(Event::EntityRemoved { kind: EntityKind::Belt, index });
    }

    fn remove_machine(&mut self, index: u32) {
        let (pos, size) = match self.machines.get(index) {
            Some(machine) => (machine.pos, machine.size_tuple()),
            None => return,
        };
        self.occupancy.unregister(pos, size);
        let last = self.machines.len().saturating_sub(1);
        if index != last {
            if let Some(moved) = self.machines.get(last) {
                let moved_pos = moved.pos;
                let moved_size = moved.size_tuple();
                self.occupancy.register(
                    moved_pos,
                    moved_size,
                    Occupant { kind: EntityKind::Machine, index },
                );
            }
        }
        self.machines.dealloc(index);
        self.events.push(Event::EntityRemoved { kind: EntityKind::Machine, index });
    }

    fn remove_inserter(&mut self, index: u32) {
        let pos = match self.inserters.get(index) {
            Some(inserter) => inserter.pos,
            None => return,
        };
        self.occupancy.unregister(pos, (1, 1));
        let last = self.inserters.len().saturating_sub(1);
        if index != last {
            if let Some(moved) = self.inserters.get(last) {
                let moved_pos = moved.pos;
                self.occupancy.set(
                    moved_pos,
                    Occupant { kind: EntityKind::Inserter, index },
                );
            }
        }
        self.inserters.dealloc(index);
        self.events.push(Event::EntityRemoved { kind: EntityKind::Inserter, index });
    }

    fn rebuild_belt_links(&mut self) {
        let count = self.belts.len();
        for index in 0..count {
            let next_pos = match self.belts.get(index) {
                Some(belt) => belt.step(),
                None => continue,
            };
            let mut next = SENTINEL;
            if let Some(occupant) = self.occupancy.get(next_pos) {
                if occupant.kind == EntityKind::Belt {
                    next = occupant.index;
                }
            }
            if let Some(belt) = self.belts.get_mut(index) {
                belt.next = next;
                belt.prev = SENTINEL;
            }
        }
        for index in 0..count {
            let next = match self.belts.get(index) {
                Some(belt) => belt.next,
                None => continue,
            };
            if next == SENTINEL {
                continue;
            }
            if let Some(belt) = self.belts.get_mut(next) {
                belt.prev = index;
            }
        }
    }

    pub fn mine_tile(&mut self, player: u32, pos: TilePos, amount: u16) -> Option<(u16, u16)> {
        let near = self
            .players
            .get(player)
            .map(|player| tile_distance(player.pos, pos) <= 1)
            .unwrap_or(false);
        if !near {
            return None;
        }
        let chunk = ChunkPos::from(pos);
        let tile = self.terrain.get(&chunk).and_then(|data| data.get(chunk, pos))?;
        if tile.resource == 0 || tile.ore == 0 {
            return None;
        }
        let resource = Resource::from_index(tile.resource - 1)?;
        let item = *self.content.resource_items.get(resource.index() as usize)?;
        let mined = amount.min(tile.ore);
        if let Some(data) = self.terrain.get_mut(&chunk) {
            data.decrease_ore(chunk, pos, mined);
        }
        if let Some(player) = self.players.get_mut(player) {
            player.inventory.add(item, mined);
        }
        self.events.push(Event::TileMined { x: pos.x, y: pos.y, item, count: mined });
        Some((item, mined))
    }

    pub fn collect_machine(&mut self, player: u32, pos: TilePos) -> u16 {
        let occupant = match self.occupancy.get(pos) {
            Some(occupant) if occupant.kind == EntityKind::Machine => occupant,
            _ => return 0,
        };
        let mut pulled = [(0u16, 0u16); 4];
        if let Some(machine) = self.machines.get_mut(occupant.index) {
            for slot in pulled.iter_mut() {
                match stack_any(&mut machine.out_buf) {
                    Some(stack) => *slot = (stack.item, stack.count),
                    None => break,
                }
            }
        }
        let mut collected = 0u16;
        if let Some(player) = self.players.get_mut(player) {
            for (item, count) in pulled.iter() {
                if *count == 0 {
                    continue;
                }
                player.inventory.add(*item, *count);
                collected = collected.saturating_add(*count);
            }
        }
        collected
    }

    fn any_id(&self, occupant: Occupant) -> Option<AnyId> {
        Some(match occupant.kind {
            EntityKind::Belt => AnyId::Belt(BeltId(occupant.index)),
            EntityKind::Machine => AnyId::Machine(MachineId(occupant.index)),
            EntityKind::Inserter => AnyId::Inserter(InserterId(occupant.index)),
            EntityKind::Player => AnyId::Player(PlayerId(occupant.index)),
        })
    }

    pub fn tech_id(&self, name: &str) -> Option<u16> {
        self.content.technology_index.get(name).copied()
    }

    pub fn research_progress(&self, tech: u16) -> u32 {
        self.research
            .iter()
            .find(|(id, _)| *id == tech)
            .map(|(_, progress)| *progress)
            .unwrap_or(0)
    }

    pub fn research_completed(&self, tech: u16) -> bool {
        let cost = self
            .content
            .technologies
            .get(tech as usize)
            .map(|def| def.cost)
            .unwrap_or(0);
        match self.research.iter().find(|(id, _)| *id == tech) {
            Some((_, progress)) => *progress >= cost,
            None => false,
        }
    }

    pub fn next_researchable_tech(&self) -> Option<u16> {
        for (index, tech) in self.content.technologies.iter().enumerate() {
            let id = index as u16;
            if tech.cost == 0 || self.research_completed(id) {
                continue;
            }
            let ready = tech.prerequisites.iter().all(|name| {
                self.tech_id(name)
                    .map(|prereq| self.research_completed(prereq))
                    .unwrap_or(false)
            });
            if ready {
                return Some(id);
            }
        }
        None
    }

    fn set_research_progress(&mut self, tech: u16, progress: u32) {
        if let Some(entry) = self.research.iter_mut().find(|(id, _)| *id == tech) {
            entry.1 = progress;
            return;
        }
        self.research.push((tech, progress));
        self.research.sort_by_key(|(id, _)| *id);
    }

    fn advance_research(&mut self, amount: u32) {
        let tech = match self.next_researchable_tech() {
            Some(tech) => tech,
            None => return,
        };
        let cost = self
            .content
            .technologies
            .get(tech as usize)
            .map(|def| def.cost)
            .unwrap_or(0);
        let progress = self.research_progress(tech).saturating_add(amount).min(cost);
        self.set_research_progress(tech, progress);
        self.events.push(Event::ResearchProgress { tech, progress });
        if progress >= cost {
            self.events.push(Event::ResearchCompleted { tech });
        }
    }

    pub fn research_tech(&mut self, player: u32, tech: u16) -> bool {
        let (cost, prerequisites) = match self.content.technologies.get(tech as usize) {
            Some(def) => (def.cost, def.prerequisites.clone()),
            None => return false,
        };
        let ready = prerequisites.iter().all(|name| {
            self.tech_id(name)
                .map(|prereq| self.research_completed(prereq))
                .unwrap_or(false)
        });
        if !ready {
            return false;
        }
        if cost > 0 {
            let packs = self.item_id("science_pack");
            let needed = cost.div_ceil(RESEARCH_PER_PACK).min(u16::MAX as u32) as u16;
            let held = self
                .players
                .get(player)
                .map(|player| player.inventory.count(packs))
                .unwrap_or(0);
            if held < needed {
                return false;
            }
            if let Some(player) = self.players.get_mut(player) {
                player.inventory.take(packs, needed);
            }
        }
        self.set_research_progress(tech, cost);
        self.events.push(Event::ResearchCompleted { tech });
        true
    }

    pub fn item_id(&self, name: &str) -> u16 {
        self.content.item_index.get(name).copied().unwrap_or(0)
    }

    pub fn spawn_enemy(&mut self, pos: TilePos, health: u32) -> u32 {
        self.enemies.alloc(Enemy { pos, health })
    }

    pub fn damage_enemy(&mut self, index: u32, damage: u32) {
        if let Some(enemy) = self.enemies.get_mut(index) {
            enemy.health = enemy.health.saturating_sub(damage);
        }
    }

    pub fn spawn_projectile(&mut self, pos: TilePos, dx: i32, dy: i32, damage: u32) -> u32 {
        self.projectiles.alloc(Projectile { pos, dx, dy, damage })
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn player(&self, index: u32) -> Option<&Player> {
        self.players.get(index)
    }

    pub fn player_mut(&mut self, index: u32) -> Option<&mut Player> {
        self.players.get_mut(index)
    }

    pub fn machine(&self, index: u32) -> Option<&Machine> {
        self.machines.get(index)
    }

    pub fn belt(&self, index: u32) -> Option<&TransportBelt> {
        self.belts.get(index)
    }

    pub fn recompute_state_hash(&mut self) {
        let mut hash = FNV_OFFSET;
        hash = fnv_update(hash, self.tick);
        hash = fnv_update(hash, self.seed);
        hash = fnv_update(hash, self.rng.state);
        hash = fnv_update(hash, self.rng.inc);
        for chunk in self.chunk_keys() {
            hash = fnv_update(hash, chunk.key());
            if let Some(data) = self.terrain.get(&chunk) {
                for index in 0..CHUNK_AREA {
                    hash = fnv_update(hash, data.resource[index] as u64);
                    hash = fnv_update(hash, data.ore_amount[index] as u64);
                    hash = fnv_update(hash, data.flags[index] as u64);
                }
            }
        }
        for (key, occupant) in self.occupancy.iter_sorted() {
            hash = fnv_update(hash, key);
            hash = fnv_update(hash, occupant.kind as u64);
            hash = fnv_update(hash, occupant.index as u64);
        }
        for (index, belt) in self.belts.iter() {
            hash = fnv_update(hash, index as u64);
            hash = fnv_update(hash, belt.pos.key());
            hash = fnv_update(hash, belt.flags as u64);
            hash = fnv_update(hash, belt.next as u64);
            hash = fnv_update(hash, belt.prev as u64);
            hash = fnv_update(hash, belt.progress as u64);
            for slot in belt.items.iter() {
                hash = fnv_update(hash, slot.item as u64);
                hash = fnv_update(hash, slot.count as u64);
            }
        }
        for (index, machine) in self.machines.iter() {
            hash = fnv_update(hash, index as u64);
            hash = fnv_update(hash, machine.kind as u64);
            hash = fnv_update(hash, machine.pos.key());
            hash = fnv_update(hash, machine.size as u64);
            hash = fnv_update(hash, machine.recipe as u64);
            hash = fnv_update(hash, machine.progress as u64);
            hash = fnv_update(hash, machine.fuel as u64);
            hash = fnv_update(hash, machine.power as u64);
            for slot in machine.in_buf.iter().chain(machine.out_buf.iter()) {
                hash = fnv_update(hash, slot.item as u64);
                hash = fnv_update(hash, slot.count as u64);
            }
            hash = fnv_update(hash, machine.tank.capacity as u64);
            hash = fnv_update(hash, tank_hash(&machine));
        }
        for (index, inserter) in self.inserters.iter() {
            hash = fnv_update(hash, index as u64);
            hash = fnv_update(hash, inserter.pos.key());
            hash = fnv_update(hash, inserter.dir as u64);
            hash = fnv_update(hash, inserter.progress as u64);
            hash = fnv_update(hash, id_hash(inserter.source));
            hash = fnv_update(hash, id_hash(inserter.target));
            match inserter.held {
                Some(stack) => {
                    hash = fnv_update(hash, stack.item as u64);
                    hash = fnv_update(hash, stack.count as u64);
                }
                None => {
                    hash = fnv_update(hash, u64::MAX);
                }
            }
        }
        for (index, player) in self.players.iter() {
            hash = fnv_update(hash, index as u64);
            hash = fnv_update(hash, player.pos.key());
            hash = fnv_update(hash, player.dir as u64);
            hash = fnv_update(hash, player.health as u64);
            match player.target {
                Some(target) => hash = fnv_update(hash, target.key()),
                None => hash = fnv_update(hash, u64::MAX),
            }
            for (item, count) in player.inventory.iter() {
                hash = fnv_update(hash, item as u64);
                hash = fnv_update(hash, count as u64);
            }
        }
        for (index, enemy) in self.enemies.iter() {
            hash = fnv_update(hash, index as u64);
            hash = fnv_update(hash, enemy.pos.key());
            hash = fnv_update(hash, enemy.health as u64);
        }
        for (index, projectile) in self.projectiles.iter() {
            hash = fnv_update(hash, index as u64);
            hash = fnv_update(hash, projectile.pos.key());
            hash = fnv_update(hash, projectile.dx as i64 as u64);
            hash = fnv_update(hash, projectile.dy as i64 as u64);
            hash = fnv_update(hash, projectile.damage as u64);
        }
        for (tech, progress) in self.research.iter() {
            hash = fnv_update(hash, *tech as u64);
            hash = fnv_update(hash, *progress as u64);
        }
        hash = fnv_update(hash, self.power_produced as u64);
        hash = fnv_update(hash, self.power_consumed as u64);
        self.state_hash = hash;
    }
}

pub fn generate_chunk(seed: u64, chunk: ChunkPos) -> TerrainChunk {
    let mut rng = Pcg32::new(seed ^ chunk.key().wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17));
    let mut data = TerrainChunk::new();
    let origin = chunk_origin(chunk);
    for dx in 0..CHUNK_SIZE {
        for dy in 0..CHUNK_SIZE {
            let pos = origin.add(dx, dy);
            let roll = rng.next_u32();
            if roll % 29 == 0 {
                data.set_water(chunk, pos);
            } else {
                let resource = ((roll / 29) % Resource::COUNT as u32) as u8;
                let amount = 200u16.saturating_add((rng.next_u32() % 300) as u16);
                data.set_ore(chunk, pos, resource, amount);
            }
        }
    }
    data
}

fn clear_area(terrain: &mut HashMap<ChunkPos, TerrainChunk>, origin: TilePos, radius: i32) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let pos = origin.add(dx, dy);
            let chunk = ChunkPos::from(pos);
            if let Some(data) = terrain.get_mut(&chunk) {
                data.clear_water(chunk, pos);
                data.set_ore(chunk, pos, 0, 0);
            }
        }
    }
}

fn tank_hash(machine: &Machine) -> u64 {
    match machine.tank.fluid {
        Some(fluid) => ((fluid.kind as u64) << 32) | fluid.amount as u64,
        None => u64::MAX,
    }
}

fn id_hash(id: Option<AnyId>) -> u64 {
    match id {
        Some(id) => ((id.kind() as u64) << 32) | id.index() as u64,
        None => u64::MAX,
    }
}

fn stack_has_space(buf: &[Stack; 4], item: u16, count: u16) -> bool {
    let mut space = 0u32;
    for slot in buf.iter() {
        if slot.count == 0 {
            space = space.saturating_add(u16::MAX as u32);
        } else if slot.item == item {
            space = space.saturating_add((u16::MAX - slot.count) as u32);
        }
    }
    space >= count as u32
}

fn take_from_source(
    id: AnyId,
    belts: &mut Pool<TransportBelt>,
    machines: &mut Pool<Machine>,
    players: &mut Pool<Player>,
) -> Option<Stack> {
    match id {
        AnyId::Belt(belt) => belts.get_mut(belt.0).and_then(|belt| stack_any(&mut belt.items)),
        AnyId::Machine(machine) => machines
            .get_mut(machine.0)
            .and_then(|machine| stack_any(&mut machine.out_buf)),
        AnyId::Player(player) => {
            let item = players
                .get(player.0)?
                .inventory
                .iter()
                .find(|(_, count)| *count > 0)
                .map(|(item, _)| item)?;
            let taken = players.get_mut(player.0)?.inventory.take(item, 1);
            if taken == 0 {
                None
            } else {
                Some(Stack::new(item, taken))
            }
        }
        AnyId::Inserter(_) => None,
    }
}

fn put_into_target(
    id: AnyId,
    stack: Stack,
    belts: &mut Pool<TransportBelt>,
    machines: &mut Pool<Machine>,
    players: &mut Pool<Player>,
) -> bool {
    match id {
        AnyId::Belt(belt) => belts
            .get_mut(belt.0)
            .map(|belt| stack_add(&mut belt.items, stack.item, stack.count) > 0)
            .unwrap_or(false),
        AnyId::Machine(machine) => machines
            .get_mut(machine.0)
            .map(|machine| stack_add(&mut machine.in_buf, stack.item, stack.count) > 0)
            .unwrap_or(false),
        AnyId::Player(player) => match players.get_mut(player.0) {
            Some(player) => {
                player.inventory.add(stack.item, stack.count);
                true
            }
            None => false,
        },
        AnyId::Inserter(_) => false,
    }
}

fn tile_distance(a: TilePos, b: TilePos) -> i32 {
    let dx = (a.x - b.x).unsigned_abs() as i32;
    let dy = (a.y - b.y).unsigned_abs() as i32;
    dx.max(dy)
}

fn sign_to(delta: i32) -> i32 {
    if delta > 0 {
        1
    } else if delta < 0 {
        -1
    } else {
        0
    }
}

fn direction_of(dx: i32, dy: i32) -> u8 {
    if dx > 0 {
        DIR_EAST
    } else if dy < 0 {
        DIR_NORTH
    } else if dx < 0 {
        DIR_WEST
    } else {
        DIR_SOUTH
    }
}

fn dir_vector(dir: u8) -> (i32, i32) {
    match dir {
        crate::belt::DIR_EAST => (1, 0),
        crate::belt::DIR_NORTH => (0, -1),
        crate::belt::DIR_WEST => (-1, 0),
        _ => (0, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Command;
    use crate::machine::{MACHINE_ASSEMBLER, MACHINE_BURNER_GENERATOR, MACHINE_OFFSHORE_PUMP, MACHINE_STONE_FURNACE};
    use crate::rng::Pcg32;

    fn find_ore(world: &World, near: TilePos) -> TilePos {
        for dy in 4..40 {
            for dx in 4..40 {
                let pos = near.add(dx, dy);
                let tile = world.terrain_tile(pos);
                if tile.resource != 0 && tile.ore > 0 {
                    return pos;
                }
            }
        }
        panic!("no ore found");
    }

    fn find_water(world: &World, from: i32) -> Option<TilePos> {
        for y in from..60 {
            for x in 1..60 {
                let pos = TilePos { x, y };
                if world.terrain_tile(pos).water {
                    return Some(pos);
                }
            }
        }
        None
    }

    fn clear_rect(world: &mut World, pos: TilePos, size: i32) {
        for dy in 0..size {
            for dx in 0..size {
                let tile = pos.add(dx, dy);
                world.edit_tile(tile, |data, chunk| {
                    data.clear_water(chunk, tile);
                    data.set_ore(chunk, tile, 0, 0);
                });
            }
        }
    }

    fn place_machine_clear(world: &mut World, pos: TilePos, kind: u8) -> u32 {
        let size = world.content.machine(kind).map(|def| def.size.0).unwrap_or(1);
        clear_rect(world, pos, size);
        world.place_machine(pos, kind).expect("placement failed")
    }

    #[test]
    fn spawn_area_is_clear() {
        let world = World::new(7);
        for dy in -3..=3 {
            for dx in -3..=3 {
                let tile = world.terrain_tile(TilePos { x: dx, y: dy });
                assert!(!tile.water);
                assert_eq!(tile.ore, 0);
            }
        }
        assert_eq!(world.chunk_keys().len(), 9);
    }

    #[test]
    fn world_hash_is_deterministic_for_same_seed() {
        let mut a = World::new(99);
        let mut b = World::new(99);
        for _ in 0..5 {
            a.tick();
            b.tick();
        }
        assert_eq!(a.state_hash, b.state_hash);
        assert_eq!(a.tick, 5);
    }

    #[test]
    fn different_seeds_diverge() {
        let a = World::new(1);
        let b = World::new(2);
        assert_ne!(a.state_hash, b.state_hash);
    }

    #[test]
    fn chunk_generation_matches_for_same_chunk() {
        let left = generate_chunk(1234, ChunkPos { x: 3, y: -2 });
        let right = generate_chunk(1234, ChunkPos { x: 3, y: -2 });
        assert_eq!(left.resource, right.resource);
        assert_eq!(left.ore_amount, right.ore_amount);
        assert_eq!(left.flags, right.flags);
    }

    #[test]
    fn player_moves_one_tile_per_tick() {
        let mut world = World::new(5);
        let start = world.player(0).unwrap().pos;
        world.queue(Command::Move { player: 0, dx: 1, dy: 0 });
        world.tick();
        let moved = world.player(0).unwrap().pos;
        assert_eq!(moved, start.add(1, 0));
        assert!(world
            .drain_events()
            .iter()
            .any(|event| *event == Event::PlayerMoved { player: 0, x: moved.x, y: moved.y }));
    }

    #[test]
    fn player_cannot_walk_into_water() {
        let mut world = World::new(11);
        let water = find_water(&world, 1).expect("water tile");
        let stand = TilePos { x: water.x, y: water.y - 1 };
        clear_rect(&mut world, stand, 3);
        world.edit_tile(water, |data, chunk| {
            data.set_water(chunk, water);
        });
        world.occupancy.unregister(TilePos { x: 0, y: 0 }, (1, 1));
        if let Some(player) = world.player_mut(0) {
            player.pos = stand;
            player.target = None;
        }
        world.occupancy.register(stand, (1, 1), Occupant { kind: EntityKind::Player, index: 0 });
        world.queue(Command::Move { player: 0, dx: 0, dy: 1 });
        world.tick();
        assert_eq!(world.player(0).unwrap().pos, stand);
        assert!(world.terrain_tile(water).water);
    }

    #[test]
    fn mining_adds_ore_to_inventory() {
        let mut world = World::new(21);
        let ore = find_ore(&world, TilePos { x: 0, y: 0 });
        let player = world.player_mut(0).unwrap();
        player.pos = ore.add(-1, 0);
        let mine_pos = player.pos;
        world.occupancy.set(
            mine_pos,
            Occupant { kind: EntityKind::Player, index: 0 },
        );
        let tile = world.terrain_tile(ore);
        world.queue(Command::MineTile { player: 0, x: ore.x, y: ore.y });
        world.tick();
        let after = world.terrain_tile(ore);
        assert_eq!(after.ore, tile.ore - 1);
        assert_eq!(world.player(0).unwrap().inventory.len(), 1);
    }

    #[test]
    fn mining_out_of_reach_is_rejected() {
        let mut world = World::new(22);
        let ore = find_ore(&world, TilePos { x: 0, y: 0 });
        world.queue(Command::MineTile { player: 0, x: ore.x, y: ore.y });
        world.tick();
        let tile = world.terrain_tile(ore);
        assert!(tile.ore > 0);
        assert!(world.player(0).unwrap().inventory.is_empty());
    }

    #[test]
    fn placement_registers_every_covered_tile() {
        let mut world = World::new(31);
        let index = place_machine_clear(&mut world, TilePos { x: 5, y: 5 }, MACHINE_ASSEMBLER);
        assert!(world.occupancy.get(TilePos { x: 7, y: 7 }).is_some());
        assert!(world.place_machine(TilePos { x: 5, y: 5 }, MACHINE_ASSEMBLER).is_none());
        assert!(world.occupancy.get(TilePos { x: 6, y: 7 }).is_some());
        assert_eq!(world.machine(index).unwrap().kind, MACHINE_ASSEMBLER);
    }

    #[test]
    fn removing_entity_frees_tiles_and_repairs_indices() {
        let mut world = World::new(32);
        place_machine_clear(&mut world, TilePos { x: 6, y: 6 }, MACHINE_ASSEMBLER);
        let second = place_machine_clear(&mut world, TilePos { x: 12, y: 6 }, MACHINE_ASSEMBLER);
        assert_eq!(second, 1);
        world.remove_at(TilePos { x: 6, y: 6 });
        assert!(world.occupancy.get(TilePos { x: 6, y: 6 }).is_none());
        assert!(world.occupancy.get(TilePos { x: 8, y: 8 }).is_none());
        assert_eq!(world.machines.len(), 1);
        assert!(world.machine(second).is_none());
        let moved = world.machine(0).unwrap();
        assert_eq!(moved.pos, TilePos { x: 12, y: 6 });
        let occupant = world.occupancy.get(TilePos { x: 12, y: 6 }).unwrap();
        assert_eq!(occupant.index, 0);
    }

    #[test]
    fn belts_link_and_transport_items() {
        let mut world = World::new(41);
        clear_rect(&mut world, TilePos { x: 4, y: 4 }, 3);
        let first = world.place_belt(TilePos { x: 4, y: 4 }, DIR_EAST).unwrap();
        let second = world.place_belt(TilePos { x: 5, y: 4 }, DIR_EAST).unwrap();
        assert_eq!(world.belt(first).unwrap().next, second);
        assert_eq!(world.belt(second).unwrap().prev, first);
        let item = world.item_id("coal");
        world.belts.get_mut(first).unwrap().items[0] = Stack::new(item, 1);
        let speed = world.content.belt_speed().max(1);
        for _ in 0..speed {
            world.tick();
        }
        assert!(world.belt(second).unwrap().items.iter().any(|slot| slot.item == item));
    }

    #[test]
    fn furnace_smelts_ore_into_plate() {
        let mut world = World::new(51);
        let index = place_machine_clear(&mut world, TilePos { x: 8, y: 8 }, MACHINE_STONE_FURNACE);
        let recipe = world.machine(index).unwrap().recipe;
        let definition = world.content.recipe(recipe).unwrap().clone();
        let coal = world.item_id("coal");
        world
            .machines
            .get_mut(index)
            .unwrap()
            .in_buf[0] = Stack::new(definition.inputs[0].0, definition.inputs[0].1);
        world.machines.get_mut(index).unwrap().in_buf[1] = Stack::new(coal, 1);
        let time = definition.time_ticks;
        for _ in 0..=time + 2 {
            world.tick();
        }
        let machine = world.machine(index).unwrap();
        assert!(machine.out_buf.iter().any(|slot| slot.item == definition.outputs[0].0 && slot.count > 0));
        assert!(world.drain_events().iter().any(|event| matches!(event, Event::MachineProduced { .. })));
    }

    #[test]
    fn generator_produces_power_only_while_fueled() {
        let mut world = World::new(61);
        let index = place_machine_clear(&mut world, TilePos { x: 10, y: 10 }, MACHINE_BURNER_GENERATOR);
        assert_eq!(world.machine(index).unwrap().power, 0);
        let coal = world.item_id("coal");
        world.machines.get_mut(index).unwrap().in_buf[0] = Stack::new(coal, 1);
        world.tick();
        assert_eq!(world.machine(index).unwrap().power, 5000);
        assert!(world.power_produced >= 5000);
        for _ in 0..FUEL_TICKS + 2 {
            world.tick();
        }
        assert_eq!(world.machine(index).unwrap().power, 0);
        assert_eq!(world.power_produced, 0);
    }

    #[test]
    fn offshore_pump_clears_water_flag_once() {
        let mut world = World::new(71);
        let pos = find_water(&world, 4).expect("water tile");
        let water = world.item_id("water");
        let index = world.place_machine(pos, MACHINE_OFFSHORE_PUMP).unwrap();
        world.tick();
        assert!(!world.terrain_tile(pos).water);
        assert_eq!(world.machine(index).unwrap().out_buf[0].item, water);
        assert_eq!(world.machine(index).unwrap().out_buf[0].count, 1);
        let extra: Vec<Event> = world
            .drain_events()
            .into_iter()
            .filter(|event| matches!(event, Event::WaterExtracted { .. }))
            .collect();
        assert_eq!(extra.len(), 1);
        for _ in 0..WATER_PER_EXTRACT / WATER_PER_ITEM {
            world.tick();
        }
        assert!(world.machine(index).unwrap().out_buf.iter().any(|slot| slot.count > 0));
        for _ in 0..200 {
            world.tick();
        }
        assert_eq!(
            world
                .drain_events()
                .into_iter()
                .filter(|event| matches!(event, Event::WaterExtracted { .. }))
                .count(),
            0
        );
        assert_eq!(stack_contains(&world.machine(index).unwrap().out_buf, water), 1);
    }

    #[test]
    fn inserter_moves_one_item_between_belt_and_machine() {
        let mut world = World::new(81);
        clear_rect(&mut world, TilePos { x: 20, y: 20 }, 4);
        let belt = world.place_belt(TilePos { x: 20, y: 20 }, DIR_EAST).unwrap();
        let item = world.item_id("copper_ore");
        world.belts.get_mut(belt).unwrap().items[0] = Stack::new(item, 3);
        world.place_inserter(TilePos { x: 21, y: 20 }, DIR_EAST);
        let furnace = world.place_machine(TilePos { x: 22, y: 20 }, MACHINE_STONE_FURNACE).unwrap();
        let swing = world.content.inserter_swing(0).max(1);
        for _ in 0..swing * 2 {
            world.tick();
        }
        assert_eq!(stack_contains(&world.machine(furnace).unwrap().in_buf, item), 1);
        assert_eq!(stack_contains(&world.belt(belt).unwrap().items, item), 2);
    }

    #[test]
    fn research_requires_prerequisites_and_science_packs() {
        let mut world = World::new(91);
        let assembler = world.tech_id("assembler").unwrap();
        let packs = world.item_id("science_pack");
        assert!(!world.research_tech(0, assembler));
        world.player_mut(0).unwrap().inventory.add(packs, 10);
        assert!(!world.research_tech(0, assembler));
        let burner = world.tech_id("burner").unwrap();
        let stone = world.tech_id("stone_furnace").unwrap();
        assert!(world.research_tech(0, burner));
        assert!(world.research_tech(0, stone));
        assert!(world.research_tech(0, assembler));
        assert!(world.research_completed(assembler));
        assert_eq!(world.player(0).unwrap().inventory.count(packs), 0);
    }

    #[test]
    fn lab_converts_science_packs_into_progress() {
        let mut world = World::new(101);
        let lab = place_machine_clear(&mut world, TilePos { x: 16, y: 16 }, MACHINE_LAB);
        clear_rect(&mut world, TilePos { x: 22, y: 16 }, 14);
        let coal = world.item_id("coal");
        let demand = world.content.machine(MACHINE_LAB).unwrap().energy_usage;
        let output = world.content.machine(MACHINE_BURNER_GENERATOR).unwrap().max_power;
        let needed = if output == 0 { 0 } else { demand.div_ceil(output) as i32 };
        for offset in 0..needed {
            let index = world
                .place_machine(TilePos { x: 22 + offset, y: 16 }, MACHINE_BURNER_GENERATOR)
                .unwrap();
            world.machines.get_mut(index).unwrap().in_buf[0] = Stack::new(coal, 1);
        }
        let packs = world.item_id("science_pack");
        assert!(world.research_tech(0, world.tech_id("burner").unwrap()));
        assert!(world.research_tech(0, world.tech_id("stone_furnace").unwrap()));
        world.machines.get_mut(lab).unwrap().in_buf[0] = Stack::new(packs, 2);
        world.tick();
        assert_eq!(world.machine(lab).unwrap().power, POWER_RATIO_FULL);
        for _ in 0..LAB_TICKS_PER_PACK * 2 + 2 {
            world.tick();
        }
        let progress = world.research_progress(0);
        assert!(progress > 0);
        assert!(world
            .drain_events()
            .iter()
            .any(|event| matches!(event, Event::ResearchProgress { .. })));
    }

    #[test]
    fn determinism_holds_with_identical_command_streams() {
        let script = [
            Command::PlaceBelt { player: 0, x: 3, y: 3, dir: DIR_EAST },
            Command::PlaceBelt { player: 0, x: 4, y: 3, dir: DIR_EAST },
            Command::PlaceInserter { player: 0, x: 5, y: 3, dir: DIR_EAST },
            Command::PlaceMachine { player: 0, x: 6, y: 3, kind: MACHINE_STONE_FURNACE },
        ];
        let mut hashes = Vec::new();
        for _ in 0..2 {
            let mut world = World::new(1234);
            for command in script.iter() {
                world.queue(*command);
            }
            for _ in 0..120 {
                world.tick();
            }
            hashes.push(world.state_hash);
        }
        assert_eq!(hashes[0], hashes[1]);
    }

    #[test]
    fn rng_is_stable_and_chunk_loading_is_order_free() {
        let mut a = Pcg32::new(4);
        let mut b = Pcg32::new(4);
        assert_eq!(a.next_u32(), b.next_u32());
        let mut world = World::new(4);
        let far = ChunkPos { x: 50, y: 50 };
        assert!(world.ensure_chunk(far));
        assert!(!world.ensure_chunk(far));
        let expected = generate_chunk(4, far);
        assert_eq!(world.chunk(far).unwrap().ore_amount, expected.ore_amount);
        assert_eq!(world.chunk(far).unwrap().flags, expected.flags);
    }
}