"""Parser for the original ASE `filters.txt` navigation tree."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path


METADATA_PREFIXES = (
    "ServerType:",
    "RegKey:",
    "ExeName:",
    "JoinTemplate:",
    "HostTemplate:",
    "GUID:",
    "VersionKey:",
    "Port:",
    "HostPort:",
    "Mutex:",
    "Flags:",
    "Group:",
    "Icon:",
    "Maps:",
    "Remote:",
    "Launch:",
    "JoinMsg:",
)
NON_GAME_ROOTS = {"Eye servers", "Modifiers", "Friends", "Special", "Favorites"}


@dataclass(slots=True)
class GameDefinition:
    """One top-level game entry parsed from ASE filters."""

    name: str
    paths: list[str] = field(default_factory=list)
    metadata: dict[str, str] = field(default_factory=dict)


def parse_filter_paths(path: Path) -> list[str]:
    """Return display paths from ASE `filters.txt`."""

    paths: list[str] = []
    seen: set[str] = set()

    for raw_line in path.read_text(encoding="latin-1").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("//"):
            continue
        if line[0].isdigit() or line.startswith("-"):
            continue
        if any(line.startswith(prefix) for prefix in METADATA_PREFIXES):
            continue
        if line not in seen:
            paths.append(line)
            seen.add(line)

    return paths


def parse_game_definitions(path: Path) -> dict[str, GameDefinition]:
    """Parse top-level games and their filter subpaths from ASE `filters.txt`."""

    games: dict[str, GameDefinition] = {}
    current_game: GameDefinition | None = None

    for raw_line in path.read_text(encoding="latin-1").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("//") or line[0].isdigit() or line.startswith("-"):
            continue

        if "\\" not in line and not any(line.startswith(prefix) for prefix in METADATA_PREFIXES):
            if line in NON_GAME_ROOTS:
                current_game = None
                continue
            current_game = games.setdefault(line, GameDefinition(name=line))
            if line not in current_game.paths:
                current_game.paths.append(line)
            continue

        if "\\" in line and not any(line.startswith(prefix) for prefix in METADATA_PREFIXES):
            root = line.split("\\", 1)[0]
            if root in NON_GAME_ROOTS:
                current_game = None
                continue
            current_game = games.setdefault(root, GameDefinition(name=root))
            if line not in current_game.paths:
                current_game.paths.append(line)
            continue

        if current_game is not None:
            key, separator, value = line.partition(":")
            if separator and key in {prefix[:-1] for prefix in METADATA_PREFIXES}:
                current_game.metadata.setdefault(key, value.strip())

    return games


def find_filters_file(start: Path) -> Path | None:
    """Find the nearest original `filters.txt` around the OpenASE folder."""

    for candidate in (start / "filters.txt", start.parent / "filters.txt", start.parent.parent / "filters.txt"):
        if candidate.exists():
            return candidate
    return None
