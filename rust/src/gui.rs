//! Desktop GUI for OpenASE.
//!
//! The interface follows the original ASE layout: navigation and filters on
//! the left, server list on the top right, and selected server details at the
//! bottom.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, TextFormat, text::LayoutJob};
use egui_extras::{Column, TableBuilder};

use openase::ase_filters::{
    GameDefinition, find_filters_file, find_in_ancestors, parse_filter_paths, parse_game_definitions,
};
use openase::colors::{parse_hex, quake_color_segments, strip_quake_colors};
use openase::games::{GameSettings, Settings, detect_game};
use openase::location::{UNKNOWN_LOCATION, find_countries_file, load_countries, server_location};
use openase::quake3::{
    DEFAULT_MASTER, DEFAULT_MASTER_PORT, DEFAULT_PORT, Quake3Client, Quake3Server, parse_address,
};

const SETTINGS_FILE: &str = "openase_settings.json";
const SCAN_THREADS: usize = 16;
const ROW_HEIGHT: f32 = 20.0;

const SERVER_COLUMNS: [(&str, &str, f32); 7] = [
    ("location", "Location", 100.0),
    ("name", "Name", 360.0),
    ("ping", "Ping", 60.0),
    ("players", "Players", 70.0),
    ("game", "Game", 110.0),
    ("map", "Map", 110.0),
    ("address", "Address", 150.0),
];
const OPTION_TABS: [&str; 8] = ["General", "Misc", "Network", "Events", "DDE", "Colors", "Sounds", "Games"];
const QUAKE3_MODS: [&str; 7] = ["baseq3", "cpma", "osp", "defrag", "ra3", "q3ut4", "quake3arena"];

/// Messages sent from worker threads to the UI thread.
enum Msg {
    Status(String),
    Server(Quake3Server),
    Added(Quake3Server),
    Error(String),
    Done,
    Detected { game: String, path: Option<String> },
}

/// One node of the left navigation tree.
struct NavNode {
    label: String,
    path: String,
    country: Option<String>,
    children: Vec<NavNode>,
}

impl NavNode {
    fn new(label: &str, path: &str) -> Self {
        Self {
            label: label.to_string(),
            path: path.to_string(),
            country: None,
            children: Vec::new(),
        }
    }

    fn child(&mut self, label: &str) -> &mut NavNode {
        let path = format!("{}\\{label}", self.path);
        self.children.push(NavNode::new(label, &path));
        self.children.last_mut().unwrap()
    }
}

/// State of the Options window while it is open.
struct OptionsState {
    tab: usize,
    selected_game: Option<String>,
    draft: BTreeMap<String, GameSettings>,
}

pub struct OpenAseApp {
    client: Quake3Client,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    servers: Vec<Quake3Server>,
    refreshing: bool,

    master: String,
    limit: u32,
    status: String,
    hide_empty: bool,
    hide_full: bool,
    max_ping: String,
    country_filter: Option<String>,

    settings_path: PathBuf,
    open_nav_paths: BTreeSet<String>,
    filters_file: Option<PathBuf>,
    game_definitions: BTreeMap<String, GameDefinition>,
    game_settings: BTreeMap<String, GameSettings>,
    country_names: BTreeSet<String>,
    nav: Vec<NavNode>,
    selected_nav: Option<String>,
    selected_address: Option<String>,

    error: Option<String>,
    info: Option<(String, String)>,
    about_open: bool,
    add_address: Option<String>,
    options: Option<OptionsState>,
}

impl OpenAseApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Light);
        let (tx, rx) = channel();

        let search_roots = search_roots();
        let find = |name: &str| search_roots.iter().find_map(|root| find_in_ancestors(root, name));
        let settings_path = find(SETTINGS_FILE)
            .unwrap_or_else(|| search_roots.first().cloned().unwrap_or_default().join(SETTINGS_FILE));
        let settings_existed = settings_path.is_file();
        let settings = Settings::load(&settings_path);

        let filters_file = search_roots.iter().find_map(|root| find_filters_file(root));
        let game_definitions = filters_file
            .as_deref()
            .and_then(|path| parse_game_definitions(path).ok())
            .unwrap_or_default();
        let country_names = search_roots
            .iter()
            .find_map(|root| find_countries_file(root))
            .and_then(|path| load_countries(&path).ok())
            .map(|countries| countries.into_iter().map(|country| country.name).collect())
            .unwrap_or_default();

        let mut game_settings: BTreeMap<String, GameSettings> = game_definitions
            .keys()
            .map(|name| {
                let mut entry = settings.games.get(name).cloned().unwrap_or_default();
                entry.name = name.clone();
                (name.clone(), entry)
            })
            .collect();
        detect_installed_games(&game_definitions, &mut game_settings);

        let mut open_nav_paths: BTreeSet<String> = settings.open_nav_paths.into_iter().collect();
        if !settings_existed {
            open_nav_paths.extend(["Modifiers", "Modifiers\\Ping", "Modifiers\\Players"].map(String::from));
        }

        let mut app = Self {
            client: Quake3Client::new(Duration::from_secs(2)),
            tx,
            rx,
            servers: Vec::new(),
            refreshing: false,
            master: format!("{DEFAULT_MASTER}:{DEFAULT_MASTER_PORT}"),
            limit: 50,
            status: "Ready".to_string(),
            hide_empty: false,
            hide_full: false,
            max_ping: String::new(),
            country_filter: None,
            settings_path,
            open_nav_paths,
            filters_file,
            game_definitions,
            game_settings,
            country_names,
            nav: Vec::new(),
            selected_nav: None,
            selected_address: None,
            error: None,
            info: None,
            about_open: false,
            add_address: None,
            options: None,
        };
        app.seed_tree();
        app
    }

    // ----- navigation tree -------------------------------------------------

    fn visible_game_names(&self) -> BTreeSet<String> {
        self.game_settings
            .iter()
            .filter(|(_, settings)| settings.visible)
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn seed_tree(&mut self) {
        let visible = self.visible_game_names();
        let mut nav = Vec::new();

        match self.filters_file.as_deref().and_then(|path| parse_filter_paths(path).ok()) {
            None => {
                let mut games = NavNode::new("Games", "Games");
                for name in &visible {
                    games.child(name);
                }
                nav.push(games);

                let mut modifiers = NavNode::new("Modifiers", "Modifiers");
                let ping = modifiers.child("Ping");
                for label in ["less than 50", "less than 100", "less than 150", "less than 200"] {
                    ping.child(label);
                }
                let players = modifiers.child("Players");
                for label in ["Not empty", "Not full", "No bots"] {
                    players.child(label);
                }
                let country_root = modifiers.child("Country");
                for name in std::iter::once(UNKNOWN_LOCATION).chain(self.country_names.iter().map(String::as_str)) {
                    country_root.child(name).country = Some(name.to_string());
                }
                nav.push(modifiers);
            }
            Some(paths) => {
                for path in paths.iter().filter(|path| {
                    visible.contains(path.split('\\').next().unwrap_or(path)) || path.starts_with("Modifiers\\")
                }) {
                    insert_path(&mut nav, path, &self.country_names);
                }
            }
        }

        nav.push(NavNode::new("Favorites", "Favorites"));
        self.nav = nav;
    }

    fn show_nav(&mut self, ui: &mut egui::Ui) {
        let nav = std::mem::take(&mut self.nav);
        let mut toggled = false;
        let mut clicked: Option<(String, Option<String>)> = None;
        for node in &nav {
            self.show_nav_node(ui, node, &mut toggled, &mut clicked);
        }
        self.nav = nav;

        if toggled {
            self.save_settings();
        }
        if let Some((path, country)) = clicked {
            self.selected_nav = Some(path);
            match country {
                Some(country) => {
                    self.status = format!("Country filter: {country}");
                    self.country_filter = Some(country);
                }
                None => self.country_filter = None,
            }
        }
    }

    fn show_nav_node(
        &mut self,
        ui: &mut egui::Ui,
        node: &NavNode,
        toggled: &mut bool,
        clicked: &mut Option<(String, Option<String>)>,
    ) {
        let open = self.open_nav_paths.contains(&node.path);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if node.children.is_empty() {
                ui.add_space(16.0);
            } else if ui
                .add(egui::Button::new(if open { "-" } else { "+" }).frame(false).min_size([14.0, 0.0].into()))
                .clicked()
            {
                if open {
                    self.open_nav_paths.remove(&node.path);
                } else {
                    self.open_nav_paths.insert(node.path.clone());
                }
                *toggled = true;
            }
            let selected = self.selected_nav.as_deref() == Some(node.path.as_str());
            let response = ui.selectable_label(selected, &node.label);
            if response.clicked() {
                *clicked = Some((node.path.clone(), node.country.clone()));
            }
            if response.double_clicked() && !node.children.is_empty() {
                if open {
                    self.open_nav_paths.remove(&node.path);
                } else {
                    self.open_nav_paths.insert(node.path.clone());
                }
                *toggled = true;
            }
        });
        if open && !node.children.is_empty() {
            ui.indent(&node.path, |ui| {
                for child in &node.children {
                    self.show_nav_node(ui, child, toggled, clicked);
                }
            });
        }
    }

    // ----- server scanning -------------------------------------------------

    fn refresh_servers(&mut self, ctx: &egui::Context) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        self.status = "Querying master server...".to_string();
        self.servers.clear();
        self.selected_address = None;

        let master = self.master.clone();
        let limit = self.limit.max(1) as usize;
        let client = self.client.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let send = |msg: Msg| {
                let _ = tx.send(msg);
                ctx.request_repaint();
            };
            match refresh_worker(&master, limit, &client, &send) {
                Ok(()) => {}
                Err(error) => send(Msg::Error(error)),
            }
            send(Msg::Done);
        });
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Status(text) => self.status = text,
                Msg::Server(server) => self.servers.push(server),
                Msg::Added(server) => {
                    self.status = format!("Added {}", server.address());
                    self.servers.push(server);
                }
                Msg::Error(text) => self.error = Some(text),
                Msg::Done => {
                    self.refreshing = false;
                    let online = self.servers.iter().filter(|server| server.online);
                    let (count, players) = online.fold((0, 0), |(count, players), server| {
                        (count + 1, players + server.players.len())
                    });
                    self.status = format!("{count} servers online, {players} players visible");
                }
                Msg::Detected { game, path } => match path {
                    Some(path) => {
                        if let Some(entry) = self.options.as_mut().and_then(|options| options.draft.get_mut(&game)) {
                            entry.program_location = path;
                            entry.visible = true;
                        }
                        self.status = format!("Detected {game}");
                    }
                    None => {
                        self.info = Some(("Auto-Search".to_string(), "No verified installation found.".to_string()))
                    }
                },
            }
        }
    }

    // ----- filtering, sorting, values -------------------------------------

    fn max_ping_limit(&self) -> Result<Option<i32>, ()> {
        let raw = self.max_ping.trim();
        if raw.is_empty() {
            return Ok(None);
        }
        raw.parse().map(Some).map_err(|_| ())
    }

    fn passes_filters(&self, server: &Quake3Server, max_ping: Option<i32>) -> bool {
        if !server.online {
            return true;
        }
        if self.hide_empty && server.players.is_empty() {
            return false;
        }
        if let Some(country) = &self.country_filter
            && server_location(&server.info) != *country
        {
            return false;
        }
        if self.hide_full
            && let Some(max_clients) = max_clients(server)
            && server.players.len() >= max_clients
        {
            return false;
        }
        if let (Some(limit), Some(ping)) = (max_ping, server_ping(server))
            && ping > limit
        {
            return false;
        }
        true
    }

    fn sort_servers(&mut self, column: usize) {
        let numeric = matches!(SERVER_COLUMNS[column].0, "ping" | "players");
        self.servers.sort_by_cached_key(|server| {
            let value = server_values(server)[column].clone();
            if numeric {
                let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
                (digits.parse::<u64>().unwrap_or(999_999), String::new())
            } else {
                (0, value.to_lowercase())
            }
        });
    }

    fn game_settings_for_server(&self, server: &Quake3Server) -> Option<&GameSettings> {
        let game = server
            .info
            .get("gamename")
            .or_else(|| server.info.get("com_gamename"))
            .map(String::as_str)
            .unwrap_or("");
        if QUAKE3_MODS.contains(&game.to_lowercase().as_str()) {
            return self.game_settings.get("Quake 3: Arena");
        }
        self.game_settings.get(game)
    }

    // ----- settings ----------------------------------------------------------

    fn save_settings(&mut self) {
        let settings = Settings {
            open_nav_paths: self.open_nav_paths.iter().cloned().collect(),
            games: self.game_settings.clone(),
        };
        if let Err(error) = settings.save(&self.settings_path) {
            self.status = format!("Could not save settings: {error}");
        }
    }

    // ----- UI sections -------------------------------------------------------

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Exit").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Server", |ui| {
                if ui.add_enabled(!self.refreshing, egui::Button::new("Refresh")).clicked() {
                    self.refresh_servers(ui.ctx());
                }
                if ui.button("Add Server").clicked() {
                    self.open_add_dialog();
                }
                ui.separator();
                if ui.button("Options").clicked() {
                    self.open_options();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About").clicked() {
                    self.about_open = true;
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.refreshing, egui::Button::new("Refresh")).clicked() {
                self.refresh_servers(ui.ctx());
            }
            if ui.button("Add").clicked() {
                self.open_add_dialog();
            }
            ui.add_space(8.0);
            ui.label("Master");
            ui.add(egui::TextEdit::singleline(&mut self.master).desired_width(220.0));
            ui.add_space(8.0);
            ui.label("Limit");
            ui.add(egui::DragValue::new(&mut self.limit).range(1..=500));
            ui.add_space(8.0);
            ui.checkbox(&mut self.hide_empty, "Not empty");
            ui.checkbox(&mut self.hide_full, "Not full");
            ui.add_space(8.0);
            ui.label("Max ping");
            ui.add(egui::TextEdit::singleline(&mut self.max_ping).desired_width(50.0));
            if self.max_ping_limit().is_err() {
                ui.colored_label(Color32::RED, "Max ping must be a number");
            }
        });
    }

    fn server_table(&mut self, ui: &mut egui::Ui) {
        let max_ping = self.max_ping_limit().unwrap_or(None);
        let visible: Vec<usize> = (0..self.servers.len())
            .filter(|&index| self.passes_filters(&self.servers[index], max_ping))
            .collect();
        let text_color = ui.visuals().text_color();
        let mut sort_column = None;
        let mut clicked_address = None;

        let mut table = TableBuilder::new(ui)
            .id_salt("servers")
            .striped(true)
            .resizable(true)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .auto_shrink(false);
        for (index, (_, _, width)) in SERVER_COLUMNS.iter().enumerate() {
            let column = if index + 1 == SERVER_COLUMNS.len() {
                Column::remainder()
            } else {
                Column::initial(*width)
            };
            table = table.column(column.at_least(30.0).clip(true));
        }

        table
            .header(22.0, |mut header| {
                for (index, (_, label, _)) in SERVER_COLUMNS.iter().enumerate() {
                    header.col(|ui| {
                        if ui.add(egui::Button::new(RichText::new(*label).strong()).frame(false)).clicked() {
                            sort_column = Some(index);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, visible.len(), |mut row| {
                    let server = &self.servers[visible[row.index()]];
                    let address = server.address();
                    row.set_selected(self.selected_address.as_deref() == Some(address.as_str()));
                    let values = server_values(server);
                    for (index, (key, _, _)) in SERVER_COLUMNS.iter().enumerate() {
                        row.col(|ui| {
                            if *key == "name" && server.online {
                                colored_label(ui, raw_server_name(server), text_color);
                            } else if matches!(*key, "ping" | "players") {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| plain_label(ui, &values[index]));
                            } else {
                                plain_label(ui, &values[index]);
                            }
                        });
                    }
                    if row.response().clicked() {
                        clicked_address = Some(address);
                    }
                });
            });

        if let Some(column) = sort_column {
            self.sort_servers(column);
        }
        if clicked_address.is_some() {
            self.selected_address = clicked_address;
        }
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        let selected = self
            .selected_address
            .as_ref()
            .and_then(|address| self.servers.iter().find(|server| &server.address() == address));
        let draw_game_font = selected
            .and_then(|server| self.game_settings_for_server(server))
            .is_none_or(|settings| settings.draw_player_names_with_game_font);
        let text_color = ui.visuals().text_color();
        let empty = Quake3Server::default();
        let server = selected.unwrap_or(&empty);

        ui.columns(2, |columns| {
            TableBuilder::new(&mut columns[0])
                .id_salt("players")
                .striped(true)
                .resizable(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .auto_shrink(false)
                .column(Column::initial(240.0).at_least(60.0).clip(true))
                .column(Column::initial(70.0).at_least(40.0))
                .column(Column::remainder().at_least(40.0))
                .header(22.0, |mut header| {
                    for label in ["Player Name", "Score", "Ping"] {
                        header.col(|ui| {
                            ui.strong(label);
                        });
                    }
                })
                .body(|body| {
                    body.rows(ROW_HEIGHT, server.players.len(), |mut row| {
                        let player = &server.players[row.index()];
                        row.col(|ui| {
                            if draw_game_font {
                                colored_label(ui, &player.name, text_color);
                            } else {
                                plain_label(ui, &strip_quake_colors(&player.name));
                            }
                        });
                        for value in [player.score, player.ping] {
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    plain_label(ui, &value.to_string())
                                });
                            });
                        }
                    });
                });

            let entries: Vec<(&String, &String)> = server.info.iter().collect();
            TableBuilder::new(&mut columns[1])
                .id_salt("info")
                .striped(true)
                .resizable(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .auto_shrink(false)
                .column(Column::initial(190.0).at_least(60.0).clip(true))
                .column(Column::remainder().clip(true))
                .header(22.0, |mut header| {
                    for label in ["Key", "Value"] {
                        header.col(|ui| {
                            ui.strong(label);
                        });
                    }
                })
                .body(|body| {
                    body.rows(ROW_HEIGHT, entries.len(), |mut row| {
                        let (key, value) = entries[row.index()];
                        row.col(|ui| plain_label(ui, key));
                        row.col(|ui| plain_label(ui, value));
                    });
                });
        });
    }

    // ----- dialogs ---------------------------------------------------------

    fn open_add_dialog(&mut self) {
        self.add_address = Some(format!("phobos.ioquake3.org:{DEFAULT_PORT}"));
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        let Some(address) = self.add_address.as_mut() else { return };
        let mut open = true;
        let mut submit = false;
        let mut cancel = false;
        egui::Window::new("Add Quake 3 Server")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Address");
                    let response = ui.add(egui::TextEdit::singleline(address).desired_width(240.0));
                    if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                });
                ui.horizontal(|ui| {
                    submit |= ui.button("Add").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });

        if submit {
            match parse_address(address.trim(), DEFAULT_PORT) {
                Ok((host, port)) => {
                    self.status = format!("Querying {host}:{port}...");
                    let client = self.client.clone();
                    let tx = self.tx.clone();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(Msg::Added(client.status(&host, port)));
                        ctx.request_repaint();
                    });
                    self.add_address = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        } else if cancel || !open {
            self.add_address = None;
        }
    }

    fn open_options(&mut self) {
        self.options = Some(OptionsState {
            tab: OPTION_TABS.len() - 1,
            selected_game: None,
            draft: self.game_settings.clone(),
        });
    }

    fn options_dialog(&mut self, ctx: &egui::Context) {
        let Some(options) = self.options.as_mut() else { return };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let mut auto_search: Option<String> = None;

        egui::Window::new("Options")
            .collapsible(false)
            .default_size([560.0, 430.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, tab) in OPTION_TABS.iter().enumerate() {
                        ui.selectable_value(&mut options.tab, index, *tab);
                    }
                });
                ui.separator();

                if OPTION_TABS[options.tab] != "Games" {
                    ui.label(format!("{} options will be implemented when needed.", OPTION_TABS[options.tab]));
                    ui.add_space(300.0);
                } else {
                    ui.horizontal_top(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("options_games")
                            .min_scrolled_height(320.0)
                            .max_height(320.0)
                            .show(ui, |ui| {
                                ui.set_width(220.0);
                                for (heading, installed) in [("Installed games", true), ("Not Installed", false)] {
                                    ui.strong(heading);
                                    ui.indent(heading, |ui| {
                                        for (name, settings) in &options.draft {
                                            if settings.detected == installed {
                                                let selected = options.selected_game.as_deref() == Some(name.as_str());
                                                if ui.selectable_label(selected, name).clicked() {
                                                    options.selected_game = Some(name.clone());
                                                }
                                            }
                                        }
                                    });
                                }
                            });
                        ui.separator();
                        ui.vertical(|ui| {
                            let Some(name) = options.selected_game.clone() else {
                                ui.label("Select a game.");
                                return;
                            };
                            let Some(settings) = options.draft.get_mut(&name) else { return };
                            ui.checkbox(&mut settings.visible, "Visible in filter list");
                            ui.add_space(6.0);
                            ui.label("Player name");
                            ui.text_edit_singleline(&mut settings.player_name);
                            ui.label("Config file");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut settings.config_file);
                                if ui.button("Browse").clicked()
                                    && let Some(path) = rfd::FileDialog::new().set_title("Select config file").pick_file()
                                {
                                    settings.config_file = path.display().to_string();
                                }
                            });
                            ui.label("Program location");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut settings.program_location);
                                if ui.button("Browse").clicked()
                                    && let Some(path) = rfd::FileDialog::new()
                                        .set_title("Select game executable")
                                        .add_filter("Executable", &["exe"])
                                        .add_filter("All files", &["*"])
                                        .pick_file()
                                {
                                    settings.program_location = path.display().to_string();
                                    settings.visible = true;
                                }
                            });
                            if ui.button("Auto-Search").clicked() {
                                auto_search = Some(name.clone());
                            }
                            ui.add_space(6.0);
                            ui.checkbox(&mut settings.allow_invitations, "allow invitations for this game");
                            ui.checkbox(&mut settings.draw_player_names_with_game_font, "draw player names with game font");
                        });
                    });
                }

                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    save = ui.button("OK").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });

        if let Some(game) = auto_search
            && let Some(definition) = self.game_definitions.get(&game).cloned()
        {
            self.status = format!("Searching for {game}...");
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let path = detect_game(&definition, true);
                let _ = tx.send(Msg::Detected { game, path });
                ctx.request_repaint();
            });
        }

        if save {
            if let Some(options) = self.options.take() {
                self.game_settings = options.draft;
            }
            self.save_settings();
            self.seed_tree();
        } else if cancel || !open {
            self.options = None;
        }
    }

    fn message_windows(&mut self, ctx: &egui::Context) {
        let mut close_error = false;
        if let Some(error) = &self.error {
            egui::Window::new("OpenASE").collapsible(false).resizable(false).show(ctx, |ui| {
                ui.colored_label(Color32::RED, error);
                close_error = ui.button("OK").clicked();
            });
        }
        if close_error {
            self.error = None;
        }

        let mut close_info = false;
        if let Some((title, message)) = &self.info {
            egui::Window::new(title.as_str()).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(message);
                close_info = ui.button("OK").clicked();
            });
        }
        if close_info {
            self.info = None;
        }

        egui::Window::new("About OpenASE")
            .collapsible(false)
            .resizable(false)
            .open(&mut self.about_open)
            .show(ctx, |ui| {
                ui.strong("OpenASE");
                ui.label("Quake 3 server browser revival.");
                ui.add_space(6.0);
                ui.label("This is a new implementation and does not use Yahoo code.");
                ui.label("Licensed under the GNU GPL v3 or later.");
            });
    }
}

impl eframe::App for OpenAseApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_messages();
        let ctx = ui.ctx().clone();

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(3.0);
            self.toolbar(ui);
            ui.add_space(3.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.refreshing {
                    ui.spinner();
                }
                ui.label(&self.status);
            });
        });
        egui::Panel::left("nav").resizable(true).default_size(230.0).show(ui, |ui| {
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| self.show_nav(ui));
        });
        egui::Panel::bottom("details").resizable(true).default_size(220.0).show(ui, |ui| self.details(ui));
        egui::CentralPanel::default().show(ui, |ui| self.server_table(ui));

        self.add_dialog(&ctx);
        self.options_dialog(&ctx);
        self.message_windows(&ctx);
    }
}

// ----- helpers ---------------------------------------------------------------

/// Directories searched for settings and original ASE data files.
fn search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        roots.push(dir);
    }
    if let Ok(dir) = std::env::current_dir() {
        roots.push(dir);
    }
    roots
}

fn detect_installed_games(
    definitions: &BTreeMap<String, GameDefinition>,
    settings: &mut BTreeMap<String, GameSettings>,
) {
    for (name, definition) in definitions {
        let Some(entry) = settings.get_mut(name) else { continue };
        if !entry.program_location.is_empty() && Path::new(&entry.program_location).exists() {
            entry.detected = true;
            entry.visible = true;
        } else if let Some(path) = detect_game(definition, false) {
            entry.detected = true;
            entry.visible = true;
            entry.program_location = path;
        }
    }
}

fn insert_path(nav: &mut Vec<NavNode>, path: &str, country_names: &BTreeSet<String>) {
    let mut level = nav;
    let mut current = String::new();
    for part in path.split('\\') {
        if !current.is_empty() {
            current.push('\\');
        }
        current.push_str(part);
        let index = match level.iter().position(|node| node.label == part) {
            Some(index) => index,
            None => {
                let mut node = NavNode::new(part, &current);
                if current.starts_with("Modifiers\\Country\\") && country_names.contains(part) {
                    node.country = Some(part.to_string());
                }
                level.push(node);
                level.len() - 1
            }
        };
        level = &mut level[index].children;
    }
}

fn refresh_worker(master: &str, limit: usize, client: &Quake3Client, send: &(dyn Fn(Msg) + Sync)) -> Result<(), String> {
    let (host, port) = parse_address(master.trim(), DEFAULT_MASTER_PORT).map_err(|error| error.to_string())?;
    let master_client = Quake3Client::new(Duration::from_secs(3));
    let mut addresses = master_client
        .master_servers(&host, port, 68)
        .map_err(|error| format!("Master server {host}:{port}: {error}"))?;
    addresses.truncate(limit);
    send(Msg::Status(format!("Scanning {} servers...", addresses.len())));

    let queue = Arc::new(Mutex::new(addresses.into_iter().collect::<VecDeque<_>>()));
    std::thread::scope(|scope| {
        for _ in 0..SCAN_THREADS {
            let queue = Arc::clone(&queue);
            scope.spawn(move || {
                loop {
                    let Some(address) = queue.lock().unwrap().pop_front() else { break };
                    if let Ok((host, port)) = parse_address(&address, DEFAULT_PORT) {
                        send(Msg::Server(client.status(&host, port)));
                    }
                }
            });
        }
    });
    Ok(())
}

fn raw_server_name(server: &Quake3Server) -> &str {
    server
        .info
        .get("sv_hostname")
        .or_else(|| server.info.get("hostname"))
        .filter(|name| !name.is_empty())
        .map(String::as_str)
        .unwrap_or("unknown")
}

fn max_clients(server: &Quake3Server) -> Option<usize> {
    server
        .info
        .get("sv_maxclients")
        .or_else(|| server.info.get("sv_maxClients"))
        .and_then(|raw| raw.trim().parse().ok())
}

/// Lowest positive player ping, used as a proxy for the server ping.
fn server_ping(server: &Quake3Server) -> Option<i32> {
    server.players.iter().map(|player| player.ping).filter(|&ping| ping > 0).min()
}

fn server_values(server: &Quake3Server) -> [String; 7] {
    if !server.online {
        return [
            UNKNOWN_LOCATION.to_string(),
            "offline".to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            server.address(),
        ];
    }
    let game = server
        .info
        .get("gamename")
        .or_else(|| server.info.get("com_gamename"))
        .filter(|game| !game.is_empty())
        .cloned()
        .unwrap_or_else(|| "Quake3".to_string());
    let max = max_clients(server).map_or("?".to_string(), |max| max.to_string());
    [
        server_location(&server.info),
        strip_quake_colors(raw_server_name(server)),
        server_ping(server).map(|ping| ping.to_string()).unwrap_or_default(),
        format!("{}/{max}", server.players.len()),
        game,
        server.info.get("mapname").cloned().unwrap_or_default(),
        server.address(),
    ]
}

fn plain_label(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(text).selectable(false).truncate());
}

/// Draw Quake-colored text. Near-white colors are darkened slightly so they stay
/// readable on the light background.
fn colored_label(ui: &mut egui::Ui, raw: &str, default_color: Color32) {
    let default_hex = format!("#{:02x}{:02x}{:02x}", default_color.r(), default_color.g(), default_color.b());
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = LayoutJob::default();
    for segment in quake_color_segments(raw, &default_hex) {
        let [r, g, b] = parse_hex(&segment.color).unwrap_or([0x20, 0x20, 0x20]);
        let color = if r > 0xe0 && g > 0xe0 && b > 0xe0 {
            Color32::from_rgb(0x60, 0x60, 0x60)
        } else {
            Color32::from_rgb(r, g, b)
        };
        job.append(&segment.text, 0.0, TextFormat::simple(font.clone(), color));
    }
    ui.add(egui::Label::new(job).selectable(false).truncate());
}
