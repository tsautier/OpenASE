//! Parser for the original ASE `filters.txt` navigation tree.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::quake3::latin1;

pub const METADATA_PREFIXES: [&str; 17] = [
    "ServerType:",
    "RegKey:",
    "ExeName:",
    "JoinTemplate:",
    "HostTemplate:",
    "GUID:",
    "VersionKey:",
    "Port:",
    "HostPort:",
    "Mutex:",
    "Flags:",
    "Group:",
    "Icon:",
    "Maps:",
    "Remote:",
    "Launch:",
    "JoinMsg:",
];
pub const NON_GAME_ROOTS: [&str; 5] = ["Eye servers", "Modifiers", "Friends", "Special", "Favorites"];

/// One top-level game entry parsed from ASE filters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameDefinition {
    pub name: String,
    pub paths: Vec<String>,
    pub metadata: BTreeMap<String, String>,
}

/// Read a Latin-1 text file.
pub fn read_latin1(path: &Path) -> io::Result<String> {
    Ok(latin1(&std::fs::read(path)?))
}

fn is_metadata(line: &str) -> bool {
    METADATA_PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

fn is_skipped(line: &str) -> bool {
    line.is_empty()
        || line.starts_with("//")
        || line.starts_with('-')
        || line.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Return display paths from ASE `filters.txt` content, in first-seen order.
pub fn parse_filter_paths_str(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for line in content.lines().map(str::trim) {
        if is_skipped(line) || is_metadata(line) {
            continue;
        }
        if seen.insert(line.to_string()) {
            paths.push(line.to_string());
        }
    }
    paths
}

/// Return display paths from an ASE `filters.txt` file.
pub fn parse_filter_paths(path: &Path) -> io::Result<Vec<String>> {
    Ok(parse_filter_paths_str(&read_latin1(path)?))
}

/// Parse top-level games and their filter subpaths from `filters.txt` content.
pub fn parse_game_definitions_str(content: &str) -> BTreeMap<String, GameDefinition> {
    let mut games: BTreeMap<String, GameDefinition> = BTreeMap::new();
    let mut current: Option<String> = None;

    for line in content.lines().map(str::trim) {
        if is_skipped(line) {
            continue;
        }

        if !is_metadata(line) {
            let root = line.split('\\').next().unwrap_or(line);
            if NON_GAME_ROOTS.contains(&root) {
                current = None;
                continue;
            }
            let game = games.entry(root.to_string()).or_insert_with(|| GameDefinition {
                name: root.to_string(),
                ..GameDefinition::default()
            });
            if !game.paths.iter().any(|path| path == line) {
                game.paths.push(line.to_string());
            }
            current = Some(root.to_string());
            continue;
        }

        if let Some(game) = current.as_ref().and_then(|name| games.get_mut(name))
            && let Some((key, value)) = line.split_once(':')
        {
            game.metadata
                .entry(key.to_string())
                .or_insert_with(|| value.trim().to_string());
        }
    }
    games
}

/// Parse top-level games and their filter subpaths from an ASE `filters.txt` file.
pub fn parse_game_definitions(path: &Path) -> io::Result<BTreeMap<String, GameDefinition>> {
    Ok(parse_game_definitions_str(&read_latin1(path)?))
}

/// Find `name` in `start` or any of its ancestors.
pub fn find_in_ancestors(start: &Path, name: &str) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Find the nearest original `filters.txt` around the OpenASE folder.
pub fn find_filters_file(start: &Path) -> Option<PathBuf> {
    find_in_ancestors(start, "filters.txt")
}
