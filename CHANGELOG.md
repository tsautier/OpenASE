# Changelog

All notable changes to OpenASE. Downloads for every version are on the
[releases page](https://github.com/tsautier/OpenASE/releases).

## 0.9.3 — 2026-11-08

### Changes
- **One request for all 333networks games**: the 16 games listed by
  master.333networks.com (UT99, Unreal, Deus Ex, Rune, MOHAA, Soldier of Fortune...) now
  get their lists with a single `\sync\` request, cached for 5 minutes, instead of one
  `\list\` request per game. OpenASE identifies itself with `\msid\OpenASE\`. Suggested by
  333networks.

## 0.9.2 — 2026-11-08

### Fixes
- **Unreal engine servers added with their game port** (UT99, Unreal, Deus Ex, Rune,
  Wheel of Time, Mobile Forces, UT2003, UT2004, SWAT 4, Tribes: Vengeance) are now queried
  on their query port. Players usually share the game port (for example `ip:7777`), and
  querying it made the server open and drop a dead connection on every refresh. Add server
  and list imports check port + 1 once and keep it when that server reports the given port
  as its game port. Favorites saved with a game port are moved once, at the next start.
  Thanks to 333networks for spotting and helping debug this.

## 0.9.1 — 2026-11-08

### New
- **Windows installer**: `OpenASE-v0.9.1-windows-x64-setup.exe`, built with Inno Setup.
  Installs per user by default (no administrator prompt), adds a Start menu entry and an
  optional desktop shortcut, upgrades older installs in place, and keeps your settings when
  uninstalled. English and French. The portable zip is still published.
- **SHA256SUMS.txt** is attached to every release with the checksum of each package.
- Package manager manifests (winget, Scoop) can now be generated from `packaging/`.

### Changes
- The website moved to **https://openase.org**. The Help menu and About window link there.
- **Friend sounds on Linux and macOS**: .wav files and the default notification sound now
  play there too, with the system's own player (pw-play, paplay or aplay on Linux, afplay
  on macOS).

### Known issues
- The installer and the program are not code-signed yet, so Windows SmartScreen may warn
  before running them ("More info" > "Run anyway").

## 0.9.0 — 2026-11-08

70 games (up from 59) and 15 query protocols (up from 9).

### New games
- **Starsiege: Tribes** and **Tribes 2**: their own binary protocols. Lists come from
  the community masters (tribes1.co, pu.net) and TribesNext.
- **DOOM 3** and **Quake 4**: id Tech 4 protocol and the id masters.
- **Soldier of Fortune**: GameSpy query, lists from 333networks, errorist and OpenSpy.
- **Savage: The Battle for Newerth**: its own protocol, list from Savage DRX.
- **Rainbow Six 3: Raven Shield**: its own protocol, list from OpenRVS.
- **Ghost Recon** and **The Sum of All Fears** (and their demos): query only. No master
  is left, so add servers by hand or find them on the LAN.

### New features
- **English and French interface**: Options > General > Language. "System" follows
  the language of your operating system.
- **Customizable toolbar**: show, hide and reorder the buttons in Options > Toolbar,
  or right-click the toolbar.
- **A sound per friend**: in the Friends window, each friend can have the default
  sound, no sound, or a .wav file (.wav playback is Windows only).
- **Colors tab** in Options: list background, alternating rows, selection, text, and
  Quake color codes on or off.

### Known issues
- The Join command of many new games is not tested with the game installed yet.
  Help is welcome: [issue #1](https://github.com/tsautier/OpenASE/issues/1).
- Rogue Spear, Urban Ops and Black Thorn are not supported: no live server and no
  documented protocol.

## 0.8.1 — 2026-11-08

### Fixes
- Server and player names sent as UTF-8 or as Russian Windows-1251 now display
  correctly, instead of garbled text such as "CafÃ©" or "Ðóññêèé".
- The "Update available" window and other dialogs are no longer stretched with
  empty space.

## 0.8.0 — 2026-11-08

Back to the full ASE list: 59 games (up from 14) and 9 query protocols.

### New games
- **Steam**: Half-Life, Condition Zero, Opposing Force, Deathmatch Classic, Ricochet,
  Counter-Strike: Source, Day of Defeat: Source, Half-Life 2: Deathmatch.
- **Quake 3 engine**: Call of Duty: United Offensive, Star Trek: Elite Force,
  Soldier of Fortune II (community masters).
- **Quake 2 and QuakeWorld protocols**: QuakeWorld, Quake 2, Kingpin.
- **20 GameSpy v1 games** from 333networks and OpenSpy: Unreal, Deus Ex, Rune,
  Wheel of Time, Mobile Forces, Serious Sam 1 and 2, No One Lives Forever 1 and 2,
  Heretic II, Gore, Drakan, Turok 2, Global Operations, Vietcong, Bridge Commander,
  Medal of Honor: Allied Assault, Spearhead and Breakthrough, Battlefield 1942.
- **Unreal Engine 2 protocol and master**: Unreal Tournament 2003 and 2004.
- **The ASE protocol** and the Multi Theft Auto: San Andreas master.
- **GameSpy v2/v3 queries and the encrypted GameSpy master**: Battlefield 2,
  Battlefield Vietnam, Halo and Halo Custom Edition, SWAT 4, Tribes: Vengeance,
  F.E.A.R., Star Wars: Battlefront.

### Fixes
- Packet loss no longer counts servers that rate-limit queries (MTA) or ignore the
  short ping request (Elite Force, NOLF 2).

## 0.7.0 — 2026-11-07

### New games
- Star Wars Jedi Knight II: Jedi Outcast, Jedi Academy, Return to Castle Wolfenstein.
- Team Fortress Classic and Day of Defeat (Steam list).

### New features
- Linux AppImage and macOS builds next to Windows, smaller release binaries.
- Auto-Search finds native Linux and macOS game binaries.
- `CONTRIBUTING.md` with a guide to adding a game, issue and pull request templates.

### Removed
- The Python prototype. It remains in the git history.

## 0.6.0 — 2026-11-07

First public release: 9 games (Quake 3, Urban Terror, OpenArena, Wolfenstein: Enemy
Territory, Call of Duty 1, 2 and 4, Unreal Tournament, Counter-Strike 1.6) and the
ASE experience:

- Quake 3, GameSpy and Valve A2S query protocols, several masters per game.
- Ping, Flux and Packet Loss from several samples per server.
- The ASE filter tree with game types, mods, maps and per-game settings.
- Favorites1..8, Friends1..4, Special lists, LAN servers, custom and color filters.
- Join with double-click or Enter, Find player, friends tracking with a notice.
- Location column and country filter (DB-IP GeoIP, monthly download).
- Profile export/import, server list save/import, update check, first-run wizard.
- Windows build, website with manual.
