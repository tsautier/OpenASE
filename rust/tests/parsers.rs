//! Offline parser tests.

use openase::ase_filters::{parse_filter_paths, parse_game_definitions};
use openase::colors::{first_quake_color, quake_color_segments, strip_quake_colors};
use openase::location::{UNKNOWN_LOCATION, load_countries, server_location};
use openase::quake3::{
    DEFAULT_PORT, OUT_OF_BAND_PREFIX, parse_address, parse_getservers_response,
    parse_info_response, parse_info_string, parse_status_response,
};
use std::collections::BTreeMap;

fn packet(parts: &[&[u8]]) -> Vec<u8> {
    let mut data = OUT_OF_BAND_PREFIX.to_vec();
    for part in parts {
        data.extend_from_slice(part);
    }
    data
}

fn write_temp(dir: &tempfile::TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

// --- quake3 ---

#[test]
fn parse_address_defaults_to_quake3_port() {
    assert_eq!(
        parse_address("example.org", DEFAULT_PORT).unwrap(),
        ("example.org".into(), DEFAULT_PORT)
    );
}

#[test]
fn parse_address_accepts_explicit_port() {
    assert_eq!(
        parse_address("example.org:27961", DEFAULT_PORT).unwrap(),
        ("example.org".into(), 27961)
    );
}

#[test]
fn parse_info_string_reads_pairs() {
    let info = parse_info_string(r"\sv_hostname\Test Server\mapname\q3dm17\clients\2").unwrap();
    assert_eq!(info["sv_hostname"], "Test Server");
    assert_eq!(info["mapname"], "q3dm17");
    assert_eq!(info["clients"], "2");
}

#[test]
fn parse_info_response_reads_body() {
    let data = packet(&[b"infoResponse\n\\hostname\\Q3\\protocol\\68"]);
    assert_eq!(parse_info_response(&data).unwrap()["protocol"], "68");
}

#[test]
fn parse_status_response_reads_info_and_players() {
    let data = packet(&[
        b"statusResponse\n",
        br"\sv_hostname\Test\sv_maxclients\16\mapname\q3dm6",
        b"\n12 45 \"Player One\"\n-3 0 \"Bot\"\n",
    ]);

    let (info, players) = parse_status_response(&data).unwrap();

    assert_eq!(info["sv_hostname"], "Test");
    assert_eq!(info["mapname"], "q3dm6");
    assert_eq!(players.len(), 2);
    assert_eq!(players[0].score, 12);
    assert_eq!(players[0].ping, 45);
    assert_eq!(players[0].name, "Player One");
    assert_eq!(players[1].score, -3);
}

#[test]
fn parse_getservers_response_reads_addresses() {
    let data = packet(&[
        b"getserversResponse",
        b"\\",
        &[127, 0, 0, 1],
        &27960u16.to_be_bytes(),
        b"\\EOT\0\0\0",
    ]);
    assert_eq!(
        parse_getservers_response(&data).unwrap(),
        vec!["127.0.0.1:27960".to_string()]
    );
}

// --- filters ---

#[test]
fn parse_filter_paths_keeps_tree_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_temp(
        &dir,
        "filters.txt",
        "// comment\nQuake 3 Arena\nHalf-Life 2: Source Mods\nServerType: Q3\n1 if servertype != \"Q3\" remove\nQuake 3 Arena\\baseq3\nMutex: Q3",
    );

    assert_eq!(
        parse_filter_paths(&path).unwrap(),
        vec![
            "Quake 3 Arena",
            "Half-Life 2: Source Mods",
            "Quake 3 Arena\\baseq3"
        ]
    );
}

#[test]
fn parse_game_definitions_excludes_modifiers() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_temp(
        &dir,
        "filters.txt",
        "Quake 3 Arena\nServerType: Q3\nExeName: quake3.exe\nQuake 3 Arena\\baseq3\nModifiers\\Ping",
    );

    let games = parse_game_definitions(&path).unwrap();

    assert!(games.contains_key("Quake 3 Arena"));
    assert!(!games.contains_key("Modifiers"));
    assert_eq!(games["Quake 3 Arena"].metadata["ExeName"], "quake3.exe");
}

// --- colors ---

#[test]
fn strip_quake_colors_removes_markers() {
    assert_eq!(strip_quake_colors("^1Red^7 Name"), "Red Name");
    assert_eq!(strip_quake_colors("^11"), "1");
    assert_eq!(strip_quake_colors("^xff0000Red ^NReset"), "Red Reset");
    assert_eq!(strip_quake_colors("^Bblink^b"), "blink");
}

#[test]
fn first_quake_color_finds_first_marker() {
    assert_eq!(first_quake_color("^4Blue").as_deref(), Some("#0000ff"));
    assert_eq!(first_quake_color("^xff0000Red").as_deref(), Some("#ff0000"));
    assert_eq!(first_quake_color("^B^F^2Green").as_deref(), Some("#00ff00"));
}

#[test]
fn quake_color_segments_split_by_color() {
    let segments: Vec<_> = quake_color_segments("^1Red ^2Green ^xff0000Hex", "#202020")
        .into_iter()
        .map(|segment| (segment.text, segment.color))
        .collect();

    assert_eq!(
        segments,
        vec![
            ("Red ".to_string(), "#ff0000".to_string()),
            ("Green ".to_string(), "#00ff00".to_string()),
            ("Hex".to_string(), "#ff0000".to_string()),
        ]
    );
}

#[test]
fn quake_color_segments_keeps_literal_after_color_code() {
    let segments = quake_color_segments("^11", "#202020");
    assert_eq!(segments.len(), 1);
    assert_eq!(
        (segments[0].text.as_str(), segments[0].color.as_str()),
        ("1", "#ff0000")
    );
}

// --- location ---

#[test]
fn load_countries_reads_code_and_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_temp(
        &dir,
        "countries.txt",
        "FR        France\nDE        Germany\n",
    );

    let countries = load_countries(&path).unwrap();

    assert_eq!(countries[0].code, "FR");
    assert_eq!(countries[0].name, "France");
}

#[test]
fn server_location_does_not_guess() {
    assert_eq!(server_location(&BTreeMap::new()), UNKNOWN_LOCATION);
    let info = BTreeMap::from([("country".to_string(), "France".to_string())]);
    assert_eq!(server_location(&info), "France");
}

#[test]
fn parse_getservers_response_accepts_unpadded_eot() {
    // id Software's master ends each packet with a bare `\EOT`.
    let data = packet(&[
        b"getserversResponse\\",
        &[10, 0, 0, 2],
        &27961u16.to_be_bytes(),
        br"\EOT",
    ]);
    assert_eq!(
        parse_getservers_response(&data).unwrap(),
        vec!["10.0.0.2:27961".to_string()]
    );
}

#[test]
fn gamespy_sync_answer_lists_every_game() {
    let lists = openase::gamespy::parse_sync_answer(
        br"\ut\1.2.3.4:7778 5.6.7.8:7778 \unreal\9.9.9.9:7778\deusex\\final\",
    );
    assert_eq!(lists["ut"], vec!["1.2.3.4:7778", "5.6.7.8:7778"]);
    assert_eq!(lists["unreal"], vec!["9.9.9.9:7778"]);
    assert!(lists["deusex"].is_empty());
    assert!(!lists.contains_key("final"));
}
