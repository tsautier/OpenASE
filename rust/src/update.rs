//! "Update Eye": check GitHub Releases for a newer OpenASE.

use std::io;
use std::time::Duration;

use serde::Deserialize;

pub const RELEASES_API: &str = "https://api.github.com/repos/tsautier/OpenASE/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/tsautier/OpenASE/releases/latest";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

/// Parse `v1.2.3` / `1.2.3` into numbers.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .split('-')
        .next()?
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

/// True when `latest` is a newer version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(a), Some(b)) if a > b)
}

/// Return the latest release tag when it is newer than this build.
pub fn check_latest(timeout: Duration) -> io::Result<Option<String>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let body = agent
        .get(RELEASES_API)
        .header("User-Agent", concat!("OpenASE/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(io::Error::other)?
        .body_mut()
        .read_to_string()
        .map_err(io::Error::other)?;
    let release: Release = serde_json::from_str(&body).map_err(io::Error::other)?;
    Ok(is_newer(&release.tag_name, env!("CARGO_PKG_VERSION")).then_some(release.tag_name))
}
