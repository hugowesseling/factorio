pub const PROTOCOL_VERSION: u32 = 1;

pub type Error = &'static str;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntentKind {
    Move = 1,
    PlaceBelt = 2,
    PlaceInserter = 3,
    PlaceMachine = 4,
    Remove = 5,
    Mine = 6,
    Collect = 7,
    Research = 8,
}

impl IntentKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            1 => IntentKind::Move,
            2 => IntentKind::PlaceBelt,
            3 => IntentKind::PlaceInserter,
            4 => IntentKind::PlaceMachine,
            5 => IntentKind::Remove,
            6 => IntentKind::Mine,
            7 => IntentKind::Collect,
            8 => IntentKind::Research,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Intent {
    pub kind: IntentKind,
    pub x: i32,
    pub y: i32,
    pub arg: i32,
}

impl Intent {
    pub fn new(kind: IntentKind, x: i32, y: i32, arg: i32) -> Self {
        Self { kind, x, y, arg }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.u8(self.kind as u8);
        writer.i32(self.x);
        writer.i32(self.y);
        writer.i32(self.arg);
        writer.into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes);
        let kind = IntentKind::from_u8(reader.u8()?).ok_or("bad intent kind")?;
        let x = reader.i32()?;
        let y = reader.i32()?;
        let arg = reader.i32()?;
        Ok(Intent { kind, x, y, arg })
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ClientMessage {
    Hello { name: String, version: u32 },
    Intent { player: u32, intent: Intent },
    Ping { stamp: u64 },
}

impl ClientMessage {
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        match self {
            ClientMessage::Hello { name, version } => {
                writer.u8(1);
                writer.string(name);
                writer.varint(*version as u64);
            }
            ClientMessage::Intent { player, intent } => {
                writer.u8(2);
                writer.varint(*player as u64);
                let payload = intent.encode();
                writer.varint(payload.len() as u64);
                writer.raw(&payload);
            }
            ClientMessage::Ping { stamp } => {
                writer.u8(3);
                writer.varint(*stamp);
            }
        }
        writer.into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes);
        let tag = reader.u8()?;
        Ok(match tag {
            1 => ClientMessage::Hello { name: reader.string()?, version: reader.varint()? as u32 },
            2 => {
                let player = reader.varint()? as u32;
                let length = reader.varint()? as usize;
                let intent = Intent::decode(reader.take(length)?)?;
                ClientMessage::Intent { player, intent }
            }
            3 => ClientMessage::Ping { stamp: reader.varint()? },
            _ => return Err("bad client tag"),
        })
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ServerMessage {
    Hello { version: u32, seed: u64 },
    Snapshot { tick: u64, hash: u64 },
    Event { tick: u64, code: u8, a: i32, b: i32, c: i32, d: i32 },
    Pong { stamp: u64 },
}

impl ServerMessage {
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        match self {
            ServerMessage::Hello { version, seed } => {
                writer.u8(1);
                writer.varint(*version as u64);
                writer.varint(*seed);
            }
            ServerMessage::Snapshot { tick, hash } => {
                writer.u8(2);
                writer.varint(*tick);
                writer.varint(*hash);
            }
            ServerMessage::Event { tick, code, a, b, c, d } => {
                writer.u8(3);
                writer.varint(*tick);
                writer.u8(*code);
                writer.i32(*a);
                writer.i32(*b);
                writer.i32(*c);
                writer.i32(*d);
            }
            ServerMessage::Pong { stamp } => {
                writer.u8(4);
                writer.varint(*stamp);
            }
        }
        writer.into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes);
        let tag = reader.u8()?;
        Ok(match tag {
            1 => ServerMessage::Hello { version: reader.varint()? as u32, seed: reader.varint()? },
            2 => ServerMessage::Snapshot { tick: reader.varint()?, hash: reader.varint()? },
            3 => ServerMessage::Event {
                tick: reader.varint()?,
                code: reader.u8()?,
                a: reader.i32()?,
                b: reader.i32()?,
                c: reader.i32()?,
                d: reader.i32()?,
            },
            4 => ServerMessage::Pong { stamp: reader.varint()? },
            _ => return Err("bad server tag"),
        })
    }
}

pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.varint(payload.len() as u64);
    writer.raw(payload);
    writer.into_bytes()
}

pub fn zigzag(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

pub fn unzigzag(value: u32) -> i32 {
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

pub struct Writer {
    pub bytes: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn varint(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(byte);
                return;
            }
            self.bytes.push(byte | 0x80);
        }
    }

    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn i32(&mut self, value: i32) {
        self.varint(zigzag(value) as u64);
    }

    pub fn string(&mut self, value: &str) {
        self.varint(value.len() as u64);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    pub fn raw(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn u8(&mut self) -> Result<u8, Error> {
        if self.pos >= self.bytes.len() {
            return Err("unexpected end of frame");
        }
        let value = self.bytes[self.pos];
        self.pos += 1;
        Ok(value)
    }

    pub fn varint(&mut self) -> Result<u64, Error> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let byte = self.u8()?;
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift > 63 {
                return Err("varint overflow");
            }
        }
    }

    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(unzigzag(self.varint()? as u32))
    }

    pub fn string(&mut self) -> Result<String, Error> {
        let length = self.varint()? as usize;
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| "invalid utf8")
    }

    pub fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        if self.pos + length > self.bytes.len() {
            return Err("unexpected end of frame");
        }
        let slice = &self.bytes[self.pos..self.pos + length];
        self.pos += length;
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_round_trip() {
        let values = [0u64, 1, 127, 128, 300, u32::MAX as u64, u64::MAX];
        for value in values {
            let mut writer = Writer::new();
            writer.varint(value);
            let bytes = writer.into_bytes();
            let mut reader = Reader::new(&bytes);
            assert_eq!(reader.varint().unwrap(), value);
        }
    }

    #[test]
    fn zigzag_round_trips() {
        let values = [0i32, -1, 1, i32::MIN, i32::MAX, -12345];
        for value in values {
            assert_eq!(unzigzag(zigzag(value)), value);
        }
    }

    #[test]
    fn intents_round_trip() {
        let intent = Intent::new(IntentKind::PlaceMachine, -412, 88, 2);
        assert_eq!(Intent::decode(&intent.encode()).unwrap(), intent);
    }

    #[test]
    fn client_messages_round_trip() {
        let messages = [
            ClientMessage::Hello { name: "hugo".to_string(), version: PROTOCOL_VERSION },
            ClientMessage::Intent { player: 3, intent: Intent::new(IntentKind::Move, 1, -1, 0) },
            ClientMessage::Ping { stamp: 987654321 },
        ];
        for message in messages.iter() {
            assert_eq!(ClientMessage::decode(&message.encode()).unwrap(), *message);
        }
    }

    #[test]
    fn server_messages_round_trip() {
        let messages = [
            ServerMessage::Hello { version: PROTOCOL_VERSION, seed: 42 },
            ServerMessage::Snapshot { tick: 600, hash: 0xdead_beef },
            ServerMessage::Event { tick: 7, code: 3, a: -1, b: 0, c: 9, d: -42 },
            ServerMessage::Pong { stamp: 12 },
        ];
        for message in messages.iter() {
            assert_eq!(ServerMessage::decode(&message.encode()).unwrap(), *message);
        }
    }

    #[test]
    fn truncated_frames_are_rejected() {
        let bytes = ClientMessage::Ping { stamp: 5 }.encode();
        assert!(ClientMessage::decode(&bytes[..bytes.len() - 1]).is_err());
    }
}