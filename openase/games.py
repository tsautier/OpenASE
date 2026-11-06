"""Game configuration and local detection."""

from __future__ import annotations

import os
from dataclasses import asdict, dataclass
from pathlib import Path

from .ase_filters import GameDefinition

try:
    import winreg
except ImportError:  # pragma: no cover - non-Windows fallback.
    winreg = None


@dataclass(slots=True)
class GameSettings:
    name: str
    visible: bool = False
    detected: bool = False
    player_name: str = ""
    config_file: str = ""
    program_location: str = ""
    allow_invitations: bool = True
    draw_player_names_with_game_font: bool = True


def game_settings_from_json(name: str, raw: dict) -> GameSettings:
    """Create game settings from persisted JSON data."""

    return GameSettings(
        name=name,
        visible=bool(raw.get("visible", False)),
        detected=bool(raw.get("detected", False)),
        player_name=str(raw.get("player_name", "")),
        config_file=str(raw.get("config_file", "")),
        program_location=str(raw.get("program_location", "")),
        allow_invitations=bool(raw.get("allow_invitations", True)),
        draw_player_names_with_game_font=bool(raw.get("draw_player_names_with_game_font", True)),
    )


def game_settings_to_json(settings: GameSettings) -> dict:
    """Serialize game settings to JSON-safe values."""

    return asdict(settings)


def detect_game(definition: GameDefinition, *, deep_search: bool = False) -> str | None:
    """Return a verified executable path for a game, or None."""

    registry_path = _detect_from_registry(definition)
    if registry_path:
        return registry_path
    if not deep_search:
        return None
    return _detect_from_common_paths(definition)


def _detect_from_registry(definition: GameDefinition) -> str | None:
    if winreg is None:
        return None

    reg_key = definition.metadata.get("RegKey")
    exe_name = definition.metadata.get("ExeName")
    if not reg_key:
        return None

    root_name, _, subkey = reg_key.partition("\\")
    root = {
        "HKEY_LOCAL_MACHINE": winreg.HKEY_LOCAL_MACHINE,
        "HKEY_CURRENT_USER": winreg.HKEY_CURRENT_USER,
    }.get(root_name)
    if root is None or not subkey:
        return None

    try:
        with winreg.OpenKey(root, subkey) as key:
            value, _value_type = winreg.QueryValueEx(key, "")
    except OSError:
        return None

    base_path = Path(str(value).strip('"'))
    candidates = [base_path]
    if exe_name:
        candidates.append(base_path / exe_name)
    for candidate in candidates:
        if candidate.exists() and candidate.is_file():
            return str(candidate)
    return None


def _detect_from_common_paths(definition: GameDefinition) -> str | None:
    exe_name = definition.metadata.get("ExeName")
    if not exe_name:
        return None

    roots = [
        os.environ.get("ProgramFiles"),
        os.environ.get("ProgramFiles(x86)"),
        os.environ.get("ProgramW6432"),
    ]
    for root in roots:
        if not root:
            continue
        root_path = Path(root)
        for candidate in root_path.rglob(exe_name):
            if candidate.is_file():
                return str(candidate)
    return None
