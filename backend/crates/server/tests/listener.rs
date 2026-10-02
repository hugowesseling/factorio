use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use factorio_net::ws::{self, Frame, FrameDecoder};
use factorio_net::FrameDecoder as ProtoDecoder;
use factorio_proto::{frame, ClientMessage, Intent, IntentKind, ServerMessage, PROTOCOL_VERSION};
use factorio_server::serve::{self, ServeOptions, TICK_HZ};
use factorio_server::ServerConfig;

struct Harness {
    addr: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<serve::ServeReport>>,
}

impl Harness {
    fn start() -> Self {
        Self::start_with(ServeOptions {
            addr: String::new(),
            config: ServerConfig { seed: 7, view_radius: 2, autosave_every: 600 },
            snapshot_every: 1,
            max_ticks: 0,
            ready: false,
            stop: None,
        })
    }

    fn start_with(mut options: ServeOptions) -> Self {
        let addr = {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("probe port");
            let addr = probe.local_addr().expect("probe addr").to_string();
            drop(probe);
            addr
        };
        let stop = Arc::new(AtomicBool::new(false));
        options.addr = addr.clone();
        options.stop = Some(Arc::clone(&stop));

        let handle = thread::spawn(move || serve::serve(options).expect("serve failed"));
        let harness = Harness { addr, stop, handle: Some(handle) };
        harness.wait_until_listening();
        harness
    }

    fn wait_until_listening(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if TcpStream::connect(&self.addr).is_ok() {
                return;
            }
            assert!(Instant::now() < deadline, "server never started listening on {}", self.addr);
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn stop(&mut self) -> serve::ServeReport {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.take().expect("serve handle").join().expect("serve thread")
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if self.handle.is_some() {
            let _ = self.stop();
        }
    }
}

struct TestClient {
    stream: TcpStream,
    ws: FrameDecoder,
    proto: ProtoDecoder,
    received: Vec<ServerMessage>,
}

impl TestClient {
    fn connect(addr: &str) -> Self {
        let mut stream = TcpStream::connect(addr).expect("connect");
        stream.set_read_timeout(Some(Duration::from_millis(2_500))).expect("timeout");
        let request = format!(
            "GET /ws HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).expect("write request");
        stream.flush().expect("flush");

        let mut response = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            let count = stream.read(&mut buffer).expect("read response");
            response.extend_from_slice(&buffer[..count]);
            if response.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let header_end = response.windows(4).position(|window| window == b"\r\n\r\n").expect("header end") + 4;
        let head = String::from_utf8_lossy(&response[..header_end]).to_string();
        assert!(head.starts_with("HTTP/1.1 101"), "unexpected handshake: {head}");
        assert!(head.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="), "bad accept key");

        let mut client = Self {
            stream,
            ws: FrameDecoder::client_side(),
            proto: ProtoDecoder::new(),
            received: Vec::new(),
        };
        let leftover = response[header_end..].to_vec();
        if !leftover.is_empty() {
            client.absorb(&leftover);
        }
        client
    }

    fn send(&mut self, message: &ClientMessage) {
        let bytes = ws::encode_masked(&Frame::Binary(frame(&message.encode())), [0x12, 0x34, 0x56, 0x78]);
        self.stream.write_all(&bytes).expect("write frame");
        self.stream.flush().expect("flush");
    }

    fn hello(&mut self, name: &str) {
        self.send(&ClientMessage::Hello { name: name.to_string(), version: PROTOCOL_VERSION });
        self.wait_for(|message| matches!(message, ServerMessage::Hello { .. }));
    }

    fn absorb(&mut self, data: &[u8]) {
        for frame in self.ws.push(data).expect("ws frames") {
            match frame {
                Frame::Binary(payload) => {
                    for decoded in self.proto.push(&payload).expect("proto frames") {
                        self.received.push(ServerMessage::decode(&decoded).expect("server message"));
                    }
                }
                other => panic!("unexpected frame {other:?}"),
            }
        }
    }

    fn poll(&mut self) {
        let mut buffer = [0u8; 4096];
        if let Ok(count) = self.stream.read(&mut buffer) {
            self.absorb(&buffer[..count]);
        }
    }

    fn wait_for<F>(&mut self, predicate: F) -> ServerMessage
    where
        F: Fn(&ServerMessage) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(found) = self.received.iter().find(|message| predicate(message)).cloned() {
                return found;
            }
            assert!(Instant::now() < deadline, "timed out waiting for a message, got {:?}", self.received);
            self.poll();
            thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
fn serves_hello_snapshots_and_pongs_over_websocket() {
    let mut harness = Harness::start();
    let mut client = TestClient::connect(&harness.addr);

    client.send(&ClientMessage::Hello { name: "tester".to_string(), version: PROTOCOL_VERSION });
    let hello = client.wait_for(|message| matches!(message, ServerMessage::Hello { .. }));
    assert_eq!(hello, ServerMessage::Hello { version: PROTOCOL_VERSION, seed: 7 });

    let snapshot = client.wait_for(|message| matches!(message, ServerMessage::Snapshot { .. }));
    match snapshot {
        ServerMessage::Snapshot { tick, hash } => {
            assert!(tick > 0, "expected the tick to advance, got {tick}");
            assert_ne!(hash, 0);
        }
        other => panic!("unexpected {other:?}"),
    }

    client.send(&ClientMessage::Ping { stamp: 4_242 });
    assert_eq!(
        client.wait_for(|message| matches!(message, ServerMessage::Pong { .. })),
        ServerMessage::Pong { stamp: 4_242 }
    );

    let report = harness.stop();
    assert_eq!(report.addr, harness.addr);
    assert_eq!(report.clients, 1);
}

#[test]
fn applies_intents_from_connected_clients() {
    let mut harness = Harness::start();
    let mut client = TestClient::connect(&harness.addr);
    client.hello("builder");

    client.send(&ClientMessage::Intent {
        player: 0,
        intent: Intent::new(IntentKind::PlaceBelt, 20, 20, 0),
    });
    client.wait_for(|message| matches!(message, ServerMessage::Event { code: 2, c: 20, d: 20, .. }));

    let report = harness.stop();
    assert!(report.intents >= 1, "expected the intent to be counted: {report:?}");
}

#[test]
fn replays_existing_entities_to_a_new_client() {
    let mut harness = Harness::start();
    let mut client = TestClient::connect(&harness.addr);
    client.hello("joiner");

    let belt = client.wait_for(|message| matches!(message, ServerMessage::Event { code: 2, a: 0, .. }));
    match belt {
        ServerMessage::Event { code: 2, a: 0, c, d, .. } => {
            assert!((4..=9).contains(&c), "belt x should be in the scripted range, got {c}");
            assert_eq!(d, 4);
        }
        other => panic!("unexpected {other:?}"),
    }

    client.wait_for(|message| matches!(message, ServerMessage::Event { code: 2, a: 2, .. }));
    client.wait_for(|message| matches!(message, ServerMessage::Event { code: 2, a: 1, .. }));
    client.wait_for(|message| matches!(message, ServerMessage::Event { code: 2, a: 3, .. }));

    let report = harness.stop();
    assert_eq!(report.clients, 1);
}

#[test]
fn rejects_plain_http_requests() {
    let mut harness = Harness::start();
    let mut stream = TcpStream::connect(&harness.addr).expect("connect");
    stream.set_read_timeout(Some(Duration::from_millis(2_500))).expect("timeout");
    stream
        .write_all(b"GET /ws HTTP/1.1\r\nHost: x\r\nAccept: text/html\r\n\r\n")
        .expect("write");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    assert!(response.starts_with("HTTP/1.1 400"), "unexpected response: {response}");
    let _ = harness.stop();
}

#[test]
fn ticks_at_sixty_hertz_until_the_budget_runs_out() {
    let options = ServeOptions {
        addr: "127.0.0.1:0".to_string(),
        config: ServerConfig { seed: 1, view_radius: 2, autosave_every: 600 },
        snapshot_every: 6,
        max_ticks: 120,
        ready: false,
        stop: None,
    };
    let started = Instant::now();
    let report = serve::serve(options).expect("serve");
    let elapsed = started.elapsed();
    assert_eq!(report.ticks, 120);
    assert_eq!(report.clients, 0);
    let expected = Duration::from_millis(120 * 1000 / TICK_HZ);
    assert!(
        elapsed + Duration::from_millis(400) >= expected,
        "expected about {expected:?} for 120 ticks, took {elapsed:?}"
    );
    assert!(elapsed < Duration::from_secs(10), "120 ticks should be quick, took {elapsed:?}");
}

#[test]
fn many_clients_are_served_concurrently() {
    let mut harness = Harness::start();
    let addr = harness.addr.clone();
    let handles: Vec<_> = (0..4)
        .map(|index| {
            let addr = addr.clone();
            thread::spawn(move || {
                let mut client = TestClient::connect(&addr);
                client.send(&ClientMessage::Hello {
                    name: format!("client-{index}"),
                    version: PROTOCOL_VERSION,
                });
                client.send(&ClientMessage::Ping { stamp: index as u64 + 1 });
                client.wait_for(|message| matches!(message, ServerMessage::Pong { .. }))
            })
        })
        .collect();

    for (index, handle) in handles.into_iter().enumerate() {
        assert_eq!(handle.join().expect("client thread"), ServerMessage::Pong { stamp: index as u64 + 1 });
    }

    let report = harness.stop();
    assert_eq!(report.clients, 4);
}

#[test]
fn a_dropping_client_does_not_stop_the_server() {
    let mut harness = Harness::start();
    {
        let mut client = TestClient::connect(&harness.addr);
        client.hello("leaver");
    }

    let mut survivor = TestClient::connect(&harness.addr);
    survivor.send(&ClientMessage::Hello { name: "stayer".to_string(), version: PROTOCOL_VERSION });
    assert_eq!(
        survivor.wait_for(|message| matches!(message, ServerMessage::Hello { .. })),
        ServerMessage::Hello { version: PROTOCOL_VERSION, seed: 7 }
    );

    survivor.send(&ClientMessage::Ping { stamp: 9 });
    assert_eq!(
        survivor.wait_for(|message| matches!(message, ServerMessage::Pong { .. })),
        ServerMessage::Pong { stamp: 9 }
    );

    let report = harness.stop();
    assert_eq!(report.clients, 2);
}

#[test]
fn snapshot_ticks_advance_monotonically() {
    let mut harness = Harness::start();
    let mut client = TestClient::connect(&harness.addr);
    client.hello("watcher");

    let first = client.wait_for(|message| matches!(message, ServerMessage::Snapshot { .. }));
    let mut last_tick = match first {
        ServerMessage::Snapshot { tick, .. } => tick,
        other => panic!("unexpected {other:?}"),
    };
    for _ in 0..8 {
        let message = client.wait_for(|message| match message {
            ServerMessage::Snapshot { tick, .. } => *tick > last_tick,
            _ => false,
        });
        if let ServerMessage::Snapshot { tick, .. } = message {
            last_tick = tick;
        }
    }

    let report = harness.stop();
    assert!(report.ticks > 0);
}