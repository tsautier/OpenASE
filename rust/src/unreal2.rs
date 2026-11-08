//! Unreal Engine 2 support: Unreal Tournament 2003 and 2004.
//!
//! Servers answer a binary UDP query on the game port + 1. A request is
//! `80 00 00 00` followed by a type byte (0 server info, 1 rules, 2 players).
//! Answers start with a 4-byte little-endian protocol number (128 for UT2004,
//! 121 for UT2003) and the type byte. Integers are 32-bit little-endian and
//! strings carry an Unreal compact-index length (negative for UTF-16) that
//! includes the trailing NUL. Text may contain `1B r g b` color codes.
//!
//! Server lists come from Epic's binary master protocol (TCP 28902), now run
//! by community masters such as 333networks and OpenSpy. Every packet has a
//! 4-byte little-endian length prefix. The masters do not check the CD key
//! hash, so OpenASE sends an all-zero placeholder instead of a real key.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use crate::model::{Player, Server};

/// Protocol number in UT2004 answers (UT2003 uses 121).
pub const UT2004_VERSION: u8 = 0x80;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// Build a query request for `kind` (0 info, 1 rules, 2 players).
pub fn request(kind: u8) -> [u8; 5] {
    [UT2004_VERSION, 0, 0, 0, kind]
}

/// True when `packet` looks like a UE2 query answer of any type.
pub fn is_answer(packet: &[u8]) -> bool {
    packet.len() >= 5 && packet[1..4] == [0, 0, 0] && matches!(packet[0], 0x70..=0x8f)
}

/// Remove `1B r g b` color codes from raw text.
pub fn strip_colors(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            chars.by_ref().take(3).for_each(drop);
        } else {
            out.push(c);
        }
    }
    out
}

/// Cursor over a UE2 answer or master packet.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let slice = self
            .data
            .get(self.pos..self.pos + n)
            .ok_or_else(|| invalid("truncated Unreal packet"))?;
        self.pos += n;
        Ok(slice)
    }

    pub fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    /// Unreal `FCompactIndex`: sign and "more" bits, then 6 + 7n value bits.
    pub fn compact(&mut self) -> io::Result<i32> {
        let first = self.u8()?;
        let mut value = i64::from(first & 0x3f);
        if first & 0x40 != 0 {
            let mut shift = 6;
            loop {
                let byte = self.u8()?;
                value |= i64::from(byte & 0x7f) << shift;
                shift += 7;
                if byte & 0x80 == 0 || shift > 27 {
                    break;
                }
            }
        }
        let value = i32::try_from(value).map_err(|_| invalid("bad compact index"))?;
        Ok(if first & 0x80 != 0 { -value } else { value })
    }

    /// Length-prefixed string with color codes removed. 8-bit strings are
    /// Latin-1, but some servers send UTF-8, which is used when valid.
    pub fn string(&mut self) -> io::Result<String> {
        let length = self.compact()?;
        let text = if length < 0 {
            let bytes = self.take(length.unsigned_abs() as usize * 2)?;
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect();
            strip_colors(&String::from_utf16_lossy(&units))
        } else {
            let bytes = self.take(length as usize)?;
            let mut plain = Vec::with_capacity(bytes.len());
            let mut iter = bytes.iter();
            while let Some(&b) = iter.next() {
                if b == 0x1b {
                    iter.by_ref().take(3).for_each(drop);
                } else {
                    plain.push(b);
                }
            }
            match String::from_utf8(plain) {
                Ok(text) => text,
                Err(error) => error.as_bytes().iter().map(|&b| b as char).collect(),
            }
        };
        Ok(text.trim_end_matches('\0').to_string())
    }
}

/// Fields of a server info (type 0) answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    /// Protocol number from the header (128 UT2004, 121 UT2003).
    pub version: u8,
    pub game_port: u32,
    pub query_port: u32,
    pub name: String,
    pub map: String,
    pub game_type: String,
    pub num_players: u32,
    pub max_players: u32,
    /// UT2004 server flags (bit 0: password).
    pub flags: u32,
    pub skill: String,
}

fn body(packet: &[u8], kind: u8) -> io::Result<Reader<'_>> {
    if !is_answer(packet) || packet[4] != kind {
        return Err(invalid("unexpected Unreal answer"));
    }
    let mut reader = Reader::new(packet);
    reader.pos = 5;
    Ok(reader)
}

/// Parse a server info answer (whole packet, header included).
pub fn parse_info(packet: &[u8]) -> io::Result<Info> {
    let mut r = body(packet, 0)?;
    let _server_id = r.u32()?;
    let _ip = r.string()?;
    let mut info = Info {
        version: packet[0],
        game_port: r.u32()?,
        query_port: r.u32()?,
        name: r.string()?,
        map: r.string()?,
        game_type: r.string()?,
        num_players: r.u32()?,
        max_players: r.u32()?,
        ..Info::default()
    };
    let _ping = r.u32().unwrap_or(0);
    info.flags = r.u32().unwrap_or(0);
    info.skill = r.string().unwrap_or_default();
    Ok(info)
}

/// Parse a rules answer into key/value pairs (keys may repeat, like `Mutator`).
pub fn parse_rules(packet: &[u8]) -> io::Result<Vec<(String, String)>> {
    let mut r = body(packet, 1)?;
    let mut rules = Vec::new();
    while !r.done() {
        let (Ok(key), Ok(value)) = (r.string(), r.string()) else {
            break;
        };
        if !key.is_empty() {
            rules.push((key, value));
        }
    }
    Ok(rules)
}

/// Parse a players answer. Onslaught's fake "Red/Blue Team Score" entries
/// are skipped.
pub fn parse_players(packet: &[u8]) -> io::Result<Vec<Player>> {
    let mut r = body(packet, 2)?;
    let mut players = Vec::new();
    while !r.done() {
        let Ok(id) = r.u32() else { break };
        let (Ok(name), Ok(ping), Ok(score), Ok(_stats_id)) =
            (r.string(), r.u32(), r.i32(), r.u32())
        else {
            break;
        };
        if id == 0 && name.ends_with("Team Score") {
            continue;
        }
        players.push(Player {
            name,
            score,
            ping: Some(ping),
            time_secs: None,
        });
    }
    Ok(players)
}

/// Build a normalized server from the collected answer packets.
pub fn server_from_packets(
    game: &str,
    host: &str,
    port: u16,
    packets: &[Vec<u8>],
) -> io::Result<Server> {
    let info = packets
        .iter()
        .find_map(|packet| parse_info(packet).ok())
        .ok_or_else(|| invalid("no Unreal server info answer"))?;
    let mut rules: BTreeMap<String, String> = BTreeMap::new();
    let mut players = Vec::new();
    for packet in packets {
        if let Ok(pairs) = parse_rules(packet) {
            for (key, value) in pairs {
                rules
                    .entry(key)
                    .and_modify(|old| {
                        old.push_str(", ");
                        old.push_str(&value);
                    })
                    .or_insert(value);
            }
        } else if let Ok(list) = parse_players(packet) {
            players.extend(list);
        }
    }
    let password = rules
        .get("GamePassword")
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        || (info.version == UT2004_VERSION && info.flags & 1 != 0);
    rules.insert("gametype".into(), info.game_type.clone());
    if !info.skill.is_empty() {
        rules.insert("botskill".into(), info.skill.clone());
    }

    let mut server = Server::new(game, host, port);
    server.online = true;
    server.name = info.name;
    server.map = info.map;
    server.game_type = info.game_type;
    server.num_players = info.num_players as usize;
    server.max_players = Some(info.max_players as usize);
    server.password = password;
    server.join_port = u16::try_from(info.game_port)
        .ok()
        .filter(|&p| p != 0)
        .unwrap_or(port.saturating_sub(1));
    server.location = crate::location::server_location(&rules);
    server.players = players;
    server.rules = rules;
    Ok(server)
}

fn open(host: &str, port: u16, timeout: Duration) -> io::Result<UdpSocket> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(timeout))?;
    socket.connect((host, port))?;
    Ok(socket)
}

/// Full status: info, rules and players. `port` is the query port.
pub fn status(
    game: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let socket = open(host, port, timeout)?;
    let started = Instant::now();
    for kind in 0..3 {
        socket.send(&request(kind))?;
    }
    let mut buffer = [0u8; 4096];
    let mut packets = Vec::new();
    let mut rtt = None;
    loop {
        match socket.recv(&mut buffer) {
            Ok(size) => {
                let packet = buffer[..size].to_vec();
                if !is_answer(&packet) {
                    continue;
                }
                if packet[4] == 0 {
                    rtt.get_or_insert_with(|| started.elapsed());
                }
                packets.push(packet);
                socket.set_read_timeout(Some(Duration::from_millis(300)))?;
            }
            Err(error) if packets.is_empty() => return Err(error),
            Err(_) => break,
        }
    }
    let rtt = rtt.unwrap_or_else(|| started.elapsed());
    Ok((server_from_packets(game, host, port, &packets)?, rtt))
}

/// Round-trip time of one server info exchange.
pub fn ping(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    let socket = open(host, port, timeout)?;
    let started = Instant::now();
    socket.send(&request(0))?;
    let mut buffer = [0u8; 4096];
    loop {
        let size = socket.recv(&mut buffer)?;
        if is_answer(&buffer[..size]) && buffer[4] == 0 {
            return Ok(started.elapsed());
        }
    }
}

// --- master server ---

fn write_string(out: &mut Vec<u8>, text: &str) {
    // Short ASCII strings only: a one-byte compact index.
    out.push((text.len() + 1) as u8);
    out.extend_from_slice(text.as_bytes());
    out.push(0);
}

fn send_packet(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let mut packet = (data.len() as u32).to_le_bytes().to_vec();
    packet.extend_from_slice(data);
    stream.write_all(&packet)
}

fn read_packet(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > 1 << 16 {
        return Err(invalid("oversized master packet"));
    }
    let mut data = vec![0u8; length];
    stream.read_exact(&mut data)?;
    Ok(data)
}

/// The client hello sent after the master's challenge.
pub fn client_hello(client: &str, version: u32) -> Vec<u8> {
    let placeholder = "0".repeat(32);
    let mut hello = Vec::new();
    write_string(&mut hello, &placeholder); // CD key hash
    write_string(&mut hello, &placeholder); // challenge response
    write_string(&mut hello, client);
    hello.extend_from_slice(&version.to_le_bytes());
    hello.push(3); // operating system: Windows
    write_string(&mut hello, "int");
    if version >= 3000 {
        hello.extend_from_slice(&[0; 13]); // GPU device, vendor, CPU speed, CPU type
    }
    hello
}

/// Parse one server record of a list answer into its query address.
pub fn parse_master_record(record: &[u8]) -> Option<String> {
    let mut r = Reader::new(record);
    let ip = Ipv4Addr::from(r.u32().ok()?.to_le_bytes());
    let game_port = r.u16().ok()?;
    let query_port = r.u16().ok()?;
    let port = if query_port == 0 {
        game_port.checked_add(1)?
    } else {
        query_port
    };
    (!ip.is_unspecified() && port != 0).then(|| format!("{ip}:{port}"))
}

/// Download the server list from a UE2 master (`client` is `UT2K4CLIENT`
/// with version 3369 for UT2004, `CLIENT` with 2225 for UT2003).
pub fn master_list(
    address: &str,
    client: &str,
    version: u32,
    timeout: Duration,
) -> io::Result<Vec<String>> {
    let target = address.to_socket_addrs()?.next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("cannot resolve {address}"))
    })?;
    let mut stream = TcpStream::connect_timeout(&target, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let _challenge = read_packet(&mut stream)?;
    send_packet(&mut stream, &client_hello(client, version))?;
    let answer = read_packet(&mut stream)?;
    if Reader::new(&answer).string()? != "APPROVED" {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "master did not approve the client",
        ));
    }
    if version >= 3000 {
        let mut packages = Vec::new();
        write_string(&mut packages, &"0".repeat(32));
        send_packet(&mut stream, &packages)?;
        let verified = read_packet(&mut stream)?;
        if Reader::new(&verified).string()? != "VERIFIED" {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "master did not verify the client",
            ));
        }
    }
    // Query with no filters.
    send_packet(&mut stream, &[0, 0])?;
    let head = read_packet(&mut stream)?;
    let count = Reader::new(&head).u32()? as usize;

    let deadline = Instant::now() + timeout * 4;
    let mut addresses = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        match read_packet(&mut stream) {
            Ok(record) => addresses.extend(parse_master_record(&record)),
            Err(_) if !addresses.is_empty() => break,
            Err(error) => return Err(error),
        }
        if Instant::now() > deadline {
            break;
        }
    }
    Ok(addresses)
}
