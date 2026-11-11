//! Command-line scan of one catalog game, without the GUI.
//!
//! Usage: cargo run --example scan -- <game-id> [limit]
//!        cargo run --example scan -- list
//! Set STEAM_API_KEY for Steam games (cs16).

use openase::catalog::{game, games};
use openase::colors::strip_quake_colors;
use openase::quake3::parse_address;
use openase::query::{QueryOptions, fetch_all_masters, query_server};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let id = args.next().unwrap_or_else(|| "q3".to_string());
    if id == "list" {
        for game in games() {
            println!("{:<6} {}", game.id, game.name);
        }
        return Ok(());
    }
    let limit: usize = args.next().map_or(Ok(8), |raw| raw.parse())?;
    let game = game(&id).ok_or_else(|| format!("unknown game {id}; try `list`"))?;
    let options = QueryOptions::default();
    let steam_key = std::env::var("STEAM_API_KEY").unwrap_or_default();

    let list = fetch_all_masters(game, options.timeout, &steam_key, 500);
    for error in &list.errors {
        eprintln!("master error: {error}");
    }
    match list.paged_total {
        Some(total) => println!(
            "{}: {} servers listed ({} in all on 333networks)",
            game.name,
            list.servers.len(),
            total
        ),
        None => println!("{}: {} servers listed", game.name, list.servers.len()),
    }

    // Servers listed with their details (333networks) are printed as listed,
    // without querying them; the others are queried.
    let (previews, addresses): (Vec<_>, Vec<_>) = list
        .servers
        .into_iter()
        .partition(|entry| entry.preview.is_some());
    for server in previews
        .into_iter()
        .filter_map(|entry| entry.preview)
        .take(limit)
    {
        println!(
            "{:<22} listed          {:>2}/{:<2} {:<18} {:<14} {}",
            server.address(),
            server.num_players,
            server
                .max_players
                .map_or("?".to_string(), |max| max.to_string()),
            game.game_type_name(&server.game_type),
            server.map,
            server.name.chars().take(40).collect::<String>(),
        );
    }

    let servers: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = addresses
            .iter()
            .take(limit)
            .map(|entry| {
                let address = &entry.address;
                let options = &options;
                scope.spawn(move || {
                    let (host, port) = parse_address(address, game.default_port).ok()?;
                    Some(query_server(game, &host, port, options))
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok().flatten())
            .collect()
    });

    for server in servers {
        if server.online {
            println!(
                "{:<22} ping {:>4} ±{:<3} PL {:>3}%  {:>2}/{:<2} {:<18} {:<14} {}",
                server.address(),
                server.ping.unwrap_or(0),
                server.flux.unwrap_or(0),
                server.packet_loss.unwrap_or(0),
                server.num_players,
                server
                    .max_players
                    .map_or("?".to_string(), |max| max.to_string()),
                game.game_type_name(&server.game_type),
                server.map,
                strip_quake_colors(&server.name)
                    .chars()
                    .take(40)
                    .collect::<String>(),
            );
        } else {
            println!(
                "{:<22} offline ({})",
                server.address(),
                server.error.unwrap_or_default()
            );
        }
    }
    Ok(())
}
