//! Quake 1 / Quake 2 family queries: QuakeWorld, Quake 2, Kingpin, Soldier of Fortune.
//!
//! Both families use connectionless UDP packets that start with four 0xFF
//! bytes. A `status` request is answered with a header line (`print` for
//! Quake 2, `n` for QuakeWorld), a backslash-delimited rule string and one
//! row per player. Masters answer with 6-byte `ip:port` records.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use crate::catalog::{Game, Protocol};
use crate::model::{Player, Server, parse_count};
use crate::quake3::{
    MASTER_IDLE_TIMEOUT, OUT_OF_BAND_PREFIX, ParseError, latin1, parse_address, parse_info_string,
};

fn err<T>(message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(message.into()))
}

/// One player row of a Quake 2 or QuakeWorld status answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quake2Player {
    pub score: i32,
    pub ping: i32,
    pub name: String,
    /// Seconds connected (QuakeWorld reports minutes, converted here).
    pub time_secs: Option<u32>,
}

/// Convert Quake 1/2 console characters to plain text: the high bit marks
/// "red" text, and 16..=27 are brackets and digits.
pub fn clean_text(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            let code = (c as u32 & 0x7f) as u8;
            match code {
                16 => '[',
                17 => ']',
                18..=27 => (b'0' + code - 18) as char,
                0..=31 | 127 => '.',
                _ => code as char,
            }
        })
        .collect()
}

/// Parse a status answer: returns the rules and the player rows.
pub fn parse_status(
    packet: &[u8],
    protocol: Protocol,
) -> Result<(BTreeMap<String, String>, Vec<Quake2Player>), ParseError> {
    let Some(rest) = packet.strip_prefix(OUT_OF_BAND_PREFIX) else {
        return err("response is missing the out-of-band prefix");
    };
    let text = latin1(rest);
    let quakeworld = protocol == Protocol::QuakeWorld;
    // Quake 2: "print\n\k\v...\n rows"; QuakeWorld: "n\k\v...\n rows".
    let body = if quakeworld {
        text.strip_prefix('n')
            .ok_or_else(|| ParseError("expected a QuakeWorld status answer".into()))?
    } else {
        text.strip_prefix("print\n")
            .ok_or_else(|| ParseError("expected a Quake 2 print answer".into()))?
    };
    let mut lines = body.split('\n').filter(|line| {
        !line
            .trim_matches(|c: char| c.is_whitespace() || c == '\0')
            .is_empty()
    });
    let Some(first) = lines.next() else {
        return err("status answer has no rules");
    };
    let info = parse_info_string(first.trim_end_matches('\0'))?;
    let players = lines
        .map(|line| parse_player_row(line, quakeworld))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((info, players))
}

/// Parse one player row.
///
/// Quake 2: `score ping "name"`.
/// QuakeWorld: `userid frags time ping "name" "skin" top bottom`.
pub fn parse_player_row(line: &str, quakeworld: bool) -> Result<Quake2Player, ParseError> {
    let invalid = || ParseError(format!("invalid player row: {line}"));
    let line = line.trim();
    let first_quote = line.find('"').ok_or_else(invalid)?;
    let numbers: Vec<i32> = line[..first_quote]
        .split_whitespace()
        .map(|part| part.parse().map_err(|_| invalid()))
        .collect::<Result<_, _>>()?;
    let after = &line[first_quote + 1..];
    let name_end = if quakeworld {
        after.find('"').ok_or_else(invalid)?
    } else {
        after.rfind('"').ok_or_else(invalid)?
    };
    let name = clean_text(&after[..name_end]);
    if quakeworld {
        let [_, frags, minutes, ping] = numbers[..] else {
            return Err(invalid());
        };
        Ok(Quake2Player {
            score: frags,
            ping,
            name,
            time_secs: u32::try_from(minutes).ok().map(|m| m.saturating_mul(60)),
        })
    } else {
        let [score, ping] = numbers[..] else {
            return Err(invalid());
        };
        Ok(Quake2Player {
            score,
            ping,
            name,
            time_secs: None,
        })
    }
}

/// Parse a master answer: a header (`servers ` for Quake 2, `d\n` for
/// QuakeWorld) followed by 6-byte records.
pub fn parse_master_response(packet: &[u8], protocol: Protocol) -> Result<Vec<String>, ParseError> {
    let Some(rest) = packet.strip_prefix(OUT_OF_BAND_PREFIX) else {
        return err("master answer is missing the out-of-band prefix");
    };
    let body = if protocol == Protocol::QuakeWorld {
        rest.strip_prefix(b"d\n".as_slice())
            .or_else(|| rest.strip_prefix(b"d".as_slice()))
    } else {
        rest.strip_prefix(b"servers ".as_slice())
            .or_else(|| rest.strip_prefix(b"servers\n".as_slice()))
    };
    let Some(body) = body else {
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
                u16::from_be_bytes([chunk[4], chunk[5]])
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
    let mut payload = OUT_OF_BAND_PREFIX.to_vec();
    payload.extend_from_slice(b"status\n");
    let started = Instant::now();
    socket.send(&payload)?;
    let mut buffer = [0u8; 16384];
    let size = socket.recv(&mut buffer)?;
    let rtt = started.elapsed();
    let (info, players) = parse_status(&buffer[..size], game.protocol)?;
    Ok((server_from_status(game, host, port, info, players), rtt))
}

/// Round-trip time of one status query.
pub fn ping(game: &Game, host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    status(game, host, port, timeout).map(|(_, rtt)| rtt)
}

/// Fetch a master's list. `request` is the raw payload (with prefix if needed).
pub fn master_list(
    address: &str,
    request: &[u8],
    protocol: Protocol,
    timeout: Duration,
) -> io::Result<Vec<String>> {
    let (host, port) = parse_address(address, 27900)?;
    let socket = connect(&host, port, timeout)?;
    socket.send(request)?;
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
                let entries = parse_master_response(&buffer[..size], protocol)?;
                addresses.extend(entries.into_iter().filter(|a| seen.insert(a.clone())));
            }
            Err(error) if !received => return Err(error),
            Err(_) => break,
        }
    }
    Ok(addresses)
}

/// Request payload for a Quake 2 master. Live masters answer the bare
/// `query` line (with or without the 0xFF prefix), so no prefix is sent.
pub fn quake2_master_request(query: &str) -> Vec<u8> {
    format!("{}\n", query.trim_end()).into_bytes()
}

/// Request payload for a QuakeWorld master (`c\n`, no prefix).
pub fn quakeworld_master_request() -> Vec<u8> {
    b"c\n".to_vec()
}

/// Build the normalized server from parsed rules and players.
pub fn server_from_status(
    game: &Game,
    host: &str,
    port: u16,
    info: BTreeMap<String, String>,
    players: Vec<Quake2Player>,
) -> Server {
    let mut server = Server::new(&game.id, host, port);
    server.online = true;
    let get = |key: &str| info.get(key).cloned().unwrap_or_default();
    server.name = clean_text(&get("hostname"));
    server.map = if info.contains_key("mapname") {
        get("mapname")
    } else {
        get("map")
    };
    if !game.game_type_key.is_empty() {
        server.game_type = get(&game.game_type_key);
    }
    server.mod_name = if game.mod_key.is_empty() {
        String::new()
    } else {
        get(&game.mod_key)
    };
    server.max_players = parse_count(info.get("maxclients").or_else(|| info.get("sv_maxclients")));
    server.password = ["needpass", "password"].iter().any(|key| {
        info.get(*key)
            .is_some_and(|v| v.trim().parse::<u32>().is_ok_and(|n| n > 0))
    });
    server.players = players
        .into_iter()
        .map(|player| Player {
            name: player.name,
            score: player.score,
            ping: u32::try_from(player.ping).ok(),
            time_secs: player.time_secs,
        })
        .collect();
    server.num_players = server.players.len();
    server.rules = info;
    server
}
