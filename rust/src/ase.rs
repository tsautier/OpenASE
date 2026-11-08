//! The All-Seeing Eye query protocol, embedded in games through the ASE SDK
//! (Multi Theft Auto, and in its day Far Cry, Painkiller, Chrome...).
//!
//! The client sends `s` over UDP to the game's ASE port (the host port + 123
//! for Multi Theft Auto). The answer starts with `EYE1`, then strings that are
//! prefixed by one length byte counting itself: game, port, server name, game
//! type, map, version, password, players, max players. Rules follow as
//! key/value strings up to an empty key, then one entry per player: a flags
//! byte (1 name, 2 team, 4 skin, 8 score, 16 ping, 32 time) and the strings
//! it announces.
//!
//! The Multi Theft Auto master list (`https://master.multitheftauto.com/ase/mta/`)
//! is a big-endian binary list, see [`parse_mta_list`].

use std::collections::BTreeMap;
use std::io;
use std::net::{Ipv4Addr, UdpSocket};
use std::time::{Duration, Instant};

use crate::model::{Player, Server};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// A parsed `EYE1` answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AseReply {
    pub game: String,
    pub port: u16,
    pub name: String,
    pub game_type: String,
    pub map: String,
    pub version: String,
    pub password: bool,
    pub num_players: usize,
    pub max_players: usize,
    pub rules: Vec<(String, String)>,
    pub players: Vec<Player>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn u8(&mut self) -> io::Result<u8> {
        let value = *self
            .data
            .get(self.pos)
            .ok_or_else(|| invalid("truncated ASE answer"))?;
        self.pos += 1;
        Ok(value)
    }

    /// String whose length byte counts itself (0 is read as empty too).
    fn string(&mut self) -> io::Result<String> {
        let length = self.u8()? as usize;
        let size = length.saturating_sub(1);
        let bytes = self
            .data
            .get(self.pos..self.pos + size)
            .ok_or_else(|| invalid("truncated ASE string"))?;
        self.pos += size;
        Ok(crate::text::decode(bytes)
            .trim_end_matches('\0')
            .to_string())
    }
}

/// Parse an `EYE1` answer.
pub fn parse_reply(data: &[u8]) -> io::Result<AseReply> {
    let rest = data
        .strip_prefix(b"EYE1")
        .ok_or_else(|| invalid("not an ASE answer"))?;
    let mut r = Reader { data: rest, pos: 0 };
    let mut reply = AseReply {
        game: r.string()?,
        port: r.string()?.trim().parse().unwrap_or(0),
        name: r.string()?,
        game_type: r.string()?,
        map: r.string()?,
        version: r.string()?,
        ..AseReply::default()
    };
    let password = r.string()?;
    reply.password = matches!(password.trim(), "1") || password.eq_ignore_ascii_case("true");
    reply.num_players = r.string()?.trim().parse().unwrap_or(0);
    reply.max_players = r.string()?.trim().parse().unwrap_or(0);

    while !r.done() {
        let key = r.string()?;
        if key.is_empty() {
            break;
        }
        let value = r.string()?;
        reply.rules.push((key, value));
    }

    while !r.done() {
        let Ok(flags) = r.u8() else { break };
        let mut player = Player::default();
        let mut complete = true;
        for bit in 0..6 {
            if flags & (1 << bit) == 0 {
                continue;
            }
            let Ok(value) = r.string() else {
                complete = false;
                break;
            };
            match bit {
                0 => player.name = value,
                3 => player.score = value.trim().parse().unwrap_or(0),
                4 => player.ping = value.trim().parse().ok(),
                5 => player.time_secs = value.trim().parse().ok(),
                _ => {}
            }
        }
        if !complete {
            break;
        }
        reply.players.push(player);
    }
    Ok(reply)
}

/// Build a normalized server from an ASE answer. `port` is the queried port.
pub fn server_from_reply(game: &str, host: &str, port: u16, reply: AseReply) -> Server {
    let mut rules: BTreeMap<String, String> = reply.rules.into_iter().collect();
    rules.insert("gametype".into(), reply.game_type.clone());
    rules.insert("version".into(), reply.version.clone());
    rules.insert("game".into(), reply.game.clone());

    let mut server = Server::new(game, host, port);
    server.online = true;
    server.name = reply.name;
    server.map = reply.map;
    server.game_type = reply.game_type;
    server.num_players = reply.num_players.max(reply.players.len());
    server.max_players = Some(reply.max_players);
    server.password = reply.password;
    if reply.port != 0 {
        server.join_port = reply.port;
    }
    server.location = crate::location::server_location(&rules);
    server.players = reply.players;
    server.rules = rules;
    server
}

fn exchange(host: &str, port: u16, timeout: Duration) -> io::Result<(Vec<u8>, Duration)> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(timeout))?;
    socket.connect((host, port))?;
    let started = Instant::now();
    socket.send(b"s")?;
    let mut buffer = vec![0u8; 16384];
    loop {
        let size = socket.recv(&mut buffer)?;
        if buffer[..size].starts_with(b"EYE1") {
            buffer.truncate(size);
            return Ok((buffer, started.elapsed()));
        }
    }
}

/// Full status from the ASE port `port`.
pub fn status(
    game: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let (data, rtt) = exchange(host, port, timeout)?;
    let reply = parse_reply(&data)?;
    Ok((server_from_reply(game, host, port, reply), rtt))
}

/// Round-trip time of one ASE exchange.
pub fn ping(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    exchange(host, port, timeout).map(|(_, rtt)| rtt)
}

// --- Multi Theft Auto master list ---

/// End marker of a version 2 MTA list.
const MTA_EOF_MARKER: u32 = 0x1234_5679;

/// Parse the MTA master list into `ip:port` game addresses.
///
/// Version 2 layout (big-endian): `u16 0`, `u16 2`, `u32 flags`, `u32
/// sequence`, `u32 count`, then per server `u16 length` (counting itself),
/// the IPv4 address (stored reversed), `u16 game port` and optional fields
/// we skip using the length; the list ends with `0x12345679`. The older
/// version 0 is `u16 count` then 4 address bytes and a `u16 query port`
/// (game port + 123) per server.
pub fn parse_mta_list(data: &[u8]) -> io::Result<Vec<String>> {
    let be16 = |pos: usize| -> Option<u16> {
        Some(u16::from_be_bytes(data.get(pos..pos + 2)?.try_into().ok()?))
    };
    let be32 = |pos: usize| -> Option<u32> {
        Some(u32::from_be_bytes(data.get(pos..pos + 4)?.try_into().ok()?))
    };
    let truncated = || invalid("truncated MTA master list");
    let count = be16(0).ok_or_else(truncated)?;
    let mut addresses = Vec::new();

    if count != 0 {
        let mut pos = 2;
        for _ in 0..count {
            let Some(entry) = data.get(pos..pos + 6) else {
                break;
            };
            let ip = Ipv4Addr::new(entry[0], entry[1], entry[2], entry[3]);
            let query_port = u16::from_be_bytes([entry[4], entry[5]]);
            if let Some(port) = query_port.checked_sub(123).filter(|&p| p != 0) {
                addresses.push(format!("{ip}:{port}"));
            }
            pos += 6;
        }
        return Ok(addresses);
    }

    match be16(2) {
        Some(2) => {}
        Some(version) => {
            return Err(invalid(&format!(
                "unsupported MTA master list version {version}"
            )));
        }
        None => return Err(truncated()),
    }
    if data.len() < 20 || be32(data.len() - 4) != Some(MTA_EOF_MARKER) {
        return Err(invalid("MTA master list without end marker"));
    }
    let count = be32(12).ok_or_else(truncated)?;
    let end = data.len() - 4;
    let mut pos = 16;
    for _ in 0..count {
        let Some(length) = be16(pos).map(usize::from) else {
            break;
        };
        if length < 8 || pos + length > end {
            break;
        }
        let entry = &data[pos + 2..pos + 8];
        let ip = Ipv4Addr::new(entry[3], entry[2], entry[1], entry[0]);
        let port = u16::from_be_bytes([entry[4], entry[5]]);
        if !ip.is_unspecified() && port != 0 {
            addresses.push(format!("{ip}:{port}"));
        }
        pos += length;
    }
    Ok(addresses)
}

/// Download and parse the MTA master list.
pub fn mta_master_list(url: &str, timeout: Duration) -> io::Result<Vec<String>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let data = agent
        .get(url)
        .call()
        .map_err(io::Error::other)?
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_vec()
        .map_err(io::Error::other)?;
    parse_mta_list(&data)
}
