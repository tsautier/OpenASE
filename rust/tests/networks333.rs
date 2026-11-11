//! 333networks JSON API: list previews, paging and the "listed, not queried"
//! server state. No network access.

use std::time::{Duration, Instant};

use openase::catalog;
use openase::model::{Listed, Server};
use openase::networks333::{
    API, CACHE_TTL, PAGE_SIZE, Page, PageCache, decode_entities, page_url, parse_page,
};
use openase::query::{has_paged_master, merge_listed};

/// A real answer to `/json/ut?r=5&s=numplayers&o=d`.
const SAMPLE: &str = r#"[[{"ip":"82.39.133.159","mapname":"CTF-ChainsGauntlet","gamename":"ut","dt_added":1778185838,"maptitle":"The Iron Gauntlet","id":442462,"hostport":7777,"gametype":"CTFGame","label":"Unreal Tournament","numplayers":23,"queryport":7778,"hostname":"zp| ComboGib Classic ~~~&gt; Grapple","maxplayers":32,"dt_updated":1794423193,"country":"GB"},{"numplayers":17,"country":"DE","dt_updated":1794423177,"hostname":".de LuckyDawgs - Tactical-Ops.eu - TOST45 - Public #1","maxplayers":32,"queryport":7788,"dt_added":1768004389,"mapname":"TO-RapidWaters_Reborn3","ip":"194.164.194.68","gamename":"ut","label":"Unreal Tournament","gametype":"TO340","hostport":7787,"id":321566,"maptitle":"RapidWaters Reborn"},{"queryport":7778,"hostname":"--- ComboGib CTF (Grapple) -- London --- EatSleepUT.com","maxplayers":18,"dt_updated":1794423172,"country":"GB","numplayers":14,"maptitle":"CTF-Almost-LE","id":133,"gametype":"CTFGame","hostport":7777,"label":"Unreal Tournament","gamename":"ut","ip":"139.162.235.20","mapname":"CTF-Almost-LE","dt_added":1738521882},{"numplayers":10,"hostname":"PG Ranked - UK Server #1","maxplayers":10,"queryport":7778,"country":"GB","dt_updated":1794423169,"mapname":"CTF-Anfractuous-PG1","ip":"3.8.205.108","gamename":"ut","dt_added":1794418577,"id":611808,"maptitle":"Scourgem's Anfractuous Rebuilt by NuLL","label":"Unreal Tournament","gametype":"NewCTF","hostport":7777},{"label":"Unreal Tournament","hostport":7777,"gametype":"NewCTF","id":611825,"maptitle":"Overflow","dt_added":1794419766,"gamename":"ut","ip":"18.195.36.63","mapname":"CTF-Overflow-PG1","country":"DE","dt_updated":1794423175,"maxplayers":10,"hostname":"PG Ranked - DE Server #1","queryport":7778,"numplayers":10}],{"players":74,"total":854}]"#;

fn previews(page: &Page) -> Vec<&Server> {
    page.servers
        .iter()
        .map(|entry| {
            entry
                .preview
                .as_ref()
                .expect("333networks entries have details")
        })
        .collect()
}

#[test]
fn sample_page_gives_listed_servers_with_details() {
    let page = parse_page(SAMPLE.as_bytes()).unwrap();
    assert_eq!(page.total, 854);
    assert_eq!(page.servers.len(), 5);
    let first = previews(&page)[0];
    assert_eq!(page.servers[0].address, "82.39.133.159:7778");
    assert_eq!(first.address(), "82.39.133.159:7778");
    assert_eq!(first.join_address(), "82.39.133.159:7777");
    assert_eq!(first.name, "zp| ComboGib Classic ~~~> Grapple");
    assert_eq!(first.map, "CTF-ChainsGauntlet");
    assert_eq!(first.game_type, "CTFGame");
    assert_eq!(first.num_players, 23);
    assert_eq!(first.max_players, Some(32));
    assert_eq!(first.country.as_deref(), Some("GB"));
    assert_eq!(first.rule("maptitle"), Some("The Iron Gauntlet"));
    assert_eq!(first.rule("gametype"), Some("CTFGame"));
    // Listed, not queried: no ping, flux or loss, and not "offline".
    assert!(first.listed && !first.online && first.has_info());
    assert_eq!(
        (first.ping, first.flux, first.packet_loss),
        (None, None, None)
    );
    assert!(first.error.is_none());
    let players: usize = previews(&page).iter().map(|s| s.num_players).sum();
    assert_eq!(players, 74);
}

#[test]
fn missing_fields_get_defaults_and_bad_entries_are_skipped() {
    let page = parse_page(
        br#"[[{"ip":"10.0.0.1","queryport":7778},
            {"ip":"::ffff:10.0.0.2","queryport":"7788","hostport":"7787","numplayers":"3",
             "maxplayers":"16","country":"de"},
            {"ip":"10.0.0.3","queryport":7778,"country":"??","hostname":null},
            {"ip":"10.0.0.4","hostport":7777},
            {"ip":"","queryport":7778},
            {"ip":"10.0.0.5","queryport":0},
            {"ip":"10.0.0.6","queryport":70000}],{"players":3}]"#,
    )
    .unwrap();
    let servers = previews(&page);
    assert_eq!(servers.len(), 3);
    // No total: the entries of this page.
    assert_eq!(page.total, 7);

    let bare = servers[0];
    assert_eq!(bare.address(), "10.0.0.1:7778");
    assert_eq!(bare.join_port, 7777, "game port defaults to query port - 1");
    assert_eq!(bare.name, "");
    assert_eq!(bare.num_players, 0);
    assert_eq!(bare.max_players, None);
    assert_eq!(bare.country, None);
    assert!(bare.listed);

    let strings = servers[1];
    assert_eq!(strings.address(), "10.0.0.2:7788");
    assert_eq!(strings.join_port, 7787);
    assert_eq!((strings.num_players, strings.max_players), (3, Some(16)));
    assert_eq!(strings.country.as_deref(), Some("DE"));

    assert_eq!(servers[2].country, None);
    assert_eq!(servers[2].name, "");
}

#[test]
fn api_errors_and_other_documents_are_errors() {
    assert!(parse_page(br#"{"error":1,"in":"scrape_detected"}"#).is_err());
    assert!(parse_page(br#"{"players":0}"#).is_err());
    assert!(parse_page(b"<html>").is_err());
}

#[test]
fn html_entities_are_decoded() {
    assert_eq!(decode_entities("a &gt; b &lt; c"), "a > b < c");
    assert_eq!(decode_entities("Tom &amp; Jerry"), "Tom & Jerry");
    assert_eq!(decode_entities("&quot;x&quot; &#39;y&apos;"), "\"x\" 'y'");
    assert_eq!(decode_entities("&#65;&#x42;&#X43;"), "ABC");
    assert_eq!(decode_entities("&amp;gt;"), "&gt;", "decoded once only");
    assert_eq!(
        decode_entities("R&D &unknown; & &#0; &"),
        "R&D &unknown; & &#0; &"
    );
    assert_eq!(decode_entities("&gt"), "&gt");
    assert_eq!(decode_entities("Ünïcode &eacute;"), "Ünïcode &eacute;");
}

#[test]
fn ue2_color_codes_and_entities_in_names() {
    let page = parse_page(
        "[[{\"ip\":\"1.2.3.4\",\"queryport\":7778,\"hostname\":\"\\u001b\\u00ff\\u0001\\u0001Red &amp; \\u001b\\u0001\\u0001\\u00ffBlue\"}],{\"total\":1}]"
            .as_bytes(),
    )
    .unwrap();
    assert_eq!(previews(&page)[0].name, "Red & Blue");
}

#[test]
fn page_urls_ask_for_one_sorted_page() {
    assert_eq!(PAGE_SIZE, 100);
    assert_eq!(
        page_url("", "ut", 1),
        "https://master.333networks.com/json/ut?r=100&p=1&s=numplayers&o=d"
    );
    assert_eq!(page_url(API, "ut", 1), page_url("", "ut", 1));
    assert_eq!(page_url("", "ut", 0), page_url("", "ut", 1));
    assert_eq!(
        page_url("https://ut2004master.333networks.com/json/", "ut2004", 3),
        "https://ut2004master.333networks.com/json/ut2004?r=100&p=3&s=numplayers&o=d"
    );
    assert_ne!(page_url("", "ut", 2), page_url("", "ut", 1));
    assert_ne!(page_url("", "unreal", 1), page_url("", "ut", 1));
}

#[test]
fn pages_are_cached_for_seven_and_a_half_minutes_by_url() {
    assert_eq!(CACHE_TTL, Duration::from_secs(450));
    let start = Instant::now();
    let url = page_url("", "ut", 1);
    let mut cache = PageCache::default();
    assert!(cache.get(&url, start).is_none());
    cache.insert(&url, start, parse_page(SAMPLE.as_bytes()).unwrap());

    let hit = cache.get(&url, start + Duration::from_secs(449)).unwrap();
    assert_eq!((hit.servers.len(), hit.total), (5, 854));
    assert!(cache.get(&url, start + CACHE_TTL).is_none(), "expired");
    assert!(cache.get(&page_url("", "ut", 2), start).is_none());
    assert!(cache.get(&page_url("", "unreal", 1), start).is_none());

    // A later insert forgets expired pages and refreshes the time.
    let later = start + Duration::from_secs(600);
    cache.insert(&url, later, Page::default());
    assert_eq!(cache.get(&url, later).unwrap().servers.len(), 0);
}

#[test]
fn listed_offline_and_unscanned_states_differ() {
    let unscanned = Server::new("ut99", "1.2.3.4", 7778);
    assert!(!unscanned.online && !unscanned.listed && !unscanned.has_info());
    assert!(unscanned.error.is_none());

    let offline = Server::offline("ut99", "1.2.3.4", 7778, "timed out");
    assert!(!offline.online && !offline.listed && !offline.has_info());
    assert!(offline.error.is_some());
    assert_eq!(offline.packet_loss, Some(100));

    let page = parse_page(SAMPLE.as_bytes()).unwrap();
    let listed = previews(&page)[0];
    assert!(!listed.online && listed.listed && listed.has_info());
    assert!(listed.error.is_none() && listed.packet_loss.is_none());
}

#[test]
fn merged_lists_prefer_details_and_set_the_game() {
    let game = catalog::game("ut2004").unwrap();
    let page = parse_page(SAMPLE.as_bytes()).unwrap();
    let mut entries = vec![
        Listed::address("82.39.133.159:7778".to_string()),
        Listed::address("9.9.9.9:7778".to_string()),
    ];
    entries.extend(page.servers);
    entries.push(Listed::address("9.9.9.9:7778".to_string()));
    let merged = merge_listed(game, entries);
    assert_eq!(merged.len(), 6);
    // Details first: the address the other master also gave is not queried.
    let with_details: Vec<&Listed> = merged.iter().filter(|e| e.preview.is_some()).collect();
    assert_eq!(with_details.len(), 5);
    assert!(merged[..5].iter().all(|entry| entry.preview.is_some()));
    assert!(
        with_details
            .iter()
            .all(|entry| entry.preview.as_ref().unwrap().game == "ut2004")
    );
    assert_eq!(merged[5].address, "9.9.9.9:7778");
    assert!(merged[5].preview.is_none());
}

#[test]
fn paged_masters_are_the_333networks_ones() {
    for id in ["ut99", "ut2004", "unreal", "sof"] {
        assert!(has_paged_master(catalog::game(id).unwrap()), "{id}");
    }
    assert!(!has_paged_master(catalog::game("q3").unwrap()));
}

#[test]
fn no_catalog_master_uses_sync() {
    // `\sync\` is for master-to-master use only. The field is gone from the
    // catalog type; make sure games.json does not carry it either (serde
    // would silently ignore it).
    let games: serde_json::Value = serde_json::from_str(include_str!("../src/games.json")).unwrap();
    for game in games.as_array().unwrap() {
        for master in game["masters"].as_array().unwrap() {
            assert!(master.get("sync").is_none(), "{}", game["id"]);
        }
    }
    assert!(!include_str!("../src/gamespy.rs").contains("\\\\sync\\\\"));
}
