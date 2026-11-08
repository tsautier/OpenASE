//! Protocol dispatch: master lists and server queries for any catalog game,
//! with ASE-style ping, flux and packet loss measurement.

use std::collections::HashSet;
use std::io;
use std::time::Duration;

use crate::catalog::{Game, Master, Protocol};
use crate::model::{Player, Server, parse_count};
use crate::quake3::{Quake3Client, Quake3Server, millis, parse_address};
use crate::{a2s, gamespy, location, quake2, steam};

/// Network settings for queries (ASE's Options > Network).
#[derive(Debug, Clone)]
pub struct QueryOptions {
    pub timeout: Duration,
    /// Number of ping samples per server (including the status query).
    pub pings: u32,
    /// Extra attempts when the status query gets no answer.
    pub retries: u32,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(1000),
            pings: 3,
            retries: 1,
        }
    }
}

/// Fetch a master's server list (query addresses, `ip:port`).
pub fn fetch_master(
    master: &Master,
    timeout: Duration,
    steam_key: &str,
    limit: usize,
) -> io::Result<Vec<String>> {
    match master {
        Master::Quake3 { address, query } => {
            let (host, port) = parse_address(address, 27950)?;
            Quake3Client::new(timeout * 3).master_query(&host, port, query)
        }
        Master::Gamespy {
            address,
            gamename,
            key,
        } => gamespy::master_list(address, gamename, key, timeout * 3),
        Master::SteamWeb { filter } => steam::server_list(steam_key, filter, limit, timeout * 5),
        Master::Quake2 { address, query } => quake2::master_list(
            address,
            &quake2::quake2_master_request(query),
            Protocol::Quake2,
            timeout * 3,
        ),
        Master::Quakeworld { address } => quake2::master_list(
            address,
            &quake2::quakeworld_master_request(),
            Protocol::QuakeWorld,
            timeout * 3,
        ),
    }
}

/// Fetch and merge the lists of all of a game's masters.
///
/// Returns the deduplicated addresses and one message per master that failed.
pub fn fetch_all_masters(
    game: &Game,
    timeout: Duration,
    steam_key: &str,
    limit: usize,
) -> (Vec<String>, Vec<String>) {
    let results: Vec<(String, io::Result<Vec<String>>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = game
            .masters
            .iter()
            .map(|master| {
                scope.spawn(move || {
                    (
                        master.label(),
                        fetch_master(master, timeout, steam_key, limit),
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect()
    });

    let mut seen = HashSet::new();
    let mut addresses = Vec::new();
    let mut errors = Vec::new();
    for (label, result) in results {
        match result {
            Ok(list) => addresses.extend(
                list.into_iter()
                    .filter(|address| seen.insert(address.clone())),
            ),
            Err(error) => errors.push(format!("{label}: {error}")),
        }
    }
    (addresses, errors)
}

/// Query one server: full status, then extra pings for ping/flux/loss.
pub fn query_server(game: &Game, host: &str, port: u16, options: &QueryOptions) -> Server {
    let mut samples: Vec<Duration> = Vec::new();
    let mut attempts = 0u32;
    let mut status = None;
    let mut last_error = String::from("no answer");

    for _ in 0..=options.retries {
        attempts += 1;
        match status_once(game, host, port, options.timeout) {
            Ok((server, rtt)) => {
                samples.push(rtt);
                status = Some(server);
                break;
            }
            Err(error) => last_error = error.to_string(),
        }
    }
    let Some(mut server) = status else {
        return Server::offline(&game.id, host, port, last_error);
    };

    for _ in 1..options.pings.max(1) {
        attempts += 1;
        if let Ok(rtt) = ping_once(game, host, port, options.timeout) {
            samples.push(rtt);
        }
    }

    apply_ping_stats(&mut server, &samples, attempts);
    server
}

/// Fill `ping`, `flux` and `packet_loss` from round-trip samples.
pub fn apply_ping_stats(server: &mut Server, samples: &[Duration], attempts: u32) {
    if samples.is_empty() {
        server.packet_loss = Some(100);
        return;
    }
    let values: Vec<u32> = samples.iter().copied().map(millis).collect();
    let sum: u64 = values.iter().map(|&v| u64::from(v)).sum();
    let (min, max) = (
        values.iter().min().copied().unwrap_or(0),
        values.iter().max().copied().unwrap_or(0),
    );
    server.ping = Some((sum / values.len() as u64) as u32);
    server.flux = Some((max - min) / 2);
    let lost = attempts.saturating_sub(values.len() as u32);
    server.packet_loss = Some((lost * 100 / attempts.max(1)) as u8);
}

fn status_once(
    game: &Game,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<(Server, Duration)> {
    match game.protocol {
        Protocol::Quake3 => {
            let raw = Quake3Client::new(timeout).status(host, port);
            if !raw.online {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    raw.error.unwrap_or_default(),
                ));
            }
            let rtt = Duration::from_millis(u64::from(raw.ping_ms.unwrap_or(0)));
            Ok((from_quake3(game, raw), rtt))
        }
        Protocol::Gamespy => {
            let (pairs, rtt) = gamespy::query(host, port, "\\status\\", timeout)?;
            if pairs.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "empty GameSpy answer",
                ));
            }
            let mut server = gamespy::server_from_status(&game.id, host, port, pairs);
            server.mod_name = String::new();
            Ok((server, rtt))
        }
        Protocol::A2s => a2s::status(&game.id, host, port, timeout),
        Protocol::Quake2 | Protocol::QuakeWorld => quake2::status(game, host, port, timeout),
    }
}

fn ping_once(game: &Game, host: &str, port: u16, timeout: Duration) -> io::Result<Duration> {
    match game.protocol {
        Protocol::Quake3 => {
            let raw = Quake3Client::new(timeout).info(host, port);
            match (raw.online, raw.ping_ms) {
                (true, Some(ms)) => Ok(Duration::from_millis(u64::from(ms))),
                _ => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    raw.error.unwrap_or_default(),
                )),
            }
        }
        Protocol::Gamespy => gamespy::query(host, port, "\\info\\", timeout).map(|(_, rtt)| rtt),
        Protocol::A2s => a2s::ping(host, port, timeout),
        Protocol::Quake2 | Protocol::QuakeWorld => quake2::ping(game, host, port, timeout),
    }
}

/// Normalize a Quake 3 family status answer.
pub fn from_quake3(game: &Game, raw: Quake3Server) -> Server {
    let info = raw.info;
    let mut server = Server::new(&game.id, &raw.host, raw.port);
    server.online = true;
    server.name = info
        .get("sv_hostname")
        .or_else(|| info.get("hostname"))
        .cloned()
        .unwrap_or_default();
    server.map = info.get("mapname").cloned().unwrap_or_default();
    if !game.game_type_key.is_empty() {
        server.game_type = info.get(&game.game_type_key).cloned().unwrap_or_default();
    }
    server.mod_name = if game.mod_key.is_empty() {
        String::new()
    } else {
        info.get(&game.mod_key).cloned().unwrap_or_default()
    };
    server.max_players = parse_count(
        info.get("sv_maxclients")
            .or_else(|| info.get("sv_maxClients")),
    );
    server.password = ["g_needpass", "pswrd", "g_needPass"]
        .iter()
        .any(|key| info.get(*key).is_some_and(|value| value.trim() == "1"));
    server.players = raw
        .players
        .into_iter()
        .map(|player| Player {
            name: player.name,
            score: player.score,
            ping: u32::try_from(player.ping).ok(),
            time_secs: None,
        })
        .collect();
    server.bots = server
        .players
        .iter()
        .filter(|player| player.ping == Some(0))
        .count();
    server.num_players = server.players.len();
    server.location = location::server_location(&info);
    server.rules = info;
    server
}
