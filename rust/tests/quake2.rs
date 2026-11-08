//! Offline tests for the Quake 1 / Quake 2 family parsers.

use openase::catalog::{Master, Protocol, game};
use openase::quake2::{
    clean_text, parse_master_response, parse_player_row, parse_status, quake2_master_request,
    quakeworld_master_request, server_from_status,
};

const Q2_STATUS: &[u8] = b"\xff\xff\xff\xffprint\n\\maxclients\\16\\mapname\\q2dm1\\hostname\\Test Q2\\gamedir\\baseq2\\deathmatch\\1\\needpass\\1\n12 45 \"Player One\"\n-3 120 \"Two \"quoted\"\"\n";

const QW_STATUS: &[u8] = b"\xff\xff\xff\xffn\\*gamedir\\qw\\map\\dm4\\maxclients\\8\\teamplay\\2\\hostname\\QW Server\\needpass\\0\n12 5 3 40 \"\xc2\xcf\xc2\" \"base\" 4 4\n7 -2 10 80 \"Two\" \"base\" 13 13\n\x00";

#[test]
fn parses_quake2_status() {
    let (info, players) = parse_status(Q2_STATUS, Protocol::Quake2).unwrap();
    assert_eq!(info["mapname"], "q2dm1");
    assert_eq!(players.len(), 2);
    assert_eq!(players[0].name, "Player One");
    assert_eq!(players[0].score, 12);
    assert_eq!(players[0].ping, 45);
    assert_eq!(players[1].score, -3);
    assert_eq!(players[1].name, "Two \"quoted\"");

    let game = game("q2").unwrap();
    let server = server_from_status(game, "1.2.3.4", 27910, info, players);
    assert!(server.online);
    assert_eq!(server.name, "Test Q2");
    assert_eq!(server.map, "q2dm1");
    assert_eq!(server.max_players, Some(16));
    assert_eq!(server.mod_name, "baseq2");
    assert_eq!(server.game_type, "1");
    assert!(server.password);
    assert_eq!(server.num_players, 2);
}

#[test]
fn parses_quakeworld_status() {
    let (info, players) = parse_status(QW_STATUS, Protocol::QuakeWorld).unwrap();
    assert_eq!(info["map"], "dm4");
    assert_eq!(players.len(), 2);
    assert_eq!(players[0].score, 5);
    assert_eq!(players[0].ping, 40);
    assert_eq!(players[0].time_secs, Some(180));
    // High-bit "red" characters are stripped to plain ASCII.
    assert_eq!(players[0].name, "BOB");

    let game = game("qw").unwrap();
    let server = server_from_status(game, "1.2.3.4", 27500, info, players);
    assert_eq!(server.map, "dm4");
    assert_eq!(server.mod_name, "qw");
    assert_eq!(server.max_players, Some(8));
    assert!(!server.password);
    assert_eq!(game.game_type_name(&server.game_type), "Team (FF)");
}

#[test]
fn rejects_wrong_headers_and_bad_rows() {
    assert!(parse_status(b"\xff\xff\xff\xffinfoResponse\n\\a\\b", Protocol::Quake2).is_err());
    assert!(parse_status(b"print\n", Protocol::Quake2).is_err());
    assert!(parse_player_row("garbage", false).is_err());
    assert!(parse_player_row("1 2 3 \"x\"", false).is_err());
}

#[test]
fn clean_text_handles_console_characters() {
    assert_eq!(clean_text("\u{c1}bc"), "Abc");
    assert_eq!(clean_text("\u{12}\u{13}"), "01");
    assert_eq!(clean_text("\u{10}x\u{11}"), "[x]");
}

#[test]
fn parses_master_records() {
    let mut q2 = b"\xff\xff\xff\xffservers ".to_vec();
    q2.extend_from_slice(&[10, 0, 0, 1, 0x6d, 0xc6, 192, 168, 1, 2, 0x7a, 0xe6, 1, 2, 3]);
    assert_eq!(
        parse_master_response(&q2, Protocol::Quake2).unwrap(),
        ["10.0.0.1:28102", "192.168.1.2:31462"]
    );
    // Kingpin masters use a newline after the header.
    let mut kp = b"\xff\xff\xff\xffservers\n".to_vec();
    kp.extend_from_slice(&[10, 0, 0, 1, 0x7b, 0x17]);
    assert_eq!(
        parse_master_response(&kp, Protocol::Quake2).unwrap(),
        ["10.0.0.1:31511"]
    );
    let mut qw = b"\xff\xff\xff\xffd\n".to_vec();
    qw.extend_from_slice(&[1, 2, 3, 4, 0x6b, 0x6c]);
    assert_eq!(
        parse_master_response(&qw, Protocol::QuakeWorld).unwrap(),
        ["1.2.3.4:27500"]
    );
    assert!(parse_master_response(b"junk", Protocol::Quake2).is_err());
}

#[test]
fn master_requests_and_catalog_entries() {
    assert_eq!(quake2_master_request("query"), b"query\n");
    assert_eq!(quakeworld_master_request(), b"c\n");
    assert_eq!(game("qw").unwrap().protocol, Protocol::QuakeWorld);
    assert_eq!(game("q2").unwrap().protocol, Protocol::Quake2);
    assert_eq!(game("kingpin").unwrap().protocol, Protocol::Quake2);
    assert!(matches!(
        game("qw").unwrap().masters[0],
        Master::Quakeworld { .. }
    ));
    assert!(matches!(
        game("kingpin").unwrap().masters[0],
        Master::Quake2 { .. }
    ));
}
