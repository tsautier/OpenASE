# OpenASE Roadmap

Goal: one open-source browser for the classic online games that still have
players, with the feel of The All-Seeing Eye.

Rust (`rust/`) is the main version going forward. The Python version stays as a
reference and receives protocol fixes only, so features are not built twice.

## v0.1 — Quake 3 browsing (done)

- Quake 3 master discovery and `getstatus` queries
- Colored server and player names (`^1`, `^A`, `^xRRGGBB`)
- Filters: not empty, not full, max ping, country
- Game detection through the Windows registry
- Python (Tkinter) and Rust (egui) versions, GitHub Pages site

## v0.2 — Usable by anyone

Make OpenASE downloadable and independent of the original ASE files.

- [ ] Built-in game list (data file in the repo) replacing `filters.txt`
- [ ] Real ping: measure round-trip time per server instead of using player pings
- [ ] Several masters per game with fallback (ioquake3 master is down)
- [ ] Sort ascending/descending, remember column widths and window layout
- [ ] CI on GitHub Actions: `cargo test`, `cargo clippy`, Python tests
- [ ] Windows release build attached to GitHub Releases

## v0.3 — The id Tech 3 family

Same protocol family, so a large gain in players for little code.

- [ ] Urban Terror
- [ ] OpenArena
- [ ] Wolfenstein: Enemy Territory
- [ ] Call of Duty 1, 2 and 4
- [ ] Per-game icons and game type names (FFA, CTF, TDM...)

## v0.4 — The ASE experience

- [ ] Favorites and recently played servers
- [ ] One-click join: launch the game with `+connect ip:port`
- [ ] Buddy list: find friends by player name across scanned servers
- [ ] Auto-refresh of the selected server
- [ ] Filters by map and game type

## v0.5 — More engines

- [ ] Unreal Tournament 99 / 2004 (GameSpy protocol, 333networks master)
- [ ] Half-Life / Counter-Strike 1.6 (Source A2S queries)
- [ ] Plugin-style protocol layer so new games are easy to add

## v1.0 — Polished release

- [ ] Signed Windows installer and update check
- [ ] Linux packages (AppImage or Flatpak)
- [ ] English and French interface
- [ ] User guide on the website, custom domain

## Community

- [ ] `CONTRIBUTING.md`, issue templates, "good first issue" labels
- [ ] Announce on retro FPS communities (ioquake3, OpenArena, Urban Terror, 333networks)
