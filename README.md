# OpenASE

OpenASE is a new implementation inspired by The All-Seeing Eye, starting with
the original ASE navigation tree and Quake 3 server discovery/status queries.

This repository does not modify or depend on the original Yahoo executable.
The original files in this workspace are treated as historical data only.

## Verified Scope

- Local `eye.exe` metadata identifies the archived binary as `Yahoo! All-Seeing Eye`
  version `2.6.0.254`.
- Local `history.txt` says Yahoo acquired The All-Seeing Eye in September 2004.
- GameDeveloper reported on 2004-12-14 that Yahoo had acquired All-Seeing Eye
  in September 2004 and planned integration with Yahoo Games and Yahoo Messenger:
  https://www.gamedeveloper.com/game-platforms/yahoo-games-adds-all-seeing-eye-gamespot-syndication
- ioquake3 documents that its engine is based on the released Quake 3 source:
  https://ioquake3.org/
- The Quake 3 connectionless query format is described by Jacek Fedorynski
  in "Quake 3 Network Protocol" (January 2020):
  https://www.jfedor.org/quake3/
- ioquake community maintainers describe `getinfo` and `getstatus` UDP requests:
  https://discourse.ioquake.org/t/how-to-get-server-status-with-python/1135

## Usage

From this folder:

```powershell
python -m openase
```

Or double-click `OpenASE.bat`.

The left navigation is loaded from the original `filters.txt` when that file is
available next to the archived ASE files. Games are hidden by default unless
OpenASE verifies a local installation or the user marks the game visible from
`Server > Options > Games`.

Quake-style color markers such as `^1`, extended indexed markers such as `^A`,
and hex markers such as `^xff0000` are rendered as colored text segments in
server names and player names.

The `Location` column shows a reported server location when the queried protocol
provides one. OpenASE does not guess countries from IP addresses without a
verifiable GeoIP database.

Default Quake 3 server port is `27960` when no port is provided.
Default Quake 3 master server is `master.quake3arena.com:27950`.

## Rust version

A native Rust port lives in [`rust/`](rust/). It has the same features and the
same `openase_settings.json` format, with an [egui](https://github.com/emilk/egui)
interface. It also scans 16 servers in parallel and reads master lists split
across several UDP packets.

```powershell
cd rust
cargo run --release          # GUI
cargo run --example scan     # command-line smoke test against the master
cargo test
```

## Tests

```powershell
python -m unittest discover -s tests
```

The current tests are offline parser tests. They do not require a live Quake 3
server.

## License

Copyright (C) 2026 tsautier

OpenASE is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See [LICENSE](LICENSE) for the full text.

OpenASE is not affiliated with Yahoo or the original All-Seeing Eye authors.
No original ASE files are distributed with this project.
