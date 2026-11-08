# Contributing to OpenASE

Thanks for helping out. Bug reports, new games and small fixes are all welcome.
If you are not sure where to start, open an issue and ask.

## Quick start

You need Rust 1.88 or newer. On Linux, install the GUI libraries first
(Debian / Ubuntu):

```sh
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libgtk-3-dev
```

Then:

```sh
cd rust
cargo run --release
```

## Project layout

Everything lives in `rust/`. The library is in `rust/src`:

| File | What it does |
|---|---|
| `main.rs` | Starts the egui app. |
| `lib.rs` | Declares the library modules (the GUI is only in the binary). |
| `catalog.rs` | Loads `games.json` and defines `Game`, `Protocol` and `Master`. |
| `games.json` | The built-in game catalog. Adding a game mostly means editing this file. |
| `query.rs` | Picks the right protocol, fetches master lists, measures Ping, Flux and PL. |
| `quake3.rs` | Quake 3 family: master `getservers` and server `getstatus`. |
| `gamespy.rs` | GameSpy master and status queries (Unreal Tournament 99). |
| `a2s.rs` | Valve A2S queries (Counter-Strike 1.6). |
| `steam.rs` | Server lists from the Steam Web API. |
| `model.rs` | Protocol-independent `Server` and player types. |
| `colors.rs` | Quake color codes (`^1`, `^xRRGGBB`) and hex colors. |
| `rules.rs` | User-defined filter rules ("My filters"). |
| `launch.rs` | Expands the `join` template and starts the game. |
| `detect.rs` | Finds installed games (registry, then disk search). |
| `lan.rs` | LAN server discovery by UDP broadcast. |
| `geoip.rs` | DB-IP country database download and lookup. |
| `location.rs` | Location helpers for server rows. |
| `countries.rs` | ISO country code to name table. |
| `settings.rs` | User settings, favorites and friends, stored as JSON. |
| `update.rs` | Update check against GitHub Releases. |
| `ase_filters.rs` | Parser for the original ASE `filters.txt` format (we never ship such a file). |
| `gui/` | The egui interface (`mod.rs`, `dialogs.rs`, `extra.rs`, `icons.rs`). |

Other folders: `rust/tests` (offline tests), `rust/examples` (command-line
tools, see below), `docs/` (the website) and `.github/` (CI and templates).

## Before you open a pull request

Run these from `rust/`:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the tests on Windows, Linux and macOS, and fmt and clippy on Windows.
Tests must work offline: use captured packets or hand-written samples, never a
live server.

Keep pull requests small and focused, and say what you tested.

## Adding a game

Most games are one new entry in [`rust/src/games.json`](rust/src/games.json),
a JSON array of games in display order. No Rust code is needed if the game uses
one of the supported protocols.

### Fields of an entry

Here is the Quake 3: Arena entry, shortened (open the file to see all of it):

```json
{
  "id": "q3",
  "name": "Quake 3: Arena",
  "short": "Q3",
  "color": "#c8321e",
  "protocol": "quake3",
  "default_port": 27960,
  "masters": [
    { "kind": "quake3", "address": "master.quake3arena.com:27950", "query": "getservers 68 empty full" }
  ],
  "game_type_key": "g_gametype",
  "game_types": [
    { "value": "0", "name": "FFA" },
    { "value": "4", "name": "CTF" }
  ],
  "mod_key": "gamename",
  "base_mod": "baseq3",
  "filters": [
    { "name": "Mods", "key": "gamename", "dynamic": true },
    { "name": "Pure", "key": "sv_pure", "options": [ { "label": "Yes", "values": ["1"] }, { "label": "No", "values": ["0"] } ] }
  ],
  "registry": [
    { "key": "HKEY_LOCAL_MACHINE\\SOFTWARE\\Id\\Quake III Arena", "value": "INSTALLPATH", "exe": "quake3.exe" }
  ],
  "exe_names": ["quake3.exe", "ioquake3.x86_64", "ioquake3"],
  "join": "[+set fs_game {mod} ][+exec \"{config}\" ][+name \"{name}\" ][+password \"{password}\" ]+connect {address}"
}
```

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Short unique id, lowercase. Used on the command line (`scan -- q3`) and in saved settings, so do not rename it later. |
| `name` | yes | Full name shown in the filter tree. |
| `short` | yes | Short badge text in the server list, like `Q3`. |
| `color` | yes | Badge color as `#rrggbb`. A test checks that it parses. |
| `protocol` | yes | How servers are queried: `quake3`, `quake2`, `quakeworld`, `gamespy`, `gamespy2`, `gamespy3`, `a2s`, `unreal2` or `ase`. |
| `default_port` | yes | Port used when an address has none. |
| `steam_appid` | no | Steam app id (see `cs16`, which uses `10`). |
| `masters` | yes | At least one master server, see below. Several masters are queried in parallel and merged. |
| `game_type_key` | no | Server rule that holds the game type, such as `g_gametype`. |
| `game_types` | no | Maps raw values to names (`"4"` to `CTF`). Unknown values are shown as they are. |
| `mod_key` | no | Server rule that holds the mod name (`gamename` for Quake 3, `folder` for CS 1.6). |
| `base_mod` | no | Mod name of the unmodified game. It is not passed as `fs_game` when joining. |
| `filters` | no | Checkbox groups in the filter tree. `"dynamic": true` builds the options from values seen on scanned servers. Otherwise list `options`, each with a `label` and the raw `values` it matches (Pure above, or VAC in `cs16`). |
| `registry` | no | Windows registry places to find the install folder: the full `key`, the `value` name holding the folder, and `exe` relative to that folder. |
| `exe_names` | no | Executable names to look for when searching disks (Auto-Search). Include Linux and macOS names. |
| `query_port_offset` | no | Added to the join port to get the query port. Default `0`. Multi Theft Auto uses `123`: the address in the list is the port players join on, the server is queried 123 higher. |
| `ping_request` | no | GameSpy (v1) request used for extra ping samples, `\info\` when empty. Some servers only answer `\status\` (SWAT 4 uses it). |
| `join` | yes | Command-line template. Must contain `{address}`. |

The `join` template can use `{address}`, `{ip}`, `{port}`, `{name}`,
`{password}`, `{config}` and `{mod}`. `{address}` is `host:port`; `{ip}` and
`{port}` are the host and the join port on their own, for games that want
them as separate arguments. A part in `[...]` is dropped when any placeholder inside it is empty,
so `[+password "{password}" ]` disappears when there is no password. The
Unreal Tournament entry (`ut99`) shows a template that is not a command-line
flag style: `{address}[?Name={name}][?Password={password}]`.

### Protocols and master kinds

| `protocol` | Used for | Master `kind` | Master fields |
|---|---|---|---|
| `quake3` | Quake 3, Urban Terror, OpenArena, ET, Call of Duty | `quake3` | `address` (`host:port`), `query` (raw request, usually `getservers <protocol> empty full`) |
| `quake2` | Quake 2, Kingpin | `quake2` | `address` (`master.quakeservers.net:27900`), `query` (`query`) |
| `quakeworld` | QuakeWorld | `quakeworld` | `address` (`master.quakeworld.nu:27000`) |
| `gamespy` | Unreal Tournament 99, Serious Sam, MoH:AA | `gamespy` | `address`, `gamename`, `key` (the game's public GameSpy key) |
| `gamespy`, `gamespy2`, `gamespy3` | SWAT 4 (v1), Halo, F.E.A.R. (v2), Battlefield 2 (v3) | `gamespy_sb` | `address` of a server-browsing master (`battlefield2.master.openspy.net:28910`), `gamename`, `key` |
| `a2s` | Counter-Strike 1.6, Source games | `steam_web` | `filter` (Steam server list filter, for example `\appid\10`) |
| `unreal2` | Unreal Tournament 2003 / 2004 | `unreal2` | `address` (`utmaster.openspy.net:28902`), `client` (`UT2K4CLIENT`), `version` (`3369`) |
| `ase` | Multi Theft Auto | `mta` | `url` (HTTP master list, `https://master.multitheftauto.com/ase/mta/`) |

The `protocol` says how a server is queried, the master `kind` how the list is
fetched: the two are independent (SWAT 4 is queried with `gamespy` but listed
by a `gamespy_sb` master). Copy an existing entry of the same protocol.

A game that speaks none of these protocols needs new Rust code first: a module
like `quake3.rs`, a variant in `Protocol` and `Master` in `catalog.rs`, and a
branch in `query.rs`. Please open an issue before starting that.

### Test it live

From `rust/`:

```sh
cargo run --example scan -- list              # list game ids
cargo run --example scan -- <id> 20           # ask the masters, query 20 servers
cargo run --example query -- <id> host:port   # query one server, print everything
```

For Steam games, set the `STEAM_API_KEY` environment variable first. A good
result shows a server count from the master, then sensible names, maps, player
counts and ping. After that, run `cargo run --release`, check that the game
shows up in the filter tree, and try a join if you have the game installed.

### Add a test

Offline tests live in `rust/tests/protocols.rs`. When you add a game:

- add its id to the list in `catalog_has_every_game_with_masters_and_join_template`
  (that test checks the exact list of ids, so it fails until you do);
- if you added game types, add an assertion to `game_type_names_are_mapped`;
- if you added or changed a protocol parser, add a test with a captured packet.

## Legal rule

OpenASE is new code. Never commit anything from the original, proprietary
All-Seeing Eye: no `filters.txt`, `countries.txt`, executables or `.dat` files.
Do not commit copyrighted assets either (game logos, art, sounds, screenshots
of game art).

Facts are fine: registry key names, executable names, default ports, public
protocol descriptions and master server addresses. If you are unsure, ask in an
issue first.

## License

OpenASE is licensed under GPL-3.0. By sending a contribution you agree that it
is licensed under the same terms. See [LICENSE](LICENSE).
