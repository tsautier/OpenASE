//! Query one server and print everything OpenASE knows about it.
//!
//! Usage: cargo run --example query -- <game-id> <host:port>

use openase::catalog::game;
use openase::colors::strip_quake_colors;
use openase::quake3::parse_address;
use openase::query::{QueryOptions, query_server};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(id), Some(address)) = (args.next(), args.next()) else {
        return Err("usage: query <game-id> <host:port>".into());
    };
    let game = game(&id).ok_or_else(|| format!("unknown game {id}"))?;
    let (host, port) = parse_address(&address, game.default_port)?;
    let server = query_server(game, &host, port, &QueryOptions::default());
    if !server.online {
        return Err(format!("offline: {}", server.error.unwrap_or_default()).into());
    }
    println!("{}", strip_quake_colors(&server.name));
    println!(
        "map {}  type {}  mod {}  players {}/{}  bots {}  password {}",
        server.map,
        game.game_type_name(&server.game_type),
        server.mod_name,
        server.num_players,
        server.max_players.unwrap_or(0),
        server.bots,
        server.password
    );
    println!(
        "ping {} ms  flux ±{}  loss {}%  join {}",
        server.ping.unwrap_or(0),
        server.flux.unwrap_or(0),
        server.packet_loss.unwrap_or(0),
        server.join_address()
    );
    for player in server.players.iter().take(8) {
        println!(
            "  {:<28} score {:>4}  ping {:>4}  time {:>5}s",
            strip_quake_colors(&player.name),
            player.score,
            player.ping.map_or("-".into(), |p| p.to_string()),
            player.time_secs.map_or("-".into(), |t| t.to_string())
        );
    }
    println!("{} rules", server.rules.len());
    Ok(())
}
