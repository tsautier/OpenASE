//! Red Storm engine status queries: Ghost Recon and The Sum of All Fears.
//!
//! Servers answer a binary UDP query on the game port + 2 (2348 for the
//! default game port 2346). Every packet starts with `C0 DE F1 11`, a start
//! flag `0x42` and a 16-bit little-endian length of the rest. The status
//! request body is `F5 03 00 78 30 63`.
//!
//! The answer body starts with a 14-byte block `F4 03 <game> <patch...>`
//! where the third byte tells the games apart (20 Ghost Recon, 11 Ghost
//! Recon demo, 13 The Sum of All Fears, 14 its demo). Four unknown bytes
//! follow, then strings with a 32-bit little-endian length: server name,
//! map, mission and mission type; a password flag (second of two bytes);
//! maximum and current player counts (32-bit, the first slot being the
//! server itself) and one name per player. After 17 unknown bytes come the
//! version and mod strings, a dedicated flag, the time played (32-bit
//! float), the round status (3 joining, 4 playing, 5 debrief), three
//! unknown bytes, the game mode (2 coop, 3 solo, 4 team), three unknown
//! bytes, the message of the day, the spawn type and the round length.
//!
//! Sources: qstat's Ghost Recon support (layout, request, patch blocks) and
//! the game bytes ASE used to tell the Red Storm titles apart. No official
//! or community master lists these games any more, so servers are added by
//! hand or found on the LAN.

use std::collections::BTreeMap;
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use crate::model::{Player, Server};

/// Packet header.
pub const HEADER: [u8; 4] = [0xc0, 0xde, 0xf1, 0x11];

/// Start flag after the header.
const START_FLAG: u8 = 0x42;

/// Status (server and player) request.
pub const REQUEST: [u8; 13] = [
    0xc0, 0xde, 0xf1, 0x11, 0x42, 0x06, 0x00, 0xf5, 0x03, 0x00, 0x78, 0x30, 0x63,
];

/// Longest name, map or mission string a server sends.
const SHORT_LEN: usize = 75;
/// Longest version, mod list or message string.
const LONG_LEN: usize = 500;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// Catalog game for a game byte.
pub fn game_for_byte(byte: u8) -> Option<&'static str> {
    match byte {
        20 => Some("ghostrecon"),
        11 => Some("ghostrecond"),
        13 => Some("soaf"),
        14 => Some("soafd"),
        _ => None,
    }
}

/// Fields of a status answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// Game byte (see [`game_for_byte`]).
    pub game_byte: u8,
    pub name: String,
    pub map: String,
    pub mission: String,
    pub mission_type: String,
    pub password: bool,
    /// Player slots, not counting the server's own slot.
    pub max_players: usize,
    /// Player names, without the server's own slot.
    pub players: Vec<String>,
    /// Remaining fields, by readable name, when the packet goes that far.
    pub rules: BTreeMap<String, String>,
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(count)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| invalid("status answer is truncated"))?;
        let bytes = &self.data[self.at..end];
        self.at = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> io::Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn string(&mut self, max: usize) -> io::Result<String> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(invalid("string field is too long"));
        }
        let bytes = self.take(len)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(crate::text::decode(&bytes[..end]))
    }
}

/// True when `packet` starts like a Red Storm status answer.
pub fn is_answer(packet: &[u8]) -> bool {
    packet.len() >= 10
        && packet[..4] == HEADER
        && packet[4] == START_FLAG
        && packet[7..9] == [0xf4, 0x03]
}

/// Format seconds as `m:ss`.
fn minutes(seconds: f32) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 {
        seconds as u32
    } else {
        0
    };
    format!("{}:{:02}", total / 60, total % 60)
}

/// Parse a status answer.
pub fn parse_status(packet: &[u8]) -> io::Result<Status> {
    if !is_answer(packet) {
        return Err(invalid("not a Red Storm status answer"));
    }
    let length = u16::from_le_bytes([packet[5], packet[6]]) as usize;
    if length != packet.len() - 7 {
        return Err(invalid("status answer length mismatch"));
    }
    let mut reader = Reader {
        data: &packet[7..],
        at: 0,
    };
    let block = reader.take(14)?;
    let mut status = Status {
        game_byte: block[2],
        ..Status::default()
    };
    let patch = match (block[3], block[4]) {
        (2, 10) => "1.2.10",
        (3, 5) => "1.3",
        (4, 0) => "1.4",
        _ => "",
    };
    if !patch.is_empty() {
        status.rules.insert("patch".into(), patch.into());
    }
    reader.take(4)?;
    status.name = reader.string(SHORT_LEN)?;
    status.map = reader.string(SHORT_LEN)?;
    status.mission = reader.string(SHORT_LEN)?;
    status.mission_type = reader.string(SHORT_LEN)?;
    status.password = reader.take(2)?[1] != 0;
    let max = reader.u32()? as usize;
    let count = reader.u32()? as usize;
    if max > 64 || count > max {
        return Err(invalid("player counts out of range"));
    }
    status.max_players = max.saturating_sub(1);
    for index in 0..count {
        let name = reader.string(SHORT_LEN)?;
        // The first slot is the server program itself.
        if index > 0 {
            status.players.push(name);
        }
    }
    // The rest is optional: keep what was read if a later patch differs.
    let _ = parse_tail(&mut reader, &mut status.rules);
    Ok(status)
}

fn parse_tail(reader: &mut Reader, rules: &mut BTreeMap<String, String>) -> io::Result<()> {
    reader.take(17)?;
    rules.insert("version".into(), reader.string(SHORT_LEN)?);
    let mods = reader.string(LONG_LEN)?;
    rules.insert(
        "mods".into(),
        mods.replace("\\mods\\", " ").trim().to_string(),
    );
    let dedicated = reader.u8()? != 0;
    rules.insert("dedicated".into(), u8::from(dedicated).to_string());
    let played = reader.f32()?;
    rules.insert("timeplayed".into(), minutes(played));
    let state = match reader.u8()? {
        3 => "Joining",
        4 => "Playing",
        5 => "Debrief",
        _ => "Undefined",
    };
    rules.insert("status".into(), state.into());
    reader.take(3)?;
    let mode = match reader.u8()? {
        2 => "Coop",
        3 => "Solo",
        4 => "Team",
        _ => "",
    };
    if !mode.is_empty() {
        rules.insert("gamemode".into(), mode.into());
    }
    reader.take(3)?;
    rules.insert("motd".into(), reader.string(LONG_LEN)?);
    let spawn = match reader.u32()? {
        0 => "None",
        1 => "Individual",
        2 => "Team",
        3 => "Infinite",
        _ => "Unknown",
    };
    rules.insert("spawntype".into(), spawn.into());
    let length = reader.f32()?;
    rules.insert("gametime".into(), minutes(length));
    rules.insert("timeleft".into(), minutes(length - played));
    Ok(())
}

/// Build the normalized server from a parsed answer.
pub fn server_from_status(game: &str, host: &str, port: u16, status: Status) -> Server {
    let mut server = Server::new(game, host, port);
    server.online = true;
    server.name = status.name;
    server.map = status.map;
    server.password = status.password;
    server.max_players = Some(status.max_players);
    server.players = status
        .players
        .into_iter()
        .map(|name| Player {
            name,
            ..Player::default()
        })
        .collect();
    server.num_players = server.players.len();
    let mut rules = status.rules;
    server.game_type = rules.get("gamemode").cloned().unwrap_or_default();
    if server.game_type == "Coop" && !status.mission.is_empty() {
        rules.insert("mission".into(), status.mission);
    }
    if !status.mission_type.is_empty() {
        rules.insert("missiontype".into(), status.mission_type);
    }
    rules.insert("mapname".into(), server.map.clone());
    rules.insert("hostname".into(), server.name.clone());
    rules.insert("password".into(), u8::from(server.password).to_string());
    rules.insert("gamebyte".into(), status.game_byte.to_string());
    server.rules = rules;
    server
}

fn open(host: &str, port: u16, timeout: Duration) -> io::Result<UdpSocket> {
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

fn exchange(host: &str, port: u16, timeout: Duration) -> io::Result<(Status, Duration)> {
    let socket = open(host, port, timeout)?;
    let started = Instant::now();
    socket.send(&REQUEST)?;
    let mut buffer = [0u8; 8192];
    loop {
        let size = socket.recv(&mut buffer)?;
        if is_answer(&buffer[..size]) {
            let rtt = started.elapsed();
            return Ok((parse_status(&buffer[..size])?, rtt));
        }
    }
}

/// Query a server's status port. Returns the normalized server and the
/// round-trip time.
pub fn status(
    game: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let (status, rtt) = exchange(host, port, timeout)?;
    Ok((server_from_status(game, host, port, status), rtt))
}

/// Round-trip time of one status exchange.
pub fn ping(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    exchange(host, port, timeout).map(|(_, rtt)| rtt)
}
