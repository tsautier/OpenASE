//! Server lists from the Steam Web API.
//!
//! Valve's old UDP master (`hl2master.steampowered.com:27011`) no longer
//! answers, so lists come from `IGameServersService/GetServerList`, which needs
//! a free Steam Web API key from <https://steamcommunity.com/dev/apikey>.

use std::io;
use std::time::Duration;

use serde::Deserialize;

const ENDPOINT: &str = "https://api.steampowered.com/IGameServersService/GetServerList/v1/";

#[derive(Deserialize)]
struct Envelope {
    response: ServerListResponse,
}

#[derive(Deserialize, Default)]
struct ServerListResponse {
    #[serde(default)]
    servers: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    addr: String,
}

/// Parse a `GetServerList` JSON answer into `ip:port` query addresses.
pub fn parse_server_list(json: &str) -> io::Result<Vec<String>> {
    let envelope: Envelope = serde_json::from_str(json).map_err(io::Error::other)?;
    Ok(envelope
        .response
        .servers
        .into_iter()
        .map(|entry| entry.addr)
        .collect())
}

/// Fetch up to `limit` servers matching a master filter such as `\appid\10`.
pub fn server_list(
    api_key: &str,
    filter: &str,
    limit: usize,
    timeout: Duration,
) -> io::Result<Vec<String>> {
    if api_key.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "a Steam Web API key is needed for this game's server list (Options > General)",
        ));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let body = agent
        .get(ENDPOINT)
        .query("key", api_key.trim())
        .query("filter", filter)
        .query("limit", limit.to_string())
        .call()
        .map_err(io::Error::other)?
        .body_mut()
        .read_to_string()
        .map_err(io::Error::other)?;
    parse_server_list(&body)
}
