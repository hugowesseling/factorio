pub mod ws;

use factorio_proto::{frame, ClientMessage, Error, ServerMessage};

pub const MAX_FRAME_BYTES: usize = 1 << 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn pending(&self) -> usize {
        self.buffer.len()
    }

    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
        self.buffer.extend_from_slice(data);
        let mut frames = Vec::new();
        loop {
            let length = match peek_varint(&self.buffer) {
                Some(length) => length,
                None => break,
            };
            if length as usize > MAX_FRAME_BYTES {
                return Err("frame too large");
            }
            let header = varint_size(length);
            if self.buffer.len() < header + length as usize {
                break;
            }
            let payload = self.buffer[header..header + length as usize].to_vec();
            self.buffer.drain(..header + length as usize);
            frames.push(payload);
        }
        Ok(frames)
    }

    pub fn messages(&mut self, data: &[u8]) -> Result<Vec<ClientMessage>, Error> {
        let mut out = Vec::new();
        for payload in self.push(data)? {
            out.push(ClientMessage::decode(&payload)?);
        }
        Ok(out)
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub id: u64,
    pub name: String,
    pub player: u32,
    pub last_tick: u64,
    pub last_seen: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SessionRegistry {
    sessions: Vec<Session>,
    next_id: u64,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self { sessions: Vec::new(), next_id: 1 }
    }

    pub fn open(&mut self, name: String, player: u32, tick: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.push(Session { id, name, player, last_tick: tick, last_seen: tick });
        id
    }

    pub fn close(&mut self, id: u64) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|session| session.id != id);
        self.sessions.len() != before
    }

    pub fn get(&self, id: u64) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|session| session.id == id)
    }

    pub fn touch(&mut self, id: u64, tick: u64) {
        if let Some(session) = self.get_mut(id) {
            session.last_tick = tick;
            session.last_seen = tick;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Session> {
        self.sessions.iter()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn broadcast(&self, message: &ServerMessage) -> Vec<(u64, Vec<u8>)> {
        let payload = frame(&message.encode());
        self.sessions
            .iter()
            .map(|session| (session.id, payload.clone()))
            .collect()
    }
}

pub fn encode_frame(message: &ClientMessage) -> Vec<u8> {
    frame(&message.encode())
}

pub fn encode_server_frame(message: &ServerMessage) -> Vec<u8> {
    frame(&message.encode())
}

fn peek_varint(bytes: &[u8]) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for byte in bytes.iter() {
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
    None
}

fn varint_size(value: u64) -> usize {
    let mut size = 1;
    let mut value = value;
    while value >= 0x80 {
        value >>= 7;
        size += 1;
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;
    use factorio_proto::{Intent, IntentKind, PROTOCOL_VERSION};

    #[test]
    fn decoder_reassembles_split_frames() {
        let bytes = encode_frame(&ClientMessage::Ping { stamp: 7 });
        let mut decoder = FrameDecoder::new();
        assert!(decoder.push(&bytes[..1]).unwrap().is_empty());
        let frames = decoder.push(&bytes[1..]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(ClientMessage::decode(&frames[0]).unwrap(), ClientMessage::Ping { stamp: 7 });
    }

    #[test]
    fn decoder_handles_multiple_frames_in_one_read() {
        let mut data = encode_frame(&ClientMessage::Ping { stamp: 1 });
        data.extend(encode_frame(&ClientMessage::Ping { stamp: 2 }));
        let mut decoder = FrameDecoder::new();
        let frames = decoder.push(&data).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn sessions_track_players() {
        let mut registry = SessionRegistry::new();
        let id = registry.open("a".to_string(), 0, 10);
        assert_eq!(registry.len(), 1);
        registry.touch(id, 20);
        assert_eq!(registry.get(id).unwrap().last_tick, 20);
        let outbox = registry.broadcast(&ServerMessage::Hello { version: PROTOCOL_VERSION, seed: 1 });
        assert_eq!(outbox.len(), 1);
        assert!(registry.close(id));
        assert!(registry.is_empty());
    }

    #[test]
    fn intent_messages_survive_the_pipeline() {
        let message = ClientMessage::Intent {
            player: 0,
            intent: Intent::new(IntentKind::PlaceMachine, 4, 4, 2),
        };
        let bytes = encode_frame(&message);
        let mut decoder = FrameDecoder::new();
        let decoded = decoder.messages(&bytes).unwrap();
        assert_eq!(decoded, vec![message]);
    }

    #[test]
    fn oversized_frames_are_rejected() {
        let mut bytes = Vec::new();
        bytes.push(0xff);
        bytes.push(0xff);
        bytes.push(0xff);
        bytes.push(0x7f);
        let mut decoder = FrameDecoder::new();
        assert!(decoder.push(&bytes).is_err());
    }
}