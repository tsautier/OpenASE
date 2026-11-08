//! Offline tests for Starsiege: Tribes and Tribes 2 packets.

use openase::catalog::{Master, Protocol, game};
use openase::tribes::{
    T2_INFO, T2_PING, T2_PING_ANSWER, is_t1_ping, is_t1_status, is_t2_answer, parse_t1_master,
    parse_t1_status, parse_t2_info, parse_t2_ping, t1_ping_request, t1_status_request, t2_request,
    t2_server,
};

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn pascal(out: &mut Vec<u8>, text: &[u8]) {
    out.push(text.len() as u8);
    out.extend_from_slice(text);
}

/// Status answer captured from a Tribes 1.11 server (one player, two teams).
const T1_CAPTURE: &str = "632a2a620654726962657304312e313120526f73636f2773204d6978746170652031393939202d20506c617954312e636f0100012000000462617365074c54204d617073094e697276616e614c54434a6f696e207468652054726962657320446973636f7264204020706c617974312e636f6d2f646973636f7264202d205365727665722056657273696f6e20312e332e3102195465616d204e616d65096f53636f726509a6506c617965727322506c61796572204e616d65096f5465616d09a653636f726509d450696e6709f4504c0b426c6f6f64204561676c650a257409202020300920300d4469616d6f6e642053776f72640a257409202020300920301800ff04596f67690f256e0925740920203009257009256c";

#[test]
fn t1_requests_carry_the_key() {
    assert_eq!(t1_status_request(0x2a2a), *b"b**");
    assert_eq!(
        t1_ping_request(0x1234),
        [0x10, 0x03, 0xff, 0x00, 0x34, 0x12, 0x00, 0x00]
    );
}

#[test]
fn t1_captured_status_is_parsed() {
    let packet = hex(T1_CAPTURE);
    assert!(is_t1_status(&packet, Some(0x2a2a)));
    assert!(!is_t1_status(&packet, Some(0x1111)));
    let server = parse_t1_status("tribes", "1.2.3.4", 28006, &packet).unwrap();
    assert!(server.online);
    assert_eq!(server.name, "Rosco's Mixtape 1999 - PlayT1.co");
    assert_eq!(server.map, "NirvanaLT");
    assert_eq!(server.game_type, "LT Maps");
    assert_eq!(server.mod_name, "base");
    assert_eq!((server.num_players, server.max_players), (1, Some(32)));
    assert!(!server.password);
    assert_eq!(server.rules["version"], "1.11");
    assert_eq!(server.rules["dedicated"], "1");
    assert_eq!(server.rules["team_1"], "Diamond Sword");
    assert_eq!(server.players.len(), 1);
    assert_eq!(server.players[0].name, "Yogi");
    assert_eq!(server.players[0].ping, Some(96));
    assert_eq!(server.players[0].score, 0);
}

#[test]
fn t1_player_score_follows_the_header_and_zero_first_byte_is_accepted() {
    let mut packet = vec![0x00, 0x01, 0x02, b'b'];
    for text in [&b"Tribes"[..], b"1.30", b"Test Server"] {
        pascal(&mut packet, text);
    }
    packet.extend_from_slice(&[1, 1, 2, 16, 0x2c, 0x01]); // dedicated, password, 2/16, cpu 300
    for text in [&b"Shifter"[..], b"CTF", b"Raindance", b"Admin: someone"] {
        pascal(&mut packet, text);
    }
    packet.push(2); // teams
    pascal(&mut packet, b"Team Name\t\xd6Score");
    pascal(
        &mut packet,
        b"Player Name\toTeam\t\xa6Score\t\xcfPing\t\xefPL",
    );
    for (team, score) in [
        (&b"Blood Eagle"[..], &b"%t\t  5"[..]),
        (b"Diamond Sword", b"%t\t  2"),
    ] {
        pascal(&mut packet, team);
        pascal(&mut packet, score);
    }
    packet.extend_from_slice(&[20, 0, 0]);
    pascal(&mut packet, b"Alice");
    pascal(&mut packet, b"%n\t%t\t 12\t%p\t%l");
    packet.extend_from_slice(&[30, 1, 1]);
    pascal(&mut packet, b"Bob");
    pascal(&mut packet, b"%n\t%t\t -3\t%p\t%l");

    assert!(is_t1_status(&packet, Some(0x0201)));
    let server = parse_t1_status("tribes", "h", 28001, &packet).unwrap();
    assert!(server.password);
    assert_eq!(server.mod_name, "Shifter");
    assert_eq!(server.rules["cpu"], "300");
    assert_eq!(server.rules["score_0"], "5");
    let scores: Vec<(String, i32, Option<u32>)> = server
        .players
        .iter()
        .map(|p| (p.name.clone(), p.score, p.ping))
        .collect();
    assert_eq!(
        scores,
        vec![
            ("Alice".to_string(), 12, Some(80)),
            ("Bob".to_string(), -3, Some(120))
        ]
    );
}

#[test]
fn t1_truncated_player_list_keeps_the_server() {
    let full = hex(T1_CAPTURE);
    let server = parse_t1_status("tribes", "h", 1, &full[..full.len() - 6]).unwrap();
    assert_eq!(server.name, "Rosco's Mixtape 1999 - PlayT1.co");
    assert!(server.players.is_empty());
    assert!(parse_t1_status("tribes", "h", 1, &full[..20]).is_err());
}

#[test]
fn t1_ping_answer_is_recognized() {
    let mut packet = vec![0x10, 0x04, 0xff, 0xf0, 0x34, 0x12, 32, 3, 1, 0];
    packet.extend_from_slice(b"Name\0");
    assert!(is_t1_ping(&packet, 0x1234));
    assert!(!is_t1_ping(&packet, 0x1235));
}

#[test]
fn t1_master_packet_is_parsed() {
    let mut packet = vec![0x10, 0x06, 1, 2, 0x34, 0x12, 0x00, 0x66];
    pascal(&mut packet, b"Tribes Master");
    pascal(&mut packet, b"<jc>Welcome");
    packet.extend_from_slice(&[0, 3]); // big-endian count
    packet.extend_from_slice(&[6, 1, 2, 3, 4, 0x61, 0x6d]); // port 28001
    packet.extend_from_slice(&[6, 0, 0, 0, 0, 0x61, 0x6d]); // skipped
    packet.extend_from_slice(&[6, 10, 0, 0, 1, 0x62, 0x6d]); // port 28002
    let parsed = parse_t1_master(&packet).unwrap();
    assert_eq!((parsed.index, parsed.total, parsed.key), (1, 2, 0x1234));
    assert_eq!(parsed.name, "Tribes Master");
    assert_eq!(parsed.servers, vec!["1.2.3.4:28001", "10.0.0.1:28002"]);
    assert!(parse_t1_master(b"\x10\x04junkjunk").is_err());
}

#[test]
fn t2_requests_and_captured_ping() {
    assert_eq!(t2_request(T2_PING, 0x0403_0201), [14, 2, 1, 2, 3, 4]);
    assert_eq!(t2_request(T2_INFO, 1), [18, 2, 1, 0, 0, 0]);
    let packet =
        hex("10020102030404564552353300000033000000ca6100000f6c6f6f70277320626173656d656e74");
    assert!(is_t2_answer(&packet, T2_PING_ANSWER, Some(0x0403_0201)));
    assert!(!is_t2_answer(&packet, T2_PING_ANSWER, Some(7)));
    let ping = parse_t2_ping(&packet).unwrap();
    assert_eq!(ping.version, "VER5");
    assert_eq!(ping.protocol, Some(51));
    assert_eq!(ping.build, 25034);
    assert_eq!(ping.name, "loop's basement");
}

fn t2_info_packet() -> Vec<u8> {
    let mut packet = vec![20, 2, 1, 2, 3, 4];
    for text in [&b"Classic"[..], b"Capture the Flag", b"Katabatic"] {
        pascal(&mut packet, text);
    }
    packet.extend_from_slice(&[0x23, 3, 64, 1, 0xac, 0x0d]); // dedicated+password, 3/64, 1 bot
    pascal(&mut packet, b"Welcome to the server\nAdmin: me");
    let block = b"2\nStorm\t12\nInferno\t7\n3\n\x10\x08\x0bTAG\x0cAlice\x11\tStorm\t5\n\x10\x0eBot One\x11\tInferno\t2\n\x10\x08Bob\x11\tStorm\t-1\n";
    packet.extend_from_slice(&(block.len() as u16).to_le_bytes());
    packet.extend_from_slice(block);
    packet
}

#[test]
fn t2_info_lists_teams_and_players() {
    let info = parse_t2_info(&t2_info_packet()).unwrap();
    assert_eq!(info.game, "Classic");
    assert_eq!(info.mission_type, "Capture the Flag");
    assert_eq!(info.map, "Katabatic");
    assert_eq!((info.num_players, info.max_players, info.bots), (3, 64, 1));
    assert_eq!(info.cpu, 3500);
    assert_eq!(
        info.teams,
        vec![("Storm".to_string(), 12), ("Inferno".to_string(), 7)]
    );
    let players: Vec<(&str, i32)> = info
        .players
        .iter()
        .map(|p| (p.name.as_str(), p.score))
        .collect();
    assert_eq!(players, vec![("TAGAlice", 5), ("Bot One", 2), ("Bob", -1)]);
}

#[test]
fn t2_server_uses_ping_name_or_info_for_ver3() {
    let ping = parse_t2_ping(&hex(
        "10020102030404564552353300000033000000ca6100000f6c6f6f70277320626173656d656e74",
    ))
    .unwrap();
    let info = parse_t2_info(&t2_info_packet()).unwrap();
    let server = t2_server(
        "tribes2",
        "1.2.3.4",
        28000,
        Some(ping.clone()),
        info.clone(),
    );
    assert_eq!(server.name, "loop's basement");
    assert_eq!(server.game_type, "Capture the Flag");
    assert_eq!(server.mod_name, "Classic");
    assert!(server.password);
    assert_eq!((server.num_players, server.bots), (3, 1));
    assert_eq!(server.rules["net_protocol"], "51");
    assert_eq!(server.rules["score_1"], "7");

    let ver3 = openase::tribes::T2Ping {
        version: "VER3".into(),
        ..ping
    };
    let server = t2_server("tribes2", "h", 1, Some(ver3), info.clone());
    assert_eq!(server.name, "Welcome to the server");
    assert_eq!(
        t2_server("tribes2", "h", 1, None, info).name,
        "Welcome to the server"
    );
}

#[test]
fn t2_captured_info_with_unusual_player_rows() {
    let packet = hex(
        "1402010203040c436f6e737472756374696f6e0c436f6e737472756374696f6e09466c6174646173682bb10200001800b35669736974207573206f6e6c696e652040207468652d636f6e7374727563742e6e65742c206f6e20446973636f7264206174203c613a646973636f72642e67672f6a62786462416d3e646973636f72642e67672f6a62786462416d3c2f613e206f722049524320696e20237468655f636f6e7374727563742040206972632e7175616b656e65742e6f72670a0a416c6c206d617073206f6e2074686973207365727665722040207261777233322e6e65740a0a2a00310a436f6e7374727563746f727309300a320a084b7261736809090b434841540a085109090b43484154",
    );
    let info = parse_t2_info(&packet).unwrap();
    assert_eq!(info.map, "Flatdash+");
    assert_eq!(info.status & 2, 0);
    assert_eq!(info.teams, vec![("Constructors".to_string(), 0)]);
    let names: Vec<&str> = info.players.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Krash", "Q"]);
}

#[test]
fn tribes_catalog_entries() {
    let t1 = game("tribes").unwrap();
    assert_eq!(t1.protocol, Protocol::Tribes);
    assert_eq!(t1.default_port, 28001);
    assert!(matches!(t1.masters[0], Master::Tribes { .. }));
    assert_eq!(t1.game_type_name("C&H"), "Capture and Hold");
    let t2 = game("tribes2").unwrap();
    assert_eq!(t2.protocol, Protocol::Tribes2);
    assert_eq!(t2.default_port, 28000);
    assert!(matches!(t2.masters[0], Master::HttpList { .. }));
}
