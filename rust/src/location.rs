//! Location helpers for server rows.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::ase_filters::{find_in_ancestors, read_latin1};

pub const UNKNOWN_LOCATION: &str = "Unknown";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Country {
    pub code: String,
    pub name: String,
}

/// Load ASE country code labels from `countries.txt` content.
pub fn load_countries_str(content: &str) -> Vec<Country> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let split = line
                .char_indices()
                .nth(2)
                .map_or(line.len(), |(index, _)| index);
            let (code, name) = line.split_at(split);
            let (code, name) = (code.trim(), name.trim());
            (!code.is_empty() && !name.is_empty()).then(|| Country {
                code: code.to_string(),
                name: name.to_string(),
            })
        })
        .collect()
}

/// Load ASE country code labels from a `countries.txt` file.
pub fn load_countries(path: &Path) -> io::Result<Vec<Country>> {
    Ok(load_countries_str(&read_latin1(path)?))
}

/// Find the nearest original `countries.txt` around the OpenASE folder.
pub fn find_countries_file(start: &Path) -> Option<PathBuf> {
    find_in_ancestors(start, "countries.txt")
}

/// Return a reported server location without guessing from IP addresses.
pub fn server_location(info: &BTreeMap<String, String>) -> String {
    [
        "location",
        "country",
        "countryCode",
        "ServerLocation",
        ".location",
        ".server-location",
        ".Location",
        "Location",
    ]
    .iter()
    .filter_map(|key| info.get(*key))
    .find(|value| !value.is_empty())
    .cloned()
    .unwrap_or_else(|| UNKNOWN_LOCATION.to_string())
}
