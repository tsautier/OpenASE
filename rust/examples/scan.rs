//! Command-line smoke test: list servers from the ioquake3 master and query a few.
//!
//! Usage: cargo run --example scan -- [master[:port]] [limit]

use std::time::Duration;

use openase::colors::strip_quake_colors;
use openase::quake3::{DEFAULT_MASTER, DEFAULT_MASTER_PORT, DEFAULT_PORT, Quake3Client, parse_address};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let master = args.next().unwrap_or_else(|| format!("{DEFAULT_MASTER}:{DEFAULT_MASTER_PORT}"));
    let limit: usize = args.next().map_or(Ok(10), |raw| raw.parse())?;

    let (host, port) = parse_address(&master, DEFAULT_MASTER_PORT)?;
    let addresses = Quake3Client::new(Duration::from_secs(3)).master_servers(&host, port, 68)?;
    println!("{} servers listed by {host}:{port}", addresses.len());

    let client = Quake3Client::new(Duration::from_secs(2));
    for address in addresses.iter().take(limit) {
        let (host, port) = parse_address(address, DEFAULT_PORT)?;
        let server = client.status(&host, port);
        if server.online {
            let name = server.info.get("sv_hostname").map(String::as_str).unwrap_or("unknown");
            let map = server.info.get("mapname").map(String::as_str).unwrap_or("");
            println!("{address:<22} {:>2} players  {map:<12} {}", server.players.len(), strip_quake_colors(name));
        } else {
            println!("{address:<22} offline ({})", server.error.unwrap_or_default());
        }
    }
    Ok(())
}
