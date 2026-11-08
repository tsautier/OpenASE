//! Joining a server: expand the game's command-line template and start it.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::Command;

use crate::catalog::Game;
use crate::model::Server;
use crate::settings::GameSettings;

/// Expand `{placeholders}` in a template. A `[...]` group is kept only when
/// every placeholder inside it has a non-empty value, as in ASE's JoinTemplate.
pub fn expand_template(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('[') {
        out.push_str(&substitute(&rest[..start], vars).0);
        let Some(len) = rest[start..].find(']') else {
            rest = &rest[start..];
            break;
        };
        let (text, complete) = substitute(&rest[start + 1..start + len], vars);
        if complete {
            out.push_str(&text);
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(&substitute(rest, vars).0);
    out
}

/// Replace placeholders; the flag is false when one of them was empty or unknown.
fn substitute(text: &str, vars: &HashMap<&str, String>) -> (String, bool) {
    let mut out = String::new();
    let mut complete = true;
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        let name = &rest[start + 1..start + len];
        match vars.get(name).filter(|value| !value.is_empty()) {
            Some(value) => out.push_str(value),
            None => complete = false,
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    (out, complete)
}

/// Split a command line into arguments, honoring double quotes.
pub fn split_args(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in line.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    args
}

/// Arguments to join `server` with `game`'s template.
pub fn join_args(
    game: &Game,
    settings: &GameSettings,
    server: &Server,
    password: &str,
) -> Vec<String> {
    let vars = HashMap::from([
        ("address", server.join_address()),
        ("name", settings.player_name.clone()),
        ("password", password.to_string()),
        ("config", settings.config_file.clone()),
        ("mod", game.launch_mod(&server.mod_name)),
        ("ip", server.host.clone()),
        ("port", server.join_port.to_string()),
    ]);
    split_args(&expand_template(&game.join, &vars))
}

/// What [`join`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launched {
    Program(String),
    SteamUrl(String),
}

/// Start the game on `server`. Falls back to a `steam://connect` link for
/// Steam games without a configured program.
pub fn join(
    game: &Game,
    settings: &GameSettings,
    server: &Server,
    password: &str,
) -> io::Result<Launched> {
    let program = settings.program_location.trim();
    if !program.is_empty() {
        let path = Path::new(program);
        let mut command = Command::new(path);
        command.args(join_args(game, settings, server, password));
        if let Some(dir) = path.parent().filter(|dir| dir.is_dir()) {
            command.current_dir(dir);
        }
        command.spawn()?;
        return Ok(Launched::Program(program.to_string()));
    }
    if game.steam_appid.is_some() {
        let mut url = format!("steam://connect/{}", server.join_address());
        if !password.is_empty() {
            url.push('/');
            url.push_str(password);
        }
        open_url(&url)?;
        return Ok(Launched::SteamUrl(url));
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "Set the program location of {} in Options > Games first.",
            game.name
        ),
    ))
}

/// Open a URL with the system handler.
pub fn open_url(url: &str) -> io::Result<()> {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    command.spawn().map(|_| ())
}
