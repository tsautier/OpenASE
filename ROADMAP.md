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

## v0.8 — Back to the full ASE list (in progress)

- [ ] Steam games: Half-Life family and Source games
- [ ] Call of Duty: United Offensive, Elite Force, Medal of Honor
- [ ] Quake 1 and 2 protocols: QuakeWorld, Quake 2, Kingpin, Soldier of Fortune
- [ ] GameSpy v1 games still listed by 333networks and OpenSpy
- [ ] Unreal Tournament 2003 and 2004
- [ ] The ASE query protocol (Multi Theft Auto and friends)
- [ ] GameSpy v2 and v3 games revived by OpenSpy and BF2Hub

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
- [ ] Call of Duty: United Offensive
- [x] Return to Castle Wolfenstein
- [x] Wolfenstein: Enemy Territory (and the ET test release)
- [x] Jedi Knight II, Jedi Academy
- [ ] ST: Elite Force, Star Trek: Elite Force 2
- [ ] Soldier of Fortune 2 (and SoF2 MP Test)
- [ ] Medal Of Honor (Allied Assault), Spearhead, Breakthrough (and demos)
- [x] Unreal Tournament
- [ ] Unreal, Tactical Ops, Unreal 2 XMP (and demo)
- [ ] Deus Ex, Rune, Nerf ArenaBlast, Wheel of Time, Mobile Forces
- [ ] Serious Sam, Serious Sam: The Second Encounter (and demo)
- [ ] No One Lives Forever, No One Lives Forever 2 (and demo)
- [ ] Aliens vs. Predator 2 (and demo), Blood 2, Shogo, KISS: Psycho Circus
- [ ] Daikatana, Heretic 2, Sin, Gore, Drakan, Turok 2, Descent 3
- [ ] Giants: Citizen Kabuto, Battlezone II, Global Operations, Codename Eagle, Codename: Outbreak
- [ ] Operation Flashpoint, Operation Flashpoint: Resistance, Vietcong
- [ ] Renegade (and demo), ST: Bridge Commander, Terminus, Laser Arena
- [ ] IL-2 Sturmovik, IL-2 Sturmovik: Forgotten Battles, Red Ace Squadron
- [ ] Baldur's Gate, Baldur's Gate 2, Baldur's Gate 2: Throne of Bhaal
- [ ] Deer Hunter 3, 4 and 5, Trophy Hunter 3, Wild Wings
- [ ] Rally Masters, RalliSport Challenge
- [x] Counter-Strike, Team Fortress Classic, Day of Defeat (Steam)
- [ ] Half-Life, Condition Zero, Opposing Force, Deathmatch Classic, Ricochet (Steam)
- [ ] Counter-Strike, Day of Defeat and Half-Life retail (non-Steam), Gunman Chronicles
- [ ] Counter-Strike: Source, Day of Defeat: Source, Half-Life 2: Deathmatch, Source mods

### Quake 1 and 2 family (small variants of the Quake protocol)

- [ ] QuakeWorld
- [ ] Quake 2, QFusion
- [ ] Kingpin
- [ ] Soldier of Fortune

### Doom 3 engine

- [ ] DOOM 3, Quake 4

### Unreal Engine 2

- [ ] Unreal Tournament 2003 (and demo)
- [ ] Unreal Tournament 2004 (demo and old versions included)

### The ASE protocol itself

These games answered ASE's own query, so OpenASE can speak it too.

- [ ] Multi Theft Auto, GTA3:MTA
- [ ] Battlefield 1942 (demo and Secret Weapons demo included), Battlefield Vietnam (and ASE SDK test)
- [ ] Far Cry, Painkiller, Chrome, Devastation, Chaser (and demo)
- [ ] Soldat, Scorched3D, IGI2 (and demo), Purge Jihad (and demo)
- [ ] Splinter Cell: Pandora Tomorrow, Chaos Theory Coop and Versus
- [ ] Xpand Rally (and demo), Cross Racing Championship 2005 (and demo), Scrapland (and demo)
- [ ] Sacred Underworld, Soldiers: Heroes of WWII, Universal Combat, Battlecruiser Millennium Gold
- [ ] Novawar, New World Order, DzWords, SDK test

### GameSpy v2 and v3 (OpenSpy, BF2Hub revivals)

- [ ] Battlefield 2 (and demo), Battlefield Vietnam
- [ ] Medal of Honor: Pacific Assault (and demo)
- [ ] SWAT 4, Tribes: Vengeance (and demo), FEAR (and demo)
- [ ] Halo, Star Wars: Battlefront, America's Army, Neverwinter Nights

### Their own protocols

- [ ] Tribes, Tribes 2
- [ ] Savage (and demo)
- [ ] Razor Unit (and demo)
- [ ] Rogue Spear, Urban Ops, Black Thorn, Raven Shield
- [ ] Ghost Recon (and demo), Sum of All Fears (and demo)
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
- [ ] Customizable toolbar, per-friend sounds, Colors options tab

## v1.0 — Polished release

- [ ] Signed Windows installer and update check
- [ ] Flatpak on Flathub
- [ ] English and French interface
- [ ] User guide on the website, custom domain

## Community

- [x] `CONTRIBUTING.md`, issue templates
- [ ] "good first issue" labels
- [ ] Announce on retro FPS communities (ioquake3, OpenArena, Urban Terror, 333networks)
