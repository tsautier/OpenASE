# OpenASE

Website: https://openase.org/

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
| Call of Duty, CoD: United Offensive, CoD 2, CoD 4 | Quake 3 | Activision masters |
| Star Wars Jedi Knight II: Jedi Outcast | Quake 3 | master.jkhub.org, master.jk2mv.org |
| Star Wars Jedi Knight: Jedi Academy | Quake 3 | master.jkhub.org, master.jk2mv.org |
| Return to Castle Wolfenstein | Quake 3 | wolfmaster.idsoftware.com |
| Star Trek: Voyager Elite Force | Quake 3 | master.stvef.org, efmaster.tjps.eu |
| Soldier of Fortune II: Double Helix | Quake 3 | master.1fxmod.org |
| DOOM 3 | Doom 3 | idnet.ua-corp.com |
| Quake 4 | Doom 3 | q4master.idsoftware.com |
| QuakeWorld | QuakeWorld | master.quakeservers.net, qwmaster.fodquake.net, master.quakeworld.nu |
| Quake 2 | Quake 2 | master.quakeservers.net, master.itbacon.com |
| Kingpin | Quake 2 | master.kingpin.info, master4.kingpin.info |
| Soldier of Fortune | GameSpy | 333networks, errorist.eu, OpenSpy |
| Unreal Tournament 2004, Unreal Tournament 2003 | Unreal Engine 2 | 333networks, OpenSpy |
| Unreal Tournament (99), Unreal, Rune | GameSpy | 333networks, OpenSpy |
| Deus Ex, Wheel of Time, Mobile Forces, Drakan, Turok 2 | GameSpy | 333networks |
| Serious Sam: The First / Second Encounter, Heretic II, Global Operations, Medal of Honor: Allied Assault (+ Spearhead, Breakthrough) | GameSpy | 333networks, OpenSpy |
| No One Lives Forever 1 and 2, Gore, Vietcong, Star Trek: Bridge Commander, Battlefield 1942 | GameSpy | OpenSpy |
| Battlefield 2 | GameSpy v3 | OpenSpy, battlefield2.ms.playbf2.ru |
| Battlefield Vietnam, F.E.A.R., Star Wars: Battlefront, Tribes: Vengeance | GameSpy v2 | OpenSpy |
| Halo: Combat Evolved, Halo: Custom Edition | GameSpy v2 | s1.master.hosthpc.com |
| SWAT 4 | GameSpy | master.swat4stats.com, OpenSpy |
| Multi Theft Auto: San Andreas | ASE (MTA) | master.multitheftauto.com |
| Starsiege: Tribes | Tribes | t1m1.tribes1.co, t1m1.pu.net |
| Tribes 2 | Tribes 2 | TribesNext (master.tribesnext.com) |
| Savage: The Battle for Newerth | Savage | Savage DRX server list |
| Rainbow Six 3: Raven Shield | Raven Shield | OpenRVS server list |
| Ghost Recon (+ demo), The Sum of All Fears (+ demo) | Ghost Recon | none: Add server and LAN only |
| Counter-Strike 1.6, Condition Zero, Team Fortress Classic, Day of Defeat, Half-Life, Opposing Force, Deathmatch Classic, Ricochet | Valve A2S | Steam Web API (needs your own key) |
| Counter-Strike: Source, Day of Defeat: Source, Half-Life 2: Deathmatch | Valve A2S | Steam Web API (needs your own key) |

70 games in total, spoken in 15 query protocols and listed by 46 master
servers. "OpenSpy" means the OpenSpy master servers
(<https://github.com/openspy>), "333networks" the 333networks masters.
Ghost Recon and The Sum of All Fears have no master server left: their
servers work through Add server and LAN broadcast. Rogue Spear, Urban Ops and
Black Thorn are not in yet (no live server, undocumented protocol). Games from
the original All-Seeing Eye that are not in the catalog yet are listed in
[ROADMAP.md](ROADMAP.md).

The Join command of many of the newer games has not been tested with the game
installed, see [issue #1](https://github.com/tsautier/OpenASE/issues/1).

Missing a game? See [CONTRIBUTING.md](CONTRIBUTING.md#adding-a-game): most
games are a single entry in the catalog.

Games are described in [`rust/src/games.json`](rust/src/games.json): masters,
game types, filter groups, registry detection and the command line used to join.

## Features

- **Ping, Flux and PL** measured like ASE: several queries per server, average
  round-trip time, half the spread between fastest and slowest, and packet loss.
- **Filter tree**: game types as folders, checkbox groups per game (Mods,
  Friendly Fire, Kill Cam, Pure, PunkBuster, Version, Maps) and **Modifiers**
  (not empty, not full, no password, unresponsive, ping, **country**).
- **Location** column and country filter from the free DB-IP country database,
  downloaded once a month (can be turned off in Options > General).
- **My filters**: build rules such as `players greater than 4` or
  `map contains toujane`; tick *color filter* to highlight matching servers.
- **Favorites1 to 8**, **Special** lists (recently added, imported, joined)
  and **LAN servers** found by broadcast.
- **Join** with a double-click or Enter, with the right mod, player name, config
  and password. Steam games fall back to `steam://connect`.
- **Friends** in groups Friends1 to 4, optionally per game, with a Friends pane,
  notices and sound. Each friend can have its own sound (default, none or a
  `.wav` file; `.wav` playback is Windows-only). **Friend name search** across
  every scanned server.
- **File menu**: export / import your whole profile, save / import server lists,
  update check against GitHub Releases, setup wizard.
- **English and French interface**: Options > General > Language. "System"
  follows the language of your operating system.
- **Customizable toolbar**: choose the buttons in Options > Toolbar, or
  right-click the toolbar.
- **Colors**: Options > Colors sets the list background, alternating rows,
  selection and text colors, and turns Quake color codes on or off.
- **Options**: General (language, Steam key, GeoIP), Network (timeouts, pings,
  retries, parallel queries), Events, Games (installed / not installed,
  **Auto-Search**), Toolbar, Colors.
- Server names sent as UTF-8 or Windows-1251 (Cyrillic) display correctly.
- Shortcuts: F5 refresh, Enter join, Ctrl+E show selected, Ctrl+D clear
  modifiers, Ctrl+Q collapse all, F1 manual.

## Download

Every [GitHub release](https://github.com/tsautier/OpenASE/releases/latest) ships three packages:

| System | Package | How to run |
| --- | --- | --- |
| Windows 10/11 (x64) | `OpenASE-<version>-windows-x64.zip` | Unzip, run `OpenASE.exe`. |
| Linux (x64) | `OpenASE-<version>-linux-x86_64.AppImage` or `OpenASE-<version>-linux-x64.tar.gz` | AppImage: `chmod +x` it and run it, on any distribution. Archive: extract, run `./openase`; an `openase.desktop` file and icon are included. |
| macOS 11+ (Apple Silicon and Intel) | `OpenASE-<version>-macos-universal.zip` | Unzip, move `OpenASE.app` to Applications. The app is not notarized: the first time, right-click it and choose **Open**, or run `xattr -dr com.apple.quarantine /Applications/OpenASE.app`. |

The games themselves still need to run on your system: native Linux and macOS
builds exist for ioquake3, Urban Terror, OpenArena, ET: Legacy, Jedi Outcast
(JK2MV), Jedi Academy (OpenJK), RtCW (iortcw), Unreal Tournament (OldUnreal
patch) and the Half-Life and Source games (Steam). More ports: Quake 2 (Yamagi
Quake II, Q2PRO), QuakeWorld (ezQuake, FTE), Serious Sam (SamTFE and SamTSE
ports) and Unreal Tournament 2004, which has a native Linux client. Call of
Duty, Multi Theft Auto, Battlefield 2, Halo and SWAT 4 run on Windows only, or
through Wine / Proton.

## Build from source (Rust)

Requires Rust 1.88 or newer. On Linux, install the GUI libraries first
(Debian / Ubuntu: `sudo apt install libxcb-render0-dev libxcb-shape0-dev
libxcb-xfixes0-dev libxkbcommon-dev libgtk-3-dev`).

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
(`~/.config/OpenASE` on Linux, `~/Library/Application Support/OpenASE` on macOS).

### Steam games

This covers every Half-Life and Source game in the catalog: Counter-Strike 1.6,
Team Fortress Classic, Day of Defeat, Half-Life, Condition Zero, Opposing
Force, Deathmatch Classic, Ricochet, Counter-Strike: Source, Day of Defeat:
Source and Half-Life 2: Deathmatch. Valve's old UDP master no longer answers,
so the list comes from the Steam Web API. Get a free key at https://steamcommunity.com/dev/apikey and paste it in
Options > General. Servers added by hand work without a key.

## References

- Quake 3 connectionless queries: Jacek Fedorynski, "Quake 3 Network Protocol"
  (2020), https://www.jfedor.org/quake3/
- Valve server queries (A2S): Valve Developer Wiki, "Server queries".
- GameSpy master protocol: 333networks, https://333networks.com/
- GameSpy v2 / v3 and server browsing (Battlefield, Halo, F.E.A.R., SWAT 4):
  OpenSpy, https://github.com/openspy
- Unreal Engine 2 master (UT2003, UT2004): 333networks and OpenSpy, as above.
- Multi Theft Auto master list: mtasa-blue source,
  https://github.com/multitheftauto/mtasa-blue
- Quake 2 and QuakeWorld protocols: connectionless queries as in the id
  Software sources, with the community masters at QuakeServers.net.
- Starsiege: Tribes and Tribes 2 queries, and the Tribes 2 master list:
  TribesNext, http://www.tribesnext.com/ and the qstat protocol descriptions
  (https://github.com/multiplay/qstat).
- DOOM 3 and Quake 4 `getInfo` / `getServers` (id Tech 4): the dhewm3 modwiki
  and the qstat descriptions.
- Savage: The Battle for Newerth: query as described by qstat, server list from
  Savage DRX, https://savagedrx.com/
- Raven Shield: beacon protocol as described by qstat, server list from the
  OpenRVS project, https://openrvs.org/
- Ghost Recon and The Sum of All Fears: the Red Storm status query, as
  described by qstat.
- Yahoo acquired The All-Seeing Eye in September 2004:
  https://www.gamedeveloper.com/game-platforms/yahoo-games-adds-all-seeing-eye-gamespot-syndication

## License

Copyright (C) 2026 Thomas Sautier

OpenASE is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See [LICENSE](LICENSE) for the full text.

OpenASE is not affiliated with Yahoo or the original All-Seeing Eye authors.
Game names are trademarks of their owners.

IP geolocation by [DB-IP](https://db-ip.com) (CC BY 4.0), downloaded at runtime. No original ASE files are distributed
with this project.
