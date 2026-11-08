//! Desktop GUI, laid out like The All-Seeing Eye: filter tree on the left,
//! server list on the right, friends / players / rules panes at the bottom.

mod dialogs;
mod extra;
mod icons;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, RichText, Sense, Stroke, TextFormat,
    TextStyle, text::LayoutJob, vec2,
};
use egui_extras::{Column, TableBuilder};

use openase::catalog::{self, Game, Protocol};
use openase::colors::{parse_hex, quake_color_segments, strip_quake_colors};
use openase::countries::country_name;
use openase::detect;
use openase::geoip::{self, GeoIp};
use openase::i18n::{self, fill, group_name, noop, tr};
use openase::model::Server;
use openase::quake3::parse_address;
use openase::query::{fetch_all_masters, query_server};
use openase::settings::{
    ColorSettings, FAVORITE_GROUPS, FRIEND_GROUPS, FriendSound, Rgb, ServerRef, Settings,
};

use dialogs::Dialogs;
use icons::{Tool, TreeIcon};

const ROW_HEIGHT: f32 = 18.0;
const PING_LIMITS: [u32; 4] = [50, 100, 150, 250];

/// Identifies a server across games.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServerKey {
    pub game: String,
    pub address: String,
}

impl ServerKey {
    fn of(server: &Server) -> Self {
        Self {
            game: server.game.clone(),
            address: server.address(),
        }
    }

    fn to_ref(&self) -> ServerRef {
        ServerRef {
            game: self.game.clone(),
            address: self.address.clone(),
        }
    }

    fn from_ref(server: &ServerRef) -> Self {
        Self {
            game: server.game.clone(),
            address: server.address.clone(),
        }
    }
}

/// What the left tree has selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nav {
    Favorites,
    FavoriteGroup(String),
    Recent,
    Friends,
    FriendGroup(String),
    Special(Special),
    Custom(usize),
    Game(String),
    GameType(String, String),
}

/// ASE's "Special" folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    Added,
    Imported,
    Lan,
}

impl Special {
    const ALL: [Special; 3] = [Special::Added, Special::Imported, Special::Lan];

    fn label(self) -> &'static str {
        match self {
            Special::Added => tr("recently added"),
            Special::Imported => tr("recently imported"),
            Special::Lan => tr("LAN servers"),
        }
    }
}

impl Nav {
    fn game(&self) -> Option<&str> {
        match self {
            Nav::Game(game) | Nav::GameType(game, _) => Some(game),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Col {
    Icon,
    Location,
    Name,
    Ping,
    Flux,
    Loss,
    Players,
    GameType,
    Map,
    Address,
}

const COLUMNS: [(Col, &str, f32); 10] = [
    (Col::Icon, "", 44.0),
    (Col::Location, noop("Location"), 80.0),
    (Col::Name, noop("Name"), 300.0),
    (Col::Ping, noop("Ping"), 46.0),
    (Col::Flux, noop("Flux"), 40.0),
    (Col::Loss, noop("PL"), 38.0),
    (Col::Players, noop("Players"), 58.0),
    (Col::GameType, noop("Game"), 120.0),
    (Col::Map, noop("Map"), 120.0),
    (Col::Address, noop("Address"), 150.0),
];

/// Messages from worker threads.
pub enum Msg {
    MasterList {
        generation: u64,
        game: String,
        addresses: Vec<String>,
        errors: Vec<String>,
    },
    Server {
        generation: u64,
        server: Server,
    },
    ScanDone {
        generation: u64,
    },
    Single(Server),
    SearchProgress(String),
    SearchFound(String),
    SearchDone,
    GeoIp(Result<Arc<GeoIp>, String>),
    Lan(Result<Vec<openase::lan::LanServer>, String>),
    Update {
        result: Result<Option<String>, String>,
        manual: bool,
    },
    /// Saved servers to move from a game port to their query port.
    PortFixes(Vec<(ServerRef, ServerRef)>),
}

struct Scan {
    generation: u64,
    stop: Arc<AtomicBool>,
    done: Arc<AtomicUsize>,
    total: usize,
    label: String,
}

/// Row actions collected while drawing, applied afterwards.
enum Action {
    Select(ServerKey),
    Join(ServerKey),
    ToggleFavorite(ServerKey),
    Refresh(ServerKey),
    Copy(String),
    FavoriteGroup(ServerKey, String),
}

pub struct OpenAseApp {
    pub(crate) settings: Settings,
    settings_dirty: bool,
    pub(crate) tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub(crate) store: BTreeMap<String, Vec<Server>>,
    nav: Nav,
    pub(crate) selected: Option<ServerKey>,
    sort: (Col, bool),
    scan: Option<Scan>,
    generation: u64,
    pub(crate) status: String,
    friend_seen: HashMap<String, ServerKey>,
    single_in_flight: bool,
    last_auto_refresh: Instant,
    show_toolbar: bool,
    /// Colors currently applied (Options > Colors previews them live).
    palette: ColorSettings,
    started: bool,
    geoip: Option<Arc<GeoIp>>,
    pub(crate) geoip_status: String,
    lan_servers: Vec<ServerKey>,
    scroll_to_selected: bool,
    pub(crate) extra: extra::Extra,
    pub(crate) dialogs: Dialogs,
}

impl OpenAseApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let (tx, rx) = channel();
        let mut settings = Settings::load();
        i18n::set_language(settings.language);
        apply_palette(&cc.egui_ctx, &settings.colors);

        // Quick registry detection for games without a program location.
        for game in catalog::games() {
            let entry = settings.games.entry(game.id.clone()).or_default();
            if entry.program_location.is_empty()
                && let Some(path) = detect::detect_from_registry(game)
            {
                entry.program_location = path.display().to_string();
            }
        }
        if settings.open_nav.is_empty() {
            settings
                .open_nav
                .extend(["Modifiers".to_string(), "Modifiers/Players".to_string()]);
        }
        let first_game = catalog::game(&settings.last_game)
            .map(|game| game.id.clone())
            .unwrap_or_else(|| catalog::games()[0].id.clone());

        let palette = settings.colors.clone();
        Self {
            settings,
            settings_dirty: true,
            tx,
            rx,
            store: BTreeMap::new(),
            nav: Nav::Game(first_game),
            selected: None,
            sort: (Col::Ping, true),
            scan: None,
            generation: 0,
            status: tr("Ready").to_string(),
            friend_seen: HashMap::new(),
            single_in_flight: false,
            last_auto_refresh: Instant::now(),
            show_toolbar: true,
            palette,
            started: false,
            geoip: None,
            geoip_status: tr("Loading...").to_string(),
            lan_servers: Vec::new(),
            scroll_to_selected: false,
            extra: extra::Extra::default(),
            dialogs: Dialogs::default(),
        }
    }

    // ----- data ------------------------------------------------------------

    pub(crate) fn find(&self, key: &ServerKey) -> Option<&Server> {
        self.store
            .get(&key.game)?
            .iter()
            .find(|server| server.address() == key.address)
    }

    fn upsert(&mut self, mut server: Server) {
        annotate(self.geoip.as_deref(), &mut server);
        self.check_friends(&server);
        let list = self.store.entry(server.game.clone()).or_default();
        let address = server.address();
        match list
            .iter_mut()
            .find(|existing| existing.address() == address)
        {
            Some(existing) => *existing = server,
            None => list.push(server),
        }
    }

    /// Make sure a remembered server has at least an "unscanned" row.
    fn ensure_placeholder(&mut self, key: &ServerKey) {
        if self.find(key).is_some() {
            return;
        }
        let Some(game) = catalog::game(&key.game) else {
            return;
        };
        if let Ok((host, port)) = parse_address(&key.address, game.default_port) {
            let mut server = Server::new(&key.game, &host, port);
            annotate(self.geoip.as_deref(), &mut server);
            self.store.entry(key.game.clone()).or_default().push(server);
        }
    }

    fn check_friends(&mut self, server: &Server) {
        let key = ServerKey::of(server);
        for friend in &self.settings.friends {
            let needle = friend.to_lowercase();
            let game = self.settings.friend_game(friend);
            let present = server.online
                && (game.is_empty() || game == server.game)
                && server.players.iter().any(|player| {
                    strip_quake_colors(&player.name)
                        .to_lowercase()
                        .contains(&needle)
                });
            let previous = self.friend_seen.get(&needle).cloned();
            if present {
                if previous.as_ref() != Some(&key) {
                    self.friend_seen.insert(needle.clone(), key.clone());
                    if self.settings.events.notify_friend_joins {
                        self.dialogs.notify(fill(
                            tr("{} is playing on {}"),
                            &[friend, &strip_quake_colors(&server.name)],
                        ));
                    }
                    play_friend_sound(
                        &self.settings.friend_sound(friend),
                        self.settings.events.sound_friend_joins,
                    );
                }
            } else if previous.as_ref() == Some(&key) {
                self.friend_seen.remove(&needle);
            }
        }
    }

    pub(crate) fn friend_location(&self, friend: &str) -> Option<&ServerKey> {
        self.friend_seen.get(&friend.to_lowercase())
    }

    fn server_has_friend(&self, server: &Server) -> bool {
        let key = ServerKey::of(server);
        self.friend_seen.values().any(|seen| *seen == key)
    }

    // ----- scanning --------------------------------------------------------

    /// Remembered server lists behind list-type tree entries.
    fn nav_list(&self, nav: &Nav) -> Option<Vec<ServerKey>> {
        let refs = match nav {
            Nav::Favorites => self.settings.favorites.clone(),
            Nav::FavoriteGroup(group) => self.settings.favorites_in(group),
            Nav::Recent => self.settings.recent.clone(),
            Nav::Special(Special::Added) => self.settings.recently_added.clone(),
            Nav::Special(Special::Imported) => self.settings.recently_imported.clone(),
            Nav::Special(Special::Lan) => return Some(self.lan_servers.clone()),
            _ => return None,
        };
        Some(refs.iter().map(ServerKey::from_ref).collect())
    }

    fn known_servers(&self) -> Vec<ServerKey> {
        self.store
            .values()
            .flatten()
            .filter(|server| server.online)
            .map(ServerKey::of)
            .collect()
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        let nav = self.nav.clone();
        if let Some(keys) = self.nav_list(&nav) {
            if nav == Nav::Special(Special::Lan) {
                self.scan_lan(ctx);
            } else {
                self.scan_list(ctx, keys, noop("servers"));
            }
            return;
        }
        match nav {
            Nav::Game(game) | Nav::GameType(game, _) => self.scan_game(ctx, &game),
            Nav::Custom(index) => {
                let game = self
                    .settings
                    .custom_filters
                    .get(index)
                    .map(|filter| filter.game.clone());
                match game.filter(|game| !game.is_empty()) {
                    Some(game) => self.scan_game(ctx, &game),
                    None => {
                        let keys = self.known_servers();
                        self.scan_list(ctx, keys, noop("known servers"));
                    }
                }
            }
            _ => {
                let keys = self.known_servers();
                self.scan_list(ctx, keys, noop("known servers"));
            }
        }
    }

    fn scan_lan(&mut self, ctx: &egui::Context) {
        self.status = tr("Looking for servers on the local network...").to_string();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = openase::lan::discover(Duration::from_millis(1500))
                .map_err(|error| error.to_string());
            let _ = tx.send(Msg::Lan(result));
            ctx.request_repaint();
        });
    }

    fn stop_scan(&mut self) {
        if let Some(scan) = &self.scan {
            scan.stop.store(true, Ordering::Relaxed);
        }
    }

    fn start_scan(&mut self, label: String) -> (u64, Arc<AtomicBool>, Arc<AtomicUsize>) {
        self.stop_scan();
        self.generation += 1;
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicUsize::new(0));
        self.scan = Some(Scan {
            generation: self.generation,
            stop: stop.clone(),
            done: done.clone(),
            total: 0,
            label,
        });
        (self.generation, stop, done)
    }

    fn scan_game(&mut self, ctx: &egui::Context, game_id: &str) {
        let Some(game) = catalog::game(game_id) else {
            return;
        };
        let (generation, stop, done) = self.start_scan(fill(tr("Scanning {}"), &[&game.name]));
        self.status = fill(tr("Querying {} master servers..."), &[&game.name]);
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let network = self.settings.network.clone();
        let steam_key = self.settings.steam_api_key.clone();
        std::thread::spawn(move || {
            let options = network.query_options();
            let (mut addresses, errors) =
                fetch_all_masters(game, options.timeout, &steam_key, network.scan_limit);
            addresses.truncate(network.scan_limit.max(1));
            let _ = tx.send(Msg::MasterList {
                generation,
                game: game.id.clone(),
                addresses: addresses.clone(),
                errors,
            });
            ctx.request_repaint();
            let targets = addresses
                .into_iter()
                .map(|address| (game.id.clone(), address))
                .collect();
            run_queries(targets, &network, &tx, &ctx, generation, &stop, &done);
        });
    }

    fn scan_list(&mut self, ctx: &egui::Context, keys: Vec<ServerKey>, what: &'static str) {
        if keys.is_empty() {
            self.status = fill(tr("No {} to refresh"), &[&tr(what)]);
            return;
        }
        for key in &keys {
            self.ensure_placeholder(key);
        }
        let (generation, stop, done) = self.start_scan(fill(tr("Refreshing {}"), &[&tr(what)]));
        if let Some(scan) = &mut self.scan {
            scan.total = keys.len();
        }
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let network = self.settings.network.clone();
        std::thread::spawn(move || {
            let targets = keys
                .into_iter()
                .map(|key| (key.game, key.address))
                .collect();
            run_queries(targets, &network, &tx, &ctx, generation, &stop, &done);
        });
    }

    pub(crate) fn query_single(&mut self, ctx: &egui::Context, key: &ServerKey) {
        let Some(game) = catalog::game(&key.game) else {
            return;
        };
        let Ok((host, port)) = parse_address(&key.address, game.default_port) else {
            self.dialogs.error = Some(fill(tr("Invalid address {}"), &[&key.address]));
            return;
        };
        self.single_in_flight = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let options = self.settings.network.query_options();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Single(query_server(game, &host, port, &options)));
            ctx.request_repaint();
        });
    }

    fn drain_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            let current = self.scan.as_ref().map(|scan| scan.generation);
            match msg {
                Msg::MasterList {
                    generation,
                    game,
                    addresses,
                    errors,
                } if Some(generation) == current => {
                    let mut list = Vec::with_capacity(addresses.len());
                    let old = self.store.remove(&game).unwrap_or_default();
                    let mut old: HashMap<String, Server> = old
                        .into_iter()
                        .map(|server| (server.address(), server))
                        .collect();
                    let definition = catalog::game(&game);
                    for address in &addresses {
                        if let Some(existing) = old.remove(address) {
                            list.push(existing);
                        } else if let Some(definition) = definition
                            && let Ok((host, port)) =
                                parse_address(address, definition.default_port)
                        {
                            let mut server = Server::new(&game, &host, port);
                            annotate(self.geoip.as_deref(), &mut server);
                            list.push(server);
                        }
                    }
                    self.store.insert(game, list);
                    if let Some(scan) = &mut self.scan {
                        scan.total = addresses.len();
                    }
                    if addresses.is_empty() && !errors.is_empty() {
                        self.dialogs.error = Some(fill(
                            tr("No server list could be downloaded:\n\n{}"),
                            &[&errors.join("\n")],
                        ));
                    } else if !errors.is_empty() {
                        self.status = fill(tr("Some masters failed: {}"), &[&errors.join("; ")]);
                    }
                }
                Msg::Server { generation, server } if Some(generation) == current => {
                    self.upsert(server)
                }
                Msg::ScanDone { generation } if Some(generation) == current => {
                    let stopped = self
                        .scan
                        .as_ref()
                        .is_some_and(|scan| scan.stop.load(Ordering::Relaxed));
                    self.scan = None;
                    self.status = if stopped {
                        tr("Scan stopped").to_string()
                    } else {
                        tr("Scan complete").to_string()
                    };
                }
                Msg::Single(server) => {
                    self.single_in_flight = false;
                    self.status = if server.online {
                        fill(tr("Refreshed {}"), &[&strip_quake_colors(&server.name)])
                    } else {
                        fill(tr("{} did not answer"), &[&server.address()])
                    };
                    self.upsert(server);
                }
                Msg::SearchProgress(dir) => self.dialogs.search_progress(dir),
                Msg::SearchFound(path) => self.dialogs.search_found(path),
                Msg::SearchDone => self.dialogs.search_done(),
                Msg::GeoIp(result) => match result {
                    Ok(geoip) => {
                        self.geoip_status = fill(tr("{} IPv4 ranges loaded"), &[&geoip.len()]);
                        for server in self.store.values_mut().flatten() {
                            annotate(Some(&geoip), server);
                        }
                        self.geoip = Some(geoip);
                    }
                    Err(error) => {
                        self.geoip_status = fill(tr("Not available: {}"), &[&error]);
                        self.status = fill(tr("GeoIP database not available: {}"), &[&error]);
                    }
                },
                Msg::Lan(result) => match result {
                    Ok(found) => {
                        self.lan_servers = found
                            .into_iter()
                            .map(|server| ServerKey {
                                game: server.game,
                                address: server.address,
                            })
                            .collect();
                        if self.lan_servers.is_empty() {
                            self.status = tr("No server found on the local network").to_string();
                        } else {
                            let keys = self.lan_servers.clone();
                            self.scan_list(ctx, keys, noop("LAN servers"));
                        }
                    }
                    Err(error) => {
                        self.dialogs.error = Some(fill(tr("LAN search failed: {}"), &[&error]))
                    }
                },
                Msg::PortFixes(fixes) => {
                    for (old, new) in &fixes {
                        self.settings.replace_server(old, new);
                    }
                    self.settings.unreal_ports_checked = true;
                    self.settings_dirty = true;
                }
                Msg::Update { result, manual } => match result {
                    Ok(Some(tag)) => self.extra.update_available = Some(tag),
                    Ok(None) if manual => self.dialogs.notify(fill(
                        tr("OpenASE {} is up to date."),
                        &[&env!("CARGO_PKG_VERSION")],
                    )),
                    Err(error) if manual => {
                        self.dialogs.error =
                            Some(fill(tr("Could not check for updates: {}"), &[&error]))
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    // ----- view ------------------------------------------------------------

    /// Servers of the current tree selection, before filters.
    fn candidates(&self) -> Vec<&Server> {
        if let Some(keys) = self.nav_list(&self.nav) {
            return keys.iter().filter_map(|key| self.find(key)).collect();
        }
        match &self.nav {
            Nav::Game(game) | Nav::GameType(game, _) => self
                .store
                .get(game)
                .map(|list| list.iter().collect())
                .unwrap_or_default(),
            Nav::Friends => self
                .store
                .values()
                .flatten()
                .filter(|server| self.server_has_friend(server))
                .collect(),
            Nav::FriendGroup(group) => {
                let keys: Vec<&ServerKey> = self
                    .settings
                    .friends
                    .iter()
                    .filter(|friend| self.settings.friend_group(friend) == *group)
                    .filter_map(|friend| self.friend_location(friend))
                    .collect();
                self.store
                    .values()
                    .flatten()
                    .filter(|server| keys.contains(&&ServerKey::of(server)))
                    .collect()
            }
            Nav::Custom(index) => match self.settings.custom_filters.get(*index) {
                Some(filter) => self
                    .store
                    .iter()
                    .filter(|(game, _)| filter.game.is_empty() || **game == filter.game)
                    .flat_map(|(_, list)| list.iter())
                    .filter(|server| filter.matches(server))
                    .collect(),
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Highlight color of the first color filter matching a server.
    fn color_filter(&self, server: &Server) -> Option<Color32> {
        self.settings
            .custom_filters
            .iter()
            .filter(|filter| filter.color.is_some() && !filter.rules.is_empty())
            .find(|filter| filter.matches(server))
            .and_then(|filter| parse_hex(filter.color.as_deref()?))
            .map(|[r, g, b]| Color32::from_rgb(r, g, b))
    }

    /// Countries present in the current selection: (code, label, count), by label.
    fn country_options(&self) -> Vec<(String, String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for server in self.candidates() {
            *counts.entry(country_code(server).to_string()).or_default() += 1;
        }
        let mut options: Vec<(String, String, usize)> = counts
            .into_iter()
            .map(|(code, count)| {
                let label = country_label(&code);
                (code, label, count)
            })
            .collect();
        options.sort_by(|a, b| a.1.cmp(&b.1));
        options
    }

    fn visible_rows(&self) -> Vec<&Server> {
        let filters = &self.settings.filters;
        let candidates = self.candidates();
        let game = self.nav.game().and_then(catalog::game);

        let mut rows: Vec<&Server> = candidates
            .into_iter()
            .filter(|server| {
                let unanswered = server.error.is_some();
                if filters.hide_offline && unanswered {
                    return false;
                }
                if !filters.countries.is_empty()
                    && !filters.countries.contains(country_code(server))
                {
                    return false;
                }
                // Unscanned or silent servers cannot satisfy content filters.
                if !server.online {
                    return !filters.not_empty
                        && filters.max_ping.is_none()
                        && !matches!(self.nav, Nav::GameType(..));
                }
                if filters.not_empty && server.num_players == 0 {
                    return false;
                }
                if filters.not_full && server.is_full() {
                    return false;
                }
                if filters.no_password && server.password {
                    return false;
                }
                if let (Some(limit), Some(ping)) = (filters.max_ping, server.ping)
                    && ping > limit
                {
                    return false;
                }
                if let Nav::GameType(_, value) = &self.nav
                    && !server.game_type.eq_ignore_ascii_case(value)
                {
                    return false;
                }
                game.is_none_or(|game| passes_groups(game, server, &filters.checked))
            })
            .collect();

        let (col, ascending) = self.sort;
        // Servers that answered always come first, whatever the sort direction.
        rows.sort_by(|a, b| {
            b.online.cmp(&a.online).then_with(|| {
                let ordering = compare(col, a, b);
                if ascending {
                    ordering
                } else {
                    ordering.reverse()
                }
            })
        });
        rows
    }

    // ----- actions ---------------------------------------------------------

    pub(crate) fn join(&mut self, key: &ServerKey) {
        let Some(server) = self.find(key) else { return };
        if server.password {
            self.dialogs.password = Some((key.clone(), String::new()));
        } else {
            self.join_with_password(key, "");
        }
    }

    pub(crate) fn join_with_password(&mut self, key: &ServerKey, password: &str) {
        let (Some(server), Some(game)) = (self.find(key).cloned(), catalog::game(&key.game)) else {
            return;
        };
        let settings = self.settings.game(&key.game);
        match openase::launch::join(game, &settings, &server, password) {
            Ok(launched) => {
                self.settings.add_recent(key.to_ref());
                self.settings_dirty = true;
                self.status = match launched {
                    openase::launch::Launched::Program(program) => {
                        fill(tr("Launched {}"), &[&program])
                    }
                    openase::launch::Launched::SteamUrl(url) => fill(tr("Opened {}"), &[&url]),
                };
            }
            Err(error) => self.dialogs.error = Some(error.to_string()),
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Select(key) => self.selected = Some(key),
            Action::Join(key) => {
                self.selected = Some(key.clone());
                self.join(&key);
            }
            Action::ToggleFavorite(key) => {
                self.settings.toggle_favorite(key.to_ref());
                self.settings_dirty = true;
            }
            Action::Refresh(key) => self.query_single(ctx, &key),
            Action::FavoriteGroup(key, group) => {
                self.settings.add_favorite_to(key.to_ref(), &group);
                self.settings_dirty = true;
                self.status = fill(tr("Added to {}"), &[&group_name(&group)]);
            }
            Action::Copy(text) => {
                ctx.copy_text(text.clone());
                self.status = fill(tr("Copied {}"), &[&text]);
            }
        }
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.settings_dirty = true;
    }

    pub(crate) fn add_server(
        &mut self,
        ctx: &egui::Context,
        game_id: &str,
        address: &str,
    ) -> Result<(), String> {
        let game = catalog::game(game_id).ok_or_else(|| tr("Unknown game").to_string())?;
        let (host, port) =
            parse_address(address.trim(), game.default_port).map_err(|error| error.to_string())?;
        // Players share the game port of Unreal engine servers: switch to the
        // query port so OpenASE never queries the game port.
        let timeout = self.settings.network.query_options().timeout;
        let port = openase::query::resolve_query_port(game, &host, port, timeout);
        let key = ServerKey {
            game: game.id.clone(),
            address: format!("{host}:{port}"),
        };
        if !self.settings.is_favorite(&key.to_ref()) {
            self.settings.toggle_favorite(key.to_ref());
            self.settings_dirty = true;
        }
        self.ensure_placeholder(&key);
        self.nav = Nav::Favorites;
        self.selected = Some(key.clone());
        self.query_single(ctx, &key);
        Ok(())
    }

    // ----- UI: menus and toolbar -------------------------------------------

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr("File"), |ui| self.file_menu(ui));
            ui.menu_button(tr("View"), |ui| {
                ui.checkbox(&mut self.show_toolbar, tr("Toolbar"));
            });
            ui.menu_button(tr("Filter"), |ui| self.filter_menu(ui));
            ui.menu_button(tr("Server"), |ui| {
                let selected = self.selected.clone();
                if ui.button(tr("Refresh list\tF5")).clicked() {
                    self.refresh(ui.ctx());
                    ui.close();
                }
                if ui
                    .add_enabled(self.scan.is_some(), egui::Button::new(tr("Stop")))
                    .clicked()
                {
                    self.stop_scan();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        selected.is_some(),
                        egui::Button::new(tr("Refresh selected server")),
                    )
                    .clicked()
                {
                    if let Some(key) = &selected {
                        self.query_single(ui.ctx(), key);
                    }
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new(tr("Join\tEnter")))
                    .clicked()
                {
                    if let Some(key) = &selected {
                        self.join(key);
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(
                        selected.is_some(),
                        egui::Button::new(tr("Add to / remove from favorites")),
                    )
                    .clicked()
                {
                    if let Some(key) = selected.clone() {
                        self.apply(ui.ctx(), Action::ToggleFavorite(key));
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new(tr("Copy address")))
                    .clicked()
                {
                    if let Some(server) = selected.as_ref().and_then(|key| self.find(key)) {
                        let text = server.join_address();
                        self.apply(ui.ctx(), Action::Copy(text));
                    }
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Add server...")).clicked() {
                    self.dialogs.open_add(self.nav.game());
                    ui.close();
                }
                if ui.button(tr("Find LAN servers")).clicked() {
                    self.nav = Nav::Special(Special::Lan);
                    self.scan_lan(ui.ctx());
                    ui.close();
                }
            });
            ui.menu_button(tr("Player"), |ui| {
                if ui.button(tr("Find player...")).clicked() {
                    self.dialogs.find = Some(String::new());
                    ui.close();
                }
            });
            ui.menu_button(tr("Friends"), |ui| self.friends_menu(ui));
            ui.menu_button(tr("Tools"), |ui| {
                if ui.button(tr("Options...")).clicked() {
                    self.dialogs.open_options(&self.settings);
                    ui.close();
                }
            });
            ui.menu_button(tr("Help"), |ui| self.help_menu(ui));
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let selected = self.selected.clone();
        let has = selected.is_some();
        let ids: Vec<String> = self
            .settings
            .toolbar
            .visible_ids()
            .into_iter()
            .map(str::to_string)
            .collect();
        // Registered first so the buttons, added later, win the clicks.
        let background = ui.interact(
            ui.max_rect(),
            ui.scope_id().with("toolbar-menu"),
            Sense::click(),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            let mut last_group = None;
            for id in &ids {
                let group = toolbar_group(id);
                if last_group.is_some_and(|last| last != group) {
                    ui.add_space(6.0);
                }
                last_group = Some(group);
                match id.as_str() {
                    "refresh" => {
                        if icons::tool_button(
                            ui,
                            Tool::Refresh,
                            tr(tr("Refresh server list (F5)")),
                            self.scan.is_none(),
                        )
                        .clicked()
                        {
                            self.refresh(ui.ctx());
                        }
                    }
                    "stop" => {
                        if icons::tool_button(ui, Tool::Stop, tr("Stop"), self.scan.is_some())
                            .clicked()
                        {
                            self.stop_scan();
                        }
                    }
                    "ping" => {
                        if icons::tool_button(
                            ui,
                            Tool::Ping,
                            tr("Refresh selected server"),
                            has && !self.single_in_flight,
                        )
                        .clicked()
                            && let Some(key) = &selected
                        {
                            self.query_single(ui.ctx(), key);
                        }
                    }
                    "join" => {
                        if icons::tool_button(ui, Tool::Join, tr(tr("Join server (Enter)")), has)
                            .clicked()
                            && let Some(key) = &selected
                        {
                            self.join(key);
                        }
                    }
                    "favorite" => {
                        if icons::tool_button(
                            ui,
                            Tool::Favorite,
                            tr("Add to / remove from favorites"),
                            has,
                        )
                        .clicked()
                            && let Some(key) = selected.clone()
                        {
                            self.apply(ui.ctx(), Action::ToggleFavorite(key));
                        }
                    }
                    "add" => {
                        if icons::tool_button(ui, Tool::Add, tr("Add server"), true).clicked() {
                            self.dialogs.open_add(self.nav.game());
                        }
                    }
                    "find" => {
                        if icons::tool_button(ui, Tool::Find, tr("Find player"), true).clicked() {
                            self.dialogs.find = Some(String::new());
                        }
                    }
                    "friends" => {
                        if icons::tool_button(ui, Tool::Friends, tr("Friends"), true).clicked() {
                            self.dialogs.friends = Some(String::new());
                        }
                    }
                    "options"
                        if icons::tool_button(ui, Tool::Options, tr("Options"), true).clicked() =>
                    {
                        self.dialogs.open_options(&self.settings);
                    }
                    _ => {}
                }
            }
        });
        background.context_menu(|ui| {
            let mut changed = false;
            for item in &mut self.settings.toolbar.items {
                changed |= ui
                    .checkbox(&mut item.visible, tr(dialogs::toolbar_label(&item.id)))
                    .changed();
            }
            ui.separator();
            if ui.button(tr(TOOLBAR_CUSTOMIZE)).clicked() {
                self.dialogs
                    .open_options_at(&self.settings, dialogs::TOOLBAR_TAB);
                ui.close();
            }
            if ui.button(tr(TOOLBAR_RESET)).clicked() {
                self.settings.toolbar = Default::default();
                changed = true;
                ui.close();
            }
            if changed {
                self.settings_dirty = true;
            }
        });
    }

    // ----- UI: tree ----------------------------------------------------------

    fn tree(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut new_nav: Option<Nav> = None;
        let mut toggles: Vec<String> = Vec::new();
        let mut filter_clicks: Vec<(String, String)> = Vec::new();
        let mut modifier_clicks: Vec<&'static str> = Vec::new();
        let mut country_clicks: Vec<String> = Vec::new();

        let mut row = |ui: &mut egui::Ui,
                       depth: usize,
                       path: Option<&str>,
                       icon: TreeIcon<'_>,
                       label: &str,
                       selected: bool|
         -> bool {
            let open = path.map(|path| self.settings.open_nav.contains(path));
            let (toggled, clicked) = tree_row(ui, depth, open, icon, label, selected);
            if toggled && let Some(path) = path {
                toggles.push(path.to_string());
            }
            clicked
        };

        // Favorites, with ASE's Favorites1..8 groups.
        if row(
            ui,
            0,
            Some("Favorites"),
            TreeIcon::Star,
            tr("Favorites"),
            self.nav == Nav::Favorites,
        ) {
            new_nav = Some(Nav::Favorites);
        }
        if self.settings.open_nav.contains("Favorites") {
            for group in FAVORITE_GROUPS {
                let count = self.settings.favorites_in(group).len();
                let label = if count > 0 {
                    format!("{} ({count})", group_name(group))
                } else {
                    group_name(group)
                };
                let nav = Nav::FavoriteGroup(group.to_string());
                if row(ui, 1, None, TreeIcon::Folder, &label, self.nav == nav) {
                    new_nav = Some(nav);
                }
            }
        }

        // Friends, with Friends1..4 groups.
        if row(
            ui,
            0,
            Some("Friends"),
            TreeIcon::Friends,
            tr("Friends"),
            self.nav == Nav::Friends,
        ) {
            new_nav = Some(Nav::Friends);
        }
        if self.settings.open_nav.contains("Friends") {
            for group in FRIEND_GROUPS {
                let online = self
                    .settings
                    .friends
                    .iter()
                    .filter(|friend| self.settings.friend_group(friend) == group)
                    .filter(|friend| self.friend_location(friend).is_some())
                    .count();
                let label = if online > 0 {
                    fill(tr("{} ({} on)"), &[&group_name(group), &online])
                } else {
                    group_name(group)
                };
                let nav = Nav::FriendGroup(group.to_string());
                if row(ui, 1, None, TreeIcon::Folder, &label, self.nav == nav) {
                    new_nav = Some(nav);
                }
            }
        }

        // Special lists.
        row(
            ui,
            0,
            Some("Special"),
            TreeIcon::Folder,
            tr("Special"),
            false,
        );
        if self.settings.open_nav.contains("Special") {
            for special in Special::ALL {
                let nav = Nav::Special(special);
                if row(
                    ui,
                    1,
                    None,
                    TreeIcon::Folder,
                    special.label(),
                    self.nav == nav,
                ) {
                    new_nav = Some(nav);
                }
            }
            if row(
                ui,
                1,
                None,
                TreeIcon::Clock,
                tr("recently joined"),
                self.nav == Nav::Recent,
            ) {
                new_nav = Some(Nav::Recent);
            }
        }

        // User filters.
        if !self.settings.custom_filters.is_empty() {
            row(
                ui,
                0,
                Some("My filters"),
                TreeIcon::Funnel,
                tr("My filters"),
                false,
            );
            if self.settings.open_nav.contains("My filters") {
                for (index, filter) in self.settings.custom_filters.iter().enumerate() {
                    let label = match &filter.color {
                        Some(_) => fill(tr("{} (color)"), &[&filter.name]),
                        None => filter.name.clone(),
                    };
                    let nav = Nav::Custom(index);
                    if row(ui, 1, None, TreeIcon::Folder, &label, self.nav == nav) {
                        new_nav = Some(nav);
                    }
                }
            }
        }

        for game in catalog::games() {
            if !self.settings.game(&game.id).visible {
                continue;
            }
            let path = game.id.clone();
            if row(
                ui,
                0,
                Some(&path),
                TreeIcon::Game(game),
                &game.name,
                self.nav == Nav::Game(game.id.clone()),
            ) {
                new_nav = Some(Nav::Game(game.id.clone()));
            }
            if !self.settings.open_nav.contains(&path) {
                continue;
            }
            for game_type in &game.game_types {
                let nav = Nav::GameType(game.id.clone(), game_type.value.clone());
                if row(
                    ui,
                    1,
                    None,
                    TreeIcon::Folder,
                    &game_type.name,
                    self.nav == nav,
                ) {
                    new_nav = Some(nav);
                }
            }
            for group in &game.filters {
                let group_path = format!("{}/{}", game.id, group.name);
                let checked_key = group_path.clone();
                let any = self
                    .settings
                    .filters
                    .checked
                    .get(&checked_key)
                    .is_some_and(|set| !set.is_empty());
                let label = if any {
                    format!("{} *", group.name)
                } else {
                    group.name.clone()
                };
                row(ui, 1, Some(&group_path), TreeIcon::Folder, &label, false);
                if !self.settings.open_nav.contains(&group_path) {
                    continue;
                }
                let options: Vec<String> = if group.dynamic {
                    dynamic_options(
                        self.store.get(&game.id).map(Vec::as_slice).unwrap_or(&[]),
                        &group.key,
                    )
                } else {
                    group
                        .options
                        .iter()
                        .map(|option| option.label.clone())
                        .collect()
                };
                if options.is_empty() {
                    row(ui, 2, None, TreeIcon::None, tr("(refresh to fill)"), false);
                }
                for option in options {
                    let checked = self
                        .settings
                        .filters
                        .checked
                        .get(&checked_key)
                        .is_some_and(|set| set.contains(&option));
                    let shown = if option.is_empty() {
                        tr(tr("(none)"))
                    } else {
                        option.as_str()
                    };
                    if row(ui, 2, None, TreeIcon::Check(checked), shown, false) {
                        filter_clicks.push((checked_key.clone(), option));
                    }
                }
            }
        }

        let filters = self.settings.filters.clone();
        row(
            ui,
            0,
            Some("Modifiers"),
            TreeIcon::Funnel,
            tr("Modifiers"),
            false,
        );
        if self.settings.open_nav.contains("Modifiers") {
            row(
                ui,
                1,
                Some("Modifiers/Players"),
                TreeIcon::Folder,
                tr("Players"),
                false,
            );
            if self.settings.open_nav.contains("Modifiers/Players") {
                for (id, label, value) in [
                    ("not_empty", tr("Not empty"), filters.not_empty),
                    ("not_full", tr("Not full"), filters.not_full),
                    ("no_password", tr("No password"), filters.no_password),
                    (
                        "hide_offline",
                        tr("Hide unresponsive"),
                        filters.hide_offline,
                    ),
                ] {
                    if row(ui, 2, None, TreeIcon::Check(value), label, false) {
                        modifier_clicks.push(id);
                    }
                }
            }
            row(
                ui,
                1,
                Some("Modifiers/Ping"),
                TreeIcon::Folder,
                tr("Ping"),
                false,
            );
            if self.settings.open_nav.contains("Modifiers/Ping") {
                for (id, limit) in [
                    ("ping50", 50),
                    ("ping100", 100),
                    ("ping150", 150),
                    ("ping250", 250),
                ] {
                    if row(
                        ui,
                        2,
                        None,
                        TreeIcon::Check(filters.max_ping == Some(limit)),
                        &fill(tr("less than {}"), &[&limit]),
                        false,
                    ) {
                        modifier_clicks.push(id);
                    }
                }
            }
            let any_country = !filters.countries.is_empty();
            row(
                ui,
                1,
                Some("Modifiers/Country"),
                TreeIcon::Folder,
                if any_country {
                    tr("Country *")
                } else {
                    tr("Country")
                },
                false,
            );
            if self.settings.open_nav.contains("Modifiers/Country") {
                let options = self.country_options();
                if options.is_empty() {
                    row(ui, 2, None, TreeIcon::None, tr("(refresh to fill)"), false);
                }
                for (code, label, count) in options {
                    let checked = filters.countries.contains(&code);
                    if row(
                        ui,
                        2,
                        None,
                        TreeIcon::Check(checked),
                        &format!("{label} ({count})"),
                        false,
                    ) {
                        country_clicks.push(code);
                    }
                }
            }
        }

        for code in country_clicks {
            let countries = &mut self.settings.filters.countries;
            if !countries.remove(&code) {
                countries.insert(code);
            }
            self.settings_dirty = true;
        }
        for path in toggles {
            if !self.settings.open_nav.remove(&path) {
                self.settings.open_nav.insert(path);
            }
            self.settings_dirty = true;
        }
        for (group, option) in filter_clicks {
            let set = self.settings.filters.checked.entry(group).or_default();
            if !set.remove(&option) {
                set.insert(option);
            }
            self.settings_dirty = true;
        }
        for id in modifier_clicks {
            let filters = &mut self.settings.filters;
            match id {
                "not_empty" => filters.not_empty ^= true,
                "not_full" => filters.not_full ^= true,
                "no_password" => filters.no_password ^= true,
                "hide_offline" => filters.hide_offline ^= true,
                ping => {
                    let limit = PING_LIMITS[["ping50", "ping100", "ping150", "ping250"]
                        .iter()
                        .position(|p| *p == ping)
                        .unwrap_or(0)];
                    filters.max_ping = if filters.max_ping == Some(limit) {
                        None
                    } else {
                        Some(limit)
                    };
                }
            }
            self.settings_dirty = true;
        }
        if let Some(nav) = new_nav {
            if let Some(game) = nav.game() {
                self.settings.last_game = game.to_string();
                let game = game.to_string();
                let first_visit = !self.store.contains_key(&game);
                if self.selected.as_ref().is_some_and(|key| key.game != game) {
                    self.selected = None;
                }
                self.nav = nav;
                if first_visit && self.scan.is_none() {
                    self.scan_game(ui.ctx(), &game);
                }
            } else {
                if let Some(keys) = self.nav_list(&nav) {
                    for key in &keys {
                        self.ensure_placeholder(key);
                    }
                }
                let lan_first_visit =
                    nav == Nav::Special(Special::Lan) && self.lan_servers.is_empty();
                self.nav = nav;
                if lan_first_visit {
                    self.scan_lan(ui.ctx());
                }
            }
            self.settings_dirty = true;
        }
    }

    // ----- UI: server list ---------------------------------------------------

    fn server_list(&mut self, ui: &mut egui::Ui) {
        let want_scroll = std::mem::take(&mut self.scroll_to_selected);
        let rows = self.visible_rows();
        let mut actions: Vec<Action> = Vec::new();
        let mut sort_click: Option<Col> = None;
        let (sort_col, ascending) = self.sort;
        let selected = self.selected.clone();
        let text_color = ui.visuals().text_color();
        let selection = ui.visuals().selection.bg_fill;

        if rows.is_empty() && self.scan.is_none() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr("<- select a game and click Refresh, or add a server"))
                        .color(Color32::from_gray(0x70)),
                );
            });
            return;
        }
        let scroll_to = if want_scroll {
            selected
                .as_ref()
                .and_then(|key| rows.iter().position(|server| ServerKey::of(server) == *key))
        } else {
            None
        };

        let mut table = TableBuilder::new(ui);
        if let Some(index) = scroll_to {
            table = table.scroll_to_row(index, Some(Align::Center));
        }
        let mut table = table
            .id_salt("server-list")
            .striped(false)
            .resizable(true)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .auto_shrink(false);
        for (index, (_, _, width)) in COLUMNS.iter().enumerate() {
            let column = if index == COLUMNS.len() - 1 {
                Column::remainder()
            } else {
                Column::initial(*width)
            };
            table = table.column(column.at_least(24.0).clip(true));
        }

        table
            .header(20.0, |mut header| {
                for (col, label, _) in COLUMNS {
                    header.col(|ui| {
                        let rect = ui.max_rect();
                        header_cell(ui, rect);
                        let response = ui.add(
                            egui::Label::new(RichText::new(tr(label)))
                                .selectable(false)
                                .sense(Sense::click()),
                        );
                        if response.clicked()
                            || ui
                                .interact(rect, ui.scope_id().with(label), Sense::click())
                                .clicked()
                        {
                            sort_click = Some(col);
                        }
                        if col == sort_col {
                            icons::sort_arrow(
                                ui.painter(),
                                egui::pos2(rect.right() - 8.0, rect.center().y),
                                ascending,
                            );
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                    let server = rows[row.index()];
                    let key = ServerKey::of(server);
                    let is_selected = selected.as_ref() == Some(&key);
                    row.set_selected(is_selected);
                    let game = catalog::game(&server.game);
                    let favorite = self.settings.is_favorite(&key.to_ref());
                    let friend = self.server_has_friend(server);
                    let highlight = if friend {
                        Some(Color32::from_rgb(0xe0, 0xf5, 0xe0))
                    } else {
                        self.color_filter(server)
                    };
                    let color = if is_selected {
                        Color32::WHITE
                    } else if !server.online {
                        Color32::from_gray(0x90)
                    } else {
                        text_color
                    };

                    for (col, _, _) in COLUMNS {
                        row.col(|ui| {
                            if is_selected {
                                ui.painter().rect_filled(
                                    ui.max_rect().expand2(vec2(4.0, 1.0)),
                                    CornerRadius::ZERO,
                                    selection,
                                );
                            } else if let Some(fill) = highlight {
                                ui.painter().rect_filled(
                                    ui.max_rect().expand2(vec2(4.0, 1.0)),
                                    CornerRadius::ZERO,
                                    fill,
                                );
                            }
                            match col {
                                Col::Icon => {
                                    let rect = ui.max_rect();
                                    if let Some(game) = game {
                                        icons::badge(
                                            ui.painter(),
                                            egui::Rect::from_min_size(
                                                rect.left_center() + vec2(1.0, -6.0),
                                                vec2(20.0, 12.0),
                                            ),
                                            game,
                                        );
                                    }
                                    if server.password {
                                        icons::lock(
                                            ui.painter(),
                                            rect.left_center() + vec2(28.0, 0.0),
                                        );
                                    }
                                    if favorite {
                                        icons::star(
                                            ui.painter(),
                                            rect.left_center() + vec2(38.0, 0.0),
                                            5.0,
                                            Color32::from_rgb(0xf0, 0xc0, 0x1e),
                                            Color32::from_rgb(0x96, 0x6e, 0x00),
                                        );
                                    }
                                }
                                Col::Name => {
                                    let colored = self.palette.quake_colors
                                        && game
                                            .is_some_and(|game| game.protocol == Protocol::Quake3)
                                        && server.online
                                        && !is_selected
                                        && self
                                            .settings
                                            .game(&server.game)
                                            .draw_player_names_with_game_font;
                                    if colored {
                                        colored_label(ui, &server.name, color);
                                    } else {
                                        cell(ui, &display_name(server), color);
                                    }
                                }
                                Col::Ping | Col::Flux | Col::Loss | Col::Players => {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        cell(ui, &cell_text(col, server, game), color);
                                    });
                                }
                                _ => cell(ui, &cell_text(col, server, game), color),
                            }
                        });
                    }

                    let response = row.response();
                    if response.double_clicked() {
                        actions.push(Action::Join(key.clone()));
                    } else if response.clicked() || response.secondary_clicked() {
                        actions.push(Action::Select(key.clone()));
                    }
                    let address = server.join_address();
                    response.context_menu(|ui| {
                        if ui.button(tr("Join")).clicked() {
                            actions.push(Action::Join(key.clone()));
                            ui.close();
                        }
                        if ui.button(tr("Refresh")).clicked() {
                            actions.push(Action::Refresh(key.clone()));
                            ui.close();
                        }
                        let label = if favorite {
                            tr("Remove from favorites")
                        } else {
                            tr("Add to favorites")
                        };
                        if ui.button(label).clicked() {
                            actions.push(Action::ToggleFavorite(key.clone()));
                            ui.close();
                        }
                        ui.menu_button(tr("Add to favorites group"), |ui| {
                            for group in FAVORITE_GROUPS {
                                if ui.button(group_name(group)).clicked() {
                                    actions.push(Action::FavoriteGroup(
                                        key.clone(),
                                        group.to_string(),
                                    ));
                                    ui.close();
                                }
                            }
                        });
                        if ui.button(tr("Copy address")).clicked() {
                            actions.push(Action::Copy(address.clone()));
                            ui.close();
                        }
                    });
                });
            });

        if let Some(col) = sort_click {
            self.sort = if self.sort.0 == col {
                (col, !self.sort.1)
            } else {
                (col, !matches!(col, Col::Players))
            };
        }
        let ctx = ui.ctx().clone();
        for action in actions {
            self.apply(&ctx, action);
        }
    }

    // ----- UI: bottom panes --------------------------------------------------

    fn friends_pane(&mut self, ui: &mut egui::Ui) {
        let friends = self.settings.friends.clone();
        let text_color = ui.visuals().text_color();
        let mut select: Option<ServerKey> = None;
        TableBuilder::new(ui)
            .id_salt("friends-pane")
            .resizable(true)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .auto_shrink(false)
            .column(Column::initial(118.0).at_least(40.0).clip(true))
            .column(Column::remainder().clip(true))
            .header(20.0, |mut header| {
                let online = friends
                    .iter()
                    .filter(|friend| self.friend_location(friend).is_some())
                    .count();
                for label in [
                    if online > 0 {
                        tr("Friend - on")
                    } else {
                        tr("Friend - off")
                    },
                    tr("Server"),
                ] {
                    header.col(|ui| {
                        header_cell(ui, ui.max_rect());
                        cell(ui, label, text_color);
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, friends.len(), |mut row| {
                    let friend = &friends[row.index()];
                    let location = self.friend_location(friend).cloned();
                    let server_name = location
                        .as_ref()
                        .and_then(|key| self.find(key))
                        .map(|server| strip_quake_colors(&server.name))
                        .unwrap_or_default();
                    let color = if location.is_some() {
                        Color32::from_rgb(0x14, 0x78, 0x1e)
                    } else {
                        Color32::from_gray(0x80)
                    };
                    row.col(|ui| cell(ui, friend, color));
                    row.col(|ui| cell(ui, &server_name, text_color));
                    if row.response().clicked() {
                        select = location;
                    }
                });
            });
        if let Some(key) = select {
            self.nav = Nav::Game(key.game.clone());
            self.selected = Some(key);
        }
    }

    fn players_pane(&self, ui: &mut egui::Ui, server: Option<&Server>) {
        let text_color = ui.visuals().text_color();
        let players = server
            .map(|server| server.players.as_slice())
            .unwrap_or(&[]);
        let colored = self.palette.quake_colors
            && server.is_some_and(|server| {
                catalog::game(&server.game).is_some_and(|game| game.protocol == Protocol::Quake3)
                    && self
                        .settings
                        .game(&server.game)
                        .draw_player_names_with_game_font
            });
        let friends: Vec<String> = self
            .settings
            .friends
            .iter()
            .map(|friend| friend.to_lowercase())
            .collect();
        let mut order: Vec<usize> = (0..players.len()).collect();
        order.sort_by_key(|&index| std::cmp::Reverse(players[index].score));

        TableBuilder::new(ui)
            .id_salt("players-pane")
            .resizable(true)
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .auto_shrink(false)
            .column(Column::initial(170.0).at_least(60.0).clip(true))
            .column(Column::initial(50.0).at_least(30.0))
            .column(Column::initial(44.0).at_least(30.0))
            .column(Column::remainder().at_least(40.0))
            .header(20.0, |mut header| {
                for label in [tr("Player Name"), tr("Score"), tr("Ping"), tr("Time")] {
                    header.col(|ui| {
                        header_cell(ui, ui.max_rect());
                        cell(ui, label, text_color);
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, order.len(), |mut row| {
                    let player = &players[order[row.index()]];
                    let plain = strip_quake_colors(&player.name);
                    let is_friend = friends
                        .iter()
                        .any(|friend| plain.to_lowercase().contains(friend));
                    row.col(|ui| {
                        if is_friend {
                            ui.painter().rect_filled(
                                ui.max_rect(),
                                CornerRadius::ZERO,
                                Color32::from_rgb(0xe0, 0xf5, 0xe0),
                            );
                        }
                        if colored {
                            colored_label(ui, &player.name, text_color);
                        } else {
                            cell(ui, &plain, text_color);
                        }
                    });
                    for value in [
                        player.score.to_string(),
                        player.ping.map(|ping| ping.to_string()).unwrap_or_default(),
                        player.time_secs.map(format_duration).unwrap_or_default(),
                    ] {
                        row.col(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                cell(ui, &value, text_color)
                            });
                        });
                    }
                });
            });
    }

    fn rules_pane(&self, ui: &mut egui::Ui, server: Option<&Server>) {
        let text_color = ui.visuals().text_color();
        let rules: Vec<(&String, &String)> = server
            .map(|server| server.rules.iter().collect())
            .unwrap_or_default();
        TableBuilder::new(ui)
            .id_salt("rules-pane")
            .resizable(true)
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .auto_shrink(false)
            .column(Column::initial(150.0).at_least(60.0).clip(true))
            .column(Column::remainder().clip(true))
            .header(20.0, |mut header| {
                for label in [tr("Key"), tr("Value")] {
                    header.col(|ui| {
                        header_cell(ui, ui.max_rect());
                        cell(ui, label, text_color);
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, rules.len(), |mut row| {
                    let (key, value) = rules[row.index()];
                    row.col(|ui| cell(ui, key, text_color));
                    row.col(|ui| cell(ui, &strip_quake_colors(value), text_color));
                });
            });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        let rows = self.visible_rows();
        let total: usize = match self.nav.game() {
            Some(game) => self.store.get(game).map_or(0, Vec::len),
            None => rows.len(),
        };
        let players: usize = rows.iter().map(|server| server.num_players).sum();
        let slots: usize = rows.iter().filter_map(|server| server.max_players).sum();
        ui.horizontal(|ui| {
            if let Some(scan) = &self.scan {
                let done = scan.done.load(Ordering::Relaxed);
                let fraction = if scan.total > 0 {
                    done as f32 / scan.total as f32
                } else {
                    0.0
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .desired_width(140.0)
                        .desired_height(12.0),
                );
                ui.label(format!("{} {}/{}", scan.label, done, scan.total));
            } else {
                ui.label(&self.status);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(fill(
                    tr("{} servers, {}/{} players total, {} visible"),
                    &[&total, &players, &slots, &rows.len()],
                ));
            });
        });
    }
}

impl eframe::App for OpenAseApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        i18n::set_language(
            self.dialogs
                .preview_language()
                .unwrap_or(self.settings.language),
        );
        self.drain_messages(&ctx);
        if !self.started {
            self.started = true;
            self.refresh(&ctx);
            if self.settings.geoip_disabled {
                self.geoip_status = tr("Disabled").to_string();
            } else {
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let result = geoip::load_or_download(Duration::from_secs(60))
                        .map(Arc::new)
                        .map_err(|error| error.to_string());
                    let _ = tx.send(Msg::GeoIp(result));
                    ctx.request_repaint();
                });
            }
            self.startup_tasks(&ctx);
        }

        if self.settings.events.auto_refresh_selected
            && !self.single_in_flight
            && self.last_auto_refresh.elapsed()
                >= Duration::from_secs(u64::from(self.settings.events.auto_refresh_seconds.max(5)))
        {
            self.last_auto_refresh = Instant::now();
            if let Some(key) = self.selected.clone() {
                self.query_single(&ctx, &key);
            }
        }
        if self.settings.events.auto_refresh_selected
            || self.scan.is_some()
            || self.dialogs.has_notices()
        {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        let no_dialog = !self.dialogs.any_modal();
        if no_dialog && !self.extra.any_modal() {
            self.shortcuts(&ctx);
        }
        if no_dialog && ctx.input(|input| input.key_pressed(egui::Key::F5)) && self.scan.is_none() {
            self.refresh(&ctx);
        }
        if no_dialog
            && !ctx.egui_wants_keyboard_input()
            && ctx.input(|input| input.key_pressed(egui::Key::Enter))
            && let Some(key) = self.selected.clone()
        {
            self.join(&key);
        }

        let frame = egui::Frame::NONE
            .fill(Color32::from_gray(0xf0))
            .inner_margin(egui::Margin::symmetric(4, 2));
        egui::Panel::top("menu")
            .frame(frame)
            .show(ui, |ui| self.menu_bar(ui));
        if self.show_toolbar {
            egui::Panel::top("toolbar")
                .frame(frame)
                .show(ui, |ui| self.toolbar(ui));
        }
        egui::Panel::bottom("status")
            .frame(frame)
            .show(ui, |ui| self.status_bar(ui));

        let list_frame = egui::Frame::NONE
            .fill(rgb(self.palette.list_bg))
            .stroke(Stroke::new(1.0, Color32::from_gray(0xa0)));
        egui::Panel::bottom("details")
            .resizable(true)
            .default_size(200.0)
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let selected = self
                    .selected
                    .clone()
                    .and_then(|key| self.find(&key).cloned());
                egui::Panel::left("friends-panel")
                    .resizable(true)
                    .default_size(220.0)
                    .frame(list_frame)
                    .show(ui, |ui| {
                        self.friends_pane(ui);
                    });
                egui::Panel::left("players-panel")
                    .resizable(true)
                    .default_size(380.0)
                    .frame(list_frame)
                    .show(ui, |ui| {
                        self.players_pane(ui, selected.as_ref());
                    });
                egui::CentralPanel::default()
                    .frame(list_frame)
                    .show(ui, |ui| {
                        self.rules_pane(ui, selected.as_ref());
                    });
            });

        egui::Panel::left("tree")
            .resizable(true)
            .default_size(230.0)
            .frame(list_frame.inner_margin(egui::Margin::same(2)))
            .show(ui, |ui| {
                egui::ScrollArea::both()
                    .auto_shrink(false)
                    .show(ui, |ui| self.tree(ui));
            });

        egui::CentralPanel::default()
            .frame(list_frame)
            .show(ui, |ui| self.server_list(ui));

        self.show_dialogs(&ctx);
        self.show_extra(&ctx);

        if self.settings_dirty {
            self.settings_dirty = false;
            if let Err(error) = self.settings.save() {
                self.status = fill(tr("Could not save settings: {}"), &[&error]);
            }
        }
    }
}

// ----- workers ---------------------------------------------------------------

fn run_queries(
    targets: Vec<(String, String)>,
    network: &openase::settings::NetworkSettings,
    tx: &Sender<Msg>,
    ctx: &egui::Context,
    generation: u64,
    stop: &AtomicBool,
    done: &AtomicUsize,
) {
    let options = network.query_options();
    let queue = Mutex::new(targets.into_iter().collect::<VecDeque<_>>());
    std::thread::scope(|scope| {
        for _ in 0..network.parallel_queries.clamp(1, 128) {
            scope.spawn(|| {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let Some((game_id, address)) = queue.lock().unwrap().pop_front() else {
                        break;
                    };
                    let Some(game) = catalog::game(&game_id) else {
                        continue;
                    };
                    if let Ok((host, port)) = parse_address(&address, game.default_port) {
                        let server = query_server(game, &host, port, &options);
                        let _ = tx.send(Msg::Server { generation, server });
                    }
                    done.fetch_add(1, Ordering::Relaxed);
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            });
        }
    });
    let _ = tx.send(Msg::ScanDone { generation });
    ctx.request_repaint();
}

// ----- helpers -----------------------------------------------------------------

fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.all_styles_mut(|style| {
        use egui::FontFamily::{Monospace, Proportional};
        style.text_styles = [
            (TextStyle::Small, FontId::new(10.0, Proportional)),
            (TextStyle::Body, FontId::new(12.5, Proportional)),
            (TextStyle::Button, FontId::new(12.5, Proportional)),
            (TextStyle::Heading, FontId::new(16.0, Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, Monospace)),
        ]
        .into();
        style.spacing.item_spacing = vec2(6.0, 3.0);
        style.spacing.button_padding = vec2(8.0, 2.0);
        style.spacing.interact_size.y = 18.0;
        let visuals = &mut style.visuals;
        visuals.panel_fill = Color32::from_gray(0xf0);
        visuals.window_fill = Color32::from_gray(0xf0);
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        visuals.window_corner_radius = CornerRadius::same(2);
        visuals.menu_corner_radius = CornerRadius::ZERO;
        visuals.window_shadow = egui::Shadow {
            offset: [2, 2],
            blur: 6,
            spread: 0,
            color: Color32::from_black_alpha(60),
        };
        for widgets in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widgets.corner_radius = CornerRadius::ZERO;
        }
        visuals.widgets.inactive.weak_bg_fill = Color32::from_gray(0xe1);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0xad));
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xe5, 0xf1, 0xfb);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x00, 0x78, 0xd7));
    });
}

/// Toolbar texts (French translation pass: keep together).
const TOOLBAR_CUSTOMIZE: &str = noop("Customize toolbar...");
const TOOLBAR_RESET: &str = noop("Reset toolbar");

/// Buttons of the same group are drawn next to each other.
fn toolbar_group(id: &str) -> u8 {
    match id {
        "refresh" | "stop" | "ping" => 0,
        "join" | "favorite" | "add" => 1,
        _ => 2,
    }
}

impl OpenAseApp {
    /// Show these colors (a no-op when they are already applied).
    pub(crate) fn show_palette(&mut self, ctx: &egui::Context, colors: &ColorSettings) {
        if self.palette != *colors {
            apply_palette(ctx, colors);
            self.palette = colors.clone();
        }
    }
}

/// Apply the user's list colors to the egui visuals.
fn apply_palette(ctx: &egui::Context, colors: &ColorSettings) {
    ctx.all_styles_mut(|style| {
        let visuals = &mut style.visuals;
        visuals.extreme_bg_color = rgb(colors.list_bg);
        visuals.faint_bg_color = rgb(colors.alt_row);
        visuals.selection.bg_fill = rgb(colors.selection);
        visuals.widgets.noninteractive.fg_stroke.color = rgb(colors.text);
    });
}

pub(crate) fn rgb([r, g, b]: Rgb) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// One tree line: indentation, optional `[+]`/`[-]` box, icon and label.
/// Returns (expander clicked, label clicked).
fn tree_row(
    ui: &mut egui::Ui,
    depth: usize,
    open: Option<bool>,
    icon: TreeIcon<'_>,
    label: &str,
    selected: bool,
) -> (bool, bool) {
    let height = 18.0;
    let indent = 14.0 * depth as f32;
    let font = TextStyle::Body.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::BLACK);
    let width = indent + 14.0 + 20.0 + galley.size().x + 8.0;
    let (rect, _) = ui.allocate_exact_size(
        vec2(width.max(ui.available_width()), height),
        Sense::hover(),
    );

    let expander_center = egui::pos2(rect.left() + indent + 7.0, rect.center().y);
    let expander_rect = egui::Rect::from_center_size(expander_center, vec2(12.0, 12.0));
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + indent + 14.0, rect.top()),
        vec2(20.0, height),
    );
    let label_rect = egui::Rect::from_min_size(
        egui::pos2(icon_rect.right() + 2.0, rect.top() + 1.0),
        vec2(galley.size().x + 6.0, height - 2.0),
    );

    let mut toggled = false;
    if let Some(open) = open {
        icons::expander(ui.painter(), expander_center, open);
        toggled = ui
            .interact(
                expander_rect,
                ui.scope_id()
                    .with(("expander", label, depth, rect.top() as i32)),
                Sense::click(),
            )
            .clicked();
    }
    icons::tree_icon(ui.painter(), icon_rect, icon);

    let response = ui.interact(
        label_rect.union(icon_rect),
        ui.scope_id()
            .with(("label", label, depth, rect.top() as i32)),
        Sense::click(),
    );
    let text_color = if selected {
        Color32::WHITE
    } else if ui.visuals().text_color() == rgb(ColorSettings::default().text) {
        Color32::from_gray(0x10)
    } else {
        ui.visuals().text_color()
    };
    if selected {
        ui.painter().rect_filled(
            label_rect,
            CornerRadius::ZERO,
            ui.visuals().selection.bg_fill,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(
            label_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0xe5, 0xf1, 0xfb),
        );
    }
    ui.painter().galley(
        label_rect.left_top() + vec2(3.0, (label_rect.height() - galley.size().y) / 2.0),
        galley,
        text_color,
    );

    if response.double_clicked() && open.is_some() {
        toggled = true;
    }
    (toggled, response.clicked())
}

fn header_cell(ui: &egui::Ui, rect: egui::Rect) {
    let rect = rect.expand2(vec2(4.0, 2.0));
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, Color32::from_gray(0xf0));
    ui.painter().line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(0xc8)),
    );
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(0xc8)),
    );
}

fn cell(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.add(
        egui::Label::new(RichText::new(text).color(color))
            .selectable(false)
            .truncate(),
    );
}

/// Draw Quake-colored text; near-white colors are darkened for the white list.
fn colored_label(ui: &mut egui::Ui, raw: &str, default_color: Color32) {
    let default_hex = format!(
        "#{:02x}{:02x}{:02x}",
        default_color.r(),
        default_color.g(),
        default_color.b()
    );
    let font = TextStyle::Body.resolve(ui.style());
    let mut job = LayoutJob::default();
    for segment in quake_color_segments(raw, &default_hex) {
        let [r, g, b] = parse_hex(&segment.color).unwrap_or([0x20, 0x20, 0x20]);
        let color = if r > 0xd0 && g > 0xd0 && b > 0xd0 {
            Color32::from_gray(0x50)
        } else {
            Color32::from_rgb(r, g, b)
        };
        job.append(&segment.text, 0.0, TextFormat::simple(font.clone(), color));
    }
    ui.add(egui::Label::new(job).selectable(false).truncate());
}

fn display_name(server: &Server) -> String {
    if server.online {
        strip_quake_colors(&server.name)
    } else if server.error.is_some() {
        tr("(no response)").to_string()
    } else {
        tr("unscanned").to_string()
    }
}

fn cell_text(col: Col, server: &Server, game: Option<&Game>) -> String {
    let number = |value: Option<u32>| value.map(|v| v.to_string()).unwrap_or_default();
    match col {
        Col::Icon => String::new(),
        Col::Location => {
            if server.location != openase::location::UNKNOWN_LOCATION && !server.location.is_empty()
            {
                server.location.clone()
            } else {
                server
                    .country
                    .as_deref()
                    .and_then(country_name)
                    .unwrap_or_default()
                    .to_string()
            }
        }
        Col::Name => display_name(server),
        Col::Ping => {
            if server.online {
                number(server.ping)
            } else {
                "9999".to_string()
            }
        }
        Col::Flux => server
            .flux
            .map(|flux| format!("±{flux}"))
            .unwrap_or_default(),
        Col::Loss => server
            .packet_loss
            .map(|loss| format!("{loss}%"))
            .unwrap_or_default(),
        Col::Players => {
            if server.online {
                format!(
                    "{}/{}",
                    server.num_players,
                    server
                        .max_players
                        .map_or("?".to_string(), |max| max.to_string())
                )
            } else {
                String::new()
            }
        }
        Col::GameType => {
            // Masters sometimes list servers of a related game (Urban Terror on the
            // Quake 3 master): name the game type with that game's table.
            let naming = catalog::games()
                .iter()
                .find(|other| {
                    !other.base_mod.is_empty()
                        && other.base_mod.eq_ignore_ascii_case(&server.mod_name)
                })
                .or(game);
            let game_type = naming
                .map(|game| game.game_type_name(&server.game_type))
                .unwrap_or_default();
            match (
                game_type.is_empty(),
                game.map(|game| game.launch_mod(&server.mod_name))
                    .unwrap_or_default(),
            ) {
                (true, modname) => modname,
                (false, modname) if modname.is_empty() => game_type,
                (false, modname) => format!("{game_type} ({modname})"),
            }
        }
        Col::Map => server.map.clone(),
        Col::Address => server.join_address(),
    }
}

fn compare(col: Col, a: &Server, b: &Server) -> std::cmp::Ordering {
    let ping = |server: &Server| {
        if server.online {
            server.ping.unwrap_or(9999)
        } else {
            9999
        }
    };
    match col {
        Col::Icon => a.game.cmp(&b.game),
        Col::Ping => ping(a).cmp(&ping(b)),
        Col::Flux => a.flux.unwrap_or(9999).cmp(&b.flux.unwrap_or(9999)),
        Col::Loss => a
            .packet_loss
            .unwrap_or(101)
            .cmp(&b.packet_loss.unwrap_or(101)),
        Col::Players => a
            .num_players
            .cmp(&b.num_players)
            .then_with(|| ping(b).cmp(&ping(a))),
        Col::Address => {
            let parse = |server: &Server| {
                server
                    .host
                    .parse::<std::net::Ipv4Addr>()
                    .ok()
                    .map(u32::from)
                    .unwrap_or(0)
            };
            parse(a).cmp(&parse(b)).then(a.port.cmp(&b.port))
        }
        _ => {
            let game_a = catalog::game(&a.game);
            let game_b = catalog::game(&b.game);
            cell_text(col, a, game_a)
                .to_lowercase()
                .cmp(&cell_text(col, b, game_b).to_lowercase())
        }
    }
    .then_with(|| b.online.cmp(&a.online))
}

/// Server passes every checked filter group of its game.
fn passes_groups(
    game: &Game,
    server: &Server,
    checked: &BTreeMap<String, std::collections::BTreeSet<String>>,
) -> bool {
    game.filters.iter().all(|group| {
        let Some(set) = checked
            .get(&format!("{}/{}", game.id, group.name))
            .filter(|set| !set.is_empty())
        else {
            return true;
        };
        let value = server.rule(&group.key).unwrap_or("");
        if group.dynamic {
            set.iter().any(|option| option.eq_ignore_ascii_case(value))
        } else {
            group
                .options
                .iter()
                .filter(|option| set.contains(&option.label))
                .any(|option| {
                    option
                        .values
                        .iter()
                        .any(|v| v.eq_ignore_ascii_case(value.trim()))
                })
        }
    })
}

/// Most common values of a rule among scanned servers, for dynamic filter groups.
fn dynamic_options(servers: &[Server], key: &str) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for server in servers.iter().filter(|server| server.online) {
        *counts
            .entry(server.rule(key).unwrap_or("").to_string())
            .or_default() += 1;
    }
    let mut values: Vec<(String, usize)> = counts.into_iter().collect();
    values.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    values.truncate(25);
    values.sort_by_key(|a| a.0.to_lowercase());
    values.into_iter().map(|(value, _)| value).collect()
}

fn format_duration(secs: u32) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Play the sound of a friend: the system sound when the global option is on,
/// nothing when the friend is silenced, or the chosen .wav file.
fn play_friend_sound(sound: &FriendSound, global: bool) {
    match sound {
        FriendSound::Default if global => play_sound(None),
        FriendSound::File(path) if !path.trim().is_empty() => play_sound(Some(path.trim())),
        _ => {}
    }
}

/// Play a .wav file asynchronously, or the system "information" sound.
#[cfg(windows)]
pub(crate) fn play_sound(file: Option<&str>) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBeep(kind: u32) -> i32;
    }
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(sound: *const u16, module: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_FILENAME: u32 = 0x0002_0000;
    match file {
        Some(path) => {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: `wide` is NUL-terminated and outlives the call; for a file
            // name, PlaySoundW reads the string before returning.
            unsafe {
                PlaySoundW(
                    wide.as_ptr(),
                    std::ptr::null_mut(),
                    SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
                );
            }
        }
        None => unsafe {
            // MB_ICONASTERISK: the system "information" sound.
            MessageBeep(0x40);
        },
    }
}

/// Play a .wav file, or the desktop's notification sound, with the player
/// programs the system already has (no audio library is linked).
#[cfg(not(windows))]
pub(crate) fn play_sound(file: Option<&str>) {
    let candidates: Vec<(&str, Vec<String>)> = match file {
        Some(path) if cfg!(target_os = "macos") => vec![("afplay", vec![path.to_string()])],
        Some(path) => ["pw-play", "paplay", "aplay"]
            .into_iter()
            .map(|player| (player, vec![path.to_string()]))
            .collect(),
        None if cfg!(target_os = "macos") => vec![(
            "afplay",
            vec!["/System/Library/Sounds/Glass.aiff".to_string()],
        )],
        None => {
            const FREEDESKTOP: &str =
                "/usr/share/sounds/freedesktop/stereo/message-new-instant.oga";
            vec![
                (
                    "canberra-gtk-play",
                    vec!["-i".to_string(), "message-new-instant".to_string()],
                ),
                ("pw-play", vec![FREEDESKTOP.to_string()]),
                ("paplay", vec![FREEDESKTOP.to_string()]),
            ]
        }
    };
    // Try each player in turn off the UI thread, and wait for the one that
    // started so it does not stay a zombie process.
    std::thread::spawn(move || {
        for (program, args) in candidates {
            let started = std::process::Command::new(program)
                .args(&args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            if let Ok(mut child) = started
                && child.wait().is_ok_and(|status| status.success())
            {
                return;
            }
        }
    });
}

/// Fill the GeoIP country of a server once.
fn annotate(geoip: Option<&GeoIp>, server: &mut Server) {
    if server.country.is_none()
        && let Some(geoip) = geoip
    {
        server.country = geoip.lookup_host(&server.host).map(str::to_string);
    }
}

/// Country code used by the Country filter, `??` when unknown.
fn country_code(server: &Server) -> &str {
    server.country.as_deref().unwrap_or("??")
}

fn country_label(code: &str) -> String {
    country_name(code).map_or_else(|| tr("Unknown").to_string(), str::to_string)
}
