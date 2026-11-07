//! Quake 3 server query support.
//!
//! Quake 3 connectionless packets start with four 0xFF bytes followed by an
//! ASCII command. OpenASE uses the documented `getinfo` and `getstatus`
//! commands and sends short challenge strings to avoid oversized queries.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 27960;
pub const DEFAULT_MASTER: &str = "master.quake3arena.com";
pub const DEFAULT_MASTER_PORT: u16 = 27950;
pub const OUT_OF_BAND_PREFIX: &[u8] = b"\xff\xff\xff\xff";
pub const CHALLENGE: &str = "openase";
/// How long to wait for further master packets once the first one arrived.
pub const MASTER_IDLE_TIMEOUT: Duration = Duration::from_millis(700);

/// Error raised while parsing a Quake 3 packet or an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

impl From<ParseError> for io::Error {
    fn from(value: ParseError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, value.0)
    }
}

fn err<T>(message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(message.into()))
}

/// One player row from a Quake 3 `statusResponse` packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quake3Player {
    pub score: i32,
    pub ping: i32,
    pub name: String,
}

/// Parsed Quake 3 server response.
#[derive(Debug, Clone, Default)]
pub struct Quake3Server {
    pub host: String,
    pub port: u16,
    pub online: bool,
    pub info: BTreeMap<String, String>,
    pub players: Vec<Quake3Player>,
    pub error: Option<String>,
    /// Round-trip time of the query.
    pub ping_ms: Option<u32>,
}

/// Convert a duration to whole milliseconds, saturating.
pub fn millis(duration: Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
}

impl Quake3Server {
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    fn offline(host: &str, port: u16, error: String) -> Self {
        Self {
            host: host.to_string(),
            port,
            online: false,
            error: Some(error),
            ..Self::default()
        }
    }
}

/// UDP client for Quake 3 server status queries.
#[derive(Debug, Clone)]
pub struct Quake3Client {
    pub timeout: Duration,
}

impl Default for Quake3Client {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl Quake3Client {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Return status for one Quake 3 server.
    ///
    /// A failed query returns an offline server instead of an error, which
    /// lets scanners keep processing the rest of a server list.
    pub fn status(&self, host: &str, port: u16) -> Quake3Server {
        let result = self
            .query(host, port, &format!("getstatus {CHALLENGE}"))
            .and_then(|(packet, rtt)| {
                parse_status_response(&packet)
                    .map(|parsed| (parsed, rtt))
                    .map_err(io::Error::from)
            });
        match result {
            Ok(((info, players), rtt)) => Quake3Server {
                host: host.to_string(),
                port,
                online: true,
                info,
                players,
                error: None,
                ping_ms: Some(millis(rtt)),
            },
            Err(error) => Quake3Server::offline(host, port, error.to_string()),
        }
    }

    /// Return lightweight server information from `infoResponse`.
    pub fn info(&self, host: &str, port: u16) -> Quake3Server {
        let result = self
            .query(host, port, &format!("getinfo {CHALLENGE}"))
            .and_then(|(packet, rtt)| {
                parse_info_response(&packet)
                    .map(|info| (info, rtt))
                    .map_err(io::Error::from)
            });
        match result {
            Ok((info, rtt)) => Quake3Server {
                host: host.to_string(),
                port,
                online: true,
                info,
                ping_ms: Some(millis(rtt)),
                ..Quake3Server::default()
            },
            Err(error) => Quake3Server::offline(host, port, error.to_string()),
        }
    }

    /// Return `host:port` entries advertised by a Quake 3 master server.
    ///
    /// Masters split large lists across several UDP packets, each ending with
    /// `EOT`, so packets are read until an `EOF` marker, a short silence
    /// ([`MASTER_IDLE_TIMEOUT`]) or the overall timeout. Duplicates are dropped.
    pub fn master_servers(&self, host: &str, port: u16, protocol: u32) -> io::Result<Vec<String>> {
        self.master_query(host, port, &format!("getservers {protocol} empty full"))
    }

    /// Send a raw master request such as `getservers 84 empty full` and collect
    /// the advertised addresses. Works for Quake 3, Urban Terror, OpenArena,
    /// Enemy Territory and Call of Duty masters.
    pub fn master_query(&self, host: &str, port: u16, command: &str) -> io::Result<Vec<String>> {
        let socket = self.send(host, port, &format!("{}\n", command.trim_end()))?;
        let deadline = Instant::now() + self.timeout;
        let mut addresses = Vec::new();
        let mut seen = HashSet::new();
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
            match socket.recv_from(&mut buffer) {
                Ok((size, _)) => {
                    received = true;
                    let (entries, end_of_list) = parse_getservers_packet(&buffer[..size])?;
                    addresses.extend(
                        entries
                            .into_iter()
                            .filter(|address| seen.insert(address.clone())),
                    );
                    if end_of_list {
                        break;
                    }
                }
                Err(error) if !received => return Err(error),
                Err(_) => break,
            }
        }
        Ok(addresses)
    }

    fn send(&self, host: &str, port: u16, command: &str) -> io::Result<UdpSocket> {
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
        socket.set_read_timeout(Some(self.timeout))?;
        let mut payload = OUT_OF_BAND_PREFIX.to_vec();
        payload.extend_from_slice(command.as_bytes());
        socket.send_to(&payload, target)?;
        Ok(socket)
    }

    fn query(&self, host: &str, port: u16, command: &str) -> io::Result<(Vec<u8>, Duration)> {
        let socket = self.send(host, port, command)?;
        let started = Instant::now();
        let mut buffer = [0u8; 16384];
        let (size, _) = socket.recv_from(&mut buffer)?;
        Ok((buffer[..size].to_vec(), started.elapsed()))
    }
}

/// Parse `host` or `host:port` into a host and UDP port.
pub fn parse_address(address: &str, default_port: u16) -> Result<(String, u16), ParseError> {
    if address.starts_with('[') {
        return err("IPv6 bracket notation is not supported yet");
    }
    match address.rsplit_once(':') {
        None => Ok((address.to_string(), default_port)),
        Some(("", _)) => err("missing host"),
        Some((host, raw_port)) => raw_port
            .trim()
            .parse::<u16>()
            .map(|port| (host.to_string(), port))
            .map_err(|_| ParseError(format!("invalid port: {raw_port}"))),
    }
}

pub fn parse_info_response(packet: &[u8]) -> Result<BTreeMap<String, String>, ParseError> {
    let text = decode_connectionless(packet, "infoResponse")?;
    parse_info_string(text.trim())
}

pub fn parse_status_response(
    packet: &[u8],
) -> Result<(BTreeMap<String, String>, Vec<Quake3Player>), ParseError> {
    let text = decode_connectionless(packet, "statusResponse")?;
    let mut lines = text.lines().filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return err("statusResponse did not contain an info string");
    };
    let info = parse_info_string(first)?;
    let players = lines
        .map(parse_player_line)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((info, players))
}

/// Parse a Quake 3 `getserversResponse` packet from a master server.
pub fn parse_getservers_response(packet: &[u8]) -> Result<Vec<String>, ParseError> {
    parse_getservers_packet(packet).map(|(addresses, _)| addresses)
}

/// Parse one `getserversResponse` packet.
///
/// Each packet ends with `\EOT` (some masters pad it with three NUL bytes,
/// others do not); the last packet of a list may end with `\EOF` instead.
/// Returns the addresses and whether the packet was marked `EOF`.
fn parse_getservers_packet(packet: &[u8]) -> Result<(Vec<String>, bool), ParseError> {
    let mut header = OUT_OF_BAND_PREFIX.to_vec();
    header.extend_from_slice(b"getserversResponse");
    let Some(body) = packet.strip_prefix(header.as_slice()) else {
        return err("response is not a Quake 3 getserversResponse packet");
    };

    let mut addresses = Vec::new();
    let mut index = 0;
    while index < body.len() {
        if body[index] != b'\\' {
            index += 1;
            continue;
        }
        let chunk = &body[index + 1..(index + 7).min(body.len())];
        if chunk.starts_with(b"EOT") && chunk[3..].iter().all(|&byte| byte == 0) {
            return Ok((addresses, false));
        }
        if chunk.starts_with(b"EOF") && chunk[3..].iter().all(|&byte| byte == 0) {
            return Ok((addresses, true));
        }
        if chunk.len() < 6 {
            return err("truncated getserversResponse address entry");
        }
        let port = u16::from_be_bytes([chunk[4], chunk[5]]);
        addresses.push(format!(
            "{}.{}.{}.{}:{port}",
            chunk[0], chunk[1], chunk[2], chunk[3]
        ));
        index += 7;
    }
    Ok((addresses, false))
}

/// Validate the packet prefix and command, then return the response body.
pub fn decode_connectionless(packet: &[u8], expected_command: &str) -> Result<String, ParseError> {
    let Some(rest) = packet.strip_prefix(OUT_OF_BAND_PREFIX) else {
        return err("response is missing the Quake 3 out-of-band prefix");
    };
    let text = latin1(rest);
    let Some((command, body)) = text.split_once('\n') else {
        let command = text.trim();
        if command != expected_command {
            return err(format!(
                "expected {expected_command}, got {}",
                if command.is_empty() {
                    "empty response"
                } else {
                    command
                }
            ));
        }
        return err(format!("{expected_command} response is missing a body"));
    };
    let command = command.trim();
    if command != expected_command {
        return err(format!(
            "expected {expected_command}, got {}",
            if command.is_empty() {
                "empty response"
            } else {
                command
            }
        ));
    }
    Ok(body.to_string())
}

/// Decode bytes as Latin-1 (every byte maps to the code point of the same value).
pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| byte as char).collect()
}

/// Parse a Quake 3 backslash-delimited info string into key/value pairs.
pub fn parse_info_string(raw: &str) -> Result<BTreeMap<String, String>, ParseError> {
    let mut result = BTreeMap::new();
    if raw.is_empty() {
        return Ok(result);
    }
    let mut parts: Vec<&str> = raw.split('\\').collect();
    if parts.first() == Some(&"") {
        parts.remove(0);
    }
    if !parts.len().is_multiple_of(2) {
        return err("info string has an odd number of fields");
    }
    for pair in parts.chunks(2) {
        if !pair[0].is_empty() {
            result.insert(pair[0].to_string(), pair[1].to_string());
        }
    }
    Ok(result)
}

/// Parse a status player row: score, ping, and (usually quoted) name.
pub fn parse_player_line(line: &str) -> Result<Quake3Player, ParseError> {
    let invalid = || ParseError(format!("invalid player row: {line}"));
    let line = line.trim();
    let (score, rest) = line.split_once(char::is_whitespace).ok_or_else(invalid)?;
    let (ping, name) = rest
        .trim_start()
        .split_once(char::is_whitespace)
        .ok_or_else(invalid)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid());
    }
    let name = name
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(name);
    Ok(Quake3Player {
        score: score.parse().map_err(|_| invalid())?,
        ping: ping.parse().map_err(|_| invalid())?,
        name: name.to_string(),
    })
}
