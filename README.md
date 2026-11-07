# OpenASE

Website: https://tsautier.github.io/OpenASE/

OpenASE is an open-source game server browser in the spirit of The All-Seeing
Eye: a filter tree on the left, the server list with Ping, Flux and PL on the
right, and friends, players and server rules at the bottom.

It is new code. It does not use, modify or ship anything from the original
Yahoo program.

![OpenASE](docs/screenshot.png)

## Supported games

| Game | Protocol | Server list from |
|---|---|---|
| Quake 3: Arena | Quake 3 | master.quake3arena.com, master0.excessiveplus.net, dpmaster.deathmask.net |
| Urban Terror | Quake 3 | master.urbanterror.info, master2.urbanterror.info |
| OpenArena | Quake 3 | dpmaster.deathmask.net |
| Wolfenstein: Enemy Territory | Quake 3 | etmaster.idsoftware.com, etmaster.etlegacy.com |
| Call of Duty, CoD 2, CoD 4 | Quake 3 | Activision masters |
| Unreal Tournament (99) | GameSpy | master.333networks.com |
| Counter-Strike 1.6 | Valve A2S | Steam Web API (needs your own key) |

Games are described in [`rust/src/games.json`](rust/src/games.json): masters,
game types, filter groups, registry detection and the command line used to join.

## Features

- **Ping, Flux and PL** measured like ASE: several queries per server, average
  round-trip time, half the spread between fastest and slowest, and packet loss.
- **Filter tree**: game types as folders, and checkbox groups per game such as
  Mods, Friendly Fire, Kill Cam, Pure, PunkBuster, Version and Maps.
  **Modifiers** hide empty, full, passworded or silent servers and cap the ping.
- **Favorites**, **Recently played**, **Add server** by address.
- **Join** with a double-click or Enter. OpenASE starts the game with the right
  mod, player name, config and password. Steam games fall back to `steam://connect`.
- **Friends**: names searched on every scanned server, a Friends pane, a notice
  (and optional sound) when one starts playing. **Find player** across all servers.
- **Options**: Network (timeouts, pings, retries, parallel queries), Events
  (friend notices, auto-refresh of the selected server) and Games (installed /
  not installed, player name, config, program location, **Auto-Search**).
- Quake color codes (`^1`, `^A`, `^xRRGGBB`) drawn in server and player names.

## Download

Windows builds are attached to [GitHub Releases](https://github.com/tsautier/OpenASE/releases)
when a version tag is pushed.

## Build from source (Rust)

Requires Rust 1.88 or newer.

```powershell
cd rust
cargo run --release
```

Command-line tools, useful to test a master or a server:

```powershell
cargo run --example scan -- list              # list game ids
cargo run --example scan -- cod2 20           # scan 20 CoD2 servers
cargo run --example query -- ut99 1.2.3.4:7778
cargo test
```

Settings are stored in `%APPDATA%\OpenASE\settings.json` on Windows
(`~/.config/OpenASE` on Linux).

### Counter-Strike 1.6

Valve's old UDP master no longer answers, so the list comes from the Steam Web
API. Get a free key at https://steamcommunity.com/dev/apikey and paste it in
Options > General. Servers added by hand work without a key.

## Python version

The first prototype, in [`openase/`](openase/), is kept as a reference for the
Quake 3 protocol. It only receives protocol fixes; new features go to Rust.

```powershell
python -m openase
python -m unittest discover -s tests
```

It reads its game tree from an original ASE `filters.txt` placed next to the
project, if there is one.

## References

- Quake 3 connectionless queries: Jacek Fedorynski, "Quake 3 Network Protocol"
  (2020), https://www.jfedor.org/quake3/
- Valve server queries (A2S): Valve Developer Wiki, "Server queries".
- GameSpy master protocol: 333networks, https://333networks.com/
- Yahoo acquired The All-Seeing Eye in September 2004:
  https://www.gamedeveloper.com/game-platforms/yahoo-games-adds-all-seeing-eye-gamespot-syndication

## License

Copyright (C) 2026 tsautier

OpenASE is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See [LICENSE](LICENSE) for the full text.

OpenASE is not affiliated with Yahoo or the original All-Seeing Eye authors.
Game names are trademarks of their owners. No original ASE files are distributed
with this project.
