//! User-defined filter rules, in the spirit of ASE's `if KEY OP "VALUE"` lines.

use serde::{Deserialize, Serialize};

use crate::countries::country_name;
use crate::model::Server;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    #[serde(rename = "==")]
    Equals,
    #[serde(rename = "!=")]
    NotEquals,
    #[serde(rename = "<")]
    Less,
    #[serde(rename = ">")]
    Greater,
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "!contains")]
    NotContains,
}

impl Op {
    pub const ALL: [Op; 6] = [
        Op::Equals,
        Op::NotEquals,
        Op::Less,
        Op::Greater,
        Op::Contains,
        Op::NotContains,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Op::Equals => "is",
            Op::NotEquals => "is not",
            Op::Less => "less than",
            Op::Greater => "greater than",
            Op::Contains => "contains",
            Op::NotContains => "does not contain",
        }
    }
}

/// One condition a server must meet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// A server rule key (`g_gametype`, `sv_pure`...) or a built-in field:
    /// `name`, `map`, `players`, `maxplayers`, `ping`, `gametype`, `mod`,
    /// `country`, `password`, `address`.
    pub key: String,
    pub op: Op,
    pub value: String,
}

/// Built-in keys offered first in the filter editor.
pub const BUILTIN_KEYS: [&str; 10] = [
    "name",
    "map",
    "players",
    "maxplayers",
    "ping",
    "gametype",
    "mod",
    "country",
    "password",
    "address",
];

/// A named filter: all rules must match. With a color it also highlights rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomFilter {
    pub name: String,
    /// Catalog game id, or empty for every game.
    #[serde(default)]
    pub game: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Row highlight color (`#rrggbb`) when used as a color filter.
    #[serde(default)]
    pub color: Option<String>,
}

impl CustomFilter {
    pub fn matches(&self, server: &Server) -> bool {
        (self.game.is_empty() || self.game == server.game)
            && self.rules.iter().all(|rule| rule.matches(server))
    }
}

impl Rule {
    /// Value of the rule's key on a server, if any.
    pub fn value_of(&self, server: &Server) -> Option<String> {
        let plain = |text: &str| crate::colors::strip_quake_colors(text);
        Some(match self.key.to_lowercase().as_str() {
            "name" => plain(&server.name),
            "map" => server.map.clone(),
            "players" => server.num_players.to_string(),
            "maxplayers" => server.max_players?.to_string(),
            "ping" => server.ping?.to_string(),
            "gametype" => {
                let game = crate::catalog::game(&server.game);
                let name = game
                    .map(|game| game.game_type_name(&server.game_type))
                    .unwrap_or_default();
                if name.is_empty() {
                    server.game_type.clone()
                } else {
                    name
                }
            }
            "mod" => server.mod_name.clone(),
            "country" => {
                let code = server.country.clone()?;
                country_name(&code).map_or(code, str::to_string)
            }
            "password" => if server.password { "1" } else { "0" }.to_string(),
            "address" => server.join_address(),
            _ => server.rules.get(&self.key).cloned()?,
        })
    }

    pub fn matches(&self, server: &Server) -> bool {
        let Some(actual) = self.value_of(server) else {
            return matches!(self.op, Op::NotEquals | Op::NotContains);
        };
        let (a, b) = (
            actual.trim().to_lowercase(),
            self.value.trim().to_lowercase(),
        );
        let numbers = a.parse::<f64>().ok().zip(b.parse::<f64>().ok());
        match self.op {
            Op::Equals => numbers.map_or(a == b, |(x, y)| x == y),
            Op::NotEquals => numbers.map_or(a != b, |(x, y)| x != y),
            Op::Less => numbers.is_some_and(|(x, y)| x < y),
            Op::Greater => numbers.is_some_and(|(x, y)| x > y),
            Op::Contains => a.contains(&b),
            Op::NotContains => !a.contains(&b),
        }
    }
}
