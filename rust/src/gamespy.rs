//! GameSpy (version 0/1) support, used by Unreal Tournament 99.
//!
//! Masters speak a text protocol over TCP: the master sends
//! `\basic\\secure\XXXXXX`, the client answers with a `\validate\` key derived
//! from the game's secret key, then requests `\list\\gamename\ut\final\`.
//! Servers answer `\status\` over UDP with backslash-delimited key/values,
//! possibly split across packets and ended by `\final\`.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::model::{Player, Server, parse_count};

/// Compute the `\validate\` answer to a master's `\secure\` challenge.
pub fn gsseckey(secure: &[u8], key: &[u8]) -> String {
    let mut table: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut a: u8 = 0;
    for i in 0..256 {
        a = a.wrapping_add(table[i]).wrapping_add(key[i % key.len()]);
        table.swap(a as usize, i);
    }

    let (mut a, mut b): (u8, u8) = (0, 0);
    let mut tmp: Vec<u8> = Vec::with_capacity(secure.len() + 2);
    for &c in secure {
        a = a.wrapping_add(c).wrapping_add(1);
        let x = table[a as usize];
        b = b.wrapping_add(x);
        let y = table[b as usize];
        table[b as usize] = x;
        table[a as usize] = y;
        tmp.push(c ^ table[x.wrapping_add(y) as usize]);
    }
    while !tmp.len().is_multiple_of(3) {
        tmp.push(0);
    }

    let encode = |value: u8| -> char {
        (match value {
            0..=25 => value + b'A',
            26..=51 => value + b'G',
            52..=61 => value - 4,
            62 => b'+',
            _ => b'/',
        }) as char
    };
    let mut out = String::with_capacity(tmp.len() / 3 * 4);
    for chunk in tmp.chunks(3) {
        let (x, y, z) = (chunk[0], chunk[1], chunk[2]);
        out.push(encode(x >> 2));
        out.push(encode(((x & 3) << 4) | (y >> 4)));
        out.push(encode(((y & 15) << 2) | (z >> 6)));
        out.push(encode(z & 63));
    }
    out
}

/// Split `\key\value\key\value` text into ordered pairs.
pub fn parse_kv(text: &str) -> Vec<(String, String)> {
    let mut parts = text.split('\\');
    if text.starts_with('\\') {
        parts.next();
    }
    let parts: Vec<&str> = parts.collect();
    parts
        .chunks(2)
        .filter(|pair| pair.len() == 2 && !pair[0].is_empty())
        .map(|pair| (crate::text::repair(pair[0]), crate::text::repair(pair[1])))
        .collect()
}

/// Extract `ip:port` entries from a master `\list\` answer.
pub fn parse_master_list(data: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(data);
    text.split("\\ip\\")
        .skip(1)
        .filter_map(|entry| entry.split('\\').next())
        .filter(|address| address.contains(':'))
        .map(str::to_string)
        .collect()
}

/// Download the server list for `gamename` from a GameSpy master.
pub fn master_list(
    address: &str,
    gamename: &str,
    key: &str,
    timeout: Duration,
) -> io::Result<Vec<String>> {
    let data = master_request(
        address,
        gamename,
        key,
        &format!("\\list\\\\gamename\\{gamename}\\final\\"),
        timeout,
    )?;
    Ok(parse_master_list(&data))
}

/// How long one `\sync\` answer is reused for the other games of that master.
const SYNC_CACHE_TTL: Duration = Duration::from_secs(300);

/// Lists from `\sync\` answers, per master address.
type SyncCache = HashMap<String, (Instant, HashMap<String, Vec<String>>)>;
static SYNC_CACHE: LazyLock<Mutex<SyncCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Server list of `gamename` from a master that supports `\sync\` (333networks).
///
/// One request returns the lists of all `gamenames` at once; the answer is
/// cached for [`SYNC_CACHE_TTL`], so browsing the other games of that master
/// opens no new connection.
pub fn synced_list(
    address: &str,
    gamename: &str,
    key: &str,
    gamenames: &[String],
    timeout: Duration,
) -> io::Result<Vec<String>> {
    if let Some((fetched, lists)) = SYNC_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(address).cloned())
        && fetched.elapsed() < SYNC_CACHE_TTL
    {
        return Ok(lists.get(gamename).cloned().unwrap_or_default());
    }
    let mut names: Vec<&str> = gamenames.iter().map(String::as_str).collect();
    if !names.contains(&gamename) {
        names.push(gamename);
    }
    let data = master_request(
        address,
        gamename,
        key,
        // `msid` identifies OpenASE in the master's logs (optional field).
        &format!("\\msid\\OpenASE\\sync\\{}\\final\\", names.join(" ")),
        timeout,
    )?;
    let lists = parse_sync_answer(&data);
    let list = lists.get(gamename).cloned().unwrap_or_default();
    if let Ok(mut cache) = SYNC_CACHE.lock() {
        cache.insert(address.to_string(), (Instant::now(), lists));
    }
    Ok(list)
}

/// Parse a `\sync\` answer: `\gamename\ip:port ip:port ...\gamename2\...\final\`.
pub fn parse_sync_answer(data: &[u8]) -> HashMap<String, Vec<String>> {
    let text = String::from_utf8_lossy(data);
    let mut parts = text.split('\\');
    if text.starts_with('\\') {
        parts.next();
    }
    let parts: Vec<&str> = parts.collect();
    let mut lists = HashMap::new();
    for pair in parts.chunks(2) {
        let [name, addresses] = pair else { continue };
        if name.is_empty() || *name == "final" {
            continue;
        }
        let list: Vec<String> = addresses
            .split_whitespace()
            .filter(|address| address.contains(':'))
            .map(str::to_string)
            .collect();
        lists.insert(name.to_string(), list);
    }
    lists
}

/// Connect, answer the `\secure\` challenge for `gamename`, send `request`
/// and read the answer until `\final\` or until the master closes.
fn master_request(
    address: &str,
    gamename: &str,
    key: &str,
    request: &str,
    timeout: Duration,
) -> io::Result<Vec<u8>> {
    let target = address.to_socket_addrs()?.next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("cannot resolve {address}"))
    })?;
    let mut stream = TcpStream::connect_timeout(&target, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let mut buffer = [0u8; 1024];
    let size = stream.read(&mut buffer)?;
    let hello = String::from_utf8_lossy(&buffer[..size]).to_string();
    let secure = parse_kv(&hello)
        .into_iter()
        .find(|(name, _)| name == "secure")
        .map(|(_, value)| value)
        .unwrap_or_default();
    let validate = gsseckey(secure.as_bytes(), key.as_bytes());
    let request =
        format!("\\gamename\\{gamename}\\location\\0\\validate\\{validate}\\final\\{request}");
    stream.write_all(request.as_bytes())?;

    let mut data = Vec::new();
    let deadline = Instant::now() + timeout * 4;
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => {
                data.extend_from_slice(&buffer[..size]);
                if data.ends_with(b"\\final\\") {
                    break;
                }
            }
            Err(error) if data.is_empty() => return Err(error),
            Err(_) => break,
        }
        if Instant::now() > deadline {
            break;
        }
    }
    Ok(data)
}

/// Send a UDP query and collect the answer up to `\final\`.
///
/// Returns the merged key/values and the round-trip time of the first packet.
pub fn query(
    host: &str,
    port: u16,
    request: &str,
    timeout: Duration,
) -> io::Result<(Vec<(String, String)>, Duration)> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(timeout))?;
    socket.connect((host, port))?;
    let started = Instant::now();
    socket.send(request.as_bytes())?;

    let mut buffer = [0u8; 4096];
    let mut pairs = Vec::new();
    let mut rtt = None;
    loop {
        match socket.recv(&mut buffer) {
            Ok(size) => {
                rtt.get_or_insert_with(|| started.elapsed());
                let text = crate::quake3::latin1(&buffer[..size]);
                let finished = text.contains("\\final\\");
                pairs.extend(parse_kv(&text));
                if finished {
                    break;
                }
                socket.set_read_timeout(Some(Duration::from_millis(400)))?;
            }
            Err(error) if rtt.is_none() => return Err(error),
            Err(_) => break,
        }
    }
    Ok((pairs, rtt.unwrap_or_default()))
}

/// Build a normalized server from a `\status\` answer.
pub fn server_from_status(
    game: &str,
    host: &str,
    port: u16,
    pairs: Vec<(String, String)>,
) -> Server {
    let mut rules: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in pairs {
        if key != "queryid" && key != "final" {
            rules.entry(key).or_insert(value);
        }
    }

    let mut players = Vec::new();
    for index in 0.. {
        // Battlefield 1942 sends `playername_N` and `score_N` instead.
        let Some(name) = rules
            .get(&format!("player_{index}"))
            .or_else(|| rules.get(&format!("playername_{index}")))
        else {
            break;
        };
        players.push(Player {
            name: name.clone(),
            score: rules
                .get(&format!("frags_{index}"))
                .or_else(|| rules.get(&format!("score_{index}")))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            ping: rules
                .get(&format!("ping_{index}"))
                .and_then(|v| v.trim().parse().ok()),
            time_secs: None,
        });
    }
    rules.retain(|key, _| {
        ![
            "player_",
            "playername_",
            "frags_",
            "score_",
            "kills_",
            "deaths_",
            "ping_",
            "team_",
            "mesh_",
            "skin_",
            "face_",
            "ngsecret_",
        ]
        .iter()
        .any(|prefix| key.starts_with(prefix))
    });

    let mut server = Server::new(game, host, port);
    server.online = true;
    server.name = rules.get("hostname").cloned().unwrap_or_default();
    server.map = rules.get("mapname").cloned().unwrap_or_default();
    server.game_type = rules.get("gametype").cloned().unwrap_or_default();
    server.num_players = parse_count(rules.get("numplayers")).unwrap_or(players.len());
    server.max_players = parse_count(rules.get("maxplayers"));
    server.password = rules
        .get("password")
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    server.join_port = parse_count(rules.get("hostport"))
        .and_then(|port| u16::try_from(port).ok())
        .unwrap_or(port.saturating_sub(1));
    server.location = crate::location::server_location(&rules);
    server.players = players;
    server.rules = rules;
    server
}
