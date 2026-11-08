# OpenASE Roadmap

Goal: one open-source browser for the classic online games that still have
players, with the look and feel of The All-Seeing Eye.

OpenASE is written in Rust (`rust/`). The first Python prototype was retired
in v0.6; it remains in the git history.

## v0.1 — Quake 3 browsing (done)

- Quake 3 master discovery and `getstatus` queries
- Colored server and player names (`^1`, `^A`, `^xRRGGBB`)
- Python (Tkinter) and Rust (egui) versions, GitHub Pages site

## v0.2 — Usable by anyone (done)

- [x] Built-in game catalog (`rust/src/games.json`) replacing `filters.txt`
- [x] Real ping, plus ASE's Flux and PL from several samples per server
- [x] Several masters per game, queried in parallel and merged
- [x] Sort ascending/descending; window, panel and column sizes remembered
- [x] CI on GitHub Actions (fmt, clippy, tests on Windows, Linux and macOS)
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

## v0.6 — The full ASE feel (done)

- [x] Location column and country filter (DB-IP GeoIP, monthly download)
- [x] Favorites1..8, Friends1..4 groups, Special lists, LAN servers
- [x] Custom filters with rules and color filters
- [x] File menu: profile export/import, server list save/import, update check
- [x] First-run setup wizard, Filter menu shortcuts (Ctrl+E/D/Q), Help menu
- [x] New website with manual

## v0.7 — More classics (done)

- [x] Star Wars Jedi Knight II: Jedi Outcast, Jedi Academy, Return to Castle Wolfenstein
- [x] Team Fortress Classic and Day of Defeat (Steam list)
- [x] Linux AppImage, smaller release binaries, CI and builds on Windows, Linux, macOS
- [x] `CONTRIBUTING.md` with a guide to adding a game, issue and pull request templates
- [x] Auto-Search finds native Linux and macOS game binaries
- [x] Python prototype retired

## v0.8 — Back to the full ASE list (done)

- [x] 59 games, up from 14: every ASE game with a master that still answers
- [x] Steam: Half-Life, Condition Zero, Opposing Force, Deathmatch Classic, Ricochet, CS:Source, DoD:Source, HL2:DM
- [x] Call of Duty: United Offensive, Elite Force, Soldier of Fortune 2 (community masters)
- [x] Quake 2 and QuakeWorld protocols: QuakeWorld, Quake 2, Kingpin
- [x] 20 GameSpy v1 games from 333networks and OpenSpy (Unreal, Deus Ex, MOHAA family, Vietcong, BF1942...)
- [x] Unreal Engine 2 protocol and master: UT2003, UT2004
- [x] The ASE query protocol and the Multi Theft Auto master
- [x] GameSpy v2/v3 queries and the encrypted server browsing master: BF2, BF Vietnam, Halo, SWAT 4, Tribes: Vengeance, F.E.A.R., Star Wars: Battlefront
- [x] Packet loss no longer counts rate-limited or unsupported ping requests

## v0.9 — Their own protocols and a French interface (done)

- [x] 70 games, up from 59, 15 query protocols, 46 master servers
- [x] Starsiege: Tribes (binary query, two UDP masters) and Tribes 2 (TribesNext HTTP list)
- [x] DOOM 3 and Quake 4 (id Tech 4 `getInfo`, `getServers` masters)
- [x] Soldier of Fortune (GameSpy v1 from 333networks, errorist.eu and OpenSpy)
- [x] Savage: The Battle for Newerth (Savage DRX list) and Rainbow Six 3: Raven Shield (OpenRVS list, query port = game port + 1000)
- [x] Ghost Recon and The Sum of All Fears, demos included (queries only: Add server and LAN)
- [x] English and French interface (Options > General > Language)
- [x] Customizable toolbar, per-friend sounds, Colors tab in Options
- [x] Server names in UTF-8 or Windows-1251 display correctly, dialog windows are no longer stretched

## Every game from the original ASE

The original All-Seeing Eye knew 175 games, demos included. All of them are on
this roadmap, however old: a dead master server only means no automatic list.
Each game gets, in order:

1. **Server queries**, so servers work by Add server, Favorites, imported lists
   and LAN broadcast, even with no master at all.
2. **A server list** from whatever master still answers: the original one,
   a community revival (333networks, OpenSpy, BF2Hub, dpmaster...) or Steam.
3. **Join and Auto-Search** (install folder detection, command line).

Games are grouped by query protocol: one protocol unlocks a whole group.
`[x]` means the game is in OpenASE today.

### Already supported protocols: Quake 3, GameSpy v1, Valve A2S

- [x] Quake 3: Arena
- [x] Call of Duty, Call of Duty 2
- [x] Call of Duty: United Offensive
- [x] Return to Castle Wolfenstein
- [x] Wolfenstein: Enemy Territory (and the ET test release)
- [x] Jedi Knight II, Jedi Academy
- [x] ST: Elite Force
- [ ] Star Trek: Elite Force 2 (no master answers)
- [x] Soldier of Fortune 2 (and SoF2 MP Test)
- [x] Medal Of Honor (Allied Assault), Spearhead, Breakthrough (and demos)
- [x] Unreal Tournament
- [x] Unreal
- [x] Tactical Ops (its servers are listed under Unreal Tournament)
- [ ] Unreal 2 XMP (and demo)
- [x] Deus Ex, Rune, Wheel of Time, Mobile Forces
- [ ] Nerf ArenaBlast
- [x] Serious Sam, Serious Sam: The Second Encounter (and demo)
- [x] No One Lives Forever, No One Lives Forever 2 (and demo)
- [ ] Aliens vs. Predator 2 (and demo), Blood 2, Shogo, KISS: Psycho Circus
- [x] Heretic 2, Gore, Drakan, Turok 2
- [ ] Daikatana, Sin, Descent 3
- [x] Global Operations
- [ ] Giants: Citizen Kabuto, Battlezone II, Codename Eagle, Codename: Outbreak
- [x] Vietcong
- [ ] Operation Flashpoint, Operation Flashpoint: Resistance
- [x] ST: Bridge Commander
- [ ] Renegade (and demo), Terminus, Laser Arena
- [ ] IL-2 Sturmovik, IL-2 Sturmovik: Forgotten Battles, Red Ace Squadron
- [ ] Baldur's Gate, Baldur's Gate 2, Baldur's Gate 2: Throne of Bhaal
- [ ] Deer Hunter 3, 4 and 5, Trophy Hunter 3, Wild Wings
- [ ] Rally Masters, RalliSport Challenge
- [x] Counter-Strike, Team Fortress Classic, Day of Defeat (Steam)
- [x] Half-Life, Condition Zero, Opposing Force, Deathmatch Classic, Ricochet (Steam)
- [ ] Counter-Strike, Day of Defeat and Half-Life retail (non-Steam), Gunman Chronicles
- [x] Counter-Strike: Source, Day of Defeat: Source, Half-Life 2: Deathmatch
- [ ] Other Source mods

### Quake 1 and 2 family (small variants of the Quake protocol)

- [x] QuakeWorld
- [x] Quake 2
- [ ] QFusion (no servers left on dpmaster)
- [x] Kingpin
- [x] Soldier of Fortune (GameSpy v1: 333networks, errorist.eu, OpenSpy)

### Doom 3 engine

- [x] DOOM 3, Quake 4

### Unreal Engine 2

- [x] Unreal Tournament 2003 (and demo)
- [x] Unreal Tournament 2004 (demo and old versions included)

### The ASE protocol itself

These games answered ASE's own query, so OpenASE can speak it too.

- [x] Multi Theft Auto (San Andreas)
- [ ] GTA3:MTA
- [x] Battlefield 1942 (GameSpy v1 via OpenSpy), Battlefield Vietnam (GameSpy v2)
- [ ] Battlefield 1942 demos, Battlefield Vietnam ASE SDK test
- [ ] Far Cry, Painkiller, Chrome, Devastation, Chaser (and demo)
- [ ] Soldat, Scorched3D, IGI2 (and demo), Purge Jihad (and demo)
- [ ] Splinter Cell: Pandora Tomorrow, Chaos Theory Coop and Versus
- [ ] Xpand Rally (and demo), Cross Racing Championship 2005 (and demo), Scrapland (and demo)
- [ ] Sacred Underworld, Soldiers: Heroes of WWII, Universal Combat, Battlecruiser Millennium Gold
- [ ] Novawar, New World Order, DzWords, SDK test

### GameSpy v2 and v3 (OpenSpy, BF2Hub revivals)

- [x] Battlefield 2 (and demo), Battlefield Vietnam
- [ ] Medal of Honor: Pacific Assault (and demo)
- [x] SWAT 4, Tribes: Vengeance (and demo), FEAR (and demo)
- [x] Halo (and Custom Edition), Star Wars: Battlefront
- [ ] America's Army, Neverwinter Nights (no servers on OpenSpy)

### Their own protocols

- [x] Tribes, Tribes 2
- [x] Savage (the demo is not added)
- [ ] Razor Unit (and demo)
- [ ] Rogue Spear, Urban Ops, Black Thorn (no live server and an undocumented protocol)
- [x] Raven Shield
- [x] Ghost Recon (and demo), Sum of All Fears (and demo): queries only, no master left
- [ ] Freelancer

### DirectPlay lobby games (LAN and direct IP)

These used Microsoft DirectPlay lobbies, which no longer exist. The goal is LAN
discovery and launching with a direct IP.

- [ ] Age of Empires II, Age of Empires II: The Conquerors
- [ ] Age of Wonders: Shadow Magic, Cossacks, Hidden & Dangerous
- [ ] Moto GP 2, Praetorians (and demo), Real War, Real War: Rogue States
- [ ] Rise of Nations

## Not done yet

- [ ] Steam server list checked with a real API key (code is unit-tested only)
- [ ] Join commands of many newer games tested with the games installed ([issue #1](https://github.com/tsautier/OpenASE/issues/1))

## v1.0 — Polished release

- [ ] Signed Windows installer and update check
- [ ] Flatpak on Flathub
- [x] English and French interface
- [x] User guide on the website, custom domain (https://openase.org)

## Community

- [x] `CONTRIBUTING.md`, issue templates
- [x] "good first issue" labels
- [ ] Announce on retro FPS communities (ioquake3, OpenArena, Urban Terror, 333networks)
