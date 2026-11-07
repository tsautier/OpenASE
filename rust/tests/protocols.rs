//! Offline tests for the catalog, GameSpy, A2S, Steam, launch and ping stats.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use openase::a2s::{parse_info, parse_players, parse_rules, reassemble_goldsrc};
use openase::catalog::{Master, Protocol, game, games};
use openase::gamespy::{gsseckey, parse_kv, parse_master_list, server_from_status};
use openase::launch::{expand_template, join_args, split_args};
use openase::model::Server;
use openase::quake3::{OUT_OF_BAND_PREFIX, Quake3Player, Quake3Server, parse_getservers_response};
use openase::query::{apply_ping_stats, from_quake3};
use openase::settings::{GameSettings, ServerRef, Settings};
use openase::steam::parse_server_list;

// --- catalog ---

#[test]
fn catalog_has_every_game_with_masters_and_join_template() {
    let ids: Vec<&str> = games().iter().map(|game| game.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "q3", "urt", "oa", "et", "cod", "cod2", "cod4", "ut99", "cs16"
        ]
    );
    for game in games() {
        assert!(!game.masters.is_empty(), "{} has no master", game.id);
        assert!(game.join.contains("{address}"), "{} join template", game.id);
        assert!(
            openase::colors::parse_hex(&game.color).is_some(),
            "{} color",
            game.id
        );
    }
}

#[test]
fn catalog_protocols_match_games() {
    assert_eq!(game("cod2").unwrap().protocol, Protocol::Quake3);
    assert_eq!(game("ut99").unwrap().protocol, Protocol::Gamespy);
    assert_eq!(game("cs16").unwrap().protocol, Protocol::A2s);
    assert!(matches!(
        game("cs16").unwrap().masters[0],
        Master::SteamWeb { .. }
    ));
}

#[test]
fn game_type_names_are_mapped() {
    assert_eq!(
        game("cod2").unwrap().game_type_name("sd"),
        "Search and Destroy"
    );
    assert_eq!(game("q3").unwrap().game_type_name("4"), "CTF");
    assert_eq!(
        game("ut99").unwrap().game_type_name("CTFGame"),
        "Capture the Flag"
    );
    assert_eq!(game("q3").unwrap().game_type_name("99"), "99");
}

// --- Quake 3 family ---

#[test]
fn cod_master_packet_with_newline_header_and_eof() {
    let mut packet = OUT_OF_BAND_PREFIX.to_vec();
    packet.extend_from_slice(b"getserversResponse\n\0\\");
    packet.extend_from_slice(&[192, 168, 1, 10, 0x70, 0xe0]);
    packet.extend_from_slice(b"\\EOF");
    assert_eq!(
        parse_getservers_response(&packet).unwrap(),
        vec!["192.168.1.10:28896".to_string()]
    );
}

#[test]
fn quake3_status_is_normalized() {
    let raw = Quake3Server {
        host: "1.2.3.4".into(),
        port: 28960,
        online: true,
        info: BTreeMap::from([
            ("sv_hostname".to_string(), "^1Red".to_string()),
            ("mapname".to_string(), "mp_toujane".to_string()),
            ("g_gametype".to_string(), "sd".to_string()),
            ("fs_game".to_string(), "mods/pam".to_string()),
            ("sv_maxclients".to_string(), "20".to_string()),
            ("pswrd".to_string(), "1".to_string()),
        ]),
        players: vec![
            Quake3Player {
                score: 5,
                ping: 40,
                name: "A".into(),
            },
            Quake3Player {
                score: 1,
                ping: 0,
                name: "Bot".into(),
            },
        ],
        error: None,
        ping_ms: Some(30),
    };
    let server = from_quake3(game("cod2").unwrap(), raw);
    assert_eq!(server.name, "^1Red");
    assert_eq!(server.map, "mp_toujane");
    assert_eq!(server.game_type, "sd");
    assert_eq!(server.mod_name, "mods/pam");
    assert_eq!(
        (server.num_players, server.max_players, server.bots),
        (2, Some(20), 1)
    );
    assert!(server.password);
}

// --- GameSpy ---

#[test]
fn gsseckey_matches_reference_vectors() {
    assert_eq!(gsseckey(b"DMCGWJ", b"Z5Nfb0"), "J/tBHrv9");
    assert_eq!(gsseckey(b"ABCDEF", b"Z5Nfb0"), "5OOIQ5Ha");
    assert_eq!(gsseckey(b"Ui4Lq8", b"Z5Nfb0"), "H68DUrhv");
}

#[test]
fn gamespy_master_list_is_parsed() {
    let data = b"\\ip\\1.2.3.4:7778\\ip\\5.6.7.8:9000\\final\\";
    assert_eq!(
        parse_master_list(data),
        vec!["1.2.3.4:7778", "5.6.7.8:9000"]
    );
}

#[test]
fn gamespy_status_builds_server_and_players() {
    let pairs = parse_kv(
        "\\hostname\\My UT\\hostport\\7777\\mapname\\CTF-Face\\gametype\\CTFGame\\numplayers\\2\\maxplayers\\16\\password\\True\\player_0\\Alice\\frags_0\\12\\ping_0\\ 48\\player_1\\Bob\\frags_1\\-1\\ping_1\\90\\queryid\\12.1\\final\\",
    );
    let server = server_from_status("ut99", "9.9.9.9", 7778, pairs);
    assert_eq!(server.name, "My UT");
    assert_eq!(server.join_address(), "9.9.9.9:7777");
    assert_eq!(server.game_type, "CTFGame");
    assert_eq!((server.num_players, server.max_players), (2, Some(16)));
    assert!(server.password);
    assert_eq!(server.players.len(), 2);
    assert_eq!(
        (server.players[0].score, server.players[0].ping),
        (12, Some(48))
    );
    assert!(!server.rules.contains_key("player_0"));
}

// --- A2S ---

#[test]
fn a2s_source_info_is_parsed() {
    let mut body = vec![b'I', 48];
    for text in ["Name", "de_dust2", "cstrike", "Counter-Strike"] {
        body.extend_from_slice(text.as_bytes());
        body.push(0);
    }
    body.extend_from_slice(&[10, 0, 12, 32, 2, b'd', b'w', 1, 1]);
    body.extend_from_slice(b"1.1.2.7\0");
    let info = parse_info(&body).unwrap();
    assert_eq!(
        (info.name.as_str(), info.map.as_str(), info.folder.as_str()),
        ("Name", "de_dust2", "cstrike")
    );
    assert_eq!((info.players, info.max_players, info.bots), (12, 32, 2));
    assert!(info.password && info.vac);
    assert_eq!(info.version, "1.1.2.7");
}

#[test]
fn a2s_goldsrc_info_is_parsed() {
    let mut body = vec![b'm'];
    for text in [
        "1.2.3.4:27015",
        "Old",
        "de_nuke",
        "cstrike",
        "Counter-Strike",
    ] {
        body.extend_from_slice(text.as_bytes());
        body.push(0);
    }
    body.extend_from_slice(&[5, 20, 47, b'd', b'w', 0, 0, 1, 3]);
    let info = parse_info(&body).unwrap();
    assert_eq!((info.name.as_str(), info.map.as_str()), ("Old", "de_nuke"));
    assert_eq!((info.players, info.max_players, info.bots), (5, 20, 3));
    assert!(info.vac && !info.password);
}

#[test]
fn a2s_players_and_rules_are_parsed() {
    let mut body = vec![b'D', 1, 0];
    body.extend_from_slice(b"Alice\0");
    body.extend_from_slice(&7i32.to_le_bytes());
    body.extend_from_slice(&125.5f32.to_le_bytes());
    let players = parse_players(&body).unwrap();
    assert_eq!(players[0].name, "Alice");
    assert_eq!((players[0].score, players[0].time_secs), (7, Some(125)));

    let mut rules = vec![b'E', 2, 0];
    rules.extend_from_slice(b"mp_timelimit\x0020\x00sv_gravity\x00800\x00");
    assert_eq!(
        parse_rules(&rules).unwrap()[1],
        ("sv_gravity".to_string(), "800".to_string())
    );
}

#[test]
fn a2s_goldsrc_split_packets_are_reassembled_in_order() {
    let header = [0xfe, 0xff, 0xff, 0xff, 1, 2, 3, 4];
    let mut first = header.to_vec();
    first.push(0x02); // index 0, total 2
    first.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, b'E', 1]);
    let mut second = header.to_vec();
    second.push(0x12); // index 1, total 2
    second.extend_from_slice(b"\0a\0b\0");
    let body = reassemble_goldsrc(&[second, first]).unwrap();
    assert_eq!(
        parse_rules(&body).unwrap(),
        vec![("a".to_string(), "b".to_string())]
    );
}

// --- Steam ---

#[test]
fn steam_server_list_json_is_parsed() {
    let json = r#"{"response":{"servers":[{"addr":"1.2.3.4:27015","gameport":27015},{"addr":"5.6.7.8:27016"}]}}"#;
    assert_eq!(
        parse_server_list(json).unwrap(),
        vec!["1.2.3.4:27015", "5.6.7.8:27016"]
    );
    assert!(parse_server_list(r#"{"response":{}}"#).unwrap().is_empty());
}

// --- launch ---

#[test]
fn template_drops_groups_with_empty_values() {
    let vars = HashMap::from([
        ("address", "1.2.3.4:28960".to_string()),
        ("name", "Thomas".to_string()),
        ("password", String::new()),
    ]);
    assert_eq!(
        expand_template(
            "[+name \"{name}\" ][+password \"{password}\" ]+connect {address}",
            &vars
        ),
        "+name \"Thomas\" +connect 1.2.3.4:28960"
    );
}

#[test]
fn split_args_honors_quotes() {
    assert_eq!(
        split_args("+name \"Big Boss\" +connect 1.2.3.4"),
        vec!["+name", "Big Boss", "+connect", "1.2.3.4"]
    );
}

#[test]
fn join_args_use_mod_and_unreal_join_port() {
    let mut server = Server::new("q3", "1.2.3.4", 27960);
    server.mod_name = "osp".into();
    let settings = GameSettings {
        player_name: "Me".into(),
        ..GameSettings::default()
    };
    assert_eq!(
        join_args(game("q3").unwrap(), &settings, &server, ""),
        vec![
            "+set",
            "fs_game",
            "osp",
            "+name",
            "Me",
            "+connect",
            "1.2.3.4:27960"
        ]
    );

    server.mod_name = "baseq3".into();
    assert_eq!(
        join_args(game("q3").unwrap(), &GameSettings::default(), &server, "pw"),
        vec!["+password", "pw", "+connect", "1.2.3.4:27960"]
    );

    let mut ut = Server::new("ut99", "5.6.7.8", 7778);
    ut.join_port = 7777;
    assert_eq!(
        join_args(game("ut99").unwrap(), &settings, &ut, ""),
        vec!["5.6.7.8:7777?Name=Me"]
    );
}

// --- ping stats and settings ---

#[test]
fn ping_flux_and_loss_are_computed() {
    let mut server = Server::new("q3", "h", 1);
    let samples = [40, 50, 60].map(Duration::from_millis);
    apply_ping_stats(&mut server, &samples, 4);
    assert_eq!(
        (server.ping, server.flux, server.packet_loss),
        (Some(50), Some(10), Some(25))
    );
}

#[test]
fn favorites_and_recent_are_managed() {
    let mut settings = Settings::default();
    let a = ServerRef {
        game: "q3".into(),
        address: "1.1.1.1:27960".into(),
    };
    settings.toggle_favorite(a.clone());
    assert!(settings.is_favorite(&a));
    settings.toggle_favorite(a.clone());
    assert!(!settings.is_favorite(&a));
    for i in 0..25 {
        settings.add_recent(ServerRef {
            game: "q3".into(),
            address: format!("1.1.1.{i}:27960"),
        });
    }
    settings.add_recent(ServerRef {
        game: "q3".into(),
        address: "1.1.1.10:27960".into(),
    });
    assert_eq!(settings.recent.len(), openase::settings::RECENT_LIMIT);
    assert_eq!(settings.recent[0].address, "1.1.1.10:27960");
}
