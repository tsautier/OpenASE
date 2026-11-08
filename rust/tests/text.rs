//! Decoding of server text sent as UTF-8 or Latin-1.

use openase::gamespy::parse_kv;
use openase::quake3::{parse_info_string, parse_player_line, parse_status_response};
use openase::text::{decode, repair};

#[test]
fn decode_prefers_utf8_and_falls_back_to_latin1() {
    assert_eq!(decode("Café".as_bytes()), "Café");
    assert_eq!(decode(b"Caf\xe9"), "Café");
    assert_eq!(decode(b"plain"), "plain");
}

#[test]
fn repair_fixes_utf8_read_as_latin1() {
    assert_eq!(repair("CafÃ©"), "Café");
    let moscow_as_latin1: String = "Москва".bytes().map(char::from).collect();
    assert_eq!(repair(&moscow_as_latin1), "Москва");
    assert_eq!(repair("Café"), "Café", "true Latin-1 stays");
    assert_eq!(repair("Москва"), "Москва", "already decoded stays");
    assert_eq!(repair("^1Red ^7Name"), "^1Red ^7Name");
    // Windows-1251 Russian names, but not a Latin-1 name with accents.
    assert_eq!(repair("***Ðóññêèé Ñåðâåð***"), "***Русский Сервер***");
    assert_eq!(repair("Serveur Français Ñandú"), "Serveur Français Ñandú");
    // Quake font glyphs (high bytes that are not UTF-8) stay as they were.
    assert_eq!(repair("\u{90}Name\u{91}"), "\u{90}Name\u{91}");
}

#[test]
fn quake3_info_and_players_are_repaired() {
    let mut packet = b"\xff\xff\xff\xffstatusResponse\n\\sv_hostname\\".to_vec();
    packet.extend_from_slice("Serveur Français".as_bytes());
    packet.extend_from_slice(b"\\mapname\\q3dm17\n5 40 \"");
    packet.extend_from_slice("Jérôme".as_bytes());
    packet.extend_from_slice(b"\"\n3 50 \"Ren\xe9\"\n");
    let (info, players) = parse_status_response(&packet).unwrap();
    assert_eq!(info["sv_hostname"], "Serveur Français");
    assert_eq!(players[0].name, "Jérôme");
    assert_eq!(players[1].name, "René", "Latin-1 name in the same packet");
}

#[test]
fn quake3_helpers_repair_fields() {
    let info = parse_info_string(r"\hostname\CafÃ©").unwrap();
    assert_eq!(info["hostname"], "Café");
    assert_eq!(
        parse_player_line("1 2 \"JÃ©rÃ´me\"").unwrap().name,
        "Jérôme"
    );
}

#[test]
fn gamespy_fields_are_repaired() {
    let pairs = parse_kv("\\hostname\\CafÃ©\\mapname\\Caf\u{e9}");
    assert_eq!(pairs[0].1, "Café");
    assert_eq!(pairs[1].1, "Café");
}
