//! ASE's File / Filter / Friends / Help menus and the windows behind them:
//! setup wizard, profile import/export, server list import, Add Friend,
//! custom (color) filter editor, license and update notices.

use std::time::Duration;

use eframe::egui::{self, Align2, Color32, Key, RichText, vec2};

use openase::catalog;
use openase::i18n::{fill, group_name, noop, tr};
use openase::rules::{BUILTIN_KEYS, CustomFilter, Op, Rule};
use openase::settings::{FAVORITE_GROUPS, FRIEND_GROUPS, ServerRef, Settings, parse_address_list};
use openase::update;

use super::{Msg, Nav, OpenAseApp, ServerKey};

pub const SITE: &str = "https://openase.org/";
pub const MANUAL: &str = "https://openase.org/#manual";
pub const MESSAGE_BOARD: &str = "https://github.com/tsautier/OpenASE/issues";
const LICENSE_TEXT: &str = include_str!("../../../LICENSE");

/// Connection presets offered by the setup wizard: (label, parallel queries, timeout ms).
const CONNECTIONS: [(&str, usize, u64); 4] = [
    (noop("Slow (mobile, satellite)"), 8, 2500),
    (noop("DSL"), 16, 1500),
    (noop("Cable or fiber (recommended)"), 32, 1000),
    (noop("Very fast, low latency"), 64, 700),
];

struct Wizard {
    page: usize,
    connection: usize,
    player_name: String,
    visible: Vec<(String, bool)>,
}

struct AddFriend {
    name: String,
    game: String,
    group: String,
}

struct ImportList {
    game: String,
    path: String,
    group: String,
}

struct FilterEditor {
    index: Option<usize>,
    draft: CustomFilter,
    color_enabled: bool,
    color: [u8; 3],
}

#[derive(Default)]
pub struct Extra {
    pub update_available: Option<String>,
    wizard: Option<Wizard>,
    add_friend: Option<AddFriend>,
    import_list: Option<ImportList>,
    filter_editor: Option<FilterEditor>,
    license: bool,
}

impl Extra {
    pub fn any_modal(&self) -> bool {
        self.wizard.is_some()
            || self.add_friend.is_some()
            || self.import_list.is_some()
            || self.filter_editor.is_some()
    }
}

/// A window titled `title` (translated); the English title is its id.
fn window(title: &'static str) -> egui::Window<'static> {
    egui::Window::new(tr(title))
        .id(egui::Id::unique(title))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
}

/// Text of a rule operator in the filter editor.
fn op_label(op: Op) -> &'static str {
    match op {
        Op::Equals => tr("is"),
        Op::NotEquals => tr("is not"),
        Op::Less => tr("less than"),
        Op::Greater => tr("greater than"),
        Op::Contains => tr("contains"),
        Op::NotContains => tr("does not contain"),
    }
}

fn game_combo(ui: &mut egui::Ui, id: &str, game: &mut String, allow_any: bool) {
    let label = catalog::game(game).map_or(tr("Any game").to_string(), |g| g.name.clone());
    egui::ComboBox::from_id_salt(id)
        .width(230.0)
        .selected_text(label)
        .show_ui(ui, |ui| {
            if allow_any {
                ui.selectable_value(game, String::new(), tr("Any game"));
            }
            for entry in catalog::games() {
                ui.selectable_value(game, entry.id.clone(), &entry.name);
            }
        });
}

fn group_combo(ui: &mut egui::Ui, id: &str, group: &mut String, groups: &[&str]) {
    egui::ComboBox::from_id_salt(id)
        .width(230.0)
        .selected_text(group_name(group))
        .show_ui(ui, |ui| {
            for name in groups {
                ui.selectable_value(group, name.to_string(), group_name(name));
            }
        });
}

impl OpenAseApp {
    // ----- startup and shortcuts ---------------------------------------------

    pub(super) fn startup_tasks(&mut self, ctx: &egui::Context) {
        if !self.settings.wizard_done {
            self.open_wizard();
        }
        if !self.settings.skip_update_check {
            self.check_updates(ctx, false);
        }
    }

    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        let (f1, ctrl_e, ctrl_d, ctrl_q) = ctx.input(|input| {
            let ctrl = input.modifiers.command;
            (
                input.key_pressed(Key::F1),
                ctrl && input.key_pressed(Key::E),
                ctrl && input.key_pressed(Key::D),
                ctrl && input.key_pressed(Key::Q),
            )
        });
        if f1 {
            ctx.open_url(egui::OpenUrl::new_tab(MANUAL));
        }
        if ctrl_e {
            self.show_selected();
        }
        if ctrl_d {
            self.clear_modifiers();
        }
        if ctrl_q {
            self.collapse_all();
        }
    }

    fn check_updates(&mut self, ctx: &egui::Context, manual: bool) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                update::check_latest(Duration::from_secs(10)).map_err(|error| error.to_string());
            let _ = tx.send(Msg::Update { result, manual });
            ctx.request_repaint();
        });
    }

    fn show_selected(&mut self) {
        if let Some(key) = self.selected.clone() {
            if self.nav.game() != Some(key.game.as_str())
                && !self.candidates().iter().any(|s| ServerKey::of(s) == key)
            {
                self.nav = Nav::Game(key.game.clone());
            }
            self.scroll_to_selected = true;
        }
    }

    fn clear_modifiers(&mut self) {
        let filters = &mut self.settings.filters;
        filters.not_empty = false;
        filters.not_full = false;
        filters.no_password = false;
        filters.hide_offline = false;
        filters.max_ping = None;
        filters.countries.clear();
        filters.checked.clear();
        self.mark_dirty();
        self.status = tr("Modifiers cleared").to_string();
    }

    fn collapse_all(&mut self) {
        self.settings.open_nav.clear();
        self.mark_dirty();
    }

    // ----- menus ----------------------------------------------------------------

    pub(super) fn file_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button(tr("Export user profile...")).clicked() {
            ui.close();
            if let Some(path) = rfd::FileDialog::new()
                .set_title(tr("Export user profile"))
                .set_file_name("openase-profile.json")
                .add_filter(tr("OpenASE profile"), &["json"])
                .save_file()
            {
                match self.settings.export_to(&path) {
                    Ok(()) => self.status = fill(tr("Profile exported to {}"), &[&path.display()]),
                    Err(error) => {
                        self.dialogs.error = Some(fill(tr("Export failed: {}"), &[&error]))
                    }
                }
            }
        }
        if ui.button(tr("Import user profile...")).clicked() {
            ui.close();
            if let Some(path) = rfd::FileDialog::new()
                .set_title(tr("Import user profile"))
                .add_filter(tr("OpenASE profile"), &["json"])
                .pick_file()
            {
                match Settings::import_from(&path) {
                    Ok(mut settings) => {
                        settings.wizard_done = true;
                        self.settings = settings;
                        self.mark_dirty();
                        self.status = fill(tr("Profile imported from {}"), &[&path.display()]);
                    }
                    Err(error) => {
                        self.dialogs.error = Some(fill(tr("Import failed: {}"), &[&error]))
                    }
                }
            }
        }
        ui.separator();
        if ui.button(tr("Save server list...")).clicked() {
            ui.close();
            self.save_server_list();
        }
        if ui.button(tr("Import server list...")).clicked() {
            ui.close();
            self.extra.import_list = Some(ImportList {
                game: self
                    .nav
                    .game()
                    .unwrap_or(&catalog::games()[0].id)
                    .to_string(),
                path: String::new(),
                group: FAVORITE_GROUPS[0].to_string(),
            });
        }
        ui.separator();
        if ui.button(tr("Check for updates")).clicked() {
            ui.close();
            let ctx = ui.ctx().clone();
            self.check_updates(&ctx, true);
        }
        let mut automatic = !self.settings.skip_update_check;
        if ui
            .checkbox(&mut automatic, tr("Automatic update check"))
            .changed()
        {
            self.settings.skip_update_check = !automatic;
            self.mark_dirty();
        }
        if ui.button(tr("Setup Wizard...")).clicked() {
            ui.close();
            self.open_wizard();
        }
        ui.separator();
        if ui.button(tr("Exit")).clicked() {
            ui.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(super) fn filter_menu(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(self.scan.is_none(), egui::Button::new(tr("Refresh\tF5")))
            .clicked()
        {
            ui.close();
            let ctx = ui.ctx().clone();
            self.refresh(&ctx);
        }
        ui.separator();
        if ui
            .add_enabled(
                self.selected.is_some(),
                egui::Button::new(tr("Show selected\tCtrl+E")),
            )
            .clicked()
        {
            ui.close();
            self.show_selected();
        }
        if ui.button(tr("Clear modifiers\tCtrl+D")).clicked() {
            ui.close();
            self.clear_modifiers();
        }
        if ui.button(tr("Collapse all\tCtrl+Q")).clicked() {
            ui.close();
            self.collapse_all();
        }
        ui.separator();
        let filters = &mut self.settings.filters;
        let mut changed = false;
        changed |= ui
            .checkbox(&mut filters.not_empty, tr("Not empty"))
            .changed();
        changed |= ui.checkbox(&mut filters.not_full, tr("Not full")).changed();
        changed |= ui
            .checkbox(&mut filters.no_password, tr("No password"))
            .changed();
        changed |= ui
            .checkbox(&mut filters.hide_offline, tr("Hide unresponsive servers"))
            .changed();
        if changed {
            self.mark_dirty();
        }
        ui.separator();
        let current = match self.nav {
            Nav::Custom(index) if index < self.settings.custom_filters.len() => Some(index),
            _ => None,
        };
        if ui.button(tr("New filter...")).clicked() {
            ui.close();
            let game = self.nav.game().unwrap_or("").to_string();
            self.open_filter_editor(None, game, false);
        }
        if ui
            .add_enabled(current.is_some(), egui::Button::new(tr("Edit filter...")))
            .clicked()
        {
            ui.close();
            self.open_filter_editor(current, String::new(), false);
        }
        if ui
            .add_enabled(
                current.is_some(),
                egui::Button::new(tr("Make color filter...")),
            )
            .clicked()
        {
            ui.close();
            self.open_filter_editor(current, String::new(), true);
        }
        if ui
            .add_enabled(current.is_some(), egui::Button::new(tr("Delete filter")))
            .clicked()
        {
            ui.close();
            if let Some(index) = current {
                let removed = self.settings.custom_filters.remove(index);
                self.nav = Nav::Favorites;
                self.mark_dirty();
                self.status = fill(tr("Deleted filter {}"), &[&removed.name]);
            }
        }
        ui.separator();
        if ui.button(tr("Configure games...")).clicked() {
            ui.close();
            self.dialogs.open_options_at(&self.settings, 3);
        }
    }

    pub(super) fn friends_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button(tr("Add friend...")).clicked() {
            ui.close();
            self.extra.add_friend = Some(AddFriend {
                name: String::new(),
                game: String::new(),
                group: FRIEND_GROUPS[0].to_string(),
            });
        }
        if ui.button(tr("Manage friends...")).clicked() {
            ui.close();
            self.dialogs.friends = Some(String::new());
        }
        if ui.button(tr("Friend name search...")).clicked() {
            ui.close();
            self.dialogs.find = Some(String::new());
        }
        ui.separator();
        if ui.button(tr("Show servers with friends")).clicked() {
            ui.close();
            self.nav = Nav::Friends;
        }
    }

    pub(super) fn help_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button(tr("OpenASE Help\tF1")).clicked() {
            ui.close();
            ui.ctx().open_url(egui::OpenUrl::new_tab(MANUAL));
        }
        if ui.button(tr("License")).clicked() {
            ui.close();
            self.extra.license = true;
        }
        if ui.button(tr("Online manual")).clicked() {
            ui.close();
            ui.ctx().open_url(egui::OpenUrl::new_tab(MANUAL));
        }
        if ui.button(tr("Message board")).clicked() {
            ui.close();
            ui.ctx().open_url(egui::OpenUrl::new_tab(MESSAGE_BOARD));
        }
        if ui.button(tr("Website")).clicked() {
            ui.close();
            ui.ctx().open_url(egui::OpenUrl::new_tab(SITE));
        }
        ui.separator();
        if ui.button(tr("Check for updates")).clicked() {
            ui.close();
            let ctx = ui.ctx().clone();
            self.check_updates(&ctx, true);
        }
        if ui.button(tr("About OpenASE...")).clicked() {
            ui.close();
            self.dialogs.about = true;
        }
    }

    // ----- actions ---------------------------------------------------------------

    fn save_server_list(&mut self) {
        let rows: Vec<String> = self
            .visible_rows()
            .iter()
            .map(|server| server.address())
            .collect();
        if rows.is_empty() {
            self.dialogs.error = Some(tr("The server list is empty.").to_string());
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Save server list"))
            .set_file_name("servers.txt")
            .add_filter(tr("Text"), &["txt"])
            .save_file()
        else {
            return;
        };
        let mut text = String::from("# OpenASE server list, one ip:port per line\n");
        for address in &rows {
            text.push_str(address);
            text.push('\n');
        }
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.status = fill(
                    tr("Saved {} servers to {}"),
                    &[&rows.len(), &path.display()],
                )
            }
            Err(error) => self.dialogs.error = Some(fill(tr("Could not save: {}"), &[&error])),
        }
    }

    fn open_wizard(&mut self) {
        let visible = catalog::games()
            .iter()
            .map(|game| (game.id.clone(), self.settings.game(&game.id).visible))
            .collect();
        let player_name = self
            .settings
            .games
            .values()
            .map(|game| game.player_name.clone())
            .find(|name| !name.is_empty())
            .unwrap_or_default();
        let connection = CONNECTIONS
            .iter()
            .position(|(_, parallel, _)| *parallel == self.settings.network.parallel_queries)
            .unwrap_or(2);
        self.extra.wizard = Some(Wizard {
            page: 0,
            connection,
            player_name,
            visible,
        });
    }

    fn open_filter_editor(&mut self, index: Option<usize>, game: String, color: bool) {
        let draft = index
            .and_then(|index| self.settings.custom_filters.get(index).cloned())
            .unwrap_or(CustomFilter {
                name: tr("My filter").to_string(),
                game,
                rules: vec![Rule {
                    key: "players".to_string(),
                    op: Op::Greater,
                    value: "0".to_string(),
                }],
                color: None,
            });
        let rgb = draft
            .color
            .as_deref()
            .and_then(openase::colors::parse_hex)
            .unwrap_or([0xff, 0xe8, 0xa0]);
        self.extra.filter_editor = Some(FilterEditor {
            index,
            color_enabled: color || draft.color.is_some(),
            color: rgb,
            draft,
        });
    }

    // ----- windows ---------------------------------------------------------------

    pub(super) fn show_extra(&mut self, ctx: &egui::Context) {
        self.wizard_window(ctx);
        self.add_friend_window(ctx);
        self.import_window(ctx);
        self.filter_editor_window(ctx);
        self.license_window(ctx);
        self.update_window(ctx);
    }

    fn wizard_window(&mut self, ctx: &egui::Context) {
        let Some(mut wizard) = self.extra.wizard.take() else {
            return;
        };
        let mut finish = false;
        let mut cancel = false;
        window("Setup Wizard").show(ctx, |ui| {
            ui.set_width(460.0);
            egui::Frame::NONE.fill(Color32::WHITE).inner_margin(12).stroke((1.0, Color32::from_gray(0xa0))).show(ui, |ui| {
                ui.set_min_size(vec2(436.0, 230.0));
                match wizard.page {
                    0 => {
                        ui.heading(tr("Welcome to OpenASE!"));
                        ui.add_space(6.0);
                        ui.label(tr("This is your first time running OpenASE, so let's set a few things up."));
                        ui.add_space(6.0);
                        ui.label(RichText::new(tr("First, tell OpenASE about your internet connection.")).strong());
                        ui.label(tr("Refreshing a server list sends thousands of small queries; matching them to your connection keeps pings accurate."));
                        ui.add_space(6.0);
                        ui.label(tr("You can run this wizard again from File > Setup Wizard."));
                    }
                    1 => {
                        ui.heading(tr("Your connection"));
                        ui.add_space(6.0);
                        for (index, (label, parallel, timeout)) in CONNECTIONS.iter().enumerate() {
                            ui.radio_value(&mut wizard.connection, index, tr(label))
                                .on_hover_text(fill(tr("{} servers at once, {} ms timeout"), &[parallel, timeout]));
                        }
                        ui.add_space(8.0);
                        ui.weak(tr("If pings look too high, come back here and pick a slower setting. Fine tuning lives in Options > Network."));
                    }
                    2 => {
                        ui.heading(tr("You and your games"));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(tr("Player name"));
                            ui.text_edit_singleline(&mut wizard.player_name);
                        });
                        ui.add_space(6.0);
                        ui.label(tr("Games shown in the filter tree:"));
                        egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                            for (id, visible) in &mut wizard.visible {
                                let Some(game) = catalog::game(id) else { continue };
                                let installed = !self.settings.game(id).program_location.is_empty();
                                let label = if installed { fill(tr("{}  (installed)"), &[&game.name]) } else { game.name.clone() };
                                ui.checkbox(visible, label);
                            }
                        });
                    }
                    _ => {
                        ui.heading(tr("Setup done."));
                        ui.add_space(6.0);
                        ui.label(tr("Tips:"));
                        ui.label(tr("•  F5 refreshes the selected game, double-click or Enter joins a server."));
                        ui.label(tr("•  Tick filters in the tree: game types, mods, maps, Kill Cam, countries..."));
                        ui.label(tr("•  Friends > Add friend tracks players on every server you scan."));
                        ui.label(tr("•  Counter-Strike 1.6 lists need a free Steam Web API key (Options > General)."));
                        ui.add_space(6.0);
                        ui.label(tr("Have fun, and see you on the servers."));
                    }
                }
            });
            ui.add_space(6.0);
            super::dialogs::buttons_right(ui, |ui| {
                cancel = ui.button(tr("Cancel")).clicked();
                if wizard.page < 3 {
                    if ui.button(tr("Next >")).clicked() {
                        wizard.page += 1;
                    }
                } else {
                    finish = ui.button(tr("Finish")).clicked();
                }
                if ui.add_enabled(wizard.page > 0, egui::Button::new(tr("< Back"))).clicked() {
                    wizard.page -= 1;
                }
            });
        });

        if finish {
            let (_, parallel, timeout) = CONNECTIONS[wizard.connection];
            self.settings.network.parallel_queries = parallel;
            self.settings.network.timeout_ms = timeout;
            for (id, visible) in wizard.visible {
                let entry = self.settings.games.entry(id).or_default();
                entry.visible = visible;
                if entry.player_name.is_empty() {
                    entry.player_name = wizard.player_name.clone();
                }
            }
            self.settings.wizard_done = true;
            self.mark_dirty();
        } else if cancel {
            self.settings.wizard_done = true;
            self.mark_dirty();
        } else {
            self.extra.wizard = Some(wizard);
        }
    }

    fn add_friend_window(&mut self, ctx: &egui::Context) {
        let Some(mut add) = self.extra.add_friend.take() else {
            return;
        };
        let mut ok = false;
        let mut cancel = false;
        let mut search = false;
        window("Add Friend").show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(tr("Enter the player's name as it appears on servers. Color codes are ignored and partial names match."));
            if ui.button(tr("Go to Friend Name Search")).clicked() {
                search = true;
            }
            ui.add_space(6.0);
            egui::Grid::new("add-friend").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label(tr("Name"));
                ui.add(egui::TextEdit::singleline(&mut add.name).desired_width(230.0)).request_focus();
                ui.end_row();
                ui.label(tr("Game"));
                game_combo(ui, "friend-game", &mut add.game, true);
                ui.end_row();
                ui.label(tr("Group"));
                group_combo(ui, "friend-group", &mut add.group, &FRIEND_GROUPS);
                ui.end_row();
            });
            if ui.input(|input| input.key_pressed(Key::Enter)) {
                ok = true;
            }
            super::dialogs::buttons_right(ui, |ui| {
                cancel = ui.button(tr("Cancel")).clicked();
                ok |= ui.button("   OK   ").clicked();
            });
        });
        let name = add.name.trim().to_string();
        if search {
            self.dialogs.find = Some(name);
        } else if ok && !name.is_empty() {
            if !self
                .settings
                .friends
                .iter()
                .any(|friend| friend.eq_ignore_ascii_case(&name))
            {
                self.settings.friends.push(name.clone());
            }
            self.settings
                .friend_groups
                .insert(name.to_lowercase(), add.group);
            self.settings
                .friend_games
                .insert(name.to_lowercase(), add.game);
            self.mark_dirty();
            let servers: Vec<_> = self.store.values().flatten().cloned().collect();
            for server in servers {
                self.check_friends(&server);
            }
            self.status = fill(tr("{} added to your friends"), &[&name]);
        } else if !cancel {
            self.extra.add_friend = Some(add);
        }
    }

    fn import_window(&mut self, ctx: &egui::Context) {
        let Some(mut import) = self.extra.import_list.take() else {
            return;
        };
        let mut ok = false;
        let mut cancel = false;
        window("Import server list").show(ctx, |ui| {
            egui::Grid::new("import-list").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label(tr("Game"));
                game_combo(ui, "import-game", &mut import.game, false);
                ui.end_row();
                ui.label(tr("File"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut import.path).desired_width(170.0));
                    if ui.button(tr("Browse")).clicked()
                        && let Some(path) = rfd::FileDialog::new().set_title(tr("Server list")).pick_file()
                    {
                        import.path = path.display().to_string();
                    }
                });
                ui.end_row();
                ui.label(tr("Add to"));
                group_combo(ui, "import-group", &mut import.group, &FAVORITE_GROUPS);
                ui.end_row();
            });
            ui.weak(tr("Any text file with ip:port entries works: OpenASE lists, GameSpy exports, configs..."));
            super::dialogs::buttons_right(ui, |ui| {
                cancel = ui.button(tr("Cancel")).clicked();
                ok = ui.button(tr("  Import  ")).clicked();
            });
        });
        if ok {
            match std::fs::read(&import.path) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let addresses = parse_address_list(&text);
                    if addresses.is_empty() {
                        self.dialogs.error =
                            Some(tr("No ip:port entries found in this file.").to_string());
                        self.extra.import_list = Some(import);
                        return;
                    }
                    let keys: Vec<ServerKey> = addresses
                        .iter()
                        .map(|address| ServerKey {
                            game: import.game.clone(),
                            address: address.clone(),
                        })
                        .collect();
                    for key in &keys {
                        let server = ServerRef {
                            game: key.game.clone(),
                            address: key.address.clone(),
                        };
                        self.settings.add_favorite_to(server.clone(), &import.group);
                        Settings::push_front(&mut self.settings.recently_imported, server);
                    }
                    self.mark_dirty();
                    self.nav = Nav::FavoriteGroup(import.group.clone());
                    self.scan_list(ctx, keys, noop("imported servers"));
                    self.status = fill(
                        tr("Imported {} servers into {}"),
                        &[&addresses.len(), &group_name(&import.group)],
                    );
                }
                Err(error) => {
                    self.dialogs.error =
                        Some(fill(tr("Cannot read {}: {}"), &[&import.path, &error]));
                    self.extra.import_list = Some(import);
                }
            }
        } else if !cancel {
            self.extra.import_list = Some(import);
        }
    }

    fn filter_editor_window(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.extra.filter_editor.take() else {
            return;
        };
        let mut save = false;
        let mut cancel = false;
        let mut remove: Option<usize> = None;
        window(if editor.index.is_some() {
            noop("Edit filter")
        } else {
            noop("New filter")
        })
        .show(ctx, |ui| {
            ui.set_width(520.0);
            egui::Grid::new("filter-head")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("Name"));
                    ui.add(egui::TextEdit::singleline(&mut editor.draft.name).desired_width(230.0));
                    ui.end_row();
                    ui.label(tr("Game"));
                    game_combo(ui, "filter-game", &mut editor.draft.game, true);
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.label(tr("Show servers where all of these are true:"));
            for (index, rule) in editor.draft.rules.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("rule-key", index))
                        .width(110.0)
                        .selected_text(rule.key.as_str())
                        .show_ui(ui, |ui| {
                            for key in BUILTIN_KEYS {
                                ui.selectable_value(&mut rule.key, key.to_string(), key);
                            }
                        });
                    ui.add(egui::TextEdit::singleline(&mut rule.key).desired_width(90.0))
                        .on_hover_text(tr("Any server rule, e.g. g_gametype"));
                    egui::ComboBox::from_id_salt(("rule-op", index))
                        .width(110.0)
                        .selected_text(op_label(rule.op))
                        .show_ui(ui, |ui| {
                            for op in Op::ALL {
                                ui.selectable_value(&mut rule.op, op, op_label(op));
                            }
                        });
                    ui.add(egui::TextEdit::singleline(&mut rule.value).desired_width(100.0));
                    if ui.small_button("x").clicked() {
                        remove = Some(index);
                    }
                });
            }
            if ui.button(tr("+ Add rule")).clicked() {
                editor.draft.rules.push(Rule {
                    key: "map".to_string(),
                    op: Op::Contains,
                    value: String::new(),
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut editor.color_enabled,
                    tr("Color filter: highlight matching servers everywhere"),
                );
                if editor.color_enabled {
                    ui.color_edit_button_srgb(&mut editor.color);
                }
            });
            super::dialogs::buttons_right(ui, |ui| {
                cancel = ui.button(tr("Cancel")).clicked();
                save = ui.button("   OK   ").clicked();
            });
        });
        if let Some(index) = remove {
            editor.draft.rules.remove(index);
        }
        if save && !editor.draft.name.trim().is_empty() {
            let [r, g, b] = editor.color;
            editor.draft.color = editor
                .color_enabled
                .then(|| format!("#{r:02x}{g:02x}{b:02x}"));
            editor
                .draft
                .rules
                .retain(|rule| !rule.key.trim().is_empty());
            let index = match editor
                .index
                .filter(|index| *index < self.settings.custom_filters.len())
            {
                Some(index) => {
                    self.settings.custom_filters[index] = editor.draft;
                    index
                }
                None => {
                    self.settings.custom_filters.push(editor.draft);
                    self.settings.custom_filters.len() - 1
                }
            };
            self.settings.open_nav.insert("My filters".to_string());
            self.nav = Nav::Custom(index);
            self.mark_dirty();
        } else if !cancel {
            self.extra.filter_editor = Some(editor);
        }
    }

    fn license_window(&mut self, ctx: &egui::Context) {
        let mut open = self.extra.license;
        egui::Window::new(tr("License"))
            .id(egui::Id::unique("License")).open(&mut open).default_size([560.0, 420.0]).show(ctx, |ui| {
            ui.label(tr("OpenASE is free software under the GNU General Public License, version 3 or later."));
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(LICENSE_TEXT).monospace().size(11.0)).wrap());
            });
        });
        self.extra.license = open;
    }

    fn update_window(&mut self, ctx: &egui::Context) {
        let Some(tag) = self.extra.update_available.clone() else {
            return;
        };
        let mut close = false;
        window("Update available").show(ctx, |ui| {
            ui.label(fill(
                tr("OpenASE {} is available. You are running {}."),
                &[&tag, &env!("CARGO_PKG_VERSION")],
            ));
            ui.hyperlink_to(
                tr("Download it from GitHub Releases"),
                update::RELEASES_PAGE,
            );
            super::dialogs::buttons_right(ui, |ui| {
                close = ui.button(tr("Later")).clicked();
            });
        });
        if close {
            self.extra.update_available = None;
        }
    }
}
