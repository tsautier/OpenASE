//! id Tech 4 queries: Doom 3 and Quake 4.
//!
//! Servers answer a connectionless UDP query on the game port. A request is
//! `FF FF "getInfo" 00` followed by a 32-bit little-endian challenge. The
//! `infoResponse` starts with `FF FF "infoResponse" 00`, the echoed challenge,
//! a 32-bit protocol version (major in the high 16 bits, minor in the low 16),
//! then null-terminated key/value strings ended by an empty key and an empty
//! value. Player rows follow until an id byte of 32: `id (u8)`, `ping (u16)`,
//! `rate (u32)`, `name`, and for Quake 4 (protocol major 2) a clan tag string.
//! A 32-bit OS mask and a few server flags close the packet.
//!
//! Masters (the id Software ones, still answered by community revivals) take
//! `FF FF "getServers" 00`, the 32-bit protocol version, a null-terminated
//! game (mod) string, for Quake 4 two more empty strings (player and clan
//! filters), and one filter-bits byte. They answer `FF FF "servers" 00`
//! followed by 6-byte records: four address bytes and a little-endian port.
//!
//! Sources: the layouts are documented by the dhewm3 modwiki ("Server query
//! protocol"), by qstat's `doom3.c` (Artistic License; described, not copied)
//! and confirmed against live Doom 3 (dhewm3) and Quake 4 servers.
//! Enemy Territory: Quake Wars and Prey use related but different layouts and
//! have no live master, so they are not supported.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use crate::catalog::Game;
use crate::model::{Player, Server, parse_count};
use crate::quake3::{MASTER_IDLE_TIMEOUT, ParseError, parse_address};

const INFO_REQUEST: &[u8] = b"\xff\xffgetInfo\0";
const INFO_RESPONSE: &[u8] = b"\xff\xffinfoResponse\0";
const MASTER_REQUEST: &[u8] = b"\xff\xffgetServers\0";
const MASTER_RESPONSE: &[u8] = b"\xff\xffservers\0";
/// Challenge sent with every query (echoed back by the server).
pub const CHALLENGE: u32 = 0x4f41_5345;
/// Id byte that ends the player list.
const PLAYER_LIST_END: u8 = 32;

fn err<T>(message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(message.into()))
}

/// One player row of an `infoResponse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doom3Player {
    pub id: u8,
    pub ping: u16,
    pub rate: u32,
    pub name: String,
    /// Clan tag (Quake 4 only).
    pub clan: String,
}

/// A parsed `infoResponse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoResponse {
    pub challenge: u32,
    /// Protocol version: major in the high 16 bits, minor in the low 16.
    pub version: u32,
    pub rules: BTreeMap<String, String>,
    pub players: Vec<Doom3Player>,
}

/// Build a `getInfo` request.
pub fn info_request(challenge: u32) -> Vec<u8> {
    let mut packet = INFO_REQUEST.to_vec();
    packet.extend_from_slice(&challenge.to_le_bytes());
    packet
}

/// Whether `packet` is a Doom 3 family `infoResponse` (for LAN detection).
pub fn is_info_response(packet: &[u8]) -> bool {
    packet.starts_with(INFO_RESPONSE)
}

/// Parse a protocol string `major.minor` (for example `1.43`) into the
/// 32-bit version used on the wire.
pub fn parse_version(text: &str) -> Result<u32, ParseError> {
    let invalid = || ParseError(format!("invalid protocol version: {text}"));
    let (major, minor) = text.split_once('.').ok_or_else(invalid)?;
    let major: u16 = major.trim().parse().map_err(|_| invalid())?;
    let minor: u16 = minor.trim().parse().map_err(|_| invalid())?;
    Ok(u32::from(major) << 16 | u32::from(minor))
}

/// Build a `getServers` request. `version` is `major.minor`; major 2 selects
/// the Quake 4 layout with its two extra filter strings.
pub fn master_request(version: &str) -> Result<Vec<u8>, ParseError> {
    let version = parse_version(version)?;
    let mut packet = MASTER_REQUEST.to_vec();
    packet.extend_from_slice(&version.to_le_bytes());
    // Empty game (mod) filter.
    packet.push(0);
    if version >> 16 == 2 {
        // Empty player and clan filters.
        packet.extend_from_slice(&[0, 0]);
    }
    // Filter bits: none.
    packet.push(0);
    Ok(packet)
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], ParseError> {
        let end = self
            .pos
            .checked_add(count)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| ParseError("truncated infoResponse".into()))?;
        let bytes = &self.data[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ParseError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, ParseError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn string(&mut self) -> Result<String, ParseError> {
        let rest = &self.data[self.pos..];
        let Some(end) = rest.iter().position(|byte| *byte == 0) else {
            return err("unterminated string in infoResponse");
        };
        self.pos += end + 1;
        Ok(crate::text::decode(&rest[..end]))
    }
}

/// Parse an `infoResponse` packet.
pub fn parse_info_response(packet: &[u8]) -> Result<InfoResponse, ParseError> {
    let Some(rest) = packet.strip_prefix(INFO_RESPONSE) else {
        return err("not an infoResponse");
    };
    let mut cursor = Cursor { data: rest, pos: 0 };
    let challenge = cursor.u32()?;
    let version = cursor.u32()?;
    let mut rules = BTreeMap::new();
    loop {
        let key = cursor.string()?;
        let value = cursor.string()?;
        if key.is_empty() {
            break;
        }
        rules.insert(key, value);
    }
    let quake4 = version >> 16 == 2;
    let mut players = Vec::new();
    loop {
        let id = cursor.u8()?;
        if id == PLAYER_LIST_END {
            break;
        }
        let ping = cursor.u16()?;
        let rate = cursor.u32()?;
        let name = cursor.string()?;
        let clan = if quake4 {
            cursor.string()?
        } else {
            String::new()
        };
        players.push(Doom3Player {
            id,
            ping,
            rate,
            name,
            clan,
        });
    }
    Ok(InfoResponse {
        challenge,
        version,
        rules,
        players,
    })
}

/// Parse a master answer: a `servers` header and 6-byte records (address and
/// little-endian port).
pub fn parse_master_response(packet: &[u8]) -> Result<Vec<String>, ParseError> {
    let Some(body) = packet.strip_prefix(MASTER_RESPONSE) else {
        return err("unexpected master answer header");
    };
    Ok(body
        .as_chunks::<6>()
        .0
        .iter()
        .filter(|chunk| chunk[4..] != [0, 0])
        .map(|chunk| {
            format!(
                "{}.{}.{}.{}:{}",
                chunk[0],
                chunk[1],
                chunk[2],
                chunk[3],
                u16::from_le_bytes([chunk[4], chunk[5]])
            )
        })
        .collect())
}

fn connect(host: &str, port: u16, timeout: Duration) -> io::Result<UdpSocket> {
    let target = (host, port)
        .to_socket_addrs()?
        .find(|address| address.is_ipv4())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no IPv4 address for {host}"),
            )
        })?;
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(timeout))?;
    socket.connect(target)?;
    Ok(socket)
}

/// Query a server's status. Returns the normalized server and the round-trip time.
pub fn status(
    game: &Game,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let socket = connect(host, port, timeout)?;
    let started = Instant::now();
    socket.send(&info_request(CHALLENGE))?;
    let mut buffer = [0u8; 16384];
    loop {
        let size = socket.recv(&mut buffer)?;
        let rtt = started.elapsed();
        let response = parse_info_response(&buffer[..size])?;
        // Ignore stray answers to someone else's query.
        if response.challenge != CHALLENGE {
            continue;
        }
        return Ok((server_from_response(game, host, port, response), rtt));
    }
}

/// Round-trip time of one status query.
pub fn ping(game: &Game, host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    status(game, host, port, timeout).map(|(_, rtt)| rtt)
}

/// Fetch a master's list for the given `major.minor` protocol version.
pub fn master_list(address: &str, version: &str, timeout: Duration) -> io::Result<Vec<String>> {
    let request = master_request(version)?;
    let (host, port) = parse_address(address, 27650)?;
    let socket = connect(&host, port, timeout)?;
    socket.send(&request)?;
    let deadline = Instant::now() + timeout;
    let mut seen = HashSet::new();
    let mut addresses = Vec::new();
    let mut received = false;
    let mut buffer = [0u8; 65536];
    loop {
        let mut wait = deadline.saturating_duration_since(Instant::now());
        if received {
            wait = wait.min(MASTER_IDLE_TIMEOUT);
        }
        if wait.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(wait))?;
        match socket.recv(&mut buffer) {
            Ok(size) => {
                received = true;
                let entries = parse_master_response(&buffer[..size])?;
                addresses.extend(entries.into_iter().filter(|a| seen.insert(a.clone())));
            }
            Err(error) if !received => return Err(error),
            Err(_) => break,
        }
    }
    Ok(addresses)
}

/// Remove id Tech 4 `^cRGB` color escapes (three digits), which the shared
/// Quake color handling does not know. Plain `^N` codes are left in place.
pub fn strip_rgb_codes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let escape = chars[i] == '^'
            && chars.get(i + 1) == Some(&'c')
            && (2..=4).all(|offset| chars.get(i + offset).is_some_and(char::is_ascii_digit));
        if escape {
            i += 5;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Build the normalized server from a parsed response.
pub fn server_from_response(game: &Game, host: &str, port: u16, response: InfoResponse) -> Server {
    let mut server = Server::new(&game.id, host, port);
    server.online = true;
    let rules = response.rules;
    let get = |key: &str| rules.get(key).cloned().unwrap_or_default();
    server.name = strip_rgb_codes(&get("si_name"));
    // Maps are reported as paths: `game/mp/d3dm3`, `mp/q4dm1`.
    server.map = get("si_map")
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_string();
    if !game.game_type_key.is_empty() {
        server.game_type = get(&game.game_type_key);
    }
    server.mod_name = if game.mod_key.is_empty() {
        String::new()
    } else {
        get(&game.mod_key)
    };
    server.max_players = parse_count(rules.get("si_maxPlayers"));
    server.password = rules
        .get("si_usePass")
        .is_some_and(|value| value.trim().parse::<u32>().is_ok_and(|n| n > 0));
    server.players = response
        .players
        .into_iter()
        .map(|player| Player {
            name: player.name,
            score: 0,
            ping: Some(u32::from(player.ping)),
            time_secs: None,
        })
        .collect();
    server.num_players = server.players.len();
    server.rules = rules;
    server
}
