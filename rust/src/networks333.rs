//! Server lists from the 333networks JSON API (<https://333networks.com/json>).
//!
//! 333networks asks external server browsers to take their lists from this
//! API (served by its web server) rather than from the master on TCP 28900,
//! which stays free for game clients. Terms of use: credit 333networks in the
//! application (Help > About), and do not repeat the same request more often
//! than the 7.5 minutes it takes them to refresh server information.
//!
//! The list already holds each server's name, map, game type, player counts
//! and country, refreshed by 333networks every 7.5 minutes. OpenASE shows
//! those details as they are and does not query the listed servers itself,
//! except one at a time on demand (selected, refreshed, joined, favorite).
//! It also loads the list one page of [`PAGE_SIZE`] servers at a time, the
//! busiest servers first, and asks for the next page only on a user action.
//!
//! Request: `https://master.333networks.com/json/<gamename>?r=100&p=<page>&s=numplayers&o=d`
//! (`r` entries per page, at most 1000; `p` page from 1; `s` sort field;
//! `o` order, `d` descending).
//! Answer: `[[{entry}, ...], {"players": n, "total": n}]`, entries with the
//! fields id ip hostport queryport hostname gamename label country numplayers
//! maxplayers maptitle mapname gametype dt_added dt_updated.
//! Errors: `{"error": 1, "in": "..."}`.

use std::collections::HashMap;
use std::io;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::model::{Listed, Server};

/// Base URL of the main serverlist API. UT2004 has its own:
/// `https://ut2004master.333networks.com/json/`.
pub const API: &str = "https://master.333networks.com/json/";

/// Entries per page.
pub const PAGE_SIZE: usize = 100;

/// 333networks refreshes server information every 7.5 minutes and blocks
/// clients that repeat a request faster than that.
pub const CACHE_TTL: Duration = Duration::from_secs(450);

/// One page of a serverlist.
#[derive(Debug, Clone, Default)]
pub struct Page {
    /// Listed servers, each with its details as a preview.
    pub servers: Vec<Listed>,
    /// Number of servers 333networks lists for the game, all pages together.
    pub total: usize,
}

/// URL of page `page` (from 1) of the list of `gamename`, busiest servers
/// first, from the API at `api` ([`API`] when empty). It is also the cache key.
pub fn page_url(api: &str, gamename: &str, page: usize) -> String {
    let api = if api.is_empty() { API } else { api };
    format!(
        "{api}{gamename}?r={PAGE_SIZE}&p={}&s=numplayers&o=d",
        page.max(1)
    )
}

/// Answers kept for [`CACHE_TTL`], by request URL.
#[derive(Debug, Default)]
pub struct PageCache {
    pages: HashMap<String, (Instant, Page)>,
}

impl PageCache {
    /// The page fetched from `url` less than [`CACHE_TTL`] before `now`.
    pub fn get(&self, url: &str, now: Instant) -> Option<Page> {
        self.pages
            .get(url)
            .filter(|(fetched, _)| now.saturating_duration_since(*fetched) < CACHE_TTL)
            .map(|(_, page)| page.clone())
    }

    /// Remember the page fetched from `url` at `fetched`, and forget expired ones.
    pub fn insert(&mut self, url: &str, fetched: Instant, page: Page) {
        self.pages
            .retain(|_, (at, _)| fetched.saturating_duration_since(*at) < CACHE_TTL);
        self.pages.insert(url.to_string(), (fetched, page));
    }
}

static CACHE: LazyLock<Mutex<PageCache>> = LazyLock::new(|| Mutex::new(PageCache::default()));

/// Held during a request, so OpenASE never has two requests to 333networks
/// in flight, and a second identical request waits and gets the cached page.
static REQUEST: Mutex<()> = Mutex::new(());

/// Fetch page `page` (from 1) of the list of `gamename` from the API at `api`
/// ([`API`] when empty). A page fetched less than [`CACHE_TTL`] ago is served
/// from memory without a request.
pub fn fetch_page(api: &str, gamename: &str, page: usize, timeout: Duration) -> io::Result<Page> {
    let url = page_url(api, gamename, page);
    let _request = REQUEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(page) = CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(&url, Instant::now()))
    {
        return Ok(page);
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let data = agent
        .get(&url)
        .header(
            "User-Agent",
            concat!(
                "OpenASE/",
                env!("CARGO_PKG_VERSION"),
                " (+https://openase.org)"
            ),
        )
        .call()
        .map_err(io::Error::other)?
        .body_mut()
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_to_vec()
        .map_err(io::Error::other)?;
    let parsed = parse_page(&data)?;
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(&url, Instant::now(), parsed.clone());
    }
    Ok(parsed)
}

/// Parse one serverlist page: the listed servers with their details, and
/// the total entry count.
pub fn parse_page(data: &[u8]) -> io::Result<Page> {
    let value: Value = serde_json::from_slice(data).map_err(io::Error::other)?;
    if value.get("error").and_then(Value::as_i64) == Some(1) {
        let place = value
            .get("in")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(io::Error::other(format!("333networks API error: {place}")));
    }
    let entries = value.get(0).and_then(Value::as_array).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "not a 333networks serverlist")
    })?;
    let total = value
        .get(1)
        .and_then(|meta| meta.get("total"))
        .and_then(Value::as_u64)
        .map_or(entries.len(), |total| total as usize);
    let servers = entries
        .iter()
        .filter_map(preview)
        .map(|server| Listed {
            address: server.address(),
            preview: Some(server),
        })
        .collect();
    Ok(Page { servers, total })
}

/// A listed (not queried) server from one list entry; `None` without a
/// usable `ip` and `queryport`. The game id is left empty for the caller.
pub fn preview(entry: &Value) -> Option<Server> {
    let ip = entry.get("ip")?.as_str()?.trim();
    let ip = ip.strip_prefix("::ffff:").unwrap_or(ip);
    let port = number(entry, "queryport")
        .and_then(|port| u16::try_from(port).ok())
        .filter(|&port| port != 0)?;
    if ip.is_empty() {
        return None;
    }
    let text = |key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .map(|value| crate::unreal2::strip_colors(&decode_entities(value)))
            .unwrap_or_default()
    };
    let mut server = Server::new("", ip, port);
    server.listed = true;
    server.name = text("hostname");
    server.map = text("mapname");
    server.game_type = text("gametype");
    server.num_players = number(entry, "numplayers").unwrap_or(0) as usize;
    server.max_players = number(entry, "maxplayers").map(|max| max as usize);
    server.join_port = number(entry, "hostport")
        .and_then(|port| u16::try_from(port).ok())
        .filter(|&port| port != 0)
        .unwrap_or(port.saturating_sub(1));
    let country = text("country").to_ascii_uppercase();
    if country.len() == 2 && country.chars().all(|c| c.is_ascii_alphabetic()) {
        server.country = Some(country);
    }
    // Rule values used by filters and shown in the rules pane.
    for (key, value) in [
        ("hostname", &server.name),
        ("mapname", &server.map),
        ("gametype", &server.game_type),
    ] {
        if !value.is_empty() {
            server.rules.insert(key.to_string(), value.clone());
        }
    }
    let title = text("maptitle");
    if !title.is_empty() {
        server.rules.insert("maptitle".to_string(), title);
    }
    server
        .rules
        .insert("hostport".to_string(), server.join_port.to_string());
    Some(server)
}

/// Unsigned number field, sent as a JSON number or a numeric string.
fn number(entry: &Value, key: &str) -> Option<u64> {
    match entry.get(key)? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// Decode the HTML character references 333networks puts in text fields
/// (`&gt;`, `&amp;`, `&#39;`, `&#x41;`...). Unknown references stay as they are.
pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let decoded = tail
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| Some((entity(&tail[1..end])?, end)));
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(code) = name.strip_prefix('#') {
        let value = match code.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => code.parse().ok()?,
        };
        return char::from_u32(value).filter(|&c| c != '\0');
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => return None,
    })
}
