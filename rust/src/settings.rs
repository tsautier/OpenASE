//! User settings, stored as JSON in the user's config folder
//! (`%APPDATA%\OpenASE\settings.json` on Windows).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::query::QueryOptions;

/// Per-game options (ASE's Options > Games).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    pub visible: bool,
    pub player_name: String,
    pub config_file: String,
    pub program_location: String,
    pub allow_invitations: bool,
    pub draw_player_names_with_game_font: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            visible: true,
            player_name: String::new(),
            config_file: String::new(),
            program_location: String::new(),
            allow_invitations: true,
            draw_player_names_with_game_font: true,
        }
    }
}

/// A server remembered in Favorites or Recently played.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ServerRef {
    pub game: String,
    /// Query address, `host:port`.
    pub address: String,
}

/// ASE's Options > Network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    /// Timeout when querying a server, in milliseconds.
    pub timeout_ms: u64,
    /// Number of times to ping a server.
    pub pings: u32,
    /// Number of times to retry a scan.
    pub retries: u32,
    /// Servers queried at the same time.
    pub parallel_queries: usize,
    /// Maximum servers taken from the masters per refresh.
    pub scan_limit: usize,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            timeout_ms: 1000,
            pings: 3,
            retries: 1,
            parallel_queries: 32,
            scan_limit: 1500,
        }
    }
}

impl NetworkSettings {
    pub fn query_options(&self) -> QueryOptions {
        QueryOptions {
            timeout: Duration::from_millis(self.timeout_ms.clamp(100, 10_000)),
            pings: self.pings.clamp(1, 10),
            retries: self.retries.min(5),
        }
    }
}

/// ASE's Options > Events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventSettings {
    /// Show a notice when a friend is found on a server.
    pub notify_friend_joins: bool,
    /// Play the system notification sound for friend events.
    pub sound_friend_joins: bool,
    /// Re-query the selected server periodically.
    pub auto_refresh_selected: bool,
    pub auto_refresh_seconds: u32,
}

impl Default for EventSettings {
    fn default() -> Self {
        Self {
            notify_friend_joins: true,
            sound_friend_joins: false,
            auto_refresh_selected: false,
            auto_refresh_seconds: 15,
        }
    }
}

/// Filter state of the left tree ("Modifiers" and per-game checkbox groups).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterSettings {
    pub not_empty: bool,
    pub not_full: bool,
    pub no_password: bool,
    pub hide_offline: bool,
    /// `None` means no ping limit.
    pub max_ping: Option<u32>,
    /// Checked options, keyed by `game_id/group name`.
    pub checked: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub games: BTreeMap<String, GameSettings>,
    pub favorites: Vec<ServerRef>,
    pub recent: Vec<ServerRef>,
    pub friends: Vec<String>,
    pub steam_api_key: String,
    pub network: NetworkSettings,
    pub events: EventSettings,
    pub filters: FilterSettings,
    pub open_nav: BTreeSet<String>,
    /// Game selected at startup.
    pub last_game: String,
}

pub const RECENT_LIMIT: usize = 20;

impl Settings {
    pub fn path() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn game(&self, id: &str) -> GameSettings {
        self.games.get(id).cloned().unwrap_or_default()
    }

    pub fn is_favorite(&self, server: &ServerRef) -> bool {
        self.favorites.contains(server)
    }

    pub fn toggle_favorite(&mut self, server: ServerRef) {
        if let Some(index) = self.favorites.iter().position(|entry| *entry == server) {
            self.favorites.remove(index);
        } else {
            self.favorites.push(server);
        }
    }

    pub fn add_recent(&mut self, server: ServerRef) {
        self.recent.retain(|entry| *entry != server);
        self.recent.insert(0, server);
        self.recent.truncate(RECENT_LIMIT);
    }
}

/// Folder holding OpenASE's settings.
pub fn config_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join("OpenASE")
}
