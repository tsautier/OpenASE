//! Server lists from the 333networks JSON API (<https://333networks.com/json>).
//!
//! 333networks asks external server browsers to take their lists from this
//! API (served by its web server) rather than from the master on TCP 28900,
//! which stays free for game clients. Terms of use: credit 333networks in the
//! application (Help > About), and do not repeat the same request more often
//! than the 7.5 minutes it takes them to refresh server information.
//!
//! Request: `https://master.333networks.com/json/<gamename>?r=1000&p=<page>`.
//! Answer: `[[{"ip": ..., "queryport": ..., "hostport": ..., ...}], {"total": n, ...}]`.

use std::collections::HashMap;
use std::io;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Base URL of the main serverlist API. UT2004 has its own:
/// `https://ut2004master.333networks.com/json/`.
pub const API: &str = "https://master.333networks.com/json/";

/// Entries per page; the API allows at most 1000.
const PAGE_SIZE: usize = 1000;

/// 333networks refreshes server information every 7.5 minutes and blocks
/// clients that repeat a request faster than that.
const CACHE_TTL: Duration = Duration::from_secs(450);

type Cache = HashMap<String, (Instant, Vec<String>)>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Query addresses (`ip:queryport`) of every listed server of `gamename`,
/// from the API at `api` ([`API`] when empty).
pub fn server_list(api: &str, gamename: &str, timeout: Duration) -> io::Result<Vec<String>> {
    let api = if api.is_empty() { API } else { api };
    let key = format!("{api}{gamename}");
    if let Some((fetched, list)) = CACHE.lock().ok().and_then(|cache| cache.get(&key).cloned())
        && fetched.elapsed() < CACHE_TTL
    {
        return Ok(list);
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let mut list = Vec::new();
    let mut page = 1;
    loop {
        let url = format!("{api}{gamename}?r={PAGE_SIZE}&p={page}");
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
        let (addresses, total) = parse_page(&data)?;
        let got = addresses.len();
        list.extend(addresses);
        if got < PAGE_SIZE || list.len() >= total || page >= 20 {
            break;
        }
        page += 1;
    }
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, (Instant::now(), list.clone()));
    }
    Ok(list)
}

/// Parse one serverlist page: the query addresses and the total entry count.
pub fn parse_page(data: &[u8]) -> io::Result<(Vec<String>, usize)> {
    let value: serde_json::Value = serde_json::from_slice(data).map_err(io::Error::other)?;
    if value.get("error").and_then(serde_json::Value::as_i64) == Some(1) {
        let place = value
            .get("in")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(io::Error::other(format!("333networks API error: {place}")));
    }
    let entries = value
        .get(0)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "not a 333networks serverlist")
        })?;
    let total = value
        .get(1)
        .and_then(|meta| meta.get("total"))
        .and_then(serde_json::Value::as_u64)
        .map_or(entries.len(), |total| total as usize);
    let addresses = entries
        .iter()
        .filter_map(|entry| {
            let ip = entry.get("ip")?.as_str()?;
            let ip = ip.strip_prefix("::ffff:").unwrap_or(ip);
            let port = entry.get("queryport")?.as_u64()?;
            (!ip.is_empty() && (1..=65535).contains(&port)).then(|| format!("{ip}:{port}"))
        })
        .collect();
    Ok((addresses, total))
}
