use factorio_proto::Error;

pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
pub const MAX_FRAME_BYTES: usize = 1 << 16;

const OP_CONTINUATION: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xa;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Binary(Vec<u8>),
    Text(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    let bit_length = (data.len() as u64).wrapping_mul(8);
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    for chunk in message.chunks(64) {
        let mut words = [0u32; 80];
        for (index, word) in chunk.chunks(4).enumerate() {
            words[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) =
            (state[0], state[1], state[2], state[3], state[4]);
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a827999u32),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let mixed = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = mixed;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
    }

    let mut digest = [0u8; 20];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(triple >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[triple as usize & 63] as char } else { '=' });
    }
    out
}

pub fn accept_key(key: &str) -> String {
    let mut input = String::with_capacity(key.len() + WS_GUID.len());
    input.push_str(key);
    input.push_str(WS_GUID);
    base64(&sha1(input.as_bytes()))
}

pub fn handshake_response(key: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(key)
    )
    .into_bytes()
}

pub fn handshake_failure(status: &str, reason: &str) -> Vec<u8> {
    let body = reason.to_string();
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

pub fn parse_handshake(request: &[u8]) -> Result<(String, usize), Error> {
    let header_end = find_header_end(request).ok_or("incomplete websocket handshake")?;
    let head = String::from_utf8_lossy(&request[..header_end]).to_string();
    let mut key: Option<String> = None;
    let mut upgrade = false;

    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or("missing request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET" {
        return Err("websocket handshake needs GET");
    }
    if !target.starts_with('/') {
        return Err("malformed request target");
    }

    for line in lines {
        let (name, value) = match line.split_once(':') {
            Some(pair) => pair,
            None => continue,
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "sec-websocket-key" => key = Some(value.to_string()),
            "upgrade" => upgrade = value.eq_ignore_ascii_case("websocket"),
            _ => {}
        }
    }

    if !upgrade {
        return Err("missing websocket upgrade header");
    }
    let key = key.ok_or("missing Sec-WebSocket-Key")?;
    if key.is_empty() || key.len() > 64 {
        return Err("implausible Sec-WebSocket-Key");
    }
    Ok((key, header_end + 4))
}

fn find_header_end(request: &[u8]) -> Option<usize> {
    request.windows(4).position(|window| window == b"\r\n\r\n")
}

pub fn encode(frame: &Frame) -> Vec<u8> {
    let (opcode, payload) = match frame {
        Frame::Binary(data) => (OP_BINARY, data.as_slice()),
        Frame::Text(data) => (OP_TEXT, data.as_slice()),
        Frame::Ping(data) => (OP_PING, data.as_slice()),
        Frame::Pong(data) => (OP_PONG, data.as_slice()),
        Frame::Close => (OP_CLOSE, &[][..]),
    };
    let mut out = Vec::with_capacity(payload.len() + 10);
    out.push(0x80 | opcode);
    write_length(&mut out, payload.len(), false);
    out.extend_from_slice(payload);
    out
}

pub fn encode_masked(frame: &Frame, mask: [u8; 4]) -> Vec<u8> {
    let (opcode, payload) = match frame {
        Frame::Binary(data) => (OP_BINARY, data.as_slice()),
        Frame::Text(data) => (OP_TEXT, data.as_slice()),
        Frame::Ping(data) => (OP_PING, data.as_slice()),
        Frame::Pong(data) => (OP_PONG, data.as_slice()),
        Frame::Close => (OP_CLOSE, &[][..]),
    };
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | opcode);
    write_length(&mut out, payload.len(), true);
    out.extend_from_slice(&mask);
    for (index, byte) in payload.iter().enumerate() {
        out.push(byte ^ mask[index % 4]);
    }
    out
}

fn write_length(out: &mut Vec<u8>, length: usize, masked: bool) {
    let flag = if masked { 0x80 } else { 0x00 };
    if length < 126 {
        out.push(length as u8 | flag);
    } else if length <= u16::MAX as usize {
        out.push(126 | flag);
        out.extend_from_slice(&(length as u16).to_be_bytes());
    } else {
        out.push(127 | flag);
        out.extend_from_slice(&(length as u64).to_be_bytes());
    }
}

#[derive(Clone, Debug)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    fragments: Vec<u8>,
    require_masked: bool,
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::server_side()
    }
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::server_side()
    }

    pub fn server_side() -> Self {
        Self { buffer: Vec::new(), fragments: Vec::new(), require_masked: true }
    }

    pub fn client_side() -> Self {
        Self { buffer: Vec::new(), fragments: Vec::new(), require_masked: false }
    }

    pub fn pending(&self) -> usize {
        self.buffer.len()
    }

    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Frame>, Error> {
        self.buffer.extend_from_slice(data);
        let mut frames = Vec::new();
        loop {
            match self.take_one()? {
                Some(frame) => frames.push(frame),
                None => break,
            }
        }
        Ok(frames)
    }

    fn take_one(&mut self) -> Result<Option<Frame>, Error> {
        if self.buffer.len() < 2 {
            return Ok(None);
        }
        let fin = self.buffer[0] & 0x80 != 0;
        let opcode = self.buffer[0] & 0x0f;
        let masked = self.buffer[1] & 0x80 != 0;
        let short_length = (self.buffer[1] & 0x7f) as usize;

        let mut offset = 2;
        let payload_length = match short_length {
            126 => {
                if self.buffer.len() < offset + 2 {
                    return Ok(None);
                }
                let value = u16::from_be_bytes([self.buffer[offset], self.buffer[offset + 1]]) as usize;
                offset += 2;
                value
            }
            127 => {
                if self.buffer.len() < offset + 8 {
                    return Ok(None);
                }
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&self.buffer[offset..offset + 8]);
                let value = u64::from_be_bytes(bytes) as usize;
                offset += 8;
                value
            }
            other => other,
        };

        if payload_length > MAX_FRAME_BYTES {
            return Err("websocket frame too large");
        }
        if self.require_masked && !masked {
            return Err("client frames must be masked");
        }
        if !self.require_masked && masked {
            return Err("server frames must not be masked");
        }

        let payload_start = if masked {
            if self.buffer.len() < offset + 4 + payload_length {
                return Ok(None);
            }
            let mut mask = [0u8; 4];
            mask.copy_from_slice(&self.buffer[offset..offset + 4]);
            offset += 4;
            let mut payload = self.buffer[offset..offset + payload_length].to_vec();
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
            offset += payload_length;
            payload
        } else {
            if self.buffer.len() < offset + payload_length {
                return Ok(None);
            }
            let payload = self.buffer[offset..offset + payload_length].to_vec();
            offset += payload_length;
            payload
        };
        self.buffer.drain(..offset);
        let payload = payload_start;

        let frame = match opcode {
            OP_PING => Frame::Ping(payload),
            OP_PONG => Frame::Pong(payload),
            OP_CLOSE => Frame::Close,
            OP_TEXT | OP_BINARY => {
                if fin {
                    if opcode == OP_BINARY {
                        Frame::Binary(payload)
                    } else {
                        Frame::Text(payload)
                    }
                } else {
                    self.fragments.extend_from_slice(&payload);
                    return Ok(None);
                }
            }
            OP_CONTINUATION => {
                self.fragments.extend_from_slice(&payload);
                if !fin {
                    return Ok(None);
                }
                let joined = std::mem::take(&mut self.fragments);
                Frame::Binary(joined)
            }
            _ => return Err("unsupported websocket opcode"),
        };
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_known_vectors() {
        assert_eq!(
            hex(&sha1(b"")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        assert_eq!(
            hex(&sha1(&vec![b'a'; 1_000_000])),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    #[test]
    fn sha1_spans_multiple_blocks() {
        let data: Vec<u8> = (0..200u32).map(|value| value as u8).collect();
        assert_eq!(hex(&sha1(&data)).len(), 40);
        assert_ne!(hex(&sha1(&data[..199])), hex(&sha1(&data)));
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn accept_key_matches_the_rfc6455_example() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn handshake_response_is_a_valid_101() {
        let response = String::from_utf8(handshake_response("dGhlIHNhbXBsZSBub25jZQ==")).unwrap();
        assert!(response.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
        assert!(response.contains("Upgrade: websocket\r\n"));
        assert!(response.contains("Connection: Upgrade\r\n"));
        assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"));
        assert!(response.ends_with("\r\n\r\n"));
    }

    #[test]
    fn handshake_parsing_reads_the_key_case_insensitively() {
        let request = b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: WebSocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: abc123\r\n\r\n";
        let (key, consumed) = parse_handshake(request).unwrap();
        assert_eq!(key, "abc123");
        assert_eq!(consumed, request.len());
    }

    #[test]
    fn handshake_parsing_waits_for_the_full_header() {
        let request = b"GET /ws HTTP/1.1\r\nUpgrade: websocket\r\nSec-WebSocket-Key: abc\r\n";
        assert!(parse_handshake(request).is_err());
    }

    #[test]
    fn handshake_parsing_rejects_non_websocket_requests() {
        assert!(parse_handshake(b"POST /ws HTTP/1.1\r\n\r\n").is_err());
        assert!(parse_handshake(b"GET /ws HTTP/1.1\r\nHost: x\r\n\r\n").is_err());
        assert!(parse_handshake(b"GET /ws HTTP/1.1\r\nUpgrade: websocket\r\n\r\n").is_err());
    }

    #[test]
    fn encoded_frames_use_unmasked_server_format() {
        let bytes = encode(&Frame::Binary(vec![0xde, 0xad]));
        assert_eq!(bytes, vec![0x82, 0x02, 0xde, 0xad]);

        let control = encode(&Frame::Close);
        assert_eq!(control, vec![0x88, 0x00]);
    }

    #[test]
    fn encoded_frames_use_wider_length_forms() {
        let medium = encode(&Frame::Binary(vec![7u8; 200]));
        assert_eq!(&medium[..4], &[0x82, 126, 0x00, 0xc8]);

        let large = encode(&Frame::Binary(vec![9u8; 70_000]));
        assert_eq!(&large[..2], &[0x82, 127]);
    }

    #[test]
    fn decoder_unmasks_client_frames() {
        let mut decoder = FrameDecoder::new();
        let payload = b"hello world";
        let mask = [0xa1u8, 0xb2, 0xc3, 0xd4];
        let mut bytes = vec![0x82, 0x80 | payload.len() as u8];
        bytes.extend_from_slice(&mask);
        for (index, byte) in payload.iter().enumerate() {
            bytes.push(byte ^ mask[index % 4]);
        }
        assert_eq!(decoder.push(&bytes).unwrap(), vec![Frame::Binary(payload.to_vec())]);
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn decoder_reassembles_split_reads() {
        let mut decoder = FrameDecoder::new();
        let bytes = encode_masked(&Frame::Binary(vec![1, 2, 3]), [1, 1, 1, 1]);
        assert!(decoder.push(&bytes[..1]).unwrap().is_empty());
        assert_eq!(decoder.push(&bytes[1..]).unwrap(), vec![Frame::Binary(vec![1, 2, 3])]);
    }

    #[test]
    fn decoder_reads_masked_control_frames() {
        let mut decoder = FrameDecoder::new();
        let bytes = encode_masked(&Frame::Ping(vec![9, 8]), [1, 2, 3, 4]);
        assert_eq!(decoder.push(&bytes).unwrap(), vec![Frame::Ping(vec![9, 8])]);
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn masked_encoding_is_accepted_by_the_decoder() {
        for frame in [
            Frame::Binary(vec![0, 1, 2, 3]),
            Frame::Text(b"hi".to_vec()),
            Frame::Ping(Vec::new()),
            Frame::Pong(vec![7]),
            Frame::Close,
        ] {
            let mut decoder = FrameDecoder::new();
            assert_eq!(decoder.push(&encode_masked(&frame, [0xde, 0xad, 0xbe, 0xef])).unwrap(), vec![frame]);
        }
    }

    #[test]
    fn masked_encoding_uses_longer_length_forms() {
        let bytes = encode_masked(&Frame::Binary(vec![1u8; 400]), [9, 8, 7, 6]);
        assert_eq!(&bytes[..4], &[0x82, 0xfe, 0x01, 0x90]);
        let mut decoder = FrameDecoder::new();
        assert_eq!(decoder.push(&bytes).unwrap(), vec![Frame::Binary(vec![1u8; 400])]);
    }

    #[test]
    fn decoder_rejects_unmasked_client_frames() {
        let mut decoder = FrameDecoder::server_side();
        assert!(decoder.push(&[0x82, 0x01, 0x00]).is_err());
    }

    #[test]
    fn client_side_decoder_reads_unmasked_server_frames() {
        let mut decoder = FrameDecoder::client_side();
        assert_eq!(decoder.push(&encode(&Frame::Binary(vec![1, 2, 3]))).unwrap(), vec![Frame::Binary(vec![1, 2, 3])]);
        assert!(decoder.push(&encode_masked(&Frame::Binary(vec![1]), [9, 9, 9, 9])).is_err());
    }

    #[test]
    fn client_side_decoder_reassembles_split_server_frames() {
        let mut decoder = FrameDecoder::client_side();
        let bytes = encode(&Frame::Binary(vec![4u8; 300]));
        assert!(decoder.push(&bytes[..5]).unwrap().is_empty());
        assert_eq!(decoder.push(&bytes[5..]).unwrap(), vec![Frame::Binary(vec![4u8; 300])]);
    }

    #[test]
    fn decoder_rejects_oversized_frames() {
        let mut decoder = FrameDecoder::new();
        assert!(decoder.push(&[0x82, 127, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).is_err());
    }

    #[test]
    fn decoder_joins_continuation_frames() {
        let mut decoder = FrameDecoder::new();
        let mask = [4, 3, 2, 1];
        let mut start = encode_masked(&Frame::Binary(vec![b'a']), mask);
        start[0] = 0x02;
        assert!(decoder.push(&start).unwrap().is_empty());
        let mut middle = encode_masked(&Frame::Binary(vec![b'b']), mask);
        middle[0] = 0x00;
        assert!(decoder.push(&middle).unwrap().is_empty());
        let mut last = encode_masked(&Frame::Binary(vec![b'c']), mask);
        last[0] = 0x80;
        assert_eq!(decoder.push(&last).unwrap(), vec![Frame::Binary(vec![b'a', b'b', b'c'])]);
    }

    #[test]
    fn decoder_rejects_unknown_opcodes() {
        let mut decoder = FrameDecoder::new();
        let mut bytes = encode_masked(&Frame::Binary(vec![1]), [0, 0, 0, 0]);
        bytes[0] = 0x8b;
        assert!(decoder.push(&bytes).is_err());
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}