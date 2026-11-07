//! Game configuration, settings persistence and local detection.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ase_filters::GameDefinition;

/// Per-game user settings. Field names match the Python version's JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    pub name: String,
    pub visible: bool,
    pub detected: bool,
    pub player_name: String,
    pub config_file: String,
    pub program_location: String,
    pub allow_invitations: bool,
    pub draw_player_names_with_game_font: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            visible: false,
            detected: false,
            player_name: String::new(),
            config_file: String::new(),
            program_location: String::new(),
            allow_invitations: true,
            draw_player_names_with_game_font: true,
        }
    }
}

impl GameSettings {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Self::default()
        }
    }
}

/// Contents of `openase_settings.json`, shared with the Python version.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub open_nav_paths: Vec<String>,
    pub games: BTreeMap<String, GameSettings>,
}

impl Settings {
    /// Load settings, falling back to defaults when the file is missing or invalid.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// Return a verified executable path for a game, or `None`.
pub fn detect_game(definition: &GameDefinition, deep_search: bool) -> Option<String> {
    if let Some(path) = detect_from_registry(definition) {
        return Some(path);
    }
    if !deep_search {
        return None;
    }
    detect_from_common_paths(definition)
}

#[cfg(windows)]
fn detect_from_registry(definition: &GameDefinition) -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let reg_key = definition.metadata.get("RegKey")?;
    let (root_name, subkey) = reg_key.split_once('\\')?;
    let root = match root_name {
        "HKEY_LOCAL_MACHINE" => RegKey::predef(HKEY_LOCAL_MACHINE),
        "HKEY_CURRENT_USER" => RegKey::predef(HKEY_CURRENT_USER),
        _ => return None,
    };
    if subkey.is_empty() {
        return None;
    }
    let value: String = root.open_subkey(subkey).ok()?.get_value("").ok()?;
    let base = PathBuf::from(value.trim_matches('"'));
    let mut candidates = vec![base.clone()];
    if let Some(exe) = definition.metadata.get("ExeName") {
        candidates.push(base.join(exe));
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|candidate| candidate.display().to_string())
}

#[cfg(not(windows))]
fn detect_from_registry(_definition: &GameDefinition) -> Option<String> {
    None
}

fn detect_from_common_paths(definition: &GameDefinition) -> Option<String> {
    let exe_name = definition.metadata.get("ExeName")?;
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .find_map(|root| find_file(Path::new(&root), exe_name))
        .map(|path| path.display().to_string())
}

/// Recursively search `dir` for a file named `name` (case-insensitive on Windows).
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_dir() {
            subdirs.push(entry.path());
        } else if file_type.is_file() && names_match(&entry.file_name().to_string_lossy(), name) {
            return Some(entry.path());
        }
    }
    subdirs.into_iter().find_map(|subdir| find_file(&subdir, name))
}

fn names_match(a: &str, b: &str) -> bool {
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}
