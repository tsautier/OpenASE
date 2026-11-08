//! Dialog windows: Options (General, Network, Events, Games), Auto-Search,
//! Add Server, Find Player, Friends, password prompt, notices and About.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Align2, Color32, Layout, RichText, vec2};
use egui_extras::{Column, TableBuilder};

use openase::catalog;
use openase::colors::strip_quake_colors;
use openase::detect;
use openase::i18n::{self, Language, fill, noop, tr};
use openase::settings::{ColorSettings, FriendSound, NetworkSettings, Settings, ToolbarSettings};

use super::{Msg, Nav, OpenAseApp, ServerKey, play_sound, rgb};

const OPTION_TABS: [&str; 6] = [
    noop("General"),
    noop("Network"),
    noop("Events"),
    noop("Games"),
    noop("Toolbar"),
    noop("Colors"),
];
pub(super) const TOOLBAR_TAB: usize = 4;

/// Per-friend sound texts (French translation pass: keep together).
const SOUND_DEFAULT: &str = noop("Default sound");
const SOUND_NONE: &str = noop("No sound");
const SOUND_FILE: &str = noop("Sound file...");
const SOUND_PATH_HINT: &str = noop("Path of a .wav file");
const SOUND_BROWSE: &str = noop("Browse...");
const SOUND_TEST: &str = noop("Play");

/// Toolbar tab texts.
const TOOLBAR_HELP: &str = noop(
    "Choose the buttons of the toolbar and their order. You can also right-click the toolbar.",
);
const TOOLBAR_UP: &str = noop("Up");
const TOOLBAR_DOWN: &str = noop("Down");
const TOOLBAR_RESET_ALL: &str = noop("Reset to default");

/// Colors tab texts.
const COLORS_HELP: &str = noop("Changes are shown right away; Cancel puts the old colors back.");
const COLORS_LIST_BG: &str = noop("List background");
const COLORS_ALT_ROW: &str = noop("Alternating row");
const COLORS_SELECTION: &str = noop("Selection");
const COLORS_TEXT: &str = noop("Text");
const COLORS_QUAKE: &str = noop("Draw Quake color codes in server and player names");
const COLORS_RESET: &str = noop("Reset to defaults");

/// Name of a toolbar button in Options and in the toolbar menu.
pub(super) fn toolbar_label(id: &str) -> &'static str {
    match id {
        "refresh" => noop("Refresh server list"),
        "stop" => noop("Stop"),
        "ping" => noop("Refresh selected server"),
        "join" => noop("Join server"),
        "favorite" => noop("Add to / remove from favorites"),
        "add" => noop("Add server"),
        "find" => noop("Find player"),
        "friends" => noop("Friends"),
        "options" => noop("Options"),
        _ => "",
    }
}

pub struct OptionsDraft {
    tab: usize,
    settings: Settings,
    selected_game: Option<String>,
}

pub struct SearchState {
    game: String,
    exe: String,
    stop: Arc<AtomicBool>,
    current: String,
    found: Vec<String>,
    selected: Option<usize>,
    done: bool,
}

#[derive(Default)]
pub struct Dialogs {
    pub error: Option<String>,
    pub about: bool,
    pub add: Option<(String, String)>,
    pub find: Option<String>,
    pub friends: Option<String>,
    pub password: Option<(ServerKey, String)>,
    pub options: Option<OptionsDraft>,
    pub search: Option<SearchState>,
    notices: Vec<(Instant, String)>,
}

impl Dialogs {
    pub fn notify(&mut self, text: String) {
        self.notices.push((Instant::now(), text));
        if self.notices.len() > 5 {
            self.notices.remove(0);
        }
    }

    /// Language chosen in the open Options window, shown before OK.
    pub fn preview_language(&self) -> Option<Language> {
        self.options.as_ref().map(|draft| draft.settings.language)
    }

    pub fn has_notices(&self) -> bool {
        !self.notices.is_empty()
    }

    pub fn any_modal(&self) -> bool {
        self.error.is_some()
            || self.add.is_some()
            || self.find.is_some()
            || self.friends.is_some()
            || self.password.is_some()
            || self.options.is_some()
    }

    pub fn open_add(&mut self, game: Option<&str>) {
        let game = game.unwrap_or(&catalog::games()[0].id).to_string();
        self.add = Some((game, String::new()));
    }

    pub fn open_options_at(&mut self, settings: &Settings, tab: usize) {
        self.open_options(settings);
        if let Some(options) = &mut self.options {
            options.tab = tab.min(OPTION_TABS.len() - 1);
        }
    }

    pub fn open_options(&mut self, settings: &Settings) {
        self.options = Some(OptionsDraft {
            tab: 0,
            settings: settings.clone(),
            selected_game: None,
        });
    }

    pub fn search_progress(&mut self, dir: String) {
        if let Some(search) = &mut self.search {
            search.current = dir;
        }
    }

    pub fn search_found(&mut self, path: String) {
        if let Some(search) = &mut self.search
            && !search.found.contains(&path)
        {
            search.found.push(path);
            search.selected.get_or_insert(0);
        }
    }

    pub fn search_done(&mut self) {
        if let Some(search) = &mut self.search {
            search.done = true;
            search.current = if search.found.is_empty() {
                tr("No installation found.").into()
            } else {
                tr("Search complete.").into()
            };
        }
    }
}

/// A modal window titled `title` (translated); the English title is its id, so
/// the window keeps its place when the language changes.
fn dialog(title: &'static str) -> egui::Window<'static> {
    egui::Window::new(tr(title))
        .id(egui::Id::unique(title))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
}

/// A row of buttons aligned right. The `horizontal` wrapper keeps the row one
/// button high: a bare right-to-left layout takes all the height left, which
/// stretches auto-sized windows.
pub(super) fn buttons_right(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    ui.horizontal(|ui| ui.with_layout(Layout::right_to_left(Align::Center), add));
}

impl OpenAseApp {
    pub(super) fn show_dialogs(&mut self, ctx: &egui::Context) {
        self.options_window(ctx);
        self.search_window(ctx);
        self.add_window(ctx);
        self.find_window(ctx);
        self.friends_window(ctx);
        self.password_window(ctx);
        self.about_window(ctx);
        self.notices(ctx);
        self.error_window(ctx);
    }

    fn error_window(&mut self, ctx: &egui::Context) {
        let Some(message) = self.dialogs.error.clone() else {
            return;
        };
        dialog("OpenASE").show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(message);
            buttons_right(ui, |ui| {
                if ui.button("   OK   ").clicked() {
                    self.dialogs.error = None;
                }
            });
        });
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.dialogs.about;
        dialog("About OpenASE").open(&mut open).show(ctx, |ui| {
            ui.heading("OpenASE");
            ui.label(fill(tr("Version {}"), &[&env!("CARGO_PKG_VERSION")]));
            ui.add_space(6.0);
            ui.label(tr(
                "An open-source game server browser in the spirit of The All-Seeing Eye.",
            ));
            ui.label(tr(
                "New code, not affiliated with Yahoo or the original authors.",
            ));
            ui.label("Copyright (C) 2026 Thomas Sautier");
            ui.label(tr("Free software under the GNU GPL v3 or later."));
            // 333networks asks API users to credit it in the application.
            ui.label(tr(
                "Server lists from 333networks, OpenSpy, dpmaster and the other community master servers.",
            ));
            ui.hyperlink_to("333networks.com", "https://333networks.com/");
            ui.hyperlink_to(openase::geoip::ATTRIBUTION, openase::geoip::ATTRIBUTION_URL);
            ui.hyperlink_to("openase.org", "https://openase.org/");
        });
        self.dialogs.about = open;
    }

    fn notices(&mut self, ctx: &egui::Context) {
        self.dialogs
            .notices
            .retain(|(at, _)| at.elapsed() < Duration::from_secs(10));
        if self.dialogs.notices.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::unique("notices"))
            .anchor(Align2::RIGHT_BOTTOM, [-12.0, -40.0])
            .show(ctx, |ui| {
                for (_, text) in &self.dialogs.notices {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(RichText::new(text).color(Color32::from_rgb(0x14, 0x64, 0x1e)));
                    });
                }
            });
    }

    fn add_window(&mut self, ctx: &egui::Context) {
        let Some((mut game, mut address)) = self.dialogs.add.clone() else {
            return;
        };
        let mut close = false;
        let mut submit = false;
        dialog("Add Server").show(ctx, |ui| {
            egui::Grid::new("add-grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("Game"));
                    egui::ComboBox::from_id_salt("add-game")
                        .width(240.0)
                        .selected_text(catalog::game(&game).map_or("", |g| g.name.as_str()))
                        .show_ui(ui, |ui| {
                            for entry in catalog::games() {
                                ui.selectable_value(&mut game, entry.id.clone(), &entry.name);
                            }
                        });
                    ui.end_row();
                    ui.label(tr("Address"));
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut address)
                            .hint_text("host:port")
                            .desired_width(240.0),
                    );
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        submit = true;
                    }
                    ui.end_row();
                });
            buttons_right(ui, |ui| {
                close |= ui.button(tr("Cancel")).clicked();
                submit |= ui.button(tr("   Add   ")).clicked();
            });
        });
        if submit {
            match self.add_server(ctx, &game, &address) {
                Ok(()) => self.dialogs.add = None,
                Err(error) => self.dialogs.error = Some(error),
            }
        } else if close {
            self.dialogs.add = None;
        } else {
            self.dialogs.add = Some((game, address));
        }
    }

    fn password_window(&mut self, ctx: &egui::Context) {
        let Some((key, mut password)) = self.dialogs.password.clone() else {
            return;
        };
        let mut join = false;
        let mut cancel = false;
        dialog("Password required").show(ctx, |ui| {
            ui.label(tr("This server needs a password."));
            let response = ui.add(
                egui::TextEdit::singleline(&mut password)
                    .password(true)
                    .desired_width(220.0),
            );
            response.request_focus();
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                join = true;
            }
            buttons_right(ui, |ui| {
                cancel = ui.button(tr("Cancel")).clicked();
                join |= ui.button(tr("   Join   ")).clicked();
            });
        });
        if join {
            self.dialogs.password = None;
            self.join_with_password(&key, &password);
        } else if cancel {
            self.dialogs.password = None;
        } else {
            self.dialogs.password = Some((key, password));
        }
    }

    fn friends_window(&mut self, ctx: &egui::Context) {
        let Some(mut new_name) = self.dialogs.friends.clone() else {
            return;
        };
        let mut open = true;
        let mut remove: Option<usize> = None;
        let mut add = false;
        let mut sound_change: Option<(String, FriendSound)> = None;
        dialog("Friends").open(&mut open).show(ctx, |ui| {
            ui.label(tr(
                "OpenASE looks for these names on every server it scans.",
            ));
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    ui.set_min_width(440.0);
                    if self.settings.friends.is_empty() {
                        ui.weak(tr("No friends yet."));
                    }
                    for (index, friend) in self.settings.friends.iter().enumerate() {
                        let mut sound = self.settings.friend_sound(friend);
                        let before = sound.clone();
                        ui.horizontal(|ui| {
                            let online = self.friend_location(friend).is_some();
                            ui.colored_label(
                                if online {
                                    Color32::from_rgb(0x14, 0x78, 0x1e)
                                } else {
                                    Color32::from_gray(0x60)
                                },
                                if online { tr("online") } else { tr("offline") },
                            );
                            ui.label(friend);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button(tr("Remove")).clicked() {
                                    remove = Some(index);
                                }
                                sound_combo(ui, ("friend-sound", index), &mut sound);
                            });
                        });
                        if let FriendSound::File(path) = &mut sound {
                            ui.horizontal(|ui| {
                                ui.add_space(52.0);
                                ui.add(
                                    egui::TextEdit::singleline(path)
                                        .hint_text(tr(SOUND_PATH_HINT))
                                        .desired_width(230.0),
                                );
                                if ui.small_button(tr(SOUND_BROWSE)).clicked()
                                    && let Some(file) = rfd::FileDialog::new()
                                        .set_title(tr("Select a sound"))
                                        .add_filter(tr("WAV sound"), &["wav"])
                                        .pick_file()
                                {
                                    *path = file.display().to_string();
                                }
                                if ui.small_button(tr(SOUND_TEST)).clicked() {
                                    play_sound(Some(path.trim()));
                                }
                            });
                        }
                        if sound != before {
                            sound_change = Some((friend.to_lowercase(), sound));
                        }
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut new_name)
                        .hint_text(tr("Player name"))
                        .desired_width(220.0),
                );
                add = ui.button(tr("Add")).clicked()
                    || (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
            });
        });
        if let Some((name, sound)) = sound_change {
            if sound == FriendSound::Default {
                self.settings.friend_sounds.remove(&name);
            } else {
                self.settings.friend_sounds.insert(name, sound);
            }
            self.mark_dirty();
        }
        if let Some(index) = remove {
            self.settings
                .friend_sounds
                .remove(&self.settings.friends[index].to_lowercase());
            self.settings.friends.remove(index);
            self.mark_dirty();
        }
        let name = new_name.trim().to_string();
        if add
            && !name.is_empty()
            && !self
                .settings
                .friends
                .iter()
                .any(|friend| friend.eq_ignore_ascii_case(&name))
        {
            self.settings.friends.push(name);
            new_name.clear();
            self.mark_dirty();
            // Re-check servers already scanned.
            let servers: Vec<_> = self.store.values().flatten().cloned().collect();
            for server in servers {
                self.check_friends(&server);
            }
        }
        self.dialogs.friends = open.then_some(new_name);
    }

    fn find_window(&mut self, ctx: &egui::Context) {
        let Some(mut query) = self.dialogs.find.clone() else {
            return;
        };
        let mut open = true;
        let mut select: Option<ServerKey> = None;
        dialog("Find Player")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Player name contains"));
                    ui.add(egui::TextEdit::singleline(&mut query).desired_width(200.0))
                        .request_focus();
                });
                let needle = query.trim().to_lowercase();
                let mut results: Vec<(String, String, String, ServerKey)> = Vec::new();
                if needle.len() >= 2 {
                    for server in self.store.values().flatten().filter(|server| server.online) {
                        for player in &server.players {
                            let name = strip_quake_colors(&player.name);
                            if name.to_lowercase().contains(&needle) {
                                let game = catalog::game(&server.game)
                                    .map_or(String::new(), |game| game.short.clone());
                                results.push((
                                    name,
                                    strip_quake_colors(&server.name),
                                    game,
                                    super::ServerKey::of(server),
                                ));
                            }
                        }
                    }
                }
                ui.label(fill(
                    tr("{} players found in scanned servers"),
                    &[&results.len()],
                ));
                ui.set_min_size(vec2(520.0, 260.0));
                TableBuilder::new(ui)
                    .id_salt("find-results")
                    .striped(true)
                    .sense(egui::Sense::click())
                    .auto_shrink(false)
                    .column(Column::initial(160.0).clip(true))
                    .column(Column::initial(270.0).clip(true))
                    .column(Column::remainder())
                    .header(18.0, |mut header| {
                        for label in [tr("Player"), tr("Server"), tr("Game")] {
                            header.col(|ui| {
                                ui.strong(label);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(18.0, results.len().min(500), |mut row| {
                            let (player, server, game, key) = &results[row.index()];
                            row.col(|ui| {
                                ui.label(player);
                            });
                            row.col(|ui| {
                                ui.label(server);
                            });
                            row.col(|ui| {
                                ui.label(game);
                            });
                            if row.response().clicked() {
                                select = Some(key.clone());
                            }
                        });
                    });
            });
        if let Some(key) = select {
            self.nav = Nav::Game(key.game.clone());
            self.selected = Some(key);
        }
        self.dialogs.find = open.then_some(query);
    }

    // ----- Options --------------------------------------------------------------

    fn options_window(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.dialogs.options.take() else {
            return;
        };
        let geoip_status = self.geoip_status.clone();
        let mut ok = false;
        let mut cancel = false;
        let mut start_search: Option<String> = None;
        let language = draft.settings.language;

        dialog("Options").show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                for (index, tab) in OPTION_TABS.iter().enumerate() {
                    ui.selectable_value(&mut draft.tab, index, tr(tab));
                }
            });
            ui.separator();
            ui.allocate_ui(vec2(560.0, 330.0), |ui| {
                ui.set_min_height(330.0);
                match OPTION_TABS[draft.tab] {
                    "General" => general_tab(ui, &mut draft.settings, &geoip_status),
                    "Network" => network_tab(ui, &mut draft.settings.network),
                    "Events" => events_tab(ui, &mut draft.settings),
                    "Toolbar" => toolbar_tab(ui, &mut draft.settings.toolbar),
                    "Colors" => colors_tab(ui, &mut draft.settings.colors),
                    _ => start_search = games_tab(ui, &mut draft),
                }
            });
            ui.separator();
            buttons_right(ui, |ui| {
                cancel = ui.button(tr("Cancel")).clicked();
                ok = ui.button("   OK   ").clicked();
            });
        });

        if draft.settings.language != language {
            i18n::set_language(draft.settings.language);
            ctx.request_repaint();
        }
        if cancel {
            i18n::set_language(self.settings.language);
        }
        if let Some(game) = start_search {
            self.start_auto_search(ctx, &game);
        }
        // Colors are previewed live, and put back on Cancel.
        let colors = if cancel {
            self.settings.colors.clone()
        } else {
            draft.settings.colors.clone()
        };
        self.show_palette(ctx, &colors);
        if ok {
            let new = draft.settings;
            self.settings.toolbar = new.toolbar;
            self.settings.colors = new.colors;
            self.settings.games = new.games;
            self.settings.steam_api_key = new.steam_api_key;
            self.settings.network = new.network;
            self.settings.events = new.events;
            self.settings.geoip_disabled = new.geoip_disabled;
            self.settings.language = new.language;
            self.mark_dirty();
            if self
                .nav
                .game()
                .is_some_and(|game| !self.settings.game(game).visible)
            {
                self.nav = Nav::Favorites;
            }
        } else if !cancel {
            self.dialogs.options = Some(draft);
        }
    }

    fn start_auto_search(&mut self, ctx: &egui::Context, game_id: &str) {
        let Some(game) = catalog::game(game_id) else {
            return;
        };
        if let Some(search) = &self.dialogs.search {
            search.stop.store(true, Ordering::Relaxed);
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.dialogs.search = Some(SearchState {
            game: game.id.clone(),
            exe: game.exe_names.join(", "),
            stop: stop.clone(),
            current: String::new(),
            found: Vec::new(),
            selected: None,
            done: false,
        });
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Some(path) = detect::detect_from_registry(game) {
                let _ = tx.send(Msg::SearchFound(path.display().to_string()));
            }
            let mut last = Instant::now();
            detect::search(
                &detect::search_roots(),
                &game.exe_names,
                &stop,
                &mut |dir: &Path| {
                    if last.elapsed() > Duration::from_millis(80) {
                        last = Instant::now();
                        let _ = tx.send(Msg::SearchProgress(dir.display().to_string()));
                        ctx.request_repaint();
                    }
                },
                &mut |path: &Path| {
                    let _ = tx.send(Msg::SearchFound(path.display().to_string()));
                    ctx.request_repaint();
                },
            );
            let _ = tx.send(Msg::SearchDone);
            ctx.request_repaint();
        });
    }

    fn search_window(&mut self, ctx: &egui::Context) {
        let Some(search) = &mut self.dialogs.search else {
            return;
        };
        let mut accept: Option<String> = None;
        let mut close = false;
        if !search.done {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        egui::Window::new(fill(tr("Searching for {}"), &[&search.exe]))
            .id(egui::Id::unique("auto-search"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 40.0])
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.horizontal(|ui| {
                    if !search.done {
                        ui.spinner();
                    }
                    ui.add(egui::Label::new(RichText::new(&search.current).small()).truncate());
                });
                ui.add_space(4.0);
                egui::Frame::NONE
                    .fill(Color32::WHITE)
                    .stroke(egui::Stroke::new(1.0, Color32::from_gray(0xa0)))
                    .show(ui, |ui| {
                        ui.set_min_size(vec2(456.0, 110.0));
                        ui.strong(tr("Location"));
                        for (index, path) in search.found.iter().enumerate() {
                            if ui
                                .selectable_label(search.selected == Some(index), path)
                                .clicked()
                            {
                                search.selected = Some(index);
                            }
                        }
                    });
                buttons_right(ui, |ui| {
                    close = ui
                        .button(if search.done {
                            tr("Close")
                        } else {
                            tr("Cancel")
                        })
                        .clicked();
                    let chosen = search
                        .selected
                        .and_then(|index| search.found.get(index))
                        .cloned();
                    if ui
                        .add_enabled(chosen.is_some(), egui::Button::new("   OK   "))
                        .clicked()
                    {
                        accept = chosen;
                    }
                });
            });

        if let Some(path) = accept {
            let game = search.game.clone();
            search.stop.store(true, Ordering::Relaxed);
            self.dialogs.search = None;
            if let Some(draft) = &mut self.dialogs.options {
                let entry = draft.settings.games.entry(game).or_default();
                entry.program_location = path;
                entry.visible = true;
            }
        } else if close {
            search.stop.store(true, Ordering::Relaxed);
            self.dialogs.search = None;
        }
    }
}

/// Sound choice of one friend.
fn sound_combo(
    ui: &mut egui::Ui,
    id: impl std::fmt::Debug + std::hash::Hash,
    sound: &mut FriendSound,
) {
    let text = tr(match sound {
        FriendSound::Default => SOUND_DEFAULT,
        FriendSound::Silent => SOUND_NONE,
        FriendSound::File(_) => SOUND_FILE,
    });
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(110.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(sound, FriendSound::Default, tr(SOUND_DEFAULT));
            ui.selectable_value(sound, FriendSound::Silent, tr(SOUND_NONE));
            let is_file = matches!(sound, FriendSound::File(_));
            if ui.selectable_label(is_file, tr(SOUND_FILE)).clicked() && !is_file {
                *sound = FriendSound::File(String::new());
            }
        });
}

fn toolbar_tab(ui: &mut egui::Ui, toolbar: &mut ToolbarSettings) {
    ui.label(tr(TOOLBAR_HELP));
    ui.add_space(6.0);
    let count = toolbar.items.len();
    let mut swap: Option<(usize, isize)> = None;
    egui::Grid::new("toolbar-grid")
        .num_columns(3)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for (index, item) in toolbar.items.iter_mut().enumerate() {
                ui.checkbox(&mut item.visible, tr(toolbar_label(&item.id)));
                if ui
                    .add_enabled(index > 0, egui::Button::new(tr(TOOLBAR_UP)))
                    .clicked()
                {
                    swap = Some((index, -1));
                }
                if ui
                    .add_enabled(index + 1 < count, egui::Button::new(tr(TOOLBAR_DOWN)))
                    .clicked()
                {
                    swap = Some((index, 1));
                }
                ui.end_row();
            }
        });
    if let Some((index, delta)) = swap {
        toolbar.move_item(index, delta);
    }
    ui.add_space(8.0);
    if ui.button(tr(TOOLBAR_RESET_ALL)).clicked() {
        *toolbar = ToolbarSettings::default();
    }
}

fn colors_tab(ui: &mut egui::Ui, colors: &mut ColorSettings) {
    ui.label(tr(COLORS_HELP));
    ui.add_space(6.0);
    egui::Grid::new("colors-grid")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            for (label, value) in [
                (COLORS_LIST_BG, &mut colors.list_bg),
                (COLORS_ALT_ROW, &mut colors.alt_row),
                (COLORS_SELECTION, &mut colors.selection),
                (COLORS_TEXT, &mut colors.text),
            ] {
                ui.label(tr(label));
                let mut color = rgb(*value);
                if ui.color_edit_button_srgba(&mut color).changed() {
                    *value = [color.r(), color.g(), color.b()];
                }
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    ui.checkbox(&mut colors.quake_colors, tr(COLORS_QUAKE));
    ui.add_space(8.0);
    if ui.button(tr(COLORS_RESET)).clicked() {
        *colors = ColorSettings::default();
    }
}

fn general_tab(ui: &mut egui::Ui, settings: &mut Settings, geoip_status: &str) {
    ui.strong(tr("Language"));
    egui::ComboBox::from_id_salt("language")
        .selected_text(tr(settings.language.label()))
        .show_ui(ui, |ui| {
            for language in Language::ALL {
                ui.selectable_value(&mut settings.language, language, tr(language.label()));
            }
        });
    ui.add_space(12.0);
    ui.strong("Steam");
    ui.label(tr("Counter-Strike 1.6 server lists come from the Steam Web API, which needs your own free key."));
    ui.horizontal(|ui| {
        ui.label(tr("Steam Web API key"));
        ui.add(
            egui::TextEdit::singleline(&mut settings.steam_api_key)
                .password(true)
                .desired_width(260.0),
        );
    });
    ui.hyperlink_to(
        tr("Get a key at steamcommunity.com/dev/apikey"),
        "https://steamcommunity.com/dev/apikey",
    );
    ui.add_space(12.0);
    ui.strong(tr("Location"));
    let mut enabled = !settings.geoip_disabled;
    ui.checkbox(
        &mut enabled,
        tr("Show server countries (downloads a free IP-to-country database once a month)"),
    );
    settings.geoip_disabled = !enabled;
    ui.horizontal(|ui| {
        ui.label(fill(tr("Database: {}."), &[&geoip_status]));
        ui.hyperlink_to(openase::geoip::ATTRIBUTION, openase::geoip::ATTRIBUTION_URL);
    });
    ui.weak(tr("Takes effect at the next start."));
    ui.add_space(12.0);
    ui.strong(tr("Settings file"));
    ui.label(openase::settings::Settings::path().display().to_string());
}

fn network_tab(ui: &mut egui::Ui, network: &mut NetworkSettings) {
    ui.label(tr("Advanced connection settings"));
    ui.add_space(4.0);
    egui::Grid::new("network-grid")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label(tr("Timeout when querying servers (ms)"));
            ui.add(
                egui::DragValue::new(&mut network.timeout_ms)
                    .range(100..=10_000)
                    .speed(10),
            );
            ui.end_row();
            ui.label(tr("Number of times to ping a server"));
            ui.add(egui::DragValue::new(&mut network.pings).range(1..=10));
            ui.end_row();
            ui.label(tr("Number of times to retry a scan"));
            ui.add(egui::DragValue::new(&mut network.retries).range(0..=5));
            ui.end_row();
            ui.label(tr("Servers queried at the same time"));
            ui.add(egui::DragValue::new(&mut network.parallel_queries).range(1..=128));
            ui.end_row();
            ui.label(tr("Maximum servers per refresh"));
            ui.add(
                egui::DragValue::new(&mut network.scan_limit)
                    .range(50..=10_000)
                    .speed(10),
            );
            ui.end_row();
        });
    ui.add_space(8.0);
    if ui.button(tr("Restore defaults")).clicked() {
        *network = NetworkSettings::default();
    }
    ui.add_space(8.0);
    ui.weak(tr("Ping is the average round-trip time, Flux half the spread between the fastest and slowest answer, PL the share of unanswered queries."));
}

fn events_tab(ui: &mut egui::Ui, settings: &mut Settings) {
    let events = &mut settings.events;
    ui.strong(tr("Friend joins a server"));
    ui.checkbox(&mut events.notify_friend_joins, tr("Show a notice"));
    ui.checkbox(
        &mut events.sound_friend_joins,
        tr("Play the system notification sound"),
    );
    ui.add_space(12.0);
    ui.strong(tr("Monitored server"));
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut events.auto_refresh_selected,
            tr("Refresh the selected server every"),
        );
        ui.add(egui::DragValue::new(&mut events.auto_refresh_seconds).range(5..=600));
        ui.label(tr("seconds"));
    });
}

/// Returns a game id when Auto-Search was clicked.
fn games_tab(ui: &mut egui::Ui, draft: &mut OptionsDraft) -> Option<String> {
    let mut search = None;
    ui.horizontal_top(|ui| {
        let top_down = Layout::top_down(Align::Min);
        ui.allocate_ui_with_layout(vec2(220.0, 320.0), top_down, |ui| {
            egui::Frame::NONE
                .fill(Color32::WHITE)
                .stroke(egui::Stroke::new(1.0, Color32::from_gray(0xa0)))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("games-list")
                        .max_height(316.0)
                        .show(ui, |ui| {
                            ui.set_width(210.0);
                            ui.set_min_height(316.0);
                            for (heading, installed) in
                                [(tr("Installed games"), true), (tr("Not installed"), false)]
                            {
                                ui.strong(heading);
                                for game in catalog::games() {
                                    let settings = draft.settings.game(&game.id);
                                    let is_installed = !settings.program_location.is_empty()
                                        && Path::new(&settings.program_location).is_file();
                                    if is_installed != installed {
                                        continue;
                                    }
                                    let selected =
                                        draft.selected_game.as_deref() == Some(game.id.as_str());
                                    if ui
                                        .selectable_label(selected, format!("    {}", game.name))
                                        .clicked()
                                    {
                                        draft.selected_game = Some(game.id.clone());
                                    }
                                }
                            }
                        });
                });
        });
        ui.add_space(8.0);
        ui.allocate_ui_with_layout(vec2(320.0, 320.0), top_down, |ui| {
            let Some(id) = draft.selected_game.clone() else {
                ui.label(tr("Select a game on the left."));
                return;
            };
            let entry = draft.settings.games.entry(id.clone()).or_default();
            ui.checkbox(&mut entry.visible, tr("Visible in filter list"));
            ui.add_space(4.0);
            ui.label(tr("Player name"));
            ui.add(egui::TextEdit::singleline(&mut entry.player_name).desired_width(300.0));
            ui.label(tr("Config file"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut entry.config_file).desired_width(230.0));
                if ui.button(tr("Browse")).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title(tr("Select config file"))
                        .pick_file()
                {
                    entry.config_file = path.display().to_string();
                }
            });
            ui.label(tr("Program location"));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut entry.program_location).desired_width(230.0),
                );
                if ui.button(tr("Browse")).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title(tr("Select game executable"))
                        .add_filter(tr("Executable"), &["exe"])
                        .add_filter(tr("All files"), &["*"])
                        .pick_file()
                {
                    entry.program_location = path.display().to_string();
                    entry.visible = true;
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.add_space(14.0);
                if ui.button(tr("Auto-Search")).clicked() {
                    search = Some(id.clone());
                }
            });
            ui.add_space(4.0);
            ui.checkbox(
                &mut entry.allow_invitations,
                tr("allow invitations for this game"),
            );
            ui.checkbox(
                &mut entry.draw_player_names_with_game_font,
                tr("draw player names with game colors"),
            );
        });
    });
    search
}
