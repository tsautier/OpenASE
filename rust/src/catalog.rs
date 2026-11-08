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
    Quake2,
    #[serde(rename = "quakeworld")]
    QuakeWorld,
    Unreal2,
    Ase,
    /// GameSpy version 2 query (Halo, SWAT 4).
    Gamespy2,
    /// GameSpy version 3 query, also called GameSpy 4 (Battlefield 2).
    Gamespy3,
    /// Starsiege: Tribes binary query (`b` status, native ping).
    Tribes,
    /// Tribes 2 (Torque) binary query: ping 14, info 18.
    Tribes2,
    /// id Tech 4 `getInfo` query (Doom 3, Quake 4).
    Doom3,
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
    /// Quake 2 family master; `query` is the raw request, usually `query`.
    Quake2 { address: String, query: String },
    /// QuakeWorld master (`c` request).
    Quakeworld { address: String },
    /// Unreal Engine 2 binary master (TCP); `client` and `version` identify
    /// the game (`UT2K4CLIENT` 3369 for UT2004, `CLIENT` 2225 for UT2003).
    Unreal2 {
        address: String,
        client: String,
        version: u32,
    },
    /// Multi Theft Auto HTTP master with a binary list (game ports).
    Mta { url: String },
    /// GameSpy server browsing master (TCP 28910, encoding X), as run by
    /// OpenSpy and BF2Hub.
    GamespySb {
        address: String,
        gamename: String,
        key: String,
    },
    /// Starsiege: Tribes UDP master (port 28000).
    Tribes { address: String },
    /// Plain-text HTTP list, one `ip:port` per line (TribesNext master).
    HttpList { url: String },
    /// id Tech 4 master (`getServers`); `version` is the protocol as
    /// `major.minor` (`1.0` for Doom 3, `2.0` for Quake 4).
    Doom3 { address: String, version: String },
}

impl Master {
    pub fn label(&self) -> String {
        match self {
            Master::Quake3 { address, .. }
            | Master::Gamespy { address, .. }
            | Master::Quake2 { address, .. }
            | Master::Doom3 { address, .. }
            | Master::Quakeworld { address } => address.clone(),
            Master::SteamWeb { .. } => "Steam Web API".to_string(),
            Master::Unreal2 { address, .. } => address.clone(),
            Master::Mta { url } => url.clone(),
            Master::GamespySb { address, .. } => address.clone(),
            Master::Tribes { address } => address.clone(),
            Master::HttpList { url } => url.clone(),
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
    /// Added to the host port to reach the query port (123 for ASE in Multi
    /// Theft Auto). Server addresses then hold the host port players join.
    #[serde(default)]
    pub query_port_offset: u16,
    /// GameSpy (v1) request used for extra ping samples; `\info\` when empty.
    /// Some servers (SWAT 4 with stats mods) only answer `\status\`.
    #[serde(default)]
    pub ping_request: String,
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
        // Some servers report a display string ("Dark-MoD v5.1") instead of
        // the mod folder: passing it as fs_game would break the join.
        let is_folder = mod_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
        if mod_name.is_empty() || !is_folder || mod_name.eq_ignore_ascii_case(&self.base_mod) {
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
