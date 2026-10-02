pub mod serve;

use factorio_content::Content;
use factorio_net::SessionRegistry;
use factorio_persistence::SaveFile;
use factorio_proto::{ClientMessage, Intent, IntentKind, ServerMessage, PROTOCOL_VERSION};
use factorio_sim::{Command, Event, TickOutcome, World};
use factorio_world::{ChunkStreamer, StreamStats};

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub seed: u64,
    pub view_radius: i32,
    pub autosave_every: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { seed: 1, view_radius: 2, autosave_every: 600 }
    }
}

pub struct GameServer {
    pub config: ServerConfig,
    pub world: World,
    pub content: Content,
    pub sessions: SessionRegistry,
    pub streamer: ChunkStreamer,
    pub last_stream: StreamStats,
}

impl GameServer {
    pub fn new(config: ServerConfig) -> Self {
        let world = World::new(config.seed);
        let content = world.content.clone();
        let streamer = ChunkStreamer::new(config.view_radius);
        let mut server = Self {
            config,
            world,
            content,
            sessions: SessionRegistry::new(),
            streamer,
            last_stream: StreamStats::default(),
        };
        let focus = server.focus();
        server.last_stream = server.streamer.update(&mut server.world, focus);
        server
    }

    pub fn focus(&self) -> factorio_sim::TilePos {
        self.world.player(0).map(|player| player.pos).unwrap_or_default()
    }

    pub fn connect(&mut self, name: &str) -> u64 {
        let tick = self.world.tick;
        let player = self.world.players.len().saturating_sub(1);
        self.sessions.open(name.to_string(), player, tick)
    }

    pub fn submit(&mut self, message: ClientMessage) {
        match message {
            ClientMessage::Hello { name, version } => {
                if version != PROTOCOL_VERSION {
                    return;
                }
                self.connect(&name);
            }
            ClientMessage::Intent { player, intent } => {
                if self.sessions.iter().any(|session| session.player == player) {
                    self.apply_intent(player, intent);
                }
            }
            ClientMessage::Ping { .. } => {}
        }
    }

    pub fn apply_intent(&mut self, player: u32, intent: Intent) {
        let command = match intent.kind {
            IntentKind::Move => Command::Move { player, dx: intent.x, dy: intent.y },
            IntentKind::PlaceBelt => Command::PlaceBelt { player, x: intent.x, y: intent.y, dir: intent.arg as u8 },
            IntentKind::PlaceInserter => Command::PlaceInserter { player, x: intent.x, y: intent.y, dir: intent.arg as u8 },
            IntentKind::PlaceMachine => Command::PlaceMachine { player, x: intent.x, y: intent.y, kind: intent.arg as u8 },
            IntentKind::Remove => Command::RemoveEntity { player, x: intent.x, y: intent.y },
            IntentKind::Mine => Command::MineTile { player, x: intent.x, y: intent.y },
            IntentKind::Collect => Command::CollectMachine { player, x: intent.x, y: intent.y },
            IntentKind::Research => Command::ResearchTech { player, tech: intent.arg as u16 },
        };
        self.world.queue(command);
    }

    pub fn advance(&mut self) -> TickOutcome {
        self.world.tick();
        let focus = self.focus();
        self.last_stream = self.streamer.update(&mut self.world, focus);
        let events = self.world.drain_events();
        TickOutcome { tick: self.world.tick, state_hash: self.world.state_hash, events }
    }

    pub fn run(&mut self, ticks: u64) -> TickOutcome {
        let mut outcome = TickOutcome { tick: self.world.tick, state_hash: self.world.state_hash, events: Vec::new() };
        for _ in 0..ticks {
            outcome = self.advance();
        }
        outcome
    }

    pub fn snapshot(&self) -> ServerMessage {
        ServerMessage::Snapshot { tick: self.world.tick, hash: self.world.state_hash }
    }

    pub fn save(&self) -> SaveFile {
        SaveFile::capture(&self.world)
    }

    pub fn broadcast(&self, message: &ServerMessage) -> Vec<(u64, Vec<u8>)> {
        self.sessions.broadcast(message)
    }

    pub fn entity_replay(&self) -> Vec<ServerMessage> {
        self.world
            .entity_placements()
            .into_iter()
            .map(|(kind, index, x, y)| {
                self.event_message(&Event::EntityPlaced { kind, index, x, y })
            })
            .collect()
    }

    pub fn event_message(&self, event: &Event) -> ServerMessage {
        ServerMessage::Event {
            tick: self.world.tick,
            code: event_code(event),
            a: event_a(event),
            b: event_b(event),
            c: event_c(event),
            d: event_d(event),
        }
    }
}

pub fn event_code(event: &Event) -> u8 {
    match event {
        Event::PlayerMoved { .. } => 1,
        Event::EntityPlaced { .. } => 2,
        Event::EntityRemoved { .. } => 3,
        Event::TileMined { .. } => 4,
        Event::MachineProduced { .. } => 5,
        Event::ResearchProgress { .. } => 6,
        Event::ResearchCompleted { .. } => 7,
        Event::WaterExtracted { .. } => 8,
        Event::TickCompleted { .. } => 9,
    }
}

pub fn event_a(event: &Event) -> i32 {
    match event {
        Event::PlayerMoved { player, .. } => *player as i32,
        Event::EntityPlaced { kind, .. } | Event::EntityRemoved { kind, .. } => *kind as i32,
        Event::TileMined { x, .. } => *x,
        Event::MachineProduced { machine, .. } => *machine as i32,
        Event::ResearchProgress { tech, .. } | Event::ResearchCompleted { tech, .. } => *tech as i32,
        Event::WaterExtracted { machine } => *machine as i32,
        Event::TickCompleted { tick } => *tick as i32,
    }
}

pub fn event_b(event: &Event) -> i32 {
    match event {
        Event::PlayerMoved { x, .. } => *x,
        Event::EntityPlaced { index, .. } | Event::EntityRemoved { index, .. } => *index as i32,
        Event::TileMined { y, .. } => *y,
        Event::MachineProduced { item, .. } => *item as i32,
        Event::ResearchProgress { progress, .. } => *progress as i32,
        Event::ResearchCompleted { .. }
        | Event::WaterExtracted { .. }
        | Event::TickCompleted { .. } => 0,
    }
}

pub fn event_c(event: &Event) -> i32 {
    match event {
        Event::PlayerMoved { y, .. } => *y,
        Event::EntityPlaced { x, .. } => *x,
        Event::TileMined { item, .. } => *item as i32,
        Event::MachineProduced { count, .. } => *count as i32,
        _ => 0,
    }
}

pub fn event_d(event: &Event) -> i32 {
    match event {
        Event::EntityPlaced { y, .. } => *y,
        Event::TileMined { count, .. } => *count as i32,
        _ => 0,
    }
}

pub fn apply_demo_script(server: &mut GameServer) {
    for step in 0..6i32 {
        server.apply_intent(0, Intent::new(IntentKind::PlaceBelt, 4 + step, 4, 0));
    }
    server.apply_intent(0, Intent::new(IntentKind::PlaceInserter, 10, 4, 0));
    server.apply_intent(0, Intent::new(IntentKind::PlaceMachine, 11, 4, 1));
    server.apply_intent(0, Intent::new(IntentKind::PlaceMachine, 16, 4, 0));
}

pub fn run_headless(config: ServerConfig, ticks: u64) -> u64 {
    let mut server = GameServer::new(config);
    let outcome = server.run(ticks);
    outcome.state_hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use factorio_sim::{DIR_EAST, MACHINE_BURNER_GENERATOR, MACHINE_STONE_FURNACE};

    #[test]
    fn headless_run_advances_ticks() {
        let mut server = GameServer::new(ServerConfig { seed: 5, ..Default::default() });
        let outcome = server.run(120);
        assert_eq!(outcome.tick, 120);
        assert_eq!(outcome.state_hash, server.world.state_hash);
    }

    #[test]
    fn intents_from_clients_reach_the_simulation() {
        let mut server = GameServer::new(ServerConfig { seed: 6, ..Default::default() });
        server.submit(ClientMessage::Hello { name: "tester".to_string(), version: PROTOCOL_VERSION });
        assert_eq!(server.sessions.len(), 1);
        server.submit(ClientMessage::Intent {
            player: 0,
            intent: Intent::new(IntentKind::PlaceBelt, 4, 4, DIR_EAST as i32),
        });
        server.submit(ClientMessage::Intent {
            player: 0,
            intent: Intent::new(IntentKind::PlaceMachine, 8, 8, MACHINE_STONE_FURNACE as i32),
        });
        server.submit(ClientMessage::Intent {
            player: 0,
            intent: Intent::new(IntentKind::PlaceMachine, 16, 8, MACHINE_BURNER_GENERATOR as i32),
        });
        server.run(5);
        assert_eq!(server.world.belts.len(), 1);
        assert_eq!(server.world.machines.len(), 2);
    }

    #[test]
    fn intents_without_a_session_are_ignored() {
        let mut server = GameServer::new(ServerConfig::default());
        server.submit(ClientMessage::Intent {
            player: 7,
            intent: Intent::new(IntentKind::PlaceBelt, 4, 4, 0),
        });
        server.run(3);
        assert_eq!(server.world.belts.len(), 0);
    }

    #[test]
    fn wrong_protocol_version_is_refused() {
        let mut server = GameServer::new(ServerConfig::default());
        server.submit(ClientMessage::Hello { name: "old".to_string(), version: 0 });
        assert!(server.sessions.is_empty());
    }

    #[test]
    fn identical_inputs_produce_identical_hashes() {
        let left = run_headless(ServerConfig { seed: 77, ..Default::default() }, 300);
        let right = run_headless(ServerConfig { seed: 77, ..Default::default() }, 300);
        assert_eq!(left, right);
    }

    #[test]
    fn saves_round_trip_through_the_server() {
        let mut server = GameServer::new(ServerConfig { seed: 78, ..Default::default() });
        server.run(75);
        let save = server.save();
        let bytes = save.to_bytes();
        let restored = SaveFile::from_bytes(&bytes).unwrap().restore();
        assert_eq!(restored.state_hash, server.world.state_hash);
    }

    #[test]
    fn events_encode_into_protocol_messages() {
        let mut server = GameServer::new(ServerConfig { seed: 79, ..Default::default() });
        server.submit(ClientMessage::Hello { name: "watcher".to_string(), version: PROTOCOL_VERSION });
        server.submit(ClientMessage::Intent {
            player: 0,
            intent: Intent::new(IntentKind::Move, 1, 0, 0),
        });
        let outcome = server.advance();
        assert!(!outcome.events.is_empty());
        for event in outcome.events.iter() {
            let message = server.event_message(event);
            assert_eq!(ServerMessage::decode(&message.encode()).unwrap(), message);
        }
    }
}