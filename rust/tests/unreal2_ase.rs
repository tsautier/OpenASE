//! Offline tests for the Unreal Engine 2 and All-Seeing Eye protocols, with
//! packets captured from live UT2004, UT2003 and Multi Theft Auto servers.

use openase::ase::{parse_mta_list, parse_reply, server_from_reply};
use openase::catalog::{Master, Protocol, game};
use openase::launch::join_args;
use openase::model::Server;
use openase::settings::GameSettings;
use openase::unreal2::{
    Reader, client_hello, parse_info, parse_master_record, parse_players, parse_rules,
    server_from_packets, strip_colors,
};

// --- Unreal Engine 2 ---

/// UT2004 server info answer (Onslaught server, colored name).
const UT2004_INFO: &[u8] = b"\x80\x00\x00\x00\x00\x00\x00\x00\x00\x00a\x1e\x00\x00\x00\x00\x00\x00K\x01\x1b\xf8\x80@AU ONS/VCTF \x1b```|\x1b\xf8@@ Bellum \x1b```| \x1b\xea\xea\xeaFriday Night Chaos Fri 8pm AEDT\x00%ONS-CaptainGordons_Box_of_Pwnage-RC1\x00\x11ONSOnslaughtGame\x00\x00\x00\x00\x00 \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x020\x00\x00\x00";

/// UT2004 rules answer (shortened), with repeated `Mutator` keys.
const UT2004_RULES: &[u8] = b"\x80\x00\x00\x00\x01\x0bServerMode\x00\ndedicated\x00\x0eServerVersion\x00\x053369\x00\x08Mutator\x00\x0fMutUTOptimiser\x00\x08Mutator\x00\nMutUTComp\x00\x0dGamePassword\x00\x05True\x00";

/// UT2004 players answer: Onslaught team score entries plus one real player.
const UT2004_PLAYERS: &[u8] = b"\x80\x00\x00\x00\x02\x00\x00\x00\x00\x13\x1b\n\xf5\nRed Team Score\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x14\x1b\n\xf5\nBlue Team Score\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x07\x00\x00\x00\x07Player\x00\x41\x00\x00\x00\x0c\x00\x00\x00\x00\x00\x00\x00";

/// UT2003 server info answer (protocol 121, `y` header).
const UT2003_INFO: &[u8] = b"y\x00\x00\x00\x00\x00\x00\x00\x00\x00W\x1e\x00\x00\x00\x00\x00\x00\x14<SCaRYeiRe-Classic>\x00\x10CTF-MorningSuns\x00\txCTFGame\x00\x06\x00\x00\x00\x0c\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

#[test]
fn unreal2_compact_index_strings() {
    // 0x64 0x01: 36 + (1 << 6) = 100.
    let mut data = vec![0x64, 0x01];
    data.extend(std::iter::repeat_n(b'a', 99));
    data.push(0);
    let mut reader = Reader::new(&data);
    assert_eq!(reader.string().unwrap(), "a".repeat(99));
    assert!(reader.done());

    // Negative length: UTF-16, here "Hé" plus NUL.
    let utf16 = [0x83, b'H', 0, 0xe9, 0, 0, 0];
    assert_eq!(Reader::new(&utf16).string().unwrap(), "Hé");
    assert_eq!(strip_colors("\u{1b}\u{ff}\u{1}\u{1}Red"), "Red");
}

#[test]
fn unreal2_info_strips_colors() {
    let info = parse_info(UT2004_INFO).unwrap();
    assert_eq!(info.version, 0x80);
    assert_eq!(info.game_port, 7777);
    assert_eq!(
        info.name,
        "AU ONS/VCTF | Bellum | Friday Night Chaos Fri 8pm AEDT"
    );
    assert_eq!(info.map, "ONS-CaptainGordons_Box_of_Pwnage-RC1");
    assert_eq!(info.game_type, "ONSOnslaughtGame");
    assert_eq!((info.num_players, info.max_players), (0, 32));
    assert_eq!(info.skill, "0");

    let old = parse_info(UT2003_INFO).unwrap();
    assert_eq!(old.version, 121);
    assert_eq!(old.game_port, 7767);
    assert_eq!(old.name, "<SCaRYeiRe-Classic>");
    assert_eq!((old.num_players, old.max_players), (6, 12));
    assert!(parse_info(UT2004_RULES).is_err());
}

#[test]
fn unreal2_rules_and_players() {
    let rules = parse_rules(UT2004_RULES).unwrap();
    assert_eq!(rules[0], ("ServerMode".into(), "dedicated".into()));
    assert_eq!(rules.len(), 5);

    let players = parse_players(UT2004_PLAYERS).unwrap();
    assert_eq!(players.len(), 1);
    assert_eq!(players[0].name, "Player");
    assert_eq!(players[0].ping, Some(65));
    assert_eq!(players[0].score, 12);
}

#[test]
fn unreal2_server_from_packets() {
    let packets = [UT2004_INFO, UT2004_RULES, UT2004_PLAYERS].map(<[u8]>::to_vec);
    let server = server_from_packets("ut2004", "1.2.3.4", 7778, &packets).unwrap();
    assert!(server.online);
    assert_eq!(server.join_port, 7777);
    assert_eq!(server.game_type, "ONSOnslaughtGame");
    assert_eq!(server.rule("gametype"), Some("ONSOnslaughtGame"));
    assert_eq!(server.rule("Mutator"), Some("MutUTOptimiser, MutUTComp"));
    assert!(server.password);
    assert_eq!(server.players.len(), 1);
    assert_eq!(server.max_players, Some(32));
    assert_eq!(
        game("ut2004").unwrap().game_type_name(&server.game_type),
        "Onslaught"
    );

    assert!(server_from_packets("ut2004", "h", 1, &[UT2004_RULES.to_vec()]).is_err());
}

#[test]
fn unreal2_master_record_and_hello() {
    // A 333networks record: 81.205.81.173, game port 7747, query port 7748.
    let record = b"Q\xcdQ\xadC\x1eD\x1e\x1a\x1b\x01?? [KFMod2.52] 333netwo\x00\x11KF-AfghanistanV3\x00\x0bKFGameType\x00\x07\x18 \x00\x00\x00\x020\x00";
    assert_eq!(
        parse_master_record(record).as_deref(),
        Some("81.205.81.173:7748")
    );
    assert_eq!(parse_master_record(b"\x01\x02"), None);

    let hello = client_hello("UT2K4CLIENT", 3369);
    assert_eq!(hello[0], 33);
    assert_eq!(hello.len(), 2 * 34 + 13 + 4 + 1 + 5 + 13);
    let old = client_hello("CLIENT", 2225);
    assert_eq!(old.len(), 2 * 34 + 8 + 4 + 1 + 5);
}

#[test]
fn unreal2_join_uses_game_port() {
    let mut server = Server::new("ut2004", "5.6.7.8", 7778);
    server.join_port = 7777;
    let settings = GameSettings {
        player_name: "Me".into(),
        ..GameSettings::default()
    };
    assert_eq!(
        join_args(game("ut2004").unwrap(), &settings, &server, "pw"),
        vec!["5.6.7.8:7777?Name=Me?Password=pw"]
    );
}

// --- All-Seeing Eye ---

/// Multi Theft Auto `s` answer with rules and no players.
const MTA_REPLY: &[u8] = b"EYE1\x04mta\x0622004%\tARGENTINA ROLEPLAY #2 | DEVELOPMENT(discord.gg/argenrp - Argentina Roleplay\x13Argentina Roleplay\x041.7\x021\x020\x0330\x07author\x13Argentina Roleplay\x06forum\x1awww.argentinaroleplay.com\x08discord\x13discord.gg/argenrp\x08version\x046.1\x01";

#[test]
fn ase_reply_with_rules() {
    let reply = parse_reply(MTA_REPLY).unwrap();
    assert_eq!(reply.game, "mta");
    assert_eq!(reply.port, 22004);
    assert_eq!(reply.name, "\tARGENTINA ROLEPLAY #2 | DEVELOPMENT");
    assert_eq!(reply.game_type, "discord.gg/argenrp - Argentina Roleplay");
    assert_eq!(reply.map, "Argentina Roleplay");
    assert_eq!(reply.version, "1.7");
    assert!(reply.password);
    assert_eq!((reply.num_players, reply.max_players), (0, 30));
    assert_eq!(reply.rules.len(), 4);
    assert_eq!(reply.rules[3], ("version".into(), "6.1".into()));
    assert!(reply.players.is_empty());
    assert!(parse_reply(b"EYE2xx").is_err());
}

#[test]
fn ase_reply_with_players() {
    // Built like MTA's ASE::QueryFull: flags 0x3f, empty team/skin/time.
    let mut data =
        b"EYE1\x04mta\x0622003\x05Test\x05race\x05none\x041.6\x020\x022\x0332\x01".to_vec();
    for (name, score, ping) in [("Alice", "10", "35"), ("Bob", "-2", "120")] {
        data.push(0x3f);
        for field in [name, "", "", score, ping, ""] {
            data.push(field.len() as u8 + 1);
            data.extend_from_slice(field.as_bytes());
        }
    }
    let reply = parse_reply(&data).unwrap();
    assert!(!reply.password);
    assert_eq!(reply.players.len(), 2);
    assert_eq!(reply.players[1].name, "Bob");
    assert_eq!(reply.players[1].score, -2);
    assert_eq!(reply.players[0].ping, Some(35));

    let server = server_from_reply("mta", "1.2.3.4", 22126, reply);
    assert_eq!(server.join_port, 22003);
    assert_eq!(server.num_players, 2);
    assert_eq!(server.game_type, "race");
    assert_eq!(server.rule("version"), Some("1.6"));
}

#[test]
fn mta_master_list_version_2() {
    let mut data = Vec::new();
    data.extend_from_slice(b"\x00\x00\x00\x02\x00\x18\xf3\x2f\x00\x00\x00\xed\x00\x00\x00\x02");
    // Two captured records: 198.245.55.243:22003 and 51.81.48.91:22004.
    data.extend_from_slice(b"\x00\x44\xf3\x37\xf5\xc6\x55\xf3\x00\x00\x00\x64\x26Free Valley | Gang War, multi-language\x031.7\x01\x00\x00\x00\x00\x00\x01\x00\x00\x01\x55\xf5\x03");
    data.extend_from_slice(b"\x00\x42\x5b\x30\x51\x33\x55\xf4\x00\x00\x00\x1e\x24\x09ARGENTINA ROLEPLAY #2 | DEVELOPMENT\x031.7\x01\x00\x00\x00\x00\x00\x01\x00\x00\x01\x55\xf6\x03");
    data.extend_from_slice(b"\x12\x34\x56\x79");
    assert_eq!(
        parse_mta_list(&data).unwrap(),
        ["198.245.55.243:22003", "51.81.48.91:22004"]
    );

    let truncated = &data[..data.len() - 4];
    assert!(parse_mta_list(truncated).is_err());
}

#[test]
fn mta_master_list_version_0() {
    // u16 count, then address bytes and the query port (game port + 123).
    let data = b"\x00\x01\x0a\x00\x00\x01\x56\x6e";
    assert_eq!(parse_mta_list(data).unwrap(), ["10.0.0.1:22003"]);
}

#[test]
fn new_games_use_their_protocols() {
    let ut2004 = game("ut2004").unwrap();
    assert_eq!(ut2004.protocol, Protocol::Unreal2);
    assert!(matches!(
        &ut2004.masters[0],
        Master::Unreal2 { version: 3369, .. }
    ));
    assert_eq!(
        game("ut2003").unwrap().game_type_name("xBombingRun"),
        "Bombing Run"
    );
    let mta = game("mta").unwrap();
    assert_eq!(mta.protocol, Protocol::Ase);
    assert_eq!(mta.query_port_offset, 123);
    assert_eq!(mta.default_port, 22003);
    assert!(matches!(&mta.masters[0], Master::Mta { .. }));
    assert_eq!(game("q3").unwrap().query_port_offset, 0);

    let server = Server::new("mta", "1.2.3.4", 22003);
    assert_eq!(
        join_args(mta, &GameSettings::default(), &server, ""),
        vec!["mtasa://1.2.3.4:22003"]
    );
}
