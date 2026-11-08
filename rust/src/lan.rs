//! LAN server discovery by UDP broadcast, like ASE's "LAN servers".

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use crate::quake3::{OUT_OF_BAND_PREFIX, latin1, parse_info_response};

/// Ports probed for each protocol family.
const QUAKE3_PORTS: [u16; 8] = [27960, 27961, 27962, 27963, 28960, 28961, 28962, 28963];
const GAMESPY_PORTS: [u16; 5] = [7778, 7788, 7798, 28910, 28911];
const A2S_PORTS: [u16; 4] = [27015, 27016, 27017, 27018];
const QUAKE2_PORTS: [u16; 3] = [27910, 31510, 31511];
const QUAKEWORLD_PORTS: [u16; 2] = [27500, 27501];
/// Unreal Engine 2 query ports (game port + 1).
const UNREAL2_PORTS: [u16; 3] = [7778, 7788, 7798];
/// id Tech 4 default ports (Doom 3, Quake 4).
const DOOM3_PORTS: [u16; 2] = [27666, 28004];
/// ASE ports (Multi Theft Auto: game port + 123).
const ASE_PORTS: [u16; 2] = [22126, 22127];
/// Starsiege: Tribes and Tribes 2 game ports.
const TRIBES_PORTS: [u16; 3] = [28001, 28002, 28003];
const TRIBES2_PORTS: [u16; 3] = [28000, 28001, 28002];
/// Key sent in LAN Tribes queries.
const TRIBES_LAN_KEY: u16 = 0x4c41;

/// Savage and Savage DRX servers.
const SAVAGE_PORTS: [u16; 2] = [11235, 11236];

/// A server that answered on the local network, with the catalog game it
/// most likely runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanServer {
    pub game: String,
    pub address: String,
}

/// Guess the catalog game of a Quake 3 family `infoResponse`.
pub fn guess_quake3_game(info: &BTreeMap<String, String>, port: u16) -> &'static str {
    let get = |key: &str| {
        info.get(key)
            .map(|value| value.to_lowercase())
            .unwrap_or_default()
    };
    let gamename = get("gamename");
    let protocol = get("protocol");
    if gamename.starts_with("q3urt") {
        return "urt";
    }
    match protocol.as_str() {
        "71" => "oa",
        "84" | "83" | "82" => "et",
        "118" | "117" | "116" | "115" => "cod2",
        "5" | "6" if port >= 28960 || gamename.contains("call of duty") => {
            if get("shortversion").starts_with("1.7") || gamename.contains("4") {
                "cod4"
            } else {
                "cod"
            }
        }
        _ => "q3",
    }
}

/// Guess the catalog game of an id Tech 4 `infoResponse`: protocol major 2
/// is Quake 4, major 1 is Doom 3.
pub fn guess_doom3_game(packet: &[u8]) -> Option<&'static str> {
    let response = crate::doom3::parse_info_response(packet).ok()?;
    Some(if response.version >> 16 == 2 {
        "quake4"
    } else {
        "doom3"
    })
}

/// Broadcast queries and collect the servers that answer within `wait`.
pub fn discover(wait: Duration) -> io::Result<Vec<LanServer>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_broadcast(true)?;
    let broadcast = Ipv4Addr::BROADCAST;

    let mut getinfo = OUT_OF_BAND_PREFIX.to_vec();
    getinfo.extend_from_slice(b"getinfo openase-lan");
    for port in QUAKE3_PORTS {
        let _ = socket.send_to(&getinfo, SocketAddrV4::new(broadcast, port));
    }
    for port in GAMESPY_PORTS {
        let _ = socket.send_to(b"\\status\\", SocketAddrV4::new(broadcast, port));
    }
    let mut a2s = vec![0xff; 4];
    a2s.extend_from_slice(b"TSource Engine Query\0");
    for port in A2S_PORTS {
        let _ = socket.send_to(&a2s, SocketAddrV4::new(broadcast, port));
    }

    let mut status = OUT_OF_BAND_PREFIX.to_vec();
    status.extend_from_slice(b"status\n");
    for port in QUAKE2_PORTS.into_iter().chain(QUAKEWORLD_PORTS) {
        let _ = socket.send_to(&status, SocketAddrV4::new(broadcast, port));
    }
    for port in UNREAL2_PORTS {
        let _ = socket.send_to(
            &crate::unreal2::request(0),
            SocketAddrV4::new(broadcast, port),
        );
    }
    for port in DOOM3_PORTS {
        let _ = socket.send_to(
            &crate::doom3::info_request(crate::doom3::CHALLENGE),
            SocketAddrV4::new(broadcast, port),
        );
    }
    for port in ASE_PORTS {
        let _ = socket.send_to(b"s", SocketAddrV4::new(broadcast, port));
    }
    let tribes = crate::tribes::t1_status_request(TRIBES_LAN_KEY);
    for port in TRIBES_PORTS {
        let _ = socket.send_to(&tribes, SocketAddrV4::new(broadcast, port));
    }
    let tribes2 = crate::tribes::t2_request(crate::tribes::T2_PING, u32::from(TRIBES_LAN_KEY));
    for port in TRIBES2_PORTS {
        let _ = socket.send_to(&tribes2, SocketAddrV4::new(broadcast, port));
    }

    for port in SAVAGE_PORTS {
        let _ = socket.send_to(
            &crate::savage::REQUEST_INFO,
            SocketAddrV4::new(broadcast, port),
        );
    }

    let deadline = Instant::now() + wait;
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(remaining))?;
        let Ok((size, from)) = socket.recv_from(&mut buffer) else {
            break;
        };
        let packet = &buffer[..size];
        let mut address = from.to_string();
        if !seen.insert(address.clone()) {
            continue;
        }
        let game = if let Some(game) = guess_doom3_game(packet) {
            Some(game)
        } else if crate::savage::is_answer(packet) {
            Some("savage")
        } else if crate::unreal2::is_answer(packet) {
            // 128 is UT2004, older protocol numbers UT2003.
            Some(if packet[0] == crate::unreal2::UT2004_VERSION {
                "ut2004"
            } else {
                "ut2003"
            })
        } else if let Ok(reply) = crate::ase::parse_reply(packet) {
            // Addresses of ASE games hold the game port, not the query port.
            crate::catalog::game(&reply.game)
                .filter(|game| game.protocol == crate::catalog::Protocol::Ase)
                .filter(|_| reply.port != 0)
                .map(|game| {
                    address = format!("{}:{}", from.ip(), reply.port);
                    game.id.as_str()
                })
        } else if crate::tribes::is_t1_status(packet, Some(TRIBES_LAN_KEY)) {
            Some("tribes")
        } else if crate::tribes::is_t2_answer(
            packet,
            crate::tribes::T2_PING_ANSWER,
            Some(u32::from(TRIBES_LAN_KEY)),
        ) {
            Some("tribes2")
        } else if let Ok(info) = parse_info_response(packet) {
            Some(guess_quake3_game(&info, from.port()))
        } else if packet.starts_with(b"\\") || latin1(packet).contains("\\final\\") {
            Some(if latin1(packet).contains("\\gamename\\sofretail") {
                "sof"
            } else {
                "ut99"
            })
        } else if packet.starts_with(b"\xff\xff\xff\xffprint\n") {
            Some(if (31500..32000).contains(&from.port()) {
                "kingpin"
            } else {
                "q2"
            })
        } else if packet.starts_with(b"\xff\xff\xff\xffn\\") {
            Some("qw")
        } else if packet.starts_with(&[0xff; 4])
            && matches!(packet.get(4), Some(b'I' | b'm' | b'A'))
        {
            Some("cs16")
        } else {
            None
        };
        if let Some(game) = game {
            found.push(LanServer {
                game: game.to_string(),
                address,
            });
        }
    }
    Ok(found)
}
