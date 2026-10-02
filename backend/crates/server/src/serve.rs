use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use factorio_net::ws::{self, Frame, FrameDecoder};
use factorio_net::{encode_server_frame, FrameDecoder as ProtoDecoder};
use factorio_proto::{ClientMessage, ServerMessage};

use crate::{GameServer, ServerConfig};

pub const TICK_HZ: u64 = 60;
pub const DEFAULT_ADDR: &str = "127.0.0.1:9000";
const DEFAULT_SNAPSHOT_EVERY: u64 = 6;
const MAX_HANDSHAKE_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug)]
pub struct ServeOptions {
    pub addr: String,
    pub config: ServerConfig,
    pub snapshot_every: u64,
    pub max_ticks: u64,
    pub ready: bool,
    pub stop: Option<Arc<AtomicBool>>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            addr: DEFAULT_ADDR.to_string(),
            config: ServerConfig::default(),
            snapshot_every: DEFAULT_SNAPSHOT_EVERY,
            max_ticks: 0,
            ready: false,
            stop: None,
        }
    }
}

struct Client {
    id: u64,
    outbox: Sender<Vec<u8>>,
    connected: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
pub struct ServeReport {
    pub addr: String,
    pub ticks: u64,
    pub clients: u64,
    pub intents: u64,
}

struct Shared {
    clients: Mutex<Vec<Client>>,
    stopped: AtomicBool,
}

enum Inbound {
    Connected { id: u64, peer: String },
    Message { id: u64, message: ClientMessage },
    Disconnected { id: u64 },
}

pub fn serve(options: ServeOptions) -> std::io::Result<ServeReport> {
    let listener = TcpListener::bind(&options.addr)?;
    let addr = listener.local_addr()?.to_string();
    listener.set_nonblocking(true)?;

    let shared = Arc::new(Shared {
        clients: Mutex::new(Vec::new()),
        stopped: AtomicBool::new(false),
    });
    let (inbound_tx, inbound_rx) = channel::<Inbound>();
    let (accept_shutdown, accept_stopped) = channel::<()>();

    let accept_shared = Arc::clone(&shared);
    let accept_inbound = inbound_tx.clone();
    let accept_handle = thread::spawn(move || {
        let mut next_id = 1u64;
        while !accept_shared.stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, peer)) => {
                    let id = next_id;
                    next_id += 1;
                    let peer_name = peer.to_string();
                    let inbound = accept_inbound.clone();
                    let clients = Arc::clone(&accept_shared);
                    thread::spawn(move || {
                        if let Err(error) = handle_connection(stream, id, peer_name.clone(), inbound, clients) {
                            eprintln!("serve: connection {id} ended: {error}");
                        }
                    });
                }
                Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(4));
                }
                Err(error) => {
                    eprintln!("serve: accept failed: {error}");
                    break;
                }
            }
        }
        let _ = accept_shutdown.send(());
    });

    if options.ready {
        println!("factorio-server listening on ws://{addr} seed={}", options.config.seed);
    }

    let mut report = run_loop(&options, &shared, &inbound_rx);
    report.addr = addr;

    shared.stopped.store(true, Ordering::Relaxed);
    for client in shared.clients.lock().unwrap_or_else(|error| error.into_inner()).iter() {
        client.connected.store(false, Ordering::Relaxed);
    }
    let _ = accept_handle.join();
    let _ = accept_stopped.recv();
    Ok(report)
}

fn run_loop(options: &ServeOptions, shared: &Arc<Shared>, inbound: &Receiver<Inbound>) -> ServeReport {
    let mut server = GameServer::new(options.config.clone());
    crate::apply_demo_script(&mut server);
    let tick_ms = 1000 / TICK_HZ;
    let mut ticks = 0u64;
    let mut intents = 0u64;
    let mut clients_seen = 0u64;
    let mut pending: VecDeque<Inbound> = VecDeque::new();
    let mut next_tick_at = Instant::now();

    loop {
        pending.extend(inbound.try_iter());
        while let Some(message) = pending.pop_front() {
            match message {
                Inbound::Connected { id, peer } => {
                    clients_seen += 1;
                    if options.ready {
                        println!("serve: client {id} connected from {peer} ({} total)", shared.clients.lock().map(|c| c.len()).unwrap_or(0));
                    }
                }
                Inbound::Message { id, message } => {
                    intents += handle_client_message(&mut server, shared, id, message);
                }
                Inbound::Disconnected { id } => {
                    server.sessions.close(id);
                    if options.ready {
                        println!("serve: client {id} disconnected at tick={}", server.world.tick);
                    }
                }
            }
        }

        let now = Instant::now();
        if now < next_tick_at {
            thread::sleep(next_tick_at - now);
        }

        let outcome = server.advance();
        ticks += 1;
        next_tick_at = Instant::now() + Duration::from_millis(tick_ms);

        if ticks % options.snapshot_every == 0 {
            broadcast(shared, &server.snapshot());
        }
        for event in &outcome.events {
            let message = server.event_message(event);
            broadcast(shared, &message);
        }
        if options.config.autosave_every > 0 && outcome.tick % options.config.autosave_every == 0 && options.ready {
            println!(
                "serve: tick={} hash={:016x} power={}/{} chunks={} events={}",
                outcome.tick,
                outcome.state_hash,
                server.world.power_produced,
                server.world.power_consumed,
                server.streamer.resident(),
                outcome.events.len()
            );
        }

        if options.max_ticks > 0 && ticks >= options.max_ticks {
            break;
        }
        if options.stop.as_ref().is_some_and(|stop| stop.load(Ordering::Relaxed)) {
            break;
        }
    }

    ServeReport { addr: String::new(), ticks, clients: clients_seen, intents }
}

fn handle_client_message(server: &mut GameServer, shared: &Arc<Shared>, id: u64, message: ClientMessage) -> u64 {
    match message {
        ClientMessage::Hello { name, version } => {
            let hello = ServerMessage::Hello {
                version: factorio_proto::PROTOCOL_VERSION,
                seed: server.config.seed,
            };
            let snapshot = server.snapshot();
            send(shared, id, &encode_server_frame(&hello));
            send(shared, id, &encode_server_frame(&snapshot));
            for replay in server.entity_replay() {
                send(shared, id, &encode_server_frame(&replay));
            }
            server.submit(ClientMessage::Hello { name, version });
            0
        }
        ClientMessage::Ping { stamp } => {
            send(shared, id, &encode_server_frame(&ServerMessage::Pong { stamp }));
            0
        }
        ClientMessage::Intent { player, intent } => {
            server.submit(ClientMessage::Intent { player, intent });
            1
        }
    }
}

fn broadcast(shared: &Arc<Shared>, message: &ServerMessage) {
    let payload = ws::encode(&Frame::Binary(encode_server_frame(message)));
    let clients = shared.clients.lock().unwrap_or_else(|error| error.into_inner());
    for client in clients.iter() {
        let _ = client.outbox.send(payload.clone());
    }
}

fn send(shared: &Arc<Shared>, id: u64, payload: &[u8]) {
    let frame = ws::encode(&Frame::Binary(payload.to_vec()));
    let clients = shared.clients.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(client) = clients.iter().find(|client| client.id == id) {
        let _ = client.outbox.send(frame);
    }
}

fn handle_connection(
    mut stream: TcpStream,
    id: u64,
    peer: String,
    inbound: Sender<Inbound>,
    shared: Arc<Shared>,
) -> std::io::Result<()> {
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    let mut leftover = handshake(&mut stream)?;
    let connected = Arc::new(AtomicBool::new(true));
    let (outbox_tx, outbox_rx) = channel::<Vec<u8>>();
    let socket = stream.try_clone()?;
    let writer_connected = Arc::clone(&connected);
    let writer = thread::spawn(move || write_loop(socket, outbox_rx, writer_connected));

    shared
        .clients
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(Client { id, outbox: outbox_tx, connected: Arc::clone(&connected) });
    let _ = inbound.send(Inbound::Connected { id, peer });

    let mut ws_decoder = FrameDecoder::new();
    let mut proto_decoder = ProtoDecoder::new();
    let mut buffer = vec![0u8; 4096];
    let mut pending: Vec<u8> = std::mem::take(&mut leftover);

    loop {
        if !connected.load(Ordering::Relaxed) {
            break;
        }
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                pending.extend_from_slice(&buffer[..count]);
                let frames = match ws_decoder.push(&pending) {
                    Ok(frames) => {
                        pending.clear();
                        frames
                    }
                    Err(error) => {
                        eprintln!("serve: client {id} sent an invalid frame: {error}");
                        break;
                    }
                };
                for frame in frames {
                    match frame {
                        Frame::Binary(payload) => match proto_decoder.messages(&payload) {
                            Ok(messages) => {
                                for message in messages {
                                    if inbound.send(Inbound::Message { id, message }).is_err() {
                                        return Ok(());
                                    }
                                }
                            }
                            Err(error) => {
                                eprintln!("serve: client {id} sent a bad message: {error}");
                            }
                        },
                        Frame::Ping(payload) => {
                            let _ = shared.clients.lock().map(|clients| {
                                if let Some(client) = clients.iter().find(|client| client.id == id) {
                                    let _ = client.outbox.send(ws::encode(&Frame::Pong(payload)));
                                }
                            });
                        }
                        Frame::Pong(_) => {}
                        Frame::Text(_) => {
                            eprintln!("serve: client {id} sent text; this server speaks binary only");
                        }
                        Frame::Close => break,
                    }
                }
            }
            Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                connected.store(false, Ordering::Relaxed);
                break;
            }
            Err(ref error) if error.kind() == std::io::ErrorKind::TimedOut => {
                connected.store(false, Ordering::Relaxed);
                break;
            }
            Err(error) => {
                connected.store(false, Ordering::Relaxed);
                eprintln!("serve: client {id} read failed: {error}");
                break;
            }
        }
    }

    connected.store(false, Ordering::Relaxed);
    shared
        .clients
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|client| client.id != id);
    let _ = inbound.send(Inbound::Disconnected { id });
    let _ = writer.join();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(())
}

fn write_loop(mut socket: TcpStream, outbox: Receiver<Vec<u8>>, connected: Arc<AtomicBool>) {
    while connected.load(Ordering::Relaxed) {
        match outbox.recv_timeout(Duration::from_millis(100)) {
            Ok(payload) => {
                if socket.write_all(&payload).is_err() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = socket.flush();
}

fn handshake(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed during the handshake",
            ));
        }
        request.extend_from_slice(&buffer[..count]);
        if request.len() > MAX_HANDSHAKE_BYTES {
            let _ = stream.write_all(&ws::handshake_failure("431 Request Header Fields Too Large", "header too large"));
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "handshake header too large"));
        }
        if let Some(_) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    match ws::parse_handshake(&request) {
        Ok((key, consumed)) => {
            stream.write_all(&ws::handshake_response(&key))?;
            stream.flush()?;
            Ok(request[consumed..].to_vec())
        }
        Err(error) => {
            let _ = stream.write_all(&ws::handshake_failure("400 Bad Request", error));
            let _ = stream.flush();
            Err(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        }
    }
}
