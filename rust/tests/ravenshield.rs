//! Offline tests for the Raven Shield beacon and OpenRVS list parsers.

use openase::catalog::{Master, Protocol, game};
use openase::launch::join_args;
use openase::ravenshield::{is_answer, parse_csv_list, parse_report, server_from_report};
use openase::settings::GameSettings;

/// Shape of a live answer (OBSOLETESUPERSTARS.COM, captured 2026-11),
/// shortened, with players added, NUL padded like the real fixed buffer.
fn report_packet() -> Vec<u8> {
    let mut packet = b"rvnshld 7877 KEYWORD  \xb6P1 7877 \xb6E1 RLC_Airport_Mini_01 \xb6I1 OBSOLETESUPERSTARS.COM \xb6F1 RGM_TerroristHuntCoopMode \xb6A1 8 \xb6G1 0 \xb6H1 1 \xb6L1 /Ding/Caf\xe9 Bob \xb6M1 /12:05/0:42 \xb6N1 /48/120 \xb6O1 /17/3 \xb6B1 2 \xb6Q1 1 \xb6R1 600 \xb6D2 PATCH 1.60 (build 412) \xb6G2 8877 \xb6L2 RavenShield \xb6L3 0 \xb6O2 Welcome \xb6K1 /RLC_Airport_Mini_01/RLC_Airport_Mini_02 \xb6J1 /RGM_TerroristHuntCoopMode/RGM_Terrori".to_vec();
    packet.resize(packet.len() + 300, 0);
    packet
}

#[test]
fn parses_beacon_report() {
    let packet = report_packet();
    assert!(is_answer(&packet));
    let report = parse_report(&packet).unwrap();
    assert_eq!(report.product, "rvnshld");
    assert_eq!(report.port, 7877);
    assert_eq!(report.get("I1"), Some("OBSOLETESUPERSTARS.COM"));
    assert_eq!(report.get("D2"), Some("PATCH 1.60 (build 412)"));
    // Truncated last field is kept as sent.
    assert_eq!(
        report.get("J1"),
        Some("/RGM_TerroristHuntCoopMode/RGM_Terrori")
    );

    let server = server_from_report("ravenshield", "172.93.104.79", 8877, &report);
    assert!(server.online);
    assert_eq!(server.name, "OBSOLETESUPERSTARS.COM");
    assert_eq!(server.map, "RLC_Airport_Mini_01");
    assert_eq!(server.game_type, "RGM_TerroristHuntCoopMode");
    assert_eq!(server.mod_name, "RavenShield");
    assert_eq!(server.max_players, Some(8));
    assert_eq!(server.num_players, 2);
    assert!(!server.password);
    assert_eq!(server.join_port, 7877);
    assert_eq!(server.players.len(), 2);
    assert_eq!(server.players[0].name, "Ding");
    assert_eq!(server.players[0].score, 17);
    assert_eq!(server.players[0].ping, Some(48));
    assert_eq!(server.players[0].time_secs, Some(725));
    assert_eq!(server.players[1].name, "Café Bob");
    assert_eq!(server.players[1].time_secs, Some(42));
    assert_eq!(server.rules["version"], "PATCH 1.60 (build 412)");
    assert_eq!(server.rules["punkbuster"], "0");
    assert_eq!(server.rules["motd"], "Welcome");
    assert_eq!(server.rules["gametype"], "RGM_TerroristHuntCoopMode");
    assert_eq!(server.rules["mapname"], "RLC_Airport_Mini_01");
    assert!(!server.rules.contains_key("L1"));
    assert!(!server.rules.contains_key("J1"));
}

#[test]
fn counts_players_without_player_count_field() {
    // Some servers send the map list early, and the count is cut off.
    let packet = b"rvnshld 7779 KEYWORD  \xb6P1 7779 \xb6I1 Locked \xb6A1 16 \xb6G1 1 \xb6L1 /One \xb6K1 /MP_Castle/MP_Hotel".to_vec();
    let report = parse_report(&packet).unwrap();
    let server = server_from_report("ravenshield", "1.2.3.4", 8779, &report);
    assert_eq!(server.num_players, 1);
    assert!(server.password);
    assert_eq!(server.max_players, Some(16));
}

#[test]
fn empty_player_list_means_no_players() {
    let packet = b"rvnshld 7777 KEYWORD  \xb6I1 Empty \xb6L1  \xb6M1  \xb6B1 0 ".to_vec();
    let report = parse_report(&packet).unwrap();
    let server = server_from_report("ravenshield", "1.2.3.4", 8777, &report);
    assert!(server.players.is_empty());
    assert_eq!(server.num_players, 0);
}

#[test]
fn rejects_other_beacon_answers() {
    assert!(parse_report(b"ut2004 7777").is_err());
    assert!(parse_report(b"rvnshld 7777").is_err());
    assert!(parse_report(b"rvnshld x KEYWORD ").is_err());
    assert!(!is_answer(b"\\status\\"));
}

#[test]
fn parses_openrvs_csv_list() {
    let text = "name,ip,port,mode\r\nALLR6 | Tango Hunt 1,162.248.92.181,7780,coop\nOne, two, and three,1.2.3.4,6777,adv\nbad line\nNo port,5.6.7.8,x,coop\nALLR6 | Tango Hunt 1,162.248.92.181,7780,coop\n";
    assert_eq!(
        parse_csv_list(text).unwrap(),
        ["162.248.92.181:7780", "1.2.3.4:6777"]
    );
    assert!(parse_csv_list("").is_err());
    assert!(parse_csv_list("name,address\n").is_err());
}

#[test]
fn catalog_entry_queries_beacon_port() {
    let game = game("ravenshield").unwrap();
    assert_eq!(game.protocol, Protocol::Ravenshield);
    assert_eq!(game.default_port, 7777);
    assert_eq!(game.query_port_offset, 1000);
    assert!(matches!(game.masters[0], Master::Csv { .. }));
    assert_eq!(game.launch_mod("RavenShield"), "");
    assert_eq!(game.launch_mod("AthenaSword"), "AthenaSword");

    let report = parse_report(&report_packet()).unwrap();
    let mut server = server_from_report("ravenshield", "172.93.104.79", 8877, &report);
    server.port = 7877;
    assert_eq!(
        join_args(game, &GameSettings::default(), &server, "secret"),
        ["-ip=172.93.104.79:7877", "-pwd=secret"]
    );
}
