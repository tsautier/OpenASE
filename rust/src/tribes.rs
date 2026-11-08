//! Starsiege: Tribes (Tribes 1) and Tribes 2 queries.
//!
//! **Tribes 1** servers (UDP 28001 by default) answer the short "GameSpy
//! style" request `b` followed by a 2-byte key with `c`, the key echo, `b`,
//! then length-prefixed (Pascal) strings and bytes: game name, version,
//! server name, dedicated, password, players, max players, CPU speed (16-bit
//! little-endian), mod, game type, mission, info text, team count, team and
//! player score headers, one name/score pair per team and one record per
//! player (ping / 4, packet loss, team, name, tab-separated score columns).
//! The native ping `10 03 FF 00 key 00 00` is answered with `10 04 .. F0 key`,
//! max and current players and the server name. The master (UDP 28000)
//! answers the same request with `10 06 index total key 00 66`, its name and
//! message of the day, a big-endian count and 7-byte records (`06`, IPv4,
//! little-endian port).
//!
//! **Tribes 2** servers (Torque, UDP 28000 by default) answer a 6-byte
//! request: type (14 ping, 18 info), flags (2: no compression) and a 4-byte
//! key. The ping answer (16) holds a version tag (`VER3`/`VER5`), the network
//! protocol numbers, the build and the (shortened) server name. The info
//! answer (20) holds mod, mission type, map, status flags (1 dedicated,
//! 2 password, 4 Linux), players, max players, bots, CPU speed, the info text
//! and a text block with the team and player lists. The TribesNext community
//! master publishes its list over HTTP (`http://master.tribesnext.com/list`).
//!
//! Sources: qstat (`qstat.c`/`qstat.h`, Tribes and Tribes 2 sections, read
//! for the packet layouts only), TheKigen's t1net-go and t1ms-go (Tribes 1
//! query and master code, Apache-2.0), the TribesNext forum (HTTP master list)
//! and live captures from community servers.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use crate::model::{Player, Server};
use crate::quake3::{MASTER_IDLE_TIMEOUT, parse_address};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A request key that changes between queries, so stale answers are ignored.
fn new_key() -> u16 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0x2a2a);
    (nanos ^ (nanos >> 16)) as u16 | 0x0101
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

/// Cursor over a Tribes packet.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let slice = self
            .data
            .get(self.pos..self.pos + n)
            .ok_or_else(|| invalid("truncated Tribes packet"))?;
        self.pos += n;
        Ok(slice)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16_le(&mut self) -> io::Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32_le(&mut self) -> io::Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Raw bytes of a Pascal string (length byte, then data).
    fn pascal_bytes(&mut self) -> io::Result<&'a [u8]> {
        let length = self.u8()? as usize;
        self.take(length)
    }

    fn pascal(&mut self) -> io::Result<String> {
        Ok(crate::text::decode(self.pascal_bytes()?))
    }
}

/// Remove control characters (Tribes 2 name markers, stray NULs).
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

/// True when a score header column label names `target`. Labels start with
/// a column-position byte (`¦Score`, `oTeam`), except some that do not.
fn header_matches(label: &str, target: &str) -> bool {
    let label = label.trim();
    if label.eq_ignore_ascii_case(target) {
        return true;
    }
    let mut chars = label.chars();
    chars.next();
    let rest = chars.as_str();
    rest.len() == target.len() && rest.eq_ignore_ascii_case(target)
}

// --- Tribes 1 ---

/// Status request for Tribes 1: `b` and the key.
pub fn t1_status_request(key: u16) -> [u8; 3] {
    let [lo, hi] = key.to_le_bytes();
    [b'b', lo, hi]
}

/// Native ping and master list request for Tribes 1.
pub fn t1_ping_request(key: u16) -> [u8; 8] {
    let [lo, hi] = key.to_le_bytes();
    [0x10, 0x03, 0xff, 0x00, lo, hi, 0x00, 0x00]
}

/// True when `packet` is a Tribes 1 status answer (for `key`, when given).
/// The first byte is normally `c`, but some servers send 0.
pub fn is_t1_status(packet: &[u8], key: Option<u16>) -> bool {
    packet.len() >= 5
        && packet[3] == b'b'
        && key.is_none_or(|key| packet[1..3] == key.to_le_bytes())
}

/// True when `packet` is a native Tribes 1 ping answer for `key`.
pub fn is_t1_ping(packet: &[u8], key: u16) -> bool {
    packet.len() >= 10
        && packet[0] == 0x10
        && packet[1] == 0x04
        && packet[4..6] == key.to_le_bytes()
}

/// Score column of a player record, found from the player score header
/// (`Score`, else `Kills`). The first column is the player name.
fn t1_player_score(header: &[&str], record: &str) -> i32 {
    let fields: Vec<&str> = record.split('\t').collect();
    ["score", "kills"]
        .iter()
        .find_map(|target| {
            header
                .iter()
                .position(|label| header_matches(label, target))
        })
        .and_then(|column| fields.get(column))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

/// Parse the team and player part of a Tribes 1 status answer.
fn parse_t1_lists(
    r: &mut Reader<'_>,
    num_players: u8,
    rules: &mut BTreeMap<String, String>,
    players: &mut Vec<Player>,
) -> io::Result<()> {
    let teams = r.u8()?;
    rules.insert("numteams".into(), teams.to_string());
    let _team_header = r.pascal()?;
    let player_header = r.pascal()?;
    let header: Vec<&str> = player_header.split('\t').collect();
    for index in 0..teams {
        let team = r.pascal()?;
        let score = r.pascal()?;
        let score = score.rsplit('\t').next().unwrap_or("").trim().to_string();
        rules.insert(format!("team_{index}"), team);
        if !score.is_empty() {
            rules.insert(format!("score_{index}"), score);
        }
    }
    for _ in 0..num_players {
        let ping = r.u8()?;
        let _loss = r.u8()?;
        let _team = r.u8()?;
        let name = r.pascal()?;
        let record = r.pascal()?;
        players.push(Player {
            name: clean(&name),
            score: t1_player_score(&header, &record),
            ping: Some(u32::from(ping) * 4),
            time_secs: None,
        });
    }
    Ok(())
}

/// Parse a Tribes 1 status answer (whole packet) into a server.
pub fn parse_t1_status(game: &str, host: &str, port: u16, packet: &[u8]) -> io::Result<Server> {
    if !is_t1_status(packet, None) {
        return Err(invalid("not a Tribes status answer"));
    }
    let mut r = Reader::new(packet, 4);
    let mut rules = BTreeMap::new();
    rules.insert("gamename".to_string(), r.pascal()?);
    rules.insert("version".to_string(), r.pascal()?);
    let name = r.pascal()?;
    let dedicated = r.u8()? != 0;
    let password = r.u8()? != 0;
    let num_players = r.u8()?;
    let max_players = r.u8()?;
    let cpu = r.u16_le()?;
    let mod_name = r.pascal()?;
    let game_type = r.pascal()?;
    let map = r.pascal()?;
    let info = r.pascal()?;
    rules.insert("dedicated".into(), u8::from(dedicated).to_string());
    rules.insert("needpass".into(), u8::from(password).to_string());
    rules.insert("cpu".into(), cpu.to_string());
    rules.insert("mods".into(), mod_name.clone());
    rules.insert("game".into(), game_type.clone());
    rules.insert("info".into(), info);

    let mut server = Server::new(game, host, port);
    server.online = true;
    server.name = name;
    server.map = map;
    server.game_type = game_type;
    server.mod_name = mod_name;
    server.password = password;
    server.num_players = usize::from(num_players);
    server.max_players = Some(usize::from(max_players));

    // Teams and players: keep what parses, a truncated tail is not fatal.
    let _ = parse_t1_lists(&mut r, num_players, &mut rules, &mut server.players);
    server.location = crate::location::server_location(&rules);
    server.rules = rules;
    Ok(server)
}

/// Tribes 1 status query. Returns the server and the round-trip time.
pub fn t1_status(
    game: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let socket = open(host, port, timeout)?;
    let key = new_key();
    let started = Instant::now();
    socket.send(&t1_status_request(key))?;
    let mut buffer = [0u8; 16384];
    loop {
        let size = socket.recv(&mut buffer)?;
        if is_t1_status(&buffer[..size], Some(key)) {
            let rtt = started.elapsed();
            return Ok((parse_t1_status(game, host, port, &buffer[..size])?, rtt));
        }
    }
}

/// Round-trip time of one native Tribes 1 ping.
pub fn t1_ping(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    let socket = open(host, port, timeout)?;
    let key = new_key();
    let started = Instant::now();
    socket.send(&t1_ping_request(key))?;
    let mut buffer = [0u8; 1024];
    loop {
        let size = socket.recv(&mut buffer)?;
        if is_t1_ping(&buffer[..size], key) {
            return Ok(started.elapsed());
        }
    }
}

/// One packet of a Tribes 1 master answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct T1MasterPacket {
    /// 1-based packet number and total.
    pub index: u8,
    pub total: u8,
    pub key: u16,
    pub name: String,
    pub servers: Vec<String>,
}

/// Parse one Tribes 1 master answer packet.
pub fn parse_t1_master(packet: &[u8]) -> io::Result<T1MasterPacket> {
    if packet.len() < 10 || packet[0] != 0x10 || packet[1] != 0x06 {
        return Err(invalid("not a Tribes master answer"));
    }
    let mut r = Reader::new(packet, 2);
    let index = r.u8()?;
    let total = r.u8()?;
    let key = r.u16_le()?;
    let _ = r.take(2)?; // 00 66
    let name = r.pascal()?;
    let _motd = r.pascal_bytes()?;
    let count = u16::from_be_bytes([r.u8()?, r.u8()?]);
    let mut servers = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let Ok(record) = r.take(7) else { break };
        if record[0] != 6 {
            break;
        }
        let port = u16::from_le_bytes([record[5], record[6]]);
        if port != 0 && record[1..5] != [0, 0, 0, 0] {
            servers.push(format!(
                "{}.{}.{}.{}:{port}",
                record[1], record[2], record[3], record[4]
            ));
        }
    }
    Ok(T1MasterPacket {
        index,
        total,
        key,
        name,
        servers,
    })
}

/// Download a Tribes 1 master list (query addresses).
pub fn t1_master_list(address: &str, timeout: Duration) -> io::Result<Vec<String>> {
    let (host, port) = parse_address(address, 28000)?;
    let socket = open(&host, port, timeout)?;
    let key = new_key();
    socket.send(&t1_ping_request(key))?;
    let deadline = Instant::now() + timeout;
    let mut seen_packets = HashSet::new();
    let mut seen = HashSet::new();
    let mut addresses = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let mut wait = deadline.saturating_duration_since(Instant::now());
        if !seen_packets.is_empty() {
            wait = wait.min(MASTER_IDLE_TIMEOUT);
        }
        if wait.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(wait))?;
        match socket.recv(&mut buffer) {
            Ok(size) => {
                let Ok(packet) = parse_t1_master(&buffer[..size]) else {
                    continue;
                };
                if packet.key != key {
                    continue;
                }
                seen_packets.insert(packet.index);
                addresses.extend(
                    packet
                        .servers
                        .into_iter()
                        .filter(|a| seen.insert(a.clone())),
                );
                if seen_packets.len() >= usize::from(packet.total.max(1)) {
                    break;
                }
            }
            Err(error) if seen_packets.is_empty() => return Err(error),
            Err(_) => break,
        }
    }
    Ok(addresses)
}

// --- Tribes 2 ---

/// Tribes 2 request types.
pub const T2_PING: u8 = 14;
pub const T2_INFO: u8 = 18;
/// Answer types.
pub const T2_PING_ANSWER: u8 = 16;
pub const T2_INFO_ANSWER: u8 = 20;

/// Tribes 2 request: type, flags (2: no compression) and key.
pub fn t2_request(kind: u8, key: u32) -> [u8; 6] {
    let k = key.to_le_bytes();
    [kind, 2, k[0], k[1], k[2], k[3]]
}

/// True when `packet` is a Tribes 2 answer of `kind` (for `key`, when given).
pub fn is_t2_answer(packet: &[u8], kind: u8, key: Option<u32>) -> bool {
    packet.len() >= 7
        && packet[0] == kind
        && key.is_none_or(|key| packet[2..6] == key.to_le_bytes())
}

/// Fields of a Tribes 2 ping answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct T2Ping {
    /// Query version tag, `VER3` or `VER5`.
    pub version: String,
    pub protocol: Option<u32>,
    pub min_protocol: u32,
    pub build: u32,
    /// Server name (at most 24 characters; empty or `VER3`-style servers
    /// send the name in the info answer instead).
    pub name: String,
}

/// Parse a Tribes 2 ping answer.
pub fn parse_t2_ping(packet: &[u8]) -> io::Result<T2Ping> {
    if !is_t2_answer(packet, T2_PING_ANSWER, None) {
        return Err(invalid("not a Tribes 2 ping answer"));
    }
    let mut r = Reader::new(packet, 6);
    let version = r.pascal()?;
    if !version.starts_with("VER") {
        return Err(invalid("unknown Tribes 2 query version"));
    }
    let protocol = if version == "VER3" {
        None
    } else {
        Some(r.u32_le()?)
    };
    let min_protocol = r.u32_le()?;
    let build = r.u32_le()?;
    let name = r.pascal().map(|name| clean(&name)).unwrap_or_default();
    Ok(T2Ping {
        version,
        protocol,
        min_protocol,
        build,
        name,
    })
}

/// Fields of a Tribes 2 info answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct T2Info {
    /// Mod (`Classic`, `base`...).
    pub game: String,
    /// Mission type (`Capture the Flag`...).
    pub mission_type: String,
    pub map: String,
    /// Status flags: 1 dedicated, 2 password, 4 Linux.
    pub status: u8,
    pub num_players: u8,
    pub max_players: u8,
    pub bots: u8,
    pub cpu: u16,
    pub info: String,
    pub teams: Vec<(String, i32)>,
    pub players: Vec<Player>,
}

/// Parse a Tribes 2 info answer.
pub fn parse_t2_info(packet: &[u8]) -> io::Result<T2Info> {
    if !is_t2_answer(packet, T2_INFO_ANSWER, None) {
        return Err(invalid("not a Tribes 2 info answer"));
    }
    let mut r = Reader::new(packet, 6);
    let mut info = T2Info {
        game: r.pascal()?,
        mission_type: r.pascal()?,
        map: r.pascal()?,
        status: r.u8()?,
        num_players: r.u8()?,
        max_players: r.u8()?,
        bots: r.u8()?,
        cpu: r.u16_le()?,
        info: r.pascal().unwrap_or_default(),
        ..T2Info::default()
    };
    let Ok(length) = r.u16_le() else {
        return Ok(info);
    };
    let end = (r.pos + usize::from(length)).min(packet.len());
    let block = crate::text::decode(&packet[r.pos..end]);
    parse_t2_lists(&block, &mut info);
    Ok(info)
}

/// Parse the team and player lists: a team count line, `name\tscore` lines,
/// a player count line, then `\x10name\x11\tteam\tscore` lines.
fn parse_t2_lists(block: &str, info: &mut T2Info) {
    let mut lines = block.split('\n');
    let Some(team_count) = lines.next().and_then(|l| l.trim().parse::<usize>().ok()) else {
        return;
    };
    for _ in 0..team_count {
        let Some(line) = lines.next() else { return };
        let (name, score) = line.split_once('\t').unwrap_or((line, ""));
        info.teams
            .push((clean(name), score.trim().parse().unwrap_or(0)));
    }
    let Some(player_count) = lines.next().and_then(|l| l.trim().parse::<usize>().ok()) else {
        return;
    };
    for line in lines.take(player_count) {
        let fields: Vec<&str> = line.split('\t').collect();
        let raw_name = fields.first().copied().unwrap_or("");
        let name = clean(raw_name);
        if name.is_empty() {
            continue;
        }
        info.players.push(Player {
            name,
            score: fields
                .get(2)
                .and_then(|score| clean(score).parse().ok())
                .unwrap_or(0),
            ping: None,
            time_secs: None,
        });
    }
}

/// Build a server from Tribes 2 answers. The ping answer gives the name; for
/// `VER3` servers (or when it is missing) the info text is used.
pub fn t2_server(game: &str, host: &str, port: u16, ping: Option<T2Ping>, info: T2Info) -> Server {
    let mut rules = BTreeMap::new();
    let mut server = Server::new(game, host, port);
    server.online = true;
    server.name = match &ping {
        Some(ping) if ping.version != "VER3" && !ping.name.is_empty() => ping.name.clone(),
        _ => clean(info.info.lines().next().unwrap_or("")),
    };
    if let Some(ping) = &ping {
        rules.insert("queryversion".into(), ping.version.clone());
        rules.insert("build_version".into(), ping.build.to_string());
        rules.insert("minimum_net_protocol".into(), ping.min_protocol.to_string());
        if let Some(protocol) = ping.protocol {
            rules.insert("net_protocol".into(), protocol.to_string());
        }
    }
    rules.insert("game".into(), info.game.clone());
    rules.insert("mission".into(), info.mission_type.clone());
    rules.insert("status".into(), info.status.to_string());
    rules.insert(
        "dedicated".into(),
        u8::from(info.status & 1 != 0).to_string(),
    );
    rules.insert(
        "password".into(),
        u8::from(info.status & 2 != 0).to_string(),
    );
    rules.insert("linux".into(), u8::from(info.status & 4 != 0).to_string());
    rules.insert("bot_count".into(), info.bots.to_string());
    rules.insert("cpu_speed".into(), info.cpu.to_string());
    rules.insert("info".into(), info.info.clone());
    rules.insert("numteams".into(), info.teams.len().to_string());
    for (index, (team, score)) in info.teams.iter().enumerate() {
        rules.insert(format!("team_{index}"), team.clone());
        rules.insert(format!("score_{index}"), score.to_string());
    }
    server.map = info.map;
    server.game_type = info.mission_type;
    server.mod_name = info.game;
    server.password = info.status & 2 != 0;
    server.num_players = usize::from(info.num_players);
    server.max_players = Some(usize::from(info.max_players));
    server.bots = usize::from(info.bots);
    server.players = info.players;
    server.location = crate::location::server_location(&rules);
    server.rules = rules;
    server
}

/// Tribes 2 status: ping and info requests, merged.
pub fn t2_status(
    game: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let socket = open(host, port, timeout)?;
    let key = u32::from(new_key()) | 0x0100_0000;
    let started = Instant::now();
    socket.send(&t2_request(T2_PING, key))?;
    socket.send(&t2_request(T2_INFO, key))?;
    let mut buffer = [0u8; 16384];
    let mut ping = None;
    let mut info = None;
    let mut rtt = None;
    while ping.is_none() || info.is_none() {
        let size = match socket.recv(&mut buffer) {
            Ok(size) => size,
            Err(error) if info.is_none() => return Err(error),
            Err(_) => break,
        };
        let packet = &buffer[..size];
        if is_t2_answer(packet, T2_PING_ANSWER, Some(key)) {
            rtt.get_or_insert_with(|| started.elapsed());
            ping = parse_t2_ping(packet).ok();
            if ping.is_none() {
                ping = Some(T2Ping::default());
            }
        } else if is_t2_answer(packet, T2_INFO_ANSWER, Some(key)) {
            rtt.get_or_insert_with(|| started.elapsed());
            info = Some(parse_t2_info(packet)?);
            // The ping answer normally arrives first; wait briefly for it.
            socket.set_read_timeout(Some(timeout.min(Duration::from_millis(300))))?;
        }
    }
    let info = info.ok_or_else(|| invalid("no Tribes 2 info answer"))?;
    let ping = ping.filter(|ping| !ping.version.is_empty());
    let rtt = rtt.unwrap_or_else(|| started.elapsed());
    Ok((t2_server(game, host, port, ping, info), rtt))
}

/// Round-trip time of one Tribes 2 ping.
pub fn t2_ping(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    let socket = open(host, port, timeout)?;
    let key = u32::from(new_key()) | 0x0100_0000;
    let started = Instant::now();
    socket.send(&t2_request(T2_PING, key))?;
    let mut buffer = [0u8; 2048];
    loop {
        let size = socket.recv(&mut buffer)?;
        if is_t2_answer(&buffer[..size], T2_PING_ANSWER, Some(key)) {
            return Ok(started.elapsed());
        }
    }
}

/// Download a plain-text master list (one `ip:port` per line), as served by
/// the TribesNext master over HTTP.
pub fn http_master_list(url: &str, timeout: Duration) -> io::Result<Vec<String>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let text = agent
        .get(url)
        .call()
        .map_err(io::Error::other)?
        .body_mut()
        .with_config()
        .limit(4 * 1024 * 1024)
        .read_to_string()
        .map_err(io::Error::other)?;
    Ok(crate::settings::parse_address_list(&text))
}
