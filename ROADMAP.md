# OpenASE Roadmap

Goal: one open-source browser for the classic online games that still have
players, with the look and feel of The All-Seeing Eye.

Rust (`rust/`) is the main version. The Python version stays as a reference and
receives protocol fixes only.

## v0.1 — Quake 3 browsing (done)

- Quake 3 master discovery and `getstatus` queries
- Colored server and player names (`^1`, `^A`, `^xRRGGBB`)
- Python (Tkinter) and Rust (egui) versions, GitHub Pages site

## v0.2 — Usable by anyone (done)

- [x] Built-in game catalog (`rust/src/games.json`) replacing `filters.txt`
- [x] Real ping, plus ASE's Flux and PL from several samples per server
- [x] Several masters per game, queried in parallel and merged
- [x] Sort ascending/descending; window, panel and column sizes remembered
- [x] CI on GitHub Actions (fmt, clippy, tests on Windows and Linux, Python tests)
- [x] Windows build attached to a GitHub Release when a `v*` tag is pushed

## v0.3 — The id Tech 3 family (done)

- [x] Urban Terror, OpenArena, Wolfenstein: Enemy Territory
- [x] Call of Duty 1, 2 and 4
- [x] Per-game badges and game type names (FFA, CTF, Search and Destroy...)

## v0.4 — The ASE experience (done)

- [x] ASE layout: filter tree, Ping/Flux/PL columns, Friends / Players / Rules panes
- [x] Favorites and recently played servers, Add server
- [x] Join with double-click or Enter (`+connect`, mod, name, password, config)
- [x] Friends list, Friends pane, notice and sound when a friend is found
- [x] Find player across scanned servers
- [x] Auto-refresh of the selected server
- [x] Filters by map, mod, version and per-game settings (Kill Cam, PunkBuster...)
- [x] Options: Network, Events, Games with Auto-Search

## v0.5 — More engines (done)

- [x] Unreal Tournament 99 (GameSpy protocol, 333networks master)
- [x] Counter-Strike 1.6 (Valve A2S queries, Steam Web API list)
- [x] Protocol layer: a new Quake 3, GameSpy or A2S game is a `games.json` entry

## Not done yet

- [ ] Unreal Tournament 2004 (different master protocol)
- [ ] Steam server list checked with a real API key (code is unit-tested only)
- [ ] Customizable toolbar, per-friend sounds, Colors options tab
- [ ] Location column from GeoIP (only shown when the server reports it)

## v1.0 — Polished release

- [ ] Signed Windows installer and update check
- [ ] Linux packages (AppImage or Flatpak)
- [ ] English and French interface
- [ ] User guide on the website, custom domain

## Community

- [ ] `CONTRIBUTING.md`, issue templates, "good first issue" labels
- [ ] Announce on retro FPS communities (ioquake3, OpenArena, Urban Terror, 333networks)
