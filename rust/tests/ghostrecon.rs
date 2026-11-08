//! Offline tests for the Ghost Recon / Sum of All Fears status parser,
//! with packets built from the layout documented in qstat.

use openase::catalog::{Protocol, game};
use openase::ghostrecon::{
    HEADER, REQUEST, game_for_byte, is_answer, parse_status, server_from_status,
};

fn string(out: &mut Vec<u8>, text: &[u8]) {
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text);
}

/// Build a status answer; `full` adds the fields after the player names.
fn answer(game_byte: u8, password: bool, players: &[&[u8]], full: bool) -> Vec<u8> {
    let mut body = vec![
        0xf4, 0x03, game_byte, 0x04, 0x00, 0x41, 0x04, 0x00, 0x41, 0x00, 0x00, 0x78, 0x30, 0x63,
    ];
    body.extend_from_slice(&[0; 4]);
    string(&mut body, b"GR Server");
    string(&mut body, b"Castle");
    string(&mut body, b"Mission 3");
    string(&mut body, b"Firefight");
    body.extend_from_slice(&[0, u8::from(password)]);
    body.extend_from_slice(&36u32.to_le_bytes());
    body.extend_from_slice(&(players.len() as u32 + 1).to_le_bytes());
    string(&mut body, b"Server");
    for name in players {
        string(&mut body, name);
    }
    if full {
        body.extend_from_slice(&[0; 17]);
        string(&mut body, b"1.4");
        string(&mut body, b"\\mods\\origmiss\\mods\\it");
        body.push(1);
        body.extend_from_slice(&90.0f32.to_le_bytes());
        body.push(4);
        body.extend_from_slice(&[0; 3]);
        body.push(2);
        body.extend_from_slice(&[0; 3]);
        string(&mut body, b"Welcome");
        body.extend_from_slice(&2u32.to_le_bytes());
        body.extend_from_slice(&600.0f32.to_le_bytes());
    }
    let mut packet = HEADER.to_vec();
    packet.push(0x42);
    packet.extend_from_slice(&(body.len() as u16).to_le_bytes());
    packet.extend_from_slice(&body);
    packet
}

#[test]
fn request_has_header_and_length() {
    assert_eq!(REQUEST[..4], HEADER);
    assert_eq!(REQUEST[4], 0x42);
    assert_eq!(
        u16::from_le_bytes([REQUEST[5], REQUEST[6]]) as usize,
        REQUEST.len() - 7
    );
}

#[test]
fn parses_full_status() {
    let packet = answer(20, true, &[b"Ghost1", b"Ghost2"], true);
    assert!(is_answer(&packet));
    let status = parse_status(&packet).unwrap();
    assert_eq!(status.game_byte, 20);
    assert_eq!(game_for_byte(status.game_byte), Some("ghostrecon"));
    assert_eq!(status.players, ["Ghost1", "Ghost2"]);
    assert_eq!(status.max_players, 35);

    let server = server_from_status("ghostrecon", "1.2.3.4", 2348, status);
    assert!(server.online);
    assert_eq!(server.name, "GR Server");
    assert_eq!(server.map, "Castle");
    assert!(server.password);
    assert_eq!(server.num_players, 2);
    assert_eq!(server.max_players, Some(35));
    assert_eq!(server.game_type, "Coop");
    assert_eq!(server.rules["patch"], "1.4");
    assert_eq!(server.rules["version"], "1.4");
    assert_eq!(server.rules["mods"], "origmiss it");
    assert_eq!(server.rules["dedicated"], "1");
    assert_eq!(server.rules["status"], "Playing");
    assert_eq!(server.rules["mission"], "Mission 3");
    assert_eq!(server.rules["missiontype"], "Firefight");
    assert_eq!(server.rules["motd"], "Welcome");
    assert_eq!(server.rules["spawntype"], "Team");
    assert_eq!(server.rules["timeplayed"], "1:30");
    assert_eq!(server.rules["timeleft"], "8:30");
}

#[test]
fn keeps_basic_fields_when_tail_is_missing() {
    let packet = answer(13, false, &[], false);
    let status = parse_status(&packet).unwrap();
    assert_eq!(game_for_byte(status.game_byte), Some("soaf"));
    let server = server_from_status("soaf", "1.2.3.4", 2348, status);
    assert_eq!(server.name, "GR Server");
    assert_eq!(server.num_players, 0);
    assert!(!server.password);
    assert_eq!(server.game_type, "");
    assert!(!server.rules.contains_key("mission"));
}

#[test]
fn rejects_bad_packets() {
    let mut packet = answer(20, false, &[b"A"], true);
    packet.push(0);
    assert!(parse_status(&packet).is_err(), "length mismatch");
    assert!(parse_status(b"\xc0\xde\xf1\x11\x42\x00\x00").is_err());
    let mut truncated = answer(20, false, &[b"A"], false);
    truncated.truncate(40);
    let len = (truncated.len() - 7) as u16;
    truncated[5..7].copy_from_slice(&len.to_le_bytes());
    assert!(parse_status(&truncated).is_err());
}

#[test]
fn game_bytes_map_to_catalog_games() {
    for (byte, id) in [
        (20, "ghostrecon"),
        (11, "ghostrecond"),
        (13, "soaf"),
        (14, "soafd"),
    ] {
        assert_eq!(game_for_byte(byte), Some(id));
        let game = game(id).unwrap();
        assert_eq!(game.protocol, Protocol::Ghostrecon);
        assert_eq!(game.default_port, 2346);
        assert_eq!(game.query_port_offset, 2);
    }
    assert_eq!(game_for_byte(0), None);
}
