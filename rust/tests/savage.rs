//! Offline tests for Savage / Savage DRX and Soldier of Fortune.

use openase::catalog::{Master, Protocol, game};
use openase::savage::{
    MAGIC, REQUEST_INFO, REQUEST_PLAYERS, is_answer, parse_fields, parse_master_list,
    parse_players, server_from_fields, strip_codes,
};

/// Answer of a Savage DRX server to the player request (hand-built from a
/// live capture): magic, `cf 0c`, echoed bytes, then `0xFF key 0xFE value`.
fn savage_answer() -> Vec<u8> {
    let mut packet = vec![
        0x9e, 0x4c, 0x23, 0x00, 0x00, 0xcf, 0x0c, 0x76, 0x46, 0x00, 0x00,
    ];
    let fields: [(&str, &str); 14] = [
        ("name", "^cTokyo ^yPublic ^cServer ^w(^yJP^w)"),
        ("world", "eden2"),
        ("cnum", "0"),
        ("vnum", "4"),
        ("cmax", "72"),
        ("time", "91159"),
        ("drx", "1\0"),
        ("gametype", "RTSS"),
        ("needcmdr", "1"),
        ("pass", "1"),
        ("pure", "0"),
        ("bal", "1"),
        ("notes", "Visit ^900savagedrx.com!"),
        (
            "players",
            "^900Team 1 (human)\n ^;GoldenBistro^:\n ^;Bobbe^:\n^900Team 2 (beast)\n ^b[O]^w^;UnclePhoe^:\n^900Spectators\n--empty--\n\0",
        ),
    ];
    for (key, value) in fields {
        packet.push(0xff);
        packet.extend_from_slice(key.as_bytes());
        packet.push(0xfe);
        packet.extend_from_slice(value.as_bytes());
    }
    packet
}

#[test]
fn requests_start_with_the_magic_bytes() {
    assert!(REQUEST_INFO.starts_with(&MAGIC));
    assert!(REQUEST_PLAYERS.starts_with(&MAGIC));
    assert_eq!(REQUEST_INFO[5], 0xc8);
    assert_eq!(REQUEST_PLAYERS[5], 0xce);
}

#[test]
fn strips_s2_color_codes() {
    assert_eq!(
        strip_codes("^cTokyo ^yPublic ^w(^yJP^w)"),
        "Tokyo Public (JP)"
    );
    assert_eq!(
        strip_codes("^048Coronavirus ^900Duels"),
        "Coronavirus Duels"
    );
    assert_eq!(
        strip_codes("^clan 3943^ ^yCommunity's ^970Kingdom ^w^clan 3943^"),
        " Community's Kingdom "
    );
    assert_eq!(strip_codes(" ^;GoldenBistro^:"), " GoldenBistro");
    assert_eq!(strip_codes("a^^b"), "a^b");
    assert_eq!(strip_codes("trailing^"), "trailing^");
}

#[test]
fn parses_savage_status() {
    let packet = savage_answer();
    assert!(is_answer(&packet));
    let fields = parse_fields(&packet).unwrap();
    assert_eq!(fields["world"], "eden2");
    assert_eq!(fields["drx"], "1");

    let server = server_from_fields("savage", "27.82.14.245", 11235, fields);
    assert!(server.online);
    assert_eq!(server.name, "Tokyo Public Server (JP)");
    assert_eq!(server.map, "eden2");
    assert_eq!(server.game_type, "RTSS");
    assert_eq!(server.max_players, Some(72));
    assert!(server.password);
    assert_eq!(server.num_players, 3);
    let names: Vec<_> = server.players.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["GoldenBistro", "Bobbe", "[O]UnclePhoe"]);
    assert_eq!(server.rules["notes"], "Visit savagedrx.com!");
    assert!(!server.rules.contains_key("players"));
}

#[test]
fn info_answer_without_players_uses_the_counts() {
    let mut packet = vec![
        0x9e, 0x4c, 0x23, 0x00, 0x00, 0xc9, 0x0c, 0x01, 0x21, 0x00, 0x00,
    ];
    packet.extend_from_slice(b"\xffname\xfeTest\xffcnum\xfe2\xffvnum\xfe3\xffcmax\xfe32\x00");
    let server = server_from_fields("savage", "1.2.3.4", 11235, parse_fields(&packet).unwrap());
    assert_eq!(server.num_players, 5);
    assert!(server.players.is_empty());
}

#[test]
fn rejects_other_packets() {
    assert!(!is_answer(b"\xff\xff\xff\xffprint\n"));
    assert!(parse_fields(b"\xff\xff\xff\xffprint\n").is_err());
    assert!(parse_fields(&[0x9e, 0x4c, 0x23, 0x00, 0x00, 0xcf, 0x0c, 0, 0, 0, 0, 0xff]).is_err());
    assert!(parse_players("^900Team 1 (human)\n--empty--\n^900Spectators\n--empty--\n").is_empty());
}

#[test]
fn parses_the_drx_server_list() {
    let json = br#"[
        {"port":11235,"id":1,"host_name":"159.69.45.211","game_type":"RTSS"},
        {"port":11236,"id":2,"host_name":"159.69.45.211"},
        {"port":11236,"id":3,"host_name":"159.69.45.211"},
        {"port":0,"host_name":"1.2.3.4"},
        {"id":9}
    ]"#;
    assert_eq!(
        parse_master_list(json).unwrap(),
        ["159.69.45.211:11235", "159.69.45.211:11236"]
    );
    assert!(parse_master_list(b"{}").is_err());
    assert!(parse_master_list(b"<html>").is_err());
}

#[test]
fn savage_catalog_entry() {
    let savage = game("savage").unwrap();
    assert_eq!(savage.protocol, Protocol::Savage);
    assert_eq!(savage.default_port, 11235);
    assert!(matches!(&savage.masters[0], Master::SavageDrx { .. }));
}

#[test]
fn soldier_of_fortune_uses_gamespy_status_and_master() {
    let sof = game("sof").unwrap();
    assert_eq!(sof.protocol, Protocol::Gamespy);
    assert!(sof.masters.iter().all(|master| matches!(
        master,
        Master::Gamespy { gamename, .. } | Master::Networks333 { gamename, .. }
            if gamename == "sofretail"
    )));

    // Captured status answer of a live SoF 1.07 server.
    let status = r"\gamename\sofretail\gamever\1.07f\location\1\hostname\Mad Onion\hostport\28910\mapname\dm/madrange\gametype\DM\numplayers\1\maxplayers\32\gamemode\openplaying\violence\63\timelimit\20\player_0\Bob\frags_0\7\ping_0\40\final\\queryid\24055.1";
    let server = openase::gamespy::server_from_status(
        "sof",
        "5.135.46.179",
        28911,
        openase::gamespy::parse_kv(status),
    );
    assert_eq!(server.name, "Mad Onion");
    assert_eq!(server.map, "dm/madrange");
    assert_eq!(server.game_type, "DM");
    assert_eq!(server.max_players, Some(32));
    assert_eq!(server.port, 28911);
    assert_eq!(server.join_port, 28910);
    assert_eq!(server.players.len(), 1);
    assert_eq!(server.players[0].name, "Bob");
    assert_eq!(server.players[0].score, 7);
}
