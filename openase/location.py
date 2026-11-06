"""Location helpers for server rows."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

UNKNOWN_LOCATION = "Unknown"


@dataclass(frozen=True, slots=True)
class Country:
    code: str
    name: str


def load_countries(path: Path) -> list[Country]:
    """Load ASE country code labels from `countries.txt`."""

    countries: list[Country] = []
    for raw_line in path.read_text(encoding="latin-1").splitlines():
        if not raw_line.strip():
            continue
        code = raw_line[:2].strip()
        name = raw_line[2:].strip()
        if code and name:
            countries.append(Country(code=code, name=name))
    return countries


def find_countries_file(start: Path) -> Path | None:
    """Find the nearest original `countries.txt` around the OpenASE folder."""

    for candidate in (start / "countries.txt", start.parent / "countries.txt", start.parent.parent / "countries.txt"):
        if candidate.exists():
            return candidate
    return None


def server_location(info: dict[str, str]) -> str:
    """Return a reported server location without guessing from IP addresses."""

    for key in ("location", "country", "countryCode", "ServerLocation"):
        value = info.get(key)
        if value:
            return value
    return UNKNOWN_LOCATION

