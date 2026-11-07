//! Valve A2S server queries (Half-Life, Counter-Strike 1.6, Source games).
//!
//! See the Valve Developer Wiki "Server queries" page. Since 2020 servers may
//! answer any request with an `A` challenge, which must be echoed back.

use std::collections::BTreeMap;
use std::io;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

use crate::model::{Player, Server};

const HEADER: [u8; 4] = [0xff; 4];
const SPLIT: [u8; 4] = [0xfe, 0xff, 0xff, 0xff];

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// Little cursor over a response body.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u8(&mut self) -> io::Result<u8> {
        let value = *self
            .data
            .get(self.pos)
            .ok_or_else(|| invalid("truncated A2S packet"))?;
        self.pos += 1;
        Ok(value)
    }

    fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let slice = self
            .data
            .get(self.pos..self.pos + N)
            .ok_or_else(|| invalid("truncated A2S packet"))?;
        self.pos += N;
        Ok(slice.try_into().unwrap())
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.bytes::<4>()?))
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.bytes::<4>()?))
    }

    fn string(&mut self) -> io::Result<String> {
        let rest = self.data.get(self.pos..).unwrap_or_default();
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("unterminated A2S string"))?;
        self.pos += end + 1;
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    }

    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }
}

/// Fields of an `A2S_INFO` answer (both the `I` and old GoldSrc `m` formats).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    pub name: String,
    pub map: String,
    pub folder: String,
    pub game: String,
    pub players: u8,
    pub max_players: u8,
    pub bots: u8,
    pub server_type: char,
    pub environment: char,
    pub password: bool,
    pub vac: bool,
    pub version: String,
}

/// Parse an `A2S_INFO` body (starting at the type byte).
pub fn parse_info(body: &[u8]) -> io::Result<Info> {
    let mut r = Reader::new(body);
    match r.u8()? {
        b'I' => {
            let _protocol = r.u8()?;
            let mut info = Info {
                name: r.string()?,
                map: r.string()?,
                folder: r.string()?,
                game: r.string()?,
                ..Info::default()
            };
            let _app_id = r.bytes::<2>()?;
            info.players = r.u8()?;
            info.max_players = r.u8()?;
            info.bots = r.u8()?;
            info.server_type = r.u8()? as char;
            info.environment = r.u8()? as char;
            info.password = r.u8()? != 0;
            info.vac = r.u8()? != 0;
            info.version = r.string().unwrap_or_default();
            Ok(info)
        }
        b'm' => {
            let _address = r.string()?;
            let mut info = Info {
                name: r.string()?,
                map: r.string()?,
                folder: r.string()?,
                game: r.string()?,
                ..Info::default()
            };
            info.players = r.u8()?;
            info.max_players = r.u8()?;
            let _protocol = r.u8()?;
            info.server_type = r.u8()? as char;
            info.environment = r.u8()? as char;
            info.password = r.u8()? != 0;
            if r.u8()? == 1 {
                r.string()?;
                r.string()?;
                r.u8()?;
                r.i32()?;
                r.i32()?;
                r.u8()?;
                r.u8()?;
            }
            info.vac = r.u8()? != 0;
            info.bots = r.u8().unwrap_or(0);
            Ok(info)
        }
        other => Err(invalid(&format!("unexpected A2S_INFO type 0x{other:02x}"))),
    }
}

/// Parse an `A2S_PLAYER` body (starting at `D`).
pub fn parse_players(body: &[u8]) -> io::Result<Vec<Player>> {
    let mut r = Reader::new(body);
    if r.u8()? != b'D' {
        return Err(invalid("unexpected A2S_PLAYER type"));
    }
    let count = r.u8()?;
    let mut players = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if r.done() {
            break;
        }
        let _index = r.u8()?;
        let name = r.string()?;
        let score = r.i32()?;
        let duration = r.f32()?;
        players.push(Player {
            name,
            score,
            ping: None,
            time_secs: (duration.is_finite() && duration >= 0.0).then_some(duration as u32),
        });
    }
    Ok(players)
}

/// Parse an `A2S_RULES` body (starting at `E`).
pub fn parse_rules(body: &[u8]) -> io::Result<Vec<(String, String)>> {
    let mut r = Reader::new(body);
    if r.u8()? != b'E' {
        return Err(invalid("unexpected A2S_RULES type"));
    }
    let count = u16::from_le_bytes(r.bytes::<2>()?);
    let mut rules = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (Ok(name), Ok(value)) = (r.string(), r.string()) else {
            break;
        };
        rules.push((name, value));
    }
    Ok(rules)
}

/// Reassemble GoldSrc split packets (`FE FF FF FF`, id, index<<4 | total).
pub fn reassemble_goldsrc(packets: &[Vec<u8>]) -> Option<Vec<u8>> {
    let first = packets.first()?;
    let total = (*first.get(8)? & 0x0f) as usize;
    if total == 0 || packets.len() < total {
        return None;
    }
    let mut parts: Vec<(usize, &[u8])> = packets
        .iter()
        .filter_map(|packet| Some(((*packet.get(8)? >> 4) as usize, packet.get(9..)?)))
        .collect();
    parts.sort_by_key(|(index, _)| *index);
    parts.dedup_by_key(|(index, _)| *index);
    if parts.len() != total {
        return None;
    }
    let joined: Vec<u8> = parts
        .into_iter()
        .flat_map(|(_, data)| data.iter().copied())
        .collect();
    joined.strip_prefix(&HEADER).map(<[u8]>::to_vec)
}

struct Connection {
    socket: UdpSocket,
}

impl Connection {
    fn open(host: &str, port: u16, timeout: Duration) -> io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(timeout))?;
        socket.connect((host, port))?;
        Ok(Self { socket })
    }

    /// Receive one logical answer, joining split packets. Returns the body after `FF FF FF FF`.
    fn receive(&self) -> io::Result<Vec<u8>> {
        let mut buffer = [0u8; 4096];
        let size = self.socket.recv(&mut buffer)?;
        let packet = buffer[..size].to_vec();
        if let Some(body) = packet.strip_prefix(&HEADER) {
            return Ok(body.to_vec());
        }
        if !packet.starts_with(&SPLIT) {
            return Err(invalid("not an A2S packet"));
        }
        let mut packets = vec![packet];
        let total = (packets[0].get(8).copied().unwrap_or(0) & 0x0f) as usize;
        while packets.len() < total {
            let size = self.socket.recv(&mut buffer)?;
            packets.push(buffer[..size].to_vec());
        }
        reassemble_goldsrc(&packets).ok_or_else(|| invalid("cannot reassemble split A2S packet"))
    }

    /// Send a request, answering up to three challenges. `with_challenge`
    /// builds the request for a given 4-byte challenge; `expected` lists the
    /// answer types to accept. Other packets are skipped: GoldSrc servers
    /// answer `A2S_INFO` twice (`m` then `I`), and the extra one would
    /// otherwise be read as the next answer.
    fn request(
        &self,
        expected: &[u8],
        with_challenge: impl Fn(Option<[u8; 4]>) -> Vec<u8>,
    ) -> io::Result<(Vec<u8>, Duration)> {
        let mut challenge = None;
        for _ in 0..4 {
            let started = Instant::now();
            self.socket.send(&with_challenge(challenge))?;
            let mut stray = 0;
            let body = loop {
                let body = self.receive()?;
                match body.first() {
                    Some(kind) if *kind == b'A' || expected.contains(kind) => break body,
                    _ if stray < 3 => stray += 1,
                    _ => return Err(invalid("unexpected A2S answer")),
                }
            };
            let rtt = started.elapsed();
            if body[0] == b'A' && body.len() >= 5 {
                challenge = Some(body[1..5].try_into().unwrap());
                continue;
            }
            return Ok((body, rtt));
        }
        Err(invalid("server kept sending challenges"))
    }
}

fn info_request(challenge: Option<[u8; 4]>) -> Vec<u8> {
    let mut request = HEADER.to_vec();
    request.extend_from_slice(b"TSource Engine Query\0");
    if let Some(challenge) = challenge {
        request.extend_from_slice(&challenge);
    }
    request
}

fn challenge_request(kind: u8) -> impl Fn(Option<[u8; 4]>) -> Vec<u8> {
    move |challenge| {
        let mut request = HEADER.to_vec();
        request.push(kind);
        request.extend_from_slice(&challenge.unwrap_or([0xff; 4]));
        request
    }
}

/// Measure the round-trip time of one `A2S_INFO` exchange.
pub fn ping(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    let connection = Connection::open(host, port, timeout)?;
    connection.request(b"Im", info_request).map(|(_, rtt)| rtt)
}

/// Full status: info, players and (best effort) rules.
pub fn status(
    game: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let connection = Connection::open(host, port, timeout)?;
    let (body, rtt) = connection.request(b"Im", info_request)?;
    let info = parse_info(&body)?;

    let players = connection
        .request(b"D", challenge_request(b'U'))
        .and_then(|(body, _)| parse_players(&body))
        .unwrap_or_default();
    let mut rules: BTreeMap<String, String> = connection
        .request(b"E", challenge_request(b'V'))
        .and_then(|(body, _)| parse_rules(&body))
        .map(|rules| rules.into_iter().collect())
        .unwrap_or_default();

    rules.insert("hostname".into(), info.name.clone());
    rules.insert("mapname".into(), info.map.clone());
    rules.insert("folder".into(), info.folder.clone());
    rules.insert("game".into(), info.game.clone());
    rules.insert("secure".into(), if info.vac { "1" } else { "0" }.into());
    rules.insert(
        "password".into(),
        if info.password { "1" } else { "0" }.into(),
    );
    rules.insert("os".into(), info.environment.to_string());
    if !info.version.is_empty() {
        rules.insert("version".into(), info.version.clone());
    }

    let mut server = Server::new(game, host, port);
    server.online = true;
    server.name = info.name;
    server.map = info.map;
    server.mod_name = info.folder;
    server.num_players = info.players as usize;
    server.max_players = Some(info.max_players as usize);
    server.bots = info.bots as usize;
    server.password = info.password;
    server.location = crate::location::server_location(&rules);
    server.players = players;
    server.rules = rules;
    Ok((server, rtt))
}
