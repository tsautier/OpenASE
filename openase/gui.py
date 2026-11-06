"""Desktop GUI for OpenASE."""

from __future__ import annotations

import json
import queue
import threading
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

from .ase_filters import find_filters_file, parse_filter_paths, parse_game_definitions
from .colors import quake_color_segments, strip_quake_colors
from .games import GameSettings, detect_game, game_settings_from_json, game_settings_to_json
from .location import UNKNOWN_LOCATION, find_countries_file, load_countries, server_location
from .quake3 import DEFAULT_MASTER, DEFAULT_MASTER_PORT, DEFAULT_PORT, Quake3Client, Quake3Server, parse_address


class ServerList(ttk.Frame):
    """Canvas-based server list with per-segment color rendering in Name."""

    COLUMNS = (
        ("location", "Location", 100, tk.W),
        ("name", "Name", 360, tk.W),
        ("ping", "Ping", 70, tk.E),
        ("players", "Players", 90, tk.CENTER),
        ("game", "Game", 120, tk.W),
        ("map", "Map", 120, tk.W),
        ("address", "Address", 150, tk.W),
    )

    def __init__(self, parent, *, sort_command, select_command) -> None:
        super().__init__(parent)
        self.sort_command = sort_command
        self.select_command = select_command
        self.row_height = 20
        self.header_height = 24
        self.servers: list[Quake3Server] = []
        self.values_by_address: dict[str, tuple[str, ...]] = {}
        self.selected_address: str | None = None

        self.header_canvas = tk.Canvas(self, height=self.header_height, background="#efefef", highlightthickness=0)
        self.header_canvas.pack(fill=tk.X)
        self.header_canvas.bind("<Button-1>", self._on_header_click)
        self.header_canvas.bind("<Configure>", lambda _event: self._draw_header())

        body = ttk.Frame(self)
        body.pack(fill=tk.BOTH, expand=True)
        self.canvas = tk.Canvas(body, background="white", highlightthickness=0)
        self.scrollbar = ttk.Scrollbar(body, orient=tk.VERTICAL, command=self.canvas.yview)
        self.canvas.configure(yscrollcommand=self.scrollbar.set)
        self.canvas.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        self.scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        self.canvas.bind("<Button-1>", self._on_click)
        self.canvas.bind("<Configure>", lambda _event: self._draw())
        self._draw_header()

    def set_servers(
        self,
        servers: list[Quake3Server],
        values_by_address: dict[str, tuple[str, ...]],
        selected_address: str | None,
    ) -> None:
        self.servers = servers
        self.values_by_address = values_by_address
        self.selected_address = selected_address
        self._draw()

    def selected_server(self) -> Quake3Server | None:
        if self.selected_address is None:
            return None
        for server in self.servers:
            if server.address == self.selected_address:
                return server
        return None

    def _on_click(self, event) -> None:
        row = int(self.canvas.canvasy(event.y) // self.row_height)
        if row < 0 or row >= len(self.servers):
            return
        self.selected_address = self.servers[row].address
        self._draw()
        self.select_command(self.servers[row])

    def _draw(self) -> None:
        self.canvas.delete("all")
        total_width = sum(column[2] for column in self.COLUMNS)
        total_height = max(len(self.servers) * self.row_height, self.canvas.winfo_height())
        self.canvas.configure(scrollregion=(0, 0, total_width, total_height))

        for row, server in enumerate(self.servers):
            y = row * self.row_height
            if server.address == self.selected_address:
                self.canvas.create_rectangle(0, y, total_width, y + self.row_height, fill="#0a64ad", outline="")
            elif row % 2:
                self.canvas.create_rectangle(0, y, total_width, y + self.row_height, fill="#f8f8f8", outline="")

            values = self.values_by_address[server.address]
            x = 0
            for index, (column, _label, width, anchor) in enumerate(self.COLUMNS):
                self.canvas.create_line(x, y, x, y + self.row_height, fill="#d0d0d0")
                text_color = "white" if server.address == self.selected_address else "#202020"
                if column == "name" and server.online:
                    self._draw_colored_name(server, x + 4, y + 2, width - 8)
                else:
                    text = values[index]
                    text_x = self._text_x(x, width, anchor)
                    self.canvas.create_text(text_x, y + 10, text=text, anchor=self._text_anchor(anchor), fill=text_color)
                x += width
            self.canvas.create_line(0, y + self.row_height, total_width, y + self.row_height, fill="#e4e4e4")

        self.canvas.create_line(total_width, 0, total_width, total_height, fill="#d0d0d0")

    def _draw_header(self) -> None:
        self.header_canvas.delete("all")
        x = 0
        for _column, label, width, _anchor in self.COLUMNS:
            self.header_canvas.create_rectangle(x, 0, x + width, self.header_height, fill="#efefef", outline="#b8b8b8")
            self.header_canvas.create_text(x + width // 2, self.header_height // 2, text=label, anchor=tk.CENTER)
            x += width
        self.header_canvas.configure(scrollregion=(0, 0, x, self.header_height))

    def _on_header_click(self, event) -> None:
        x = event.x
        cursor = 0
        for column, _label, width, _anchor in self.COLUMNS:
            if cursor <= x < cursor + width:
                self.sort_command(column)
                return
            cursor += width

    def _draw_colored_name(self, server: Quake3Server, x: int, y: int, max_width: int) -> None:
        raw_name = server.info.get("sv_hostname") or server.info.get("hostname") or "unknown"
        cursor = x
        limit = x + max_width
        for segment in quake_color_segments(raw_name):
            if cursor >= limit:
                break
            item = self.canvas.create_text(cursor, y, text=segment.text, anchor=tk.NW, fill=segment.color)
            bbox = self.canvas.bbox(item)
            if bbox and bbox[2] > limit:
                self.canvas.delete(item)
                remaining = max(0, int((limit - cursor) / 7))
                if remaining > 1:
                    self.canvas.create_text(
                        cursor, y, text=segment.text[: remaining - 1] + "...", anchor=tk.NW, fill=segment.color
                    )
                break
            cursor = bbox[2] if bbox else cursor

    @staticmethod
    def _text_x(x: int, width: int, anchor) -> int:
        if anchor == tk.E:
            return x + width - 4
        if anchor == tk.CENTER:
            return x + width // 2
        return x + 4

    @staticmethod
    def _text_anchor(anchor) -> str:
        if anchor == tk.E:
            return tk.E
        if anchor == tk.CENTER:
            return tk.CENTER
        return tk.W


class PlayerList(ttk.Frame):
    """Canvas-based player list with per-segment color rendering in player names."""

    COLUMNS = (
        ("name", "Player Name", 240, tk.W),
        ("score", "Score", 70, tk.E),
        ("ping", "Ping", 70, tk.E),
    )

    def __init__(self, parent) -> None:
        super().__init__(parent)
        self.row_height = 20
        self.players = []
        self.draw_game_font = True

        self.header_height = 24
        self.header_canvas = tk.Canvas(self, height=self.header_height, background="#efefef", highlightthickness=0)
        self.header_canvas.pack(fill=tk.X)
        self.header_canvas.bind("<Configure>", lambda _event: self._draw_header())

        body = ttk.Frame(self)
        body.pack(fill=tk.BOTH, expand=True)
        self.canvas = tk.Canvas(body, background="white", highlightthickness=0)
        scrollbar = ttk.Scrollbar(body, orient=tk.VERTICAL, command=self.canvas.yview)
        self.canvas.configure(yscrollcommand=scrollbar.set)
        self.canvas.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.canvas.bind("<Configure>", lambda _event: self._draw())
        self._draw_header()

    def set_players(self, players, *, draw_game_font: bool = True) -> None:
        self.players = list(players)
        self.draw_game_font = draw_game_font
        self._draw()

    def clear(self) -> None:
        self.set_players([])

    def _draw(self) -> None:
        self.canvas.delete("all")
        total_width = sum(column[2] for column in self.COLUMNS)
        total_height = max(len(self.players) * self.row_height, self.canvas.winfo_height())
        self.canvas.configure(scrollregion=(0, 0, total_width, total_height))

        for row, player in enumerate(self.players):
            y = row * self.row_height
            if row % 2:
                self.canvas.create_rectangle(0, y, total_width, y + self.row_height, fill="#f8f8f8", outline="")

            values = (player.name, str(player.score), str(player.ping))
            x = 0
            for index, (column, _label, width, anchor) in enumerate(self.COLUMNS):
                self.canvas.create_line(x, y, x, y + self.row_height, fill="#d0d0d0")
                if column == "name":
                    if self.draw_game_font:
                        self._draw_colored_name(player.name, x + 4, y + 2, width - 8)
                    else:
                        self.canvas.create_text(
                            x + 4, y + 10, text=strip_quake_colors(player.name), anchor=tk.W, fill="#202020"
                        )
                else:
                    self.canvas.create_text(
                        ServerList._text_x(x, width, anchor),
                        y + 10,
                        text=values[index],
                        anchor=ServerList._text_anchor(anchor),
                        fill="#202020",
                    )
                x += width
            self.canvas.create_line(0, y + self.row_height, total_width, y + self.row_height, fill="#e4e4e4")

        self.canvas.create_line(total_width, 0, total_width, total_height, fill="#d0d0d0")

    def _draw_header(self) -> None:
        self.header_canvas.delete("all")
        x = 0
        for _column, label, width, _anchor in self.COLUMNS:
            self.header_canvas.create_rectangle(x, 0, x + width, self.header_height, fill="#efefef", outline="#b8b8b8")
            self.header_canvas.create_text(x + width // 2, self.header_height // 2, text=label, anchor=tk.CENTER)
            x += width
        self.header_canvas.configure(scrollregion=(0, 0, x, self.header_height))

    def _draw_colored_name(self, raw_name: str, x: int, y: int, max_width: int) -> None:
        cursor = x
        limit = x + max_width
        for segment in quake_color_segments(raw_name):
            if cursor >= limit:
                break
            item = self.canvas.create_text(cursor, y, text=segment.text, anchor=tk.NW, fill=segment.color)
            bbox = self.canvas.bbox(item)
            if bbox and bbox[2] > limit:
                self.canvas.delete(item)
                remaining = max(0, int((limit - cursor) / 7))
                if remaining > 1:
                    self.canvas.create_text(
                        cursor, y, text=segment.text[: remaining - 1] + "...", anchor=tk.NW, fill=segment.color
                    )
                break
            cursor = bbox[2] if bbox else cursor


class OpenASEApp(tk.Tk):
    """Main OpenASE window.

    The interface intentionally follows the original ASE layout: navigation and
    filters on the left, server list on the top right, and selected server
    details at the bottom.
    """

    def __init__(self) -> None:
        super().__init__()
        self.title("OpenASE")
        self.geometry("1280x760")
        self.minsize(960, 560)

        self.client = Quake3Client(timeout=2.0)
        self.result_queue: queue.Queue[tuple[str, object]] = queue.Queue()
        self.servers: list[Quake3Server] = []
        self.refreshing = False

        self.master_var = tk.StringVar(value=f"{DEFAULT_MASTER}:{DEFAULT_MASTER_PORT}")
        self.limit_var = tk.IntVar(value=50)
        self.status_var = tk.StringVar(value="Ready")
        self.filter_hide_empty = tk.BooleanVar(value=False)
        self.filter_hide_full = tk.BooleanVar(value=False)
        self.filter_max_ping = tk.StringVar(value="")
        self.country_filter: str | None = None
        self.project_root = Path(__file__).resolve().parents[1]
        self.settings_path = self.project_root / "openase_settings.json"
        self.settings = self._load_settings()
        self.open_nav_paths = self._load_open_nav_paths()
        self.filters_file = find_filters_file(self.project_root)
        self.game_definitions = parse_game_definitions(self.filters_file) if self.filters_file else {}
        self.game_settings = self._load_game_settings()
        self._detect_installed_games()
        countries_file = find_countries_file(self.project_root)
        self.country_names = {country.name for country in load_countries(countries_file)} if countries_file else set()

        self._build_menu()
        self._build_toolbar()
        self._build_layout()
        self._seed_tree()
        self.protocol("WM_DELETE_WINDOW", self.close)
        self.after(100, self._drain_queue)

    def _build_menu(self) -> None:
        menu = tk.Menu(self)
        file_menu = tk.Menu(menu, tearoff=False)
        file_menu.add_command(label="Exit", command=self.close)
        menu.add_cascade(label="File", menu=file_menu)

        server_menu = tk.Menu(menu, tearoff=False)
        server_menu.add_command(label="Refresh", command=self.refresh_servers)
        server_menu.add_command(label="Add Server", command=self.add_server_dialog)
        server_menu.add_separator()
        server_menu.add_command(label="Options", command=self.open_options_dialog)
        menu.add_cascade(label="Server", menu=server_menu)

        help_menu = tk.Menu(menu, tearoff=False)
        help_menu.add_command(label="About", command=self.show_about)
        menu.add_cascade(label="Help", menu=help_menu)
        self.config(menu=menu)

    def _build_toolbar(self) -> None:
        toolbar = ttk.Frame(self, padding=(4, 3))
        toolbar.pack(side=tk.TOP, fill=tk.X)

        self.refresh_button = ttk.Button(toolbar, text="Refresh", command=self.refresh_servers)
        self.refresh_button.pack(side=tk.LEFT, padx=(0, 4))
        ttk.Button(toolbar, text="Add", command=self.add_server_dialog).pack(side=tk.LEFT, padx=(0, 10))

        ttk.Label(toolbar, text="Master").pack(side=tk.LEFT)
        ttk.Entry(toolbar, textvariable=self.master_var, width=34).pack(side=tk.LEFT, padx=(4, 10))

        ttk.Label(toolbar, text="Limit").pack(side=tk.LEFT)
        ttk.Spinbox(toolbar, from_=1, to=500, textvariable=self.limit_var, width=6).pack(side=tk.LEFT, padx=(4, 10))

        ttk.Checkbutton(toolbar, text="Not empty", variable=self.filter_hide_empty, command=self.apply_filters).pack(
            side=tk.LEFT, padx=(0, 4)
        )
        ttk.Checkbutton(toolbar, text="Not full", variable=self.filter_hide_full, command=self.apply_filters).pack(
            side=tk.LEFT, padx=(0, 10)
        )

        ttk.Label(toolbar, text="Max ping").pack(side=tk.LEFT)
        ping_entry = ttk.Entry(toolbar, textvariable=self.filter_max_ping, width=6)
        ping_entry.pack(side=tk.LEFT, padx=(4, 10))
        ping_entry.bind("<Return>", lambda _event: self.apply_filters())

    def _build_layout(self) -> None:
        main = ttk.PanedWindow(self, orient=tk.HORIZONTAL)
        main.pack(fill=tk.BOTH, expand=True)

        left_frame = ttk.Frame(main)
        main.add(left_frame, weight=1)

        right = ttk.PanedWindow(main, orient=tk.VERTICAL)
        main.add(right, weight=4)

        self.nav_tree = ttk.Treeview(left_frame, show="tree")
        self.nav_tree.pack(fill=tk.BOTH, expand=True)

        self.server_list = ServerList(right, sort_command=self.sort_servers, select_command=self.on_server_selected)
        right.add(self.server_list, weight=3)

        bottom = ttk.PanedWindow(right, orient=tk.HORIZONTAL)
        right.add(bottom, weight=1)

        self.player_list = PlayerList(bottom)
        bottom.add(self.player_list, weight=1)

        self.info_tree = ttk.Treeview(bottom, columns=("key", "value"), show="headings")
        self.info_tree.heading("key", text="Key")
        self.info_tree.heading("value", text="Value")
        self.info_tree.column("key", width=190, anchor=tk.W)
        self.info_tree.column("value", width=380, anchor=tk.W)
        bottom.add(self.info_tree, weight=1)

        status = ttk.Label(self, textvariable=self.status_var, anchor=tk.W, relief=tk.SUNKEN, padding=(4, 2))
        status.pack(side=tk.BOTTOM, fill=tk.X)

    def _seed_tree(self) -> None:
        self._clear_tree(self.nav_tree)
        visible_game_names = self._visible_game_names()
        if self.filters_file is None:
            games = self.nav_tree.insert("", tk.END, text="Games", open="Games" in self.open_nav_paths)
            for game_name in visible_game_names:
                self.nav_tree.insert(games, tk.END, text=game_name, open=False)
            modifiers = self.nav_tree.insert("", tk.END, text="Modifiers", open=True)
            ping = self.nav_tree.insert(modifiers, tk.END, text="Ping", open=True)
            for label in ("less than 50", "less than 100", "less than 150", "less than 200"):
                self.nav_tree.insert(ping, tk.END, text=label)
            players = self.nav_tree.insert(modifiers, tk.END, text="Players", open=True)
            for label in ("Not empty", "Not full", "No bots"):
                self.nav_tree.insert(players, tk.END, text=label)
            country_root = self.nav_tree.insert(modifiers, tk.END, text="Country", open=False)
            self.nav_tree.insert(country_root, tk.END, text=UNKNOWN_LOCATION, values=("country", UNKNOWN_LOCATION))
            for country_name in sorted(self.country_names):
                self.nav_tree.insert(country_root, tk.END, text=country_name, values=("country", country_name))
        else:
            visible_paths = [
                path
                for path in parse_filter_paths(self.filters_file)
                if path.split("\\", 1)[0] in visible_game_names or path.startswith("Modifiers\\")
            ]
            self._insert_filter_paths(visible_paths)
        self.nav_tree.insert("", tk.END, text="Favorites", open="Favorites" in self.open_nav_paths)
        self.nav_tree.bind("<<TreeviewSelect>>", self.on_nav_selected)

    def _insert_filter_paths(self, paths: list[str]) -> None:
        items: dict[str, str] = {}
        for path in paths:
            parent = ""
            current_parts: list[str] = []
            for part in path.split("\\"):
                current_parts.append(part)
                current_path = "\\".join(current_parts)
                if current_path not in items:
                    values = ()
                    if current_path.startswith("Modifiers\\Country\\") and part in self.country_names:
                        values = ("country", part)
                    items[current_path] = self.nav_tree.insert(
                        parent, tk.END, text=part, open=current_path in self.open_nav_paths, values=values
                    )
                parent = items[current_path]

    def refresh_servers(self) -> None:
        if self.refreshing:
            return
        self.refreshing = True
        self.refresh_button.config(state=tk.DISABLED)
        self.status_var.set("Querying master server...")
        self.servers.clear()
        self.server_list.set_servers([], {}, None)
        self._clear_details()

        thread = threading.Thread(target=self._refresh_worker, daemon=True)
        thread.start()

    def _refresh_worker(self) -> None:
        try:
            master_host, master_port = parse_address(self.master_var.get(), default_port=DEFAULT_MASTER_PORT)
            limit = max(1, int(self.limit_var.get()))
            master_client = Quake3Client(timeout=3.0)
            addresses = master_client.master_servers(master_host, master_port)[:limit]
            self.result_queue.put(("status", f"Scanning {len(addresses)} servers..."))

            for address in addresses:
                host, port = parse_address(address, default_port=DEFAULT_PORT)
                server = self.client.status(host, port)
                self.result_queue.put(("server", server))
        except Exception as exc:  # noqa: BLE001 - UI must surface worker failures.
            self.result_queue.put(("error", str(exc)))
        finally:
            self.result_queue.put(("done", None))

    def _drain_queue(self) -> None:
        try:
            while True:
                kind, payload = self.result_queue.get_nowait()
                if kind == "status":
                    self.status_var.set(str(payload))
                elif kind == "server":
                    self.servers.append(payload)
                    self.apply_filters()
                elif kind == "error":
                    messagebox.showerror("OpenASE", str(payload))
                elif kind == "done":
                    self.refreshing = False
                    self.refresh_button.config(state=tk.NORMAL)
                    online = sum(1 for server in self.servers if server.online)
                    players = sum(len(server.players) for server in self.servers if server.online)
                    self.status_var.set(f"{online} servers online, {players} players visible")
        except queue.Empty:
            pass
        self.after(100, self._drain_queue)

    def apply_filters(self) -> None:
        selected_address = self._selected_address()
        visible_servers: list[Quake3Server] = []
        values_by_address: dict[str, tuple[str, ...]] = {}

        for server in self.servers:
            if not self._passes_filters(server):
                continue
            visible_servers.append(server)
            values_by_address[server.address] = self._server_values(server)

        self.server_list.set_servers(visible_servers, values_by_address, selected_address)

    def _passes_filters(self, server: Quake3Server) -> bool:
        if not server.online:
            return True
        if self.filter_hide_empty.get() and len(server.players) == 0:
            return False
        if self.country_filter and self._server_location(server) != self.country_filter:
            return False

        max_clients = self._max_clients(server)
        if self.filter_hide_full.get() and max_clients is not None and len(server.players) >= max_clients:
            return False

        max_ping = self.filter_max_ping.get().strip()
        if max_ping:
            try:
                limit = int(max_ping)
                ping = self._server_ping(server)
                if ping is not None and ping > limit:
                    return False
            except ValueError:
                self.status_var.set("Max ping must be a number")
        return True

    def sort_servers(self, column: str) -> None:
        numeric = column in {"ping", "players"}

        def sort_key(server: Quake3Server):
            values = dict(
                zip(("location", "name", "ping", "players", "game", "map", "address"), self._server_values(server))
            )
            value = values[column]
            if numeric:
                digits = "".join(ch for ch in value if ch.isdigit())
                return int(digits) if digits else 999999
            return value.lower()

        self.servers.sort(key=sort_key)
        self.apply_filters()

    def on_server_selected(self, server: Quake3Server | None = None) -> None:
        self._clear_details()
        if server is None:
            server = self.server_list.selected_server()
        if server is None:
            return

        game_settings = self._game_settings_for_server(server)
        draw_game_font = True if game_settings is None else game_settings.draw_player_names_with_game_font
        self.player_list.set_players(server.players, draw_game_font=draw_game_font)
        for key in sorted(server.info):
            self.info_tree.insert("", tk.END, values=(key, server.info[key]))

    def on_nav_selected(self, _event=None) -> None:
        selection = self.nav_tree.selection()
        if not selection:
            return
        values = self.nav_tree.item(selection[0], "values")
        if len(values) == 2 and values[0] == "country":
            self.country_filter = values[1]
            self.status_var.set(f"Country filter: {self.country_filter}")
            self.apply_filters()
        elif self.country_filter:
            self.country_filter = None
            self.apply_filters()

    def add_server_dialog(self) -> None:
        dialog = tk.Toplevel(self)
        dialog.title("Add Quake 3 Server")
        dialog.resizable(False, False)
        dialog.transient(self)
        dialog.grab_set()

        address_var = tk.StringVar(value=f"phobos.ioquake3.org:{DEFAULT_PORT}")
        ttk.Label(dialog, text="Address").grid(row=0, column=0, padx=8, pady=8, sticky=tk.W)
        entry = ttk.Entry(dialog, textvariable=address_var, width=34)
        entry.grid(row=0, column=1, padx=8, pady=8)
        entry.focus_set()

        def add() -> None:
            try:
                host, port = parse_address(address_var.get())
                server = self.client.status(host, port)
                self.servers.append(server)
                self.apply_filters()
                self.status_var.set(f"Added {server.address}")
                dialog.destroy()
            except Exception as exc:  # noqa: BLE001 - shown to the user.
                messagebox.showerror("OpenASE", str(exc), parent=dialog)

        ttk.Button(dialog, text="Add", command=add).grid(row=1, column=0, padx=8, pady=(0, 8))
        ttk.Button(dialog, text="Cancel", command=dialog.destroy).grid(row=1, column=1, padx=8, pady=(0, 8), sticky=tk.E)
        dialog.bind("<Return>", lambda _event: add())

    def open_options_dialog(self) -> None:
        dialog = tk.Toplevel(self)
        dialog.title("Options")
        dialog.geometry("560x430")
        dialog.transient(self)
        dialog.grab_set()

        notebook = ttk.Notebook(dialog)
        notebook.pack(fill=tk.BOTH, expand=True, padx=8, pady=8)
        for tab_name in ("General", "Misc", "Network", "Events", "DDE", "Colors", "Sounds"):
            frame = ttk.Frame(notebook)
            ttk.Label(frame, text=f"{tab_name} options will be implemented when needed.").pack(anchor=tk.W, padx=12, pady=12)
            notebook.add(frame, text=tab_name)

        games_frame = ttk.Frame(notebook)
        notebook.add(games_frame, text="Games")
        self._build_games_options(games_frame)

        buttons = ttk.Frame(dialog)
        buttons.pack(fill=tk.X, padx=8, pady=(0, 8))
        ttk.Button(buttons, text="OK", command=lambda: self._close_options(dialog, save=True)).pack(side=tk.RIGHT, padx=4)
        ttk.Button(buttons, text="Cancel", command=dialog.destroy).pack(side=tk.RIGHT, padx=4)

    def _build_games_options(self, parent: ttk.Frame) -> None:
        left = ttk.Frame(parent)
        left.pack(side=tk.LEFT, fill=tk.BOTH, expand=False, padx=(0, 8), pady=4)
        right = ttk.Frame(parent)
        right.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, pady=4)

        self.options_game_tree = ttk.Treeview(left, show="tree", height=16)
        self.options_game_tree.pack(fill=tk.BOTH, expand=True)
        installed = self.options_game_tree.insert("", tk.END, text="Installed games", open=True)
        not_installed = self.options_game_tree.insert("", tk.END, text="Not Installed", open=True)
        for name in sorted(self.game_definitions):
            settings = self.game_settings[name]
            parent_item = installed if settings.detected else not_installed
            self.options_game_tree.insert(parent_item, tk.END, text=name, values=(name,))

        self.option_visible = tk.BooleanVar()
        self.option_player_name = tk.StringVar()
        self.option_config_file = tk.StringVar()
        self.option_program_location = tk.StringVar()
        self.option_allow_invitations = tk.BooleanVar()
        self.option_draw_game_font = tk.BooleanVar()
        self.selected_options_game: str | None = None

        ttk.Checkbutton(right, text="Visible in filter list", variable=self.option_visible).pack(anchor=tk.W, pady=(4, 8))
        ttk.Label(right, text="Player name").pack(anchor=tk.W)
        ttk.Entry(right, textvariable=self.option_player_name).pack(fill=tk.X, pady=(0, 6))
        ttk.Label(right, text="Config file").pack(anchor=tk.W)
        config_row = ttk.Frame(right)
        config_row.pack(fill=tk.X, pady=(0, 6))
        ttk.Entry(config_row, textvariable=self.option_config_file).pack(side=tk.LEFT, fill=tk.X, expand=True)
        ttk.Button(config_row, text="Browse", command=self._browse_config_file).pack(side=tk.LEFT, padx=(4, 0))
        ttk.Label(right, text="Program location").pack(anchor=tk.W)
        program_row = ttk.Frame(right)
        program_row.pack(fill=tk.X, pady=(0, 8))
        ttk.Entry(program_row, textvariable=self.option_program_location).pack(side=tk.LEFT, fill=tk.X, expand=True)
        ttk.Button(program_row, text="Browse", command=self._browse_program_location).pack(side=tk.LEFT, padx=(4, 0))
        ttk.Button(right, text="Auto-Search", command=self._auto_search_selected_game).pack(anchor=tk.E, pady=(0, 8))
        ttk.Checkbutton(right, text="allow invitations for this game", variable=self.option_allow_invitations).pack(
            anchor=tk.W
        )
        ttk.Checkbutton(
            right, text="draw player names with game font", variable=self.option_draw_game_font
        ).pack(anchor=tk.W)

        self.options_game_tree.bind("<<TreeviewSelect>>", self._on_options_game_selected)

    def _on_options_game_selected(self, _event=None) -> None:
        self._store_selected_options_game()
        selection = self.options_game_tree.selection()
        if not selection:
            return
        values = self.options_game_tree.item(selection[0], "values")
        if not values:
            return
        name = values[0]
        self.selected_options_game = name
        settings = self.game_settings[name]
        self.option_visible.set(settings.visible)
        self.option_player_name.set(settings.player_name)
        self.option_config_file.set(settings.config_file)
        self.option_program_location.set(settings.program_location)
        self.option_allow_invitations.set(settings.allow_invitations)
        self.option_draw_game_font.set(settings.draw_player_names_with_game_font)

    def _store_selected_options_game(self) -> None:
        if not getattr(self, "selected_options_game", None):
            return
        settings = self.game_settings[self.selected_options_game]
        settings.visible = bool(self.option_visible.get())
        settings.player_name = self.option_player_name.get()
        settings.config_file = self.option_config_file.get()
        settings.program_location = self.option_program_location.get()
        settings.allow_invitations = bool(self.option_allow_invitations.get())
        settings.draw_player_names_with_game_font = bool(self.option_draw_game_font.get())

    def _browse_config_file(self) -> None:
        filename = filedialog.askopenfilename(title="Select config file")
        if filename:
            self.option_config_file.set(filename)

    def _browse_program_location(self) -> None:
        filename = filedialog.askopenfilename(title="Select game executable", filetypes=[("Executable", "*.exe"), ("All files", "*.*")])
        if filename:
            self.option_program_location.set(filename)
            self.option_visible.set(True)

    def _auto_search_selected_game(self) -> None:
        if not getattr(self, "selected_options_game", None):
            return
        definition = self.game_definitions.get(self.selected_options_game)
        if definition is None:
            return
        detected = detect_game(definition, deep_search=True)
        if detected:
            self.option_program_location.set(detected)
            self.option_visible.set(True)
            self.status_var.set(f"Detected {self.selected_options_game}")
        else:
            messagebox.showinfo("Auto-Search", "No verified installation found.", parent=self)

    def _close_options(self, dialog: tk.Toplevel, *, save: bool) -> None:
        if save:
            self._store_selected_options_game()
            self._save_settings()
            self._seed_tree()
        dialog.destroy()

    def show_about(self) -> None:
        messagebox.showinfo(
            "About OpenASE",
            "OpenASE\nQuake 3 server browser revival.\n\nThis is a new implementation and does not use Yahoo code.",
        )

    def close(self) -> None:
        self._save_open_nav_paths()
        self.destroy()

    def _server_values(self, server: Quake3Server) -> tuple[str, str, str, str, str, str, str]:
        if not server.online:
            return (UNKNOWN_LOCATION, "offline", "", "", "", "", server.address)

        name = strip_quake_colors(server.info.get("sv_hostname") or server.info.get("hostname") or "unknown")
        game = server.info.get("gamename") or server.info.get("com_gamename") or "Quake3"
        map_name = server.info.get("mapname", "")
        max_clients = self._max_clients(server)
        players = f"{len(server.players)}/{max_clients if max_clients is not None else '?'}"
        ping = str(self._server_ping(server) or "")
        return (self._server_location(server), name, ping, players, game, map_name, server.address)

    def _server_location(self, server: Quake3Server) -> str:
        return server_location(server.info)

    def _game_settings_for_server(self, server: Quake3Server) -> GameSettings | None:
        game_name = server.info.get("gamename") or server.info.get("com_gamename") or ""
        if game_name.lower() in {"baseq3", "cpma", "osp", "defrag", "ra3", "q3ut4", "quake3arena"}:
            return self.game_settings.get("Quake 3: Arena")
        return self.game_settings.get(game_name)

    def _server_ping(self, server: Quake3Server) -> int | None:
        pings = [player.ping for player in server.players if player.ping > 0]
        return min(pings) if pings else None

    def _max_clients(self, server: Quake3Server) -> int | None:
        raw = server.info.get("sv_maxclients") or server.info.get("sv_maxClients")
        try:
            return int(raw) if raw is not None else None
        except ValueError:
            return None

    def _selected_address(self) -> str | None:
        return self.server_list.selected_address

    def _visible_game_names(self) -> set[str]:
        return {name for name, settings in self.game_settings.items() if settings.visible}

    def _clear_details(self) -> None:
        self.player_list.clear()
        self._clear_tree(self.info_tree)

    @staticmethod
    def _clear_tree(tree: ttk.Treeview) -> None:
        for item in tree.get_children():
            tree.delete(item)

    def _load_settings(self) -> dict:
        if not self.settings_path.exists():
            return {}
        try:
            return json.loads(self.settings_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}

    def _load_open_nav_paths(self) -> set[str]:
        paths = self.settings.get("open_nav_paths", [])
        if not isinstance(paths, list):
            return set()
        return {path for path in paths if isinstance(path, str)}

    def _load_game_settings(self) -> dict[str, GameSettings]:
        raw_games = self.settings.get("games", {})
        if not isinstance(raw_games, dict):
            raw_games = {}
        result: dict[str, GameSettings] = {}
        for name in self.game_definitions:
            raw = raw_games.get(name, {})
            result[name] = game_settings_from_json(name, raw if isinstance(raw, dict) else {})
        return result

    def _detect_installed_games(self) -> None:
        for name, definition in self.game_definitions.items():
            settings = self.game_settings[name]
            if settings.program_location and Path(settings.program_location).exists():
                settings.detected = True
                settings.visible = True
                continue
            detected = detect_game(definition)
            if detected:
                settings.detected = True
                settings.visible = True
                settings.program_location = detected

    def _save_settings(self) -> None:
        self.settings = {
            "open_nav_paths": sorted(self._collect_open_nav_paths()),
            "games": {name: game_settings_to_json(settings) for name, settings in self.game_settings.items()},
        }
        self.settings_path.write_text(json.dumps(self.settings, indent=2), encoding="utf-8")

    def _save_open_nav_paths(self) -> None:
        self.settings["open_nav_paths"] = sorted(self._collect_open_nav_paths())
        self.settings["games"] = {
            name: game_settings_to_json(settings) for name, settings in self.game_settings.items()
        }
        self.settings_path.write_text(json.dumps(self.settings, indent=2), encoding="utf-8")

    def _collect_open_nav_paths(self) -> set[str]:
        paths: set[str] = set()

        def visit(item: str, parent_path: str = "") -> None:
            label = self.nav_tree.item(item, "text")
            current_path = f"{parent_path}\\{label}" if parent_path else label
            if self.nav_tree.item(item, "open"):
                paths.add(current_path)
            for child in self.nav_tree.get_children(item):
                visit(child, current_path)

        for root in self.nav_tree.get_children():
            visit(root)
        return paths


def main() -> int:
    app = OpenASEApp()
    app.mainloop()
    return 0
