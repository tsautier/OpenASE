//! Protocol-independent server and player model.

use std::collections::BTreeMap;

/// One player on a server.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Player {
    pub name: String,
    pub score: i32,
    pub ping: Option<u32>,
    /// Time connected, in seconds (A2S only).
    pub time_secs: Option<u32>,
}

/// One game server, normalized across protocols.
#[derive(Debug, Clone, Default)]
pub struct Server {
    /// Catalog id of the game (`q3`, `cod2`, `ut99`...).
    pub game: String,
    pub host: String,
    /// Port used for queries.
    pub port: u16,
    /// Port players connect to (differs from `port` for Unreal).
    pub join_port: u16,
    pub online: bool,
    pub error: Option<String>,

    /// Raw server name, possibly with Quake color codes.
    pub name: String,
    pub map: String,
    /// Raw game type value as reported (`4`, `sd`, `CTFGame`...).
    pub game_type: String,
    pub mod_name: String,
    pub num_players: usize,
    pub max_players: Option<usize>,
    pub bots: usize,
    pub password: bool,
    pub location: String,

    /// Average round-trip time in milliseconds.
    pub ping: Option<u32>,
    /// Ping variation (half the spread between fastest and slowest sample).
    pub flux: Option<u32>,
    /// Packet loss in percent over the ping samples.
    pub packet_loss: Option<u8>,

    pub players: Vec<Player>,
    /// Every key/value the server reported.
    pub rules: BTreeMap<String, String>,
}

impl Server {
    pub fn new(game: &str, host: &str, port: u16) -> Self {
        Self {
            game: game.to_string(),
            host: host.to_string(),
            port,
            join_port: port,
            ..Self::default()
        }
    }

    pub fn offline(game: &str, host: &str, port: u16, error: impl Into<String>) -> Self {
        Self {
            error: Some(error.into()),
            packet_loss: Some(100),
            ..Self::new(game, host, port)
        }
    }

    /// Query address, `host:port`.
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Address players connect to, `host:join_port`.
    pub fn join_address(&self) -> String {
        format!("{}:{}", self.host, self.join_port)
    }

    /// Value of a rule, with `mapname` falling back to the normalized map.
    pub fn rule(&self, key: &str) -> Option<&str> {
        match self.rules.get(key) {
            Some(value) => Some(value.as_str()),
            None if key == "mapname" => Some(self.map.as_str()),
            None => None,
        }
    }

    pub fn is_full(&self) -> bool {
        self.max_players
            .is_some_and(|max| max > 0 && self.num_players >= max)
    }
}

/// Parse a rule value as an unsigned integer.
pub fn parse_count(value: Option<&String>) -> Option<usize> {
    value.and_then(|raw| raw.trim().parse().ok())
}
