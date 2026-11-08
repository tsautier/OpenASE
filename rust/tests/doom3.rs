//! Offline tests for the id Tech 4 (Doom 3 / Quake 4) parsers.

use openase::catalog::{Master, Protocol, game};
use openase::doom3::{
    info_request, is_info_response, master_request, parse_info_response, parse_master_response,
    parse_version, server_from_response, strip_rgb_codes,
};
use openase::lan::guess_doom3_game;

fn info_packet(version: u32, pairs: &[(&str, &str)], players: &[(u8, u16, &str, &str)]) -> Vec<u8> {
    let mut packet = b"\xff\xffinfoResponse\0".to_vec();
    packet.extend_from_slice(&0x1234_5678u32.to_le_bytes());
    packet.extend_from_slice(&version.to_le_bytes());
    for (key, value) in pairs {
        packet.extend_from_slice(key.as_bytes());
        packet.push(0);
        packet.extend_from_slice(value.as_bytes());
        packet.push(0);
    }
    // Empty key and empty value end the rules.
    packet.extend_from_slice(&[0, 0]);
    let quake4 = version >> 16 == 2;
    for (id, ping, name, clan) in players {
        packet.push(*id);
        packet.extend_from_slice(&ping.to_le_bytes());
        packet.extend_from_slice(&25600u32.to_le_bytes());
        packet.extend_from_slice(name.as_bytes());
        packet.push(0);
        if quake4 {
            packet.extend_from_slice(clan.as_bytes());
            packet.push(0);
        }
    }
    packet.push(32);
    // OS mask and trailing server flags.
    packet.extend_from_slice(&7u32.to_le_bytes());
    packet.extend_from_slice(b"dm\0\0\x01");
    packet
}

const D3_PAIRS: &[(&str, &str)] = &[
    ("si_name", "Test Doom"),
    ("si_map", "game/mp/d3dm3"),
    ("si_gameType", "Tourney"),
    ("si_maxPlayers", "4"),
    ("si_usePass", "1"),
    ("fs_game", "librecoop"),
];

#[test]
fn builds_requests() {
    assert_eq!(
        info_request(0x0403_0201),
        b"\xff\xffgetInfo\0\x01\x02\x03\x04"
    );
    assert_eq!(parse_version("1.43").unwrap(), 0x0001_002b);
    assert!(parse_version("two").is_err());
    // Doom 3: header, version, empty game string, filter byte (19 bytes).
    let request = master_request("1.0").unwrap();
    assert_eq!(request.len(), 19);
    assert_eq!(&request[..13], b"\xff\xffgetServers\0");
    assert_eq!(&request[13..17], &[0, 0, 1, 0]);
    // Quake 4 adds two empty strings (21 bytes).
    let request = master_request("2.0").unwrap();
    assert_eq!(request.len(), 21);
    assert_eq!(&request[13..17], &[0, 0, 2, 0]);
}

#[test]
fn parses_doom3_info_response() {
    let packet = info_packet(
        0x0001_002a,
        D3_PAIRS,
        &[(0, 45, "Marine", ""), (3, 80, "Bob", "")],
    );
    assert!(is_info_response(&packet));
    let response = parse_info_response(&packet).unwrap();
    assert_eq!(response.challenge, 0x1234_5678);
    assert_eq!(response.version, 0x0001_002a);
    assert_eq!(response.rules["si_name"], "Test Doom");
    assert_eq!(response.players.len(), 2);
    assert_eq!(response.players[1].id, 3);
    assert_eq!(response.players[1].ping, 80);
    assert_eq!(response.players[1].rate, 25600);
    assert_eq!(response.players[1].name, "Bob");

    let game = game("doom3").unwrap();
    assert_eq!(game.protocol, Protocol::Doom3);
    let server = server_from_response(game, "1.2.3.4", 27666, response);
    assert!(server.online);
    assert_eq!(server.name, "Test Doom");
    assert_eq!(server.map, "d3dm3");
    assert_eq!(server.mod_name, "librecoop");
    assert_eq!(server.max_players, Some(4));
    assert!(server.password);
    assert_eq!(server.num_players, 2);
    assert_eq!(server.players[0].ping, Some(45));
    assert_eq!(game.game_type_name(&server.game_type), "Tourney");
}

#[test]
fn parses_quake4_players_with_clan_tags() {
    let packet = info_packet(
        0x0002_0055,
        &[
            ("si_name", "^c900Red ^3Q4"),
            ("si_map", "mp/q4dm1"),
            ("si_maxPlayers", "12"),
        ],
        &[(1, 20, "Strogg", "[TAG]"), (2, 35, "Marine", "")],
    );
    assert_eq!(guess_doom3_game(&packet), Some("quake4"));
    let response = parse_info_response(&packet).unwrap();
    assert_eq!(response.players.len(), 2);
    assert_eq!(response.players[0].clan, "[TAG]");
    assert_eq!(response.players[1].name, "Marine");
    let server = server_from_response(game("quake4").unwrap(), "1.2.3.4", 28004, response);
    assert_eq!(server.name, "Red ^3Q4");
    assert_eq!(server.map, "q4dm1");
    assert!(!server.password);
    assert_eq!(server.num_players, 2);
}

#[test]
fn guesses_lan_game() {
    let packet = info_packet(0x0001_002a, D3_PAIRS, &[]);
    assert_eq!(guess_doom3_game(&packet), Some("doom3"));
    assert_eq!(guess_doom3_game(b"\xff\xff\xff\xffprint\n"), None);
}

#[test]
fn rejects_malformed_info_responses() {
    assert!(parse_info_response(b"\xff\xff\xff\xffinfoResponse\n").is_err());
    let mut packet = info_packet(0x0001_002a, D3_PAIRS, &[(0, 1, "A", "")]);
    packet.truncate(packet.len() - 12);
    assert!(parse_info_response(&packet).is_err());
    assert!(parse_info_response(b"\xff\xffinfoResponse\0\x01\x02").is_err());
}

#[test]
fn strips_rgb_color_codes() {
    assert_eq!(strip_rgb_codes("^c999Quake^c9024.net"), "Quake4.net");
    assert_eq!(strip_rgb_codes("^3A ^cab ^c12"), "^3A ^cab ^c12");
}

#[test]
fn parses_master_response() {
    let mut packet = b"\xff\xffservers\0".to_vec();
    packet.extend_from_slice(&[10, 0, 0, 1, 0x12, 0x6c]);
    packet.extend_from_slice(&[10, 0, 0, 2, 0, 0]);
    packet.extend_from_slice(&[192, 168, 1, 5, 0x64, 0x6d]);
    packet.extend_from_slice(&[1, 2]);
    assert_eq!(
        parse_master_response(&packet).unwrap(),
        ["10.0.0.1:27666", "192.168.1.5:28004"]
    );
    assert!(parse_master_response(b"\xff\xffinfoResponse\0").is_err());
}

#[test]
fn catalog_entries_have_masters() {
    for (id, version) in [("doom3", "1.0"), ("quake4", "2.0")] {
        let game = game(id).unwrap();
        assert!(
            game.masters
                .iter()
                .any(|master| matches!(master, Master::Doom3 { version: v, .. } if v == version))
        );
        for master in &game.masters {
            if let Master::Doom3 { version, .. } = master {
                assert!(master_request(version).is_ok());
            }
        }
    }
}
