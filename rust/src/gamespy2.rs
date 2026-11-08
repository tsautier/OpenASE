//! GameSpy version 2 and version 3 ("GameSpy4") server queries.
//!
//! Both speak binary UDP packets starting with `\xfe\xfd`, a request type and
//! a 4-byte request id that the server echoes back. Strings are
//! null-terminated.
//!
//! - **Version 2** (Halo, SWAT 4...): `\xfe\xfd\x00` + id + `\xff\xff\xff`
//!   asks for rules, players and teams in one packet. Rules are `key\0value\0`
//!   pairs ended by an empty key. Players and teams each start with a `\0`
//!   separator, a count byte and a list of field names (`player_`, `score_`...) ended by an empty
//!   name, followed by one value per field and row.
//! - **Version 3** (Battlefield 2...): some servers first want a challenge
//!   (`\xfe\xfd\x09` + id, answered with a decimal number sent back as 4
//!   big-endian bytes). The query is `\xfe\xfd\x00` + id + challenge +
//!   `\xff\xff\xff\x01`. Answers are split into packets marked `splitnum` with
//!   an index byte (high bit set on the last one). Each packet holds sections:
//!   `0` rules, `1` players and `2` teams. Player and team fields are sent
//!   column by column: name, start offset byte, values ended by an empty one.

use std::collections::BTreeMap;
use std::io;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

use crate::model::{Player, Server, parse_count};

/// Request id sent with every query and echoed by the server.
pub const REQUEST_ID: [u8; 4] = [0x04, 0x05, 0x06, 0x07];

/// Rules, players and teams of one answer, in the order the server sent them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Response {
    pub rules: Vec<(String, String)>,
    /// One map per player, keyed by field name without the trailing `_`.
    pub players: Vec<BTreeMap<String, String>>,
    /// One map per team, keyed by raw field name (`team_t`, `score_t`).
    pub teams: Vec<BTreeMap<String, String>>,
}

/// Version 2 request for rules, players and teams.
pub fn v2_request() -> Vec<u8> {
    let mut packet = vec![0xfe, 0xfd, 0x00];
    packet.extend_from_slice(&REQUEST_ID);
    packet.extend_from_slice(&[0xff, 0xff, 0xff]);
    packet
}

/// Version 2 request for rules only (used to measure ping).
pub fn v2_info_request() -> Vec<u8> {
    let mut packet = vec![0xfe, 0xfd, 0x00];
    packet.extend_from_slice(&REQUEST_ID);
    packet.extend_from_slice(&[0xff, 0x00, 0x00]);
    packet
}

/// Version 3 challenge request.
pub fn v3_challenge_request() -> Vec<u8> {
    let mut packet = vec![0xfe, 0xfd, 0x09];
    packet.extend_from_slice(&REQUEST_ID);
    packet
}

/// Version 3 full query, with the challenge answer when the server wants one.
pub fn v3_request(challenge: Option<[u8; 4]>) -> Vec<u8> {
    let mut packet = vec![0xfe, 0xfd, 0x00];
    packet.extend_from_slice(&REQUEST_ID);
    if let Some(challenge) = challenge {
        packet.extend_from_slice(&challenge);
    }
    packet.extend_from_slice(&[0xff, 0xff, 0xff, 0x01]);
    packet
}

/// Read the decimal challenge from a `\x09` answer, as 4 big-endian bytes.
pub fn parse_challenge(packet: &[u8]) -> Option<[u8; 4]> {
    if packet.len() < 6 || packet[0] != 0x09 || packet[1..5] != REQUEST_ID {
        return None;
    }
    let text: String = packet[5..]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let value: i64 = text.trim().parse().ok()?;
    Some((value as u32).to_be_bytes())
}

/// Bytes to text: UTF-8 when valid, otherwise Latin-1.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// Cursor over null-terminated strings.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn byte(&mut self) -> Option<u8> {
        let value = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(value)
    }

    /// Next string; a string cut by the end of the packet is returned as is.
    fn string(&mut self) -> String {
        let rest = &self.data[self.pos.min(self.data.len())..];
        let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        self.pos += len + 1;
        decode(&rest[..len])
    }
}

/// Field name of a player column, without the trailing `_` (`player_` -> `player`).
fn player_field(name: &str) -> String {
    name.trim_end_matches('_').to_string()
}

/// Parse a version 2 answer (`\x00` + id + rules, players, teams).
pub fn parse_v2(packet: &[u8]) -> io::Result<Response> {
    if packet.len() < 5 || packet[0] != 0x00 || packet[1..5] != REQUEST_ID {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a GameSpy 2 answer",
        ));
    }
    let mut reader = Reader {
        data: packet,
        pos: 5,
    };
    let mut response = Response::default();
    while !reader.done() {
        let key = reader.string();
        if key.is_empty() {
            break;
        }
        let value = reader.string();
        response.rules.push((key, value));
    }
    response.players = read_v2_table(&mut reader, true);
    response.teams = read_v2_table(&mut reader, false);
    Ok(response)
}

/// Separator byte `\0`, count byte, field names ended by an empty one, then
/// `count` rows of values.
fn read_v2_table(reader: &mut Reader, players: bool) -> Vec<BTreeMap<String, String>> {
    if reader.data.get(reader.pos) == Some(&0) {
        reader.pos += 1;
    }
    let Some(count) = reader.byte() else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    while !reader.done() {
        let field = reader.string();
        if field.is_empty() {
            break;
        }
        fields.push(if players { player_field(&field) } else { field });
    }
    let mut rows = Vec::new();
    if fields.is_empty() {
        return rows;
    }
    for _ in 0..count {
        if reader.done() {
            break;
        }
        let row: BTreeMap<String, String> = fields
            .iter()
            .map(|field| (field.clone(), reader.string()))
            .collect();
        rows.push(row);
    }
    rows
}

/// One version 3 packet: its index, whether it is the last one, and its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V3Packet {
    pub index: u8,
    pub last: bool,
    pub data: Vec<u8>,
}

/// Split a version 3 packet into its `splitnum` header and section data.
pub fn parse_v3_packet(packet: &[u8]) -> io::Result<V3Packet> {
    if packet.len() < 5 || packet[0] != 0x00 || packet[1..5] != REQUEST_ID {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a GameSpy 3 answer",
        ));
    }
    let body = &packet[5..];
    const SPLITNUM: &[u8] = b"splitnum\0";
    if let Some(rest) = body.strip_prefix(SPLITNUM)
        && let Some((&num, data)) = rest.split_first()
    {
        return Ok(V3Packet {
            index: num & 0x7f,
            last: num & 0x80 != 0,
            data: data.to_vec(),
        });
    }
    Ok(V3Packet {
        index: 0,
        last: true,
        data: body.to_vec(),
    })
}

/// Merge version 3 packets (any order, duplicates allowed) into one answer.
pub fn merge_v3(packets: &[V3Packet]) -> Response {
    let mut sorted: Vec<&V3Packet> = packets.iter().collect();
    sorted.sort_by_key(|packet| packet.index);

    let mut rules: Vec<(String, String)> = Vec::new();
    let mut players: BTreeMap<usize, BTreeMap<String, String>> = BTreeMap::new();
    let mut teams: BTreeMap<usize, BTreeMap<String, String>> = BTreeMap::new();
    for packet in sorted {
        let mut reader = Reader {
            data: &packet.data,
            pos: 0,
        };
        while let Some(section) = reader.byte() {
            match section {
                0 => {
                    while !reader.done() {
                        let key = reader.string();
                        if key.is_empty() {
                            break;
                        }
                        let value = reader.string();
                        match rules.iter_mut().find(|(name, _)| *name == key) {
                            Some(existing) => existing.1 = value,
                            None => rules.push((key, value)),
                        }
                    }
                }
                1 | 2 => {
                    let table = if section == 1 {
                        &mut players
                    } else {
                        &mut teams
                    };
                    read_v3_columns(&mut reader, table, section == 1);
                }
                _ => break,
            }
        }
    }
    Response {
        rules,
        players: players.into_values().collect(),
        teams: teams.into_values().collect(),
    }
}

/// Columns of a player or team section: `field\0` offset byte, values, `\0`.
fn read_v3_columns(
    reader: &mut Reader,
    table: &mut BTreeMap<usize, BTreeMap<String, String>>,
    players: bool,
) {
    while !reader.done() {
        let field = reader.string();
        if field.is_empty() {
            return;
        }
        let field = if players { player_field(&field) } else { field };
        let Some(offset) = reader.byte() else {
            return;
        };
        let mut index = usize::from(offset);
        while !reader.done() {
            let value = reader.string();
            if value.is_empty() {
                break;
            }
            table.entry(index).or_default().insert(field.clone(), value);
            index += 1;
        }
    }
}

/// Build a normalized server from a version 2 or 3 answer.
pub fn server_from_response(game: &str, host: &str, port: u16, response: Response) -> Server {
    let mut rules: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in response.rules {
        rules.entry(key).or_insert(value);
    }
    for (index, team) in response.teams.iter().enumerate() {
        for (field, value) in team {
            rules
                .entry(format!("{field}{index}"))
                .or_insert(value.clone());
        }
    }

    let get = |row: &BTreeMap<String, String>, names: &[&str]| -> Option<String> {
        names.iter().find_map(|name| row.get(*name).cloned())
    };
    let players: Vec<Player> = response
        .players
        .iter()
        .map(|row| Player {
            name: get(row, &["player", "name", "playername"]).unwrap_or_default(),
            score: get(row, &["score", "frags"])
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0),
            ping: get(row, &["ping"]).and_then(|value| value.trim().parse().ok()),
            time_secs: None,
        })
        .collect();
    let bots = response
        .players
        .iter()
        .filter(|row| get(row, &["AIBot", "aibot", "bot"]).is_some_and(|value| value.trim() == "1"))
        .count();

    let mut server = Server::new(game, host, port);
    server.online = true;
    server.name = rules.get("hostname").cloned().unwrap_or_default();
    server.map = rules.get("mapname").cloned().unwrap_or_default();
    server.game_type = rules.get("gametype").cloned().unwrap_or_default();
    server.num_players = parse_count(rules.get("numplayers")).unwrap_or(players.len());
    server.max_players = parse_count(rules.get("maxplayers"));
    server.password = rules.get("password").is_some_and(|value| {
        let value = value.trim();
        value == "1" || value.eq_ignore_ascii_case("true")
    });
    server.join_port = parse_count(rules.get("hostport"))
        .and_then(|port| u16::try_from(port).ok())
        .filter(|&port| port != 0)
        .unwrap_or(port);
    server.bots = bots;
    server.location = crate::location::server_location(&rules);
    server.players = players;
    server.rules = rules;
    server
}

fn connect(host: &str, port: u16, timeout: Duration) -> io::Result<UdpSocket> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(timeout))?;
    socket.connect((host, port))?;
    Ok(socket)
}

/// Query a version 2 server. Returns the answer and the round-trip time.
pub fn query_v2(host: &str, port: u16, timeout: Duration) -> io::Result<(Response, Duration)> {
    let socket = connect(host, port, timeout)?;
    let started = Instant::now();
    socket.send(&v2_request())?;
    let mut buffer = [0u8; 8192];
    let deadline = started + timeout;
    loop {
        let size = socket.recv(&mut buffer)?;
        let rtt = started.elapsed();
        if let Ok(response) = parse_v2(&buffer[..size]) {
            return Ok((response, rtt));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "no answer"));
        }
        socket.set_read_timeout(Some(left))?;
    }
}

/// Measure the round-trip time of a version 2 rules-only request.
pub fn ping_v2(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    let socket = connect(host, port, timeout)?;
    let started = Instant::now();
    socket.send(&v2_info_request())?;
    let mut buffer = [0u8; 8192];
    socket.recv(&mut buffer)?;
    Ok(started.elapsed())
}

/// Query a version 3 server.
///
/// The plain query (Battlefield 2 style) and a challenge request are sent
/// together; servers that want a challenge only answer the second one, and
/// get the challenged query right after. Returns the merged answer and the
/// round-trip time of the first reply.
pub fn query_v3(host: &str, port: u16, timeout: Duration) -> io::Result<(Response, Duration)> {
    let socket = connect(host, port, timeout)?;
    let started = Instant::now();
    socket.send(&v3_request(None))?;
    socket.send(&v3_challenge_request())?;

    let mut buffer = [0u8; 8192];
    let mut packets: Vec<V3Packet> = Vec::new();
    let mut rtt = None;
    let mut challenged = false;
    let deadline = started + timeout * 2;
    loop {
        match socket.recv(&mut buffer) {
            Ok(size) => {
                rtt.get_or_insert_with(|| started.elapsed());
                let data = &buffer[..size];
                if let Some(challenge) = parse_challenge(data) {
                    if !challenged && packets.is_empty() {
                        challenged = true;
                        socket.send(&v3_request(Some(challenge)))?;
                    }
                } else if let Ok(packet) = parse_v3_packet(data) {
                    packets.retain(|known| known.index != packet.index);
                    packets.push(packet);
                    if is_complete(&packets) {
                        break;
                    }
                }
            }
            Err(error) if packets.is_empty() => return Err(error),
            Err(_) => break,
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let wait = if packets.is_empty() {
            left
        } else {
            left.min(Duration::from_millis(500))
        };
        socket.set_read_timeout(Some(wait))?;
    }
    if packets.is_empty() {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "no answer"));
    }
    Ok((merge_v3(&packets), rtt.unwrap_or_default()))
}

/// True once the last packet and every packet before it arrived.
pub fn is_complete(packets: &[V3Packet]) -> bool {
    let Some(last) = packets.iter().find(|packet| packet.last) else {
        return false;
    };
    (0..=last.index).all(|index| packets.iter().any(|packet| packet.index == index))
}

/// Measure the round-trip time to a version 3 server (first reply to a
/// rules-only query or to a challenge request).
pub fn ping_v3(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    let socket = connect(host, port, timeout)?;
    let started = Instant::now();
    let mut info = vec![0xfe, 0xfd, 0x00];
    info.extend_from_slice(&REQUEST_ID);
    info.extend_from_slice(&[0xff, 0x00, 0x00, 0x01]);
    socket.send(&info)?;
    socket.send(&v3_challenge_request())?;
    let mut buffer = [0u8; 8192];
    socket.recv(&mut buffer)?;
    Ok(started.elapsed())
}
