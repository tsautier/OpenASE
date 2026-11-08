//! Rainbow Six 3: Raven Shield (Unreal Engine 2.5) server beacon queries.
//!
//! A server listens for UDP beacon requests on its game port + 1000 (8777
//! for the default game port 7777). The text `REPORT` is answered with one
//! packet: `rvnshld <game port> KEYWORD`, then fields that each start with
//! the pilcrow byte 0xB6 followed by a two-character key, a space and the
//! value (`¶I1 server name ¶E1 map`). Player names, times, pings and kills
//! are `/`-separated lists (`¶L1 /one/two`). The answer is sent in a fixed
//! size buffer, so it ends in NUL padding and long map lists are cut off.
//!
//! Server lists come from the OpenRVS community registry, an HTTP CSV list
//! (`name,ip,port,mode`) of game ports at `http://api.openrvs.org/servers`.
//!
//! Sources: the OpenRVS mod (`OpenClientBeaconReceiver.uc`,
//! `OpenServerList.uc`, MIT license) and the field list of qstat's
//! Raven Shield support, checked against live servers.

use std::collections::BTreeMap;
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use crate::model::{Player, Server};

/// Beacon request.
pub const REQUEST: &[u8] = b"REPORT";

/// Product name at the start of every Raven Shield beacon answer.
pub const PRODUCT: &str = "rvnshld";

/// Field separator (pilcrow in Latin-1).
const MARKER: u8 = 0xb6;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// A parsed beacon answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Product name (`rvnshld`).
    pub product: String,
    /// Game port players join.
    pub port: u16,
    /// Fields in packet order, by their two-character key.
    pub fields: Vec<(String, String)>,
}

impl Report {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(field, _)| field == key)
            .map(|(_, value)| value.as_str())
    }
}

/// True when `packet` looks like a Raven Shield `REPORT` answer.
pub fn is_answer(packet: &[u8]) -> bool {
    packet.starts_with(PRODUCT.as_bytes()) && packet.get(PRODUCT.len()) == Some(&b' ')
}

/// Parse a `REPORT` answer.
pub fn parse_report(packet: &[u8]) -> io::Result<Report> {
    if !is_answer(packet) {
        return Err(invalid("not a Raven Shield beacon answer"));
    }
    let end = packet
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(packet.len());
    let mut parts = packet[..end].split(|&byte| byte == MARKER);
    let header = crate::text::decode(parts.next().unwrap_or_default());
    let mut words = header.split_whitespace();
    let product = words.next().unwrap_or_default().to_string();
    let port = words
        .next()
        .and_then(|word| word.parse().ok())
        .ok_or_else(|| invalid("beacon answer has no game port"))?;
    if words.next() != Some("KEYWORD") {
        return Err(invalid("beacon answer is not a server report"));
    }
    let fields = parts
        .filter_map(|part| {
            let text = crate::text::decode(part);
            let (key, value) = text.split_once(' ').unwrap_or((text.as_str(), ""));
            // Each value is followed by one space before the next marker.
            let value = value.strip_suffix(' ').unwrap_or(value);
            (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
        })
        .collect();
    Ok(Report {
        product,
        port,
        fields,
    })
}

/// Readable rule names for the known keys.
fn rule_name(key: &str) -> &str {
    match key {
        "A1" => "maxplayers",
        "A2" => "tkpenalty",
        "B1" => "numplayers",
        "B2" => "allowradar",
        "D2" => "version",
        "E1" => "mapname",
        "F1" => "gametype",
        "G1" => "password",
        "G2" => "beaconport",
        "H1" => "dedicated",
        "H2" => "terrorists",
        "I1" => "hostname",
        "I2" => "aibackup",
        "J2" => "rotatemap",
        "K2" => "forcefirstpersonweapon",
        "L2" => "mod",
        "L3" => "punkbuster",
        "O2" => "motd",
        "P1" => "gameport",
        "Q1" => "roundspermatch",
        "R1" => "roundtime",
        "S1" => "betweenroundtime",
        "T1" => "bombtime",
        "W1" => "shownames",
        "X1" => "internetserver",
        "Y1" => "friendlyfire",
        "Z1" => "autobalance",
        other => other,
    }
}

/// Split a `/one/two` list into its items (empty for an empty list).
fn list(value: Option<&str>) -> Vec<&str> {
    value
        .map(|value| value.strip_prefix('/').unwrap_or(value))
        .filter(|value| !value.is_empty())
        .map(|value| value.split('/').collect())
        .unwrap_or_default()
}

/// Parse `mm:ss` into seconds.
fn parse_time(value: &str) -> Option<u32> {
    let (minutes, seconds) = value.trim().split_once(':')?;
    Some(minutes.parse::<u32>().ok()? * 60 + seconds.parse::<u32>().ok()?)
}

/// Build the normalized server from a beacon answer. `port` is the port
/// that was queried (the beacon port).
pub fn server_from_report(game: &str, host: &str, port: u16, report: &Report) -> Server {
    let mut server = Server::new(game, host, port);
    server.online = true;
    server.join_port = report.port;
    let get = |key: &str| report.get(key).unwrap_or_default().to_string();
    server.name = get("I1");
    server.map = get("E1");
    server.game_type = get("F1");
    server.mod_name = get("L2");
    server.max_players = report.get("A1").and_then(|v| v.trim().parse().ok());
    server.password = report.get("G1").is_some_and(|v| v.trim() == "1");

    let names = list(report.get("L1"));
    let times = list(report.get("M1"));
    let pings = list(report.get("N1"));
    let kills = list(report.get("O1"));
    server.players = names
        .iter()
        .enumerate()
        .map(|(index, name)| Player {
            name: name.to_string(),
            score: kills
                .get(index)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            ping: pings.get(index).and_then(|v| v.trim().parse().ok()),
            time_secs: times.get(index).and_then(|v| parse_time(v)),
        })
        .collect();
    server.num_players = report
        .get("B1")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(server.players.len());

    let mut rules = BTreeMap::new();
    for (key, value) in &report.fields {
        // Player lists are shown in the player table, and the map and game
        // type rotations are long `/` lists cut off by the packet size.
        if matches!(key.as_str(), "L1" | "M1" | "N1" | "O1" | "J1") {
            continue;
        }
        let name = match key.as_str() {
            "K1" => "maplist",
            other => rule_name(other),
        };
        rules.insert(name.to_string(), value.clone());
    }
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

fn exchange(host: &str, port: u16, timeout: Duration) -> io::Result<(Report, Duration)> {
    let socket = open(host, port, timeout)?;
    let started = Instant::now();
    socket.send(REQUEST)?;
    let mut buffer = [0u8; 8192];
    loop {
        let size = socket.recv(&mut buffer)?;
        if is_answer(&buffer[..size]) {
            let rtt = started.elapsed();
            return Ok((parse_report(&buffer[..size])?, rtt));
        }
    }
}

/// Query a server's beacon port. Returns the normalized server and the
/// round-trip time.
pub fn status(
    game: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    let (report, rtt) = exchange(host, port, timeout)?;
    Ok((server_from_report(game, host, port, &report), rtt))
}

/// Round-trip time of one beacon request.
pub fn ping(host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    exchange(host, port, timeout).map(|(_, rtt)| rtt)
}

/// Parse an OpenRVS registry list: CSV with a header row that names the
/// `ip` and `port` columns. Returns `ip:port` game addresses.
pub fn parse_csv_list(text: &str) -> io::Result<Vec<String>> {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let header: Vec<String> = lines
        .next()
        .ok_or_else(|| invalid("empty server list"))?
        .split(',')
        .map(|column| column.trim().to_ascii_lowercase())
        .collect();
    let column = |name: &str| {
        header
            .iter()
            .position(|column| column == name)
            .ok_or_else(|| invalid(&format!("server list has no {name} column")))
    };
    let (ip_column, port_column) = (column("ip")?, column("port")?);
    let columns = header.len();
    let mut addresses = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').collect();
        // Server names may contain commas: count columns from the right.
        if fields.len() < columns {
            continue;
        }
        let shift = fields.len() - columns;
        let pick = |index: usize| {
            if index == 0 {
                fields[0]
            } else {
                fields[index + shift]
            }
        };
        let ip = pick(ip_column).trim();
        let Ok(port) = pick(port_column).trim().parse::<u16>() else {
            continue;
        };
        if ip.parse::<std::net::Ipv4Addr>().is_ok() && port != 0 {
            let address = format!("{ip}:{port}");
            if !addresses.contains(&address) {
                addresses.push(address);
            }
        }
    }
    Ok(addresses)
}

/// Fetch a CSV server list over HTTP.
pub fn csv_master_list(url: &str, timeout: Duration) -> io::Result<Vec<String>> {
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
    parse_csv_list(&text)
}
