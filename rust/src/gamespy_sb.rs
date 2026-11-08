//! GameSpy "server browsing" master protocol (TCP 28910, encoding "X").
//!
//! Used by the GameSpy 2/3 era games and their revivals (OpenSpy, BF2Hub).
//! The client sends a length-prefixed list request with an 8-byte challenge.
//! The master answers with a clear header (its own random challenges), then
//! the list encrypted with a stream cipher whose key mixes the game's secret
//! key, the client challenge and the server challenge.
//!
//! Decrypted list: our public IP (4 bytes), the default query port (2 bytes,
//! big-endian), the requested fields (count, then type byte and name each),
//! the popular values (count, then strings), then one entry per server: a flag
//! byte, the IP, optional port / private address / ICMP address, then optional
//! key values. The list ends with an entry whose IP is `255.255.255.255`.
//!
//! Based on the public GameSpy SDK cipher as published in OpenSpy
//! (`openspy-core`, `sb_crypt.cpp` and `V2Peer.cpp`) and Luigi Auriemma's
//! `enctypex_decoder.c`.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

const CHALLENGE_LEN: usize = 8;

const PRIVATE_IP_FLAG: u8 = 2;
const ICMP_IP_FLAG: u8 = 8;
const NONSTANDARD_PORT_FLAG: u8 = 16;
const NONSTANDARD_PRIVATE_PORT_FLAG: u8 = 32;
const HAS_KEYS_FLAG: u8 = 64;
const HAS_FULL_RULES_FLAG: u8 = 128;

/// The GameSpy SDK stream cipher (a card-shuffling enigma rotor).
#[derive(Clone)]
pub struct GoaCrypt {
    cards: [u8; 256],
    rotor: u8,
    ratchet: u8,
    avalanche: u8,
    last_plain: u8,
    last_cipher: u8,
}

impl GoaCrypt {
    /// Initialize from a key of 1 to 255 bytes.
    pub fn new(key: &[u8]) -> Self {
        let mut cards: [u8; 256] = std::array::from_fn(|i| i as u8);
        let mut rsum: u8 = 0;
        let mut keypos = 0usize;
        let mut mask: u32 = 255;
        for i in (1..=255u32).rev() {
            let toswap = Self::keyrand(&cards, i, mask, key, &mut rsum, &mut keypos);
            cards.swap(i as usize, toswap);
            if i & (i - 1) == 0 {
                mask >>= 1;
            }
        }
        Self {
            rotor: cards[1],
            ratchet: cards[3],
            avalanche: cards[5],
            last_plain: cards[7],
            last_cipher: cards[rsum as usize],
            cards,
        }
    }

    fn keyrand(
        cards: &[u8; 256],
        limit: u32,
        mask: u32,
        key: &[u8],
        rsum: &mut u8,
        keypos: &mut usize,
    ) -> usize {
        if limit == 0 || key.is_empty() {
            return 0;
        }
        let mut retries = 0;
        loop {
            *rsum = cards[*rsum as usize].wrapping_add(key[*keypos]);
            *keypos += 1;
            if *keypos >= key.len() {
                *keypos = 0;
                *rsum = rsum.wrapping_add(key.len() as u8);
            }
            let mut value = mask & u32::from(*rsum);
            retries += 1;
            if retries > 11 {
                value %= limit;
            }
            if value <= limit {
                return value as usize;
            }
        }
    }

    /// Shuffle the cards for the next byte; returns the keystream byte.
    fn step(&mut self) -> u8 {
        let c = &mut self.cards;
        self.ratchet = self.ratchet.wrapping_add(c[self.rotor as usize]);
        self.rotor = self.rotor.wrapping_add(1);
        let swaptemp = c[self.last_cipher as usize];
        c[self.last_cipher as usize] = c[self.ratchet as usize];
        c[self.ratchet as usize] = c[self.last_plain as usize];
        c[self.last_plain as usize] = c[self.rotor as usize];
        c[self.rotor as usize] = swaptemp;
        self.avalanche = self.avalanche.wrapping_add(c[swaptemp as usize]);
        let first = c[c[self.avalanche as usize].wrapping_add(c[self.rotor as usize]) as usize];
        let mix = c[self.last_plain as usize]
            .wrapping_add(c[self.last_cipher as usize])
            .wrapping_add(c[self.ratchet as usize]);
        first ^ c[c[mix as usize] as usize]
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        for byte in data {
            let plain = *byte;
            *byte = plain ^ self.step();
            self.last_plain = plain;
            self.last_cipher = *byte;
        }
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        for byte in data {
            let cipher = *byte;
            *byte = cipher ^ self.step();
            self.last_plain = *byte;
            self.last_cipher = cipher;
        }
    }
}

/// Mix the secret key and the master's challenge into our challenge, giving
/// the cipher key.
pub fn cipher_key(secret_key: &[u8], challenge: &[u8; 8], server_challenge: &[u8]) -> [u8; 8] {
    let mut key = *challenge;
    if secret_key.is_empty() {
        return key;
    }
    for (i, &byte) in server_challenge.iter().enumerate() {
        let target = (i * usize::from(secret_key[i % secret_key.len()])) % CHALLENGE_LEN;
        key[target] ^= key[i % CHALLENGE_LEN] ^ byte;
    }
    key
}

/// Length of the clear header and the server challenge inside it, once
/// enough bytes arrived.
pub fn split_header(data: &[u8]) -> Option<(usize, &[u8])> {
    let crypt_len = usize::from(*data.first()? ^ 0xec);
    let server_len = usize::from(*data.get(crypt_len + 1)? ^ 0xea);
    let start = crypt_len + 2;
    let end = start + server_len;
    (data.len() >= end).then(|| (end, &data[start..end]))
}

/// Build a list request: `fields` is a backslash list such as `\hostname`.
pub fn list_request(gamename: &str, challenge: &[u8; 8], fields: &str) -> Vec<u8> {
    let mut body = vec![0x00, 0x01, 0x03, 0, 0, 0, 0];
    for name in [gamename, gamename] {
        body.extend_from_slice(name.as_bytes());
        body.push(0);
    }
    body.extend_from_slice(challenge);
    body.push(0); // empty filter
    body.extend_from_slice(fields.as_bytes());
    body.push(0);
    body.extend_from_slice(&0u32.to_be_bytes()); // options
    let mut packet = ((body.len() + 2) as u16).to_be_bytes().to_vec();
    packet.extend_from_slice(&body);
    packet
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, len: usize) -> Option<&[u8]> {
        let slice = self.data.get(self.pos..self.pos + len)?;
        self.pos += len;
        Some(slice)
    }

    fn byte(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn port(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    fn string(&mut self) -> Option<String> {
        let rest = self.data.get(self.pos..)?;
        let len = rest.iter().position(|&b| b == 0)?;
        self.pos += len + 1;
        Some(String::from_utf8_lossy(&rest[..len]).to_string())
    }
}

/// Parse a decrypted list into `ip:port` query addresses.
///
/// Returns `None` while the list is incomplete (more data needed).
pub fn parse_list(data: &[u8]) -> Option<Vec<String>> {
    let mut cursor = Cursor { data, pos: 0 };
    cursor.take(4)?; // our public IP
    let default_port = cursor.port()?;
    let field_count = cursor.byte()?;
    let mut field_types = Vec::with_capacity(usize::from(field_count));
    for _ in 0..field_count {
        field_types.push(cursor.byte()?);
        cursor.string()?;
    }
    let popular_count = cursor.byte()?;
    for _ in 0..popular_count {
        cursor.string()?;
    }

    let mut servers = Vec::new();
    loop {
        let flags = cursor.byte()?;
        let ip = cursor.take(4)?;
        let ip = Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]);
        if ip == Ipv4Addr::BROADCAST {
            return Some(servers);
        }
        let port = if flags & NONSTANDARD_PORT_FLAG != 0 {
            cursor.port()?
        } else {
            default_port
        };
        if flags & PRIVATE_IP_FLAG != 0 {
            cursor.take(4)?;
        }
        if flags & NONSTANDARD_PRIVATE_PORT_FLAG != 0 {
            cursor.take(2)?;
        }
        if flags & ICMP_IP_FLAG != 0 {
            cursor.take(4)?;
        }
        if flags & HAS_KEYS_FLAG != 0 {
            for &kind in &field_types {
                match kind {
                    1 => {
                        cursor.byte()?;
                    }
                    2 => {
                        cursor.port()?;
                    }
                    _ => {
                        let index = cursor.byte()?;
                        // 0xff: inline string; otherwise an index into `popular`.
                        if index == 0xff {
                            cursor.string()?;
                        }
                    }
                }
            }
        }
        if flags & HAS_FULL_RULES_FLAG != 0 {
            loop {
                let key = cursor.string()?;
                if key.is_empty() {
                    break;
                }
                cursor.string()?;
            }
        }
        if !ip.is_unspecified() && port != 0 {
            servers.push(format!("{ip}:{port}"));
        }
    }
}

fn random_challenge() -> [u8; 8] {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0x5eed)
        ^ u64::from(std::process::id()).rotate_left(32);
    let mut state = seed | 1;
    std::array::from_fn(|_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        b'!' + (state % 94) as u8
    })
}

/// Download the server list of `gamename` from a server browsing master.
pub fn master_list(
    address: &str,
    gamename: &str,
    key: &str,
    timeout: Duration,
) -> io::Result<Vec<String>> {
    let target = address.to_socket_addrs()?.next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("cannot resolve {address}"))
    })?;
    let mut stream = TcpStream::connect_timeout(&target, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let challenge = random_challenge();
    stream.write_all(&list_request(gamename, &challenge, "\\hostname"))?;

    let mut raw = Vec::new();
    let mut plain = Vec::new();
    let mut crypt: Option<(GoaCrypt, usize)> = None;
    let mut buffer = [0u8; 16384];
    let deadline = Instant::now() + timeout * 4;
    loop {
        let size = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => size,
            Err(error) if raw.is_empty() => return Err(error),
            Err(_) => break,
        };
        raw.extend_from_slice(&buffer[..size]);
        if crypt.is_none()
            && let Some((header_len, server_challenge)) = split_header(&raw)
        {
            let key = cipher_key(key.as_bytes(), &challenge, server_challenge);
            crypt = Some((GoaCrypt::new(&key), header_len));
        }
        if let Some((cipher, done)) = crypt.as_mut() {
            let mut chunk = raw[*done..].to_vec();
            cipher.decrypt(&mut chunk);
            *done = raw.len();
            plain.extend_from_slice(&chunk);
            if let Some(list) = parse_list(&plain) {
                return Ok(list);
            }
        }
        if Instant::now() > deadline {
            break;
        }
    }

    if !raw.is_empty() && crypt.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "master refused the request: {}",
                String::from_utf8_lossy(&raw).trim()
            ),
        ));
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "incomplete or undecodable server list (wrong game key?)",
    ))
}
