//! Savage: The Battle for Newerth (S2 Games, 2003) and its community revival
//! Savage DRX.
//!
//! Protocol facts (own implementation), from the public description in
//! qstat's Savage support (`savage_serverquery`, `savage_playerquery` and
//! `deal_with_savage_packet`, Artistic License) and from live Savage DRX
//! servers:
//!
//! * UDP, default port 11235. A request is five magic bytes `9e 4c 23 00 00`,
//!   a request type (`c8` for the server info, `ce` for info plus the player
//!   list) and four more bytes that the server echoes back.
//! * The answer starts with the same five magic bytes, the request type plus
//!   one (`c9` / `cf`), `0c` and the echoed bytes, then a sequence of
//!   `0xFF key 0xFE value` pairs. Keys: `name`, `world` (map), `cnum` and
//!   `vnum` (client counts), `cmax`, `time`, `gametype`, `pass`, `needcmdr`,
//!   `race1`, `race2`, `pure`, `bal`, `notes`, `ver`, and for the player
//!   request `players`: lines of `Team N (race)`, one player per line, the
//!   `--empty--` marker for an empty team, then `Spectators`.
//! * Names carry S2 color codes (`^900`, `^w`, `^clan 123^`), removed here.
//!
//! Server list: the original master (S2 Games, then Newerth.com for Savage
//! XR) is gone. Savage DRX publishes the online servers as JSON at
//! `https://savagedrx.com/api/server/online` (`host_name`, `port`), which is
//! the list used here.

use std::collections::BTreeMap;
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use crate::model::{Player, Server, parse_count};
use crate::quake3::{ParseError, latin1};

/// Default query and join port.
pub const DEFAULT_PORT: u16 = 11235;

/// Magic prefix of every request and answer.
pub const MAGIC: [u8; 5] = [0x9e, 0x4c, 0x23, 0x00, 0x00];

/// Request for the server info and the player list.
pub const REQUEST_PLAYERS: [u8; 10] = [0x9e, 0x4c, 0x23, 0x00, 0x00, 0xce, 0x76, 0x46, 0x00, 0x00];

/// Request for the server info only (short answer, used for pings).
pub const REQUEST_INFO: [u8; 10] = [0x9e, 0x4c, 0x23, 0x00, 0x00, 0xc8, 0x01, 0x21, 0x00, 0x00];

/// True when `packet` looks like a Savage answer.
pub fn is_answer(packet: &[u8]) -> bool {
    packet.len() > 11 && packet.starts_with(&MAGIC) && matches!(packet[5], 0xc9 | 0xcf)
}

/// Remove S2 color codes: `^RGB` (three digits), `^clan N^` icons and
/// `^x` single character codes.
pub fn strip_codes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '^' || i + 1 >= chars.len() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let rest = &chars[i + 1..];
        if rest.len() >= 3 && rest[..3].iter().all(char::is_ascii_digit) {
            i += 4;
        } else if rest.starts_with(&['c', 'l', 'a', 'n', ' ']) {
            // Clan icon: `^clan 1234^`.
            match rest.iter().position(|&c| c == '^') {
                Some(end) => i += end + 2,
                None => i += 1,
            }
        } else if rest[0] == '^' {
            out.push('^');
            i += 2;
        } else {
            i += 2;
        }
    }
    out
}

/// Parse an answer into its key/value pairs.
pub fn parse_fields(packet: &[u8]) -> Result<BTreeMap<String, String>, ParseError> {
    if !is_answer(packet) {
        return Err(ParseError("not a Savage answer".into()));
    }
    let body = &packet[6..];
    let Some(start) = body.iter().position(|&b| b == 0xff) else {
        return Err(ParseError("Savage answer has no fields".into()));
    };
    let mut fields = BTreeMap::new();
    for pair in body[start + 1..].split(|&b| b == 0xff) {
        let Some(split) = pair.iter().position(|&b| b == 0xfe) else {
            continue;
        };
        let key = latin1(&pair[..split]);
        let value = crate::text::repair(&latin1(&pair[split + 1..]));
        fields.insert(key, value.trim_end_matches('\0').to_string());
    }
    if fields.is_empty() {
        return Err(ParseError("Savage answer has no fields".into()));
    }
    Ok(fields)
}

/// Parse the `players` field: team headers, names and `--empty--` markers.
pub fn parse_players(raw: &str) -> Vec<Player> {
    raw.lines()
        .map(|line| strip_codes(line).trim().to_string())
        .filter(|line| {
            !line.is_empty()
                && line != "--empty--"
                && !line.starts_with("Team ")
                && line != "Spectators"
        })
        .map(|name| Player {
            name,
            ..Player::default()
        })
        .collect()
}

/// Build the normalized server from the parsed fields.
pub fn server_from_fields(
    game: &str,
    host: &str,
    port: u16,
    fields: BTreeMap<String, String>,
) -> Server {
    let mut server = Server::new(game, host, port);
    server.online = true;
    let get = |key: &str| fields.get(key).cloned().unwrap_or_default();
    server.name = strip_codes(&get("name")).trim().to_string();
    server.map = get("world");
    server.game_type = get("gametype");
    server.max_players = parse_count(fields.get("cmax"));
    server.password = get("pass").trim() == "1";
    server.players = parse_players(&get("players"));
    // `players` is only sent for the player request: fall back on the counts.
    server.num_players = if fields.contains_key("players") {
        server.players.len()
    } else {
        parse_count(fields.get("cnum")).unwrap_or(0) + parse_count(fields.get("vnum")).unwrap_or(0)
    };
    server.rules = fields
        .into_iter()
        .filter(|(key, _)| key != "players")
        .map(|(key, value)| (key, strip_codes(&value)))
        .collect();
    server
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

/// Receive until an answer to our request arrives, ignoring stray packets.
fn receive(socket: &UdpSocket) -> io::Result<Vec<u8>> {
    let mut buffer = [0u8; 4096];
    loop {
        let size = socket.recv(&mut buffer)?;
        if is_answer(&buffer[..size]) {
            return Ok(buffer[..size].to_vec());
        }
    }
}

/// Query a server's status and player list. Returns the server and the round-trip time.
pub fn status(
    game: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let socket = connect(host, port, timeout)?;
    let started = Instant::now();
    socket.send(&REQUEST_PLAYERS)?;
    let packet = receive(&socket)?;
    let rtt = started.elapsed();
    let fields = parse_fields(&packet)?;
    Ok((server_from_fields(game, host, port, fields), rtt))
}

/// Round-trip time of the short info request.
pub fn ping(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    let socket = connect(host, port, timeout)?;
    let started = Instant::now();
    socket.send(&REQUEST_INFO)?;
    receive(&socket)?;
    Ok(started.elapsed())
}

/// Parse the Savage DRX server list: a JSON array of objects with
/// `host_name` and `port`.
pub fn parse_master_list(data: &[u8]) -> io::Result<Vec<String>> {
    let value: serde_json::Value = serde_json::from_slice(data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let entries = value.as_array().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Savage list is not a JSON array",
        )
    })?;
    let mut addresses: Vec<String> = Vec::new();
    for entry in entries {
        let host = entry.get("host_name").and_then(|v| v.as_str());
        let port = entry
            .get("port")
            .and_then(|v| v.as_u64())
            .and_then(|p| u16::try_from(p).ok());
        if let (Some(host), Some(port)) = (host, port) {
            let address = format!("{host}:{port}");
            if !host.is_empty() && port != 0 && !addresses.contains(&address) {
                addresses.push(address);
            }
        }
    }
    Ok(addresses)
}

/// Download the Savage DRX server list.
pub fn master_list(url: &str, timeout: Duration) -> io::Result<Vec<String>> {
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
        .limit(4 * 1024 * 1024)
        .read_to_vec()
        .map_err(io::Error::other)?;
    parse_master_list(&data)
}
