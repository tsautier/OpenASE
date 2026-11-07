//! Built-in game catalog, loaded from `games.json` embedded in the binary.

use std::sync::LazyLock;

use serde::Deserialize;

const GAMES_JSON: &str = include_str!("games.json");

static CATALOG: LazyLock<Vec<Game>> =
    LazyLock::new(|| serde_json::from_str(GAMES_JSON).expect("games.json is valid"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Quake3,
    Gamespy,
    A2s,
}

/// Where a game's server list comes from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Master {
    /// Quake 3 style UDP master; `query` is the raw request, e.g. `getservers 68 empty full`.
    Quake3 { address: String, query: String },
    /// GameSpy TCP master with `\validate\` challenge.
    Gamespy {
        address: String,
        gamename: String,
        key: String,
    },
    /// Steam Web API `IGameServersService/GetServerList` (needs a user API key).
    SteamWeb { filter: String },
}

impl Master {
    pub fn label(&self) -> String {
        match self {
            Master::Quake3 { address, .. } | Master::Gamespy { address, .. } => address.clone(),
            Master::SteamWeb { .. } => "Steam Web API".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GameType {
    pub value: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FilterOption {
    pub label: String,
    pub values: Vec<String>,
}

/// A group of checkbox filters on one server rule, like ASE's "Kill Cam: On/Off".
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FilterGroup {
    pub name: String,
    pub key: String,
    /// Options are built from the values seen on scanned servers.
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default)]
    pub options: Vec<FilterOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RegistryEntry {
    /// Full key path, starting with `HKEY_LOCAL_MACHINE` or `HKEY_CURRENT_USER`.
    pub key: String,
    /// Value name holding the install folder.
    pub value: String,
    /// Executable path relative to that folder.
    pub exe: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub short: String,
    pub color: String,
    pub protocol: Protocol,
    pub default_port: u16,
    #[serde(default)]
    pub steam_appid: Option<u32>,
    pub masters: Vec<Master>,
    #[serde(default)]
    pub game_type_key: String,
    #[serde(default)]
    pub game_types: Vec<GameType>,
    #[serde(default)]
    pub mod_key: String,
    #[serde(default)]
    pub base_mod: String,
    #[serde(default)]
    pub filters: Vec<FilterGroup>,
    #[serde(default)]
    pub registry: Vec<RegistryEntry>,
    #[serde(default)]
    pub exe_names: Vec<String>,
    pub join: String,
}

impl Game {
    /// Display name for a raw game type value.
    pub fn game_type_name(&self, value: &str) -> String {
        self.game_types
            .iter()
            .find(|game_type| game_type.value.eq_ignore_ascii_case(value))
            .map(|game_type| game_type.name.clone())
            .unwrap_or_else(|| value.to_string())
    }

    /// Badge color as RGB.
    pub fn rgb(&self) -> [u8; 3] {
        crate::colors::parse_hex(&self.color).unwrap_or([0x80, 0x80, 0x80])
    }

    /// Mod name to pass on the command line, or empty for the base game.
    pub fn launch_mod(&self, mod_name: &str) -> String {
        if mod_name.is_empty() || mod_name.eq_ignore_ascii_case(&self.base_mod) {
            String::new()
        } else {
            mod_name.to_string()
        }
    }
}

/// All built-in games, in display order.
pub fn games() -> &'static [Game] {
    &CATALOG
}

pub fn game(id: &str) -> Option<&'static Game> {
    CATALOG.iter().find(|game| game.id == id)
}
