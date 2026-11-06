"""Quake-style color code helpers.

WolfWiki documents the Quake III color lookup as `(*char + 16) & 31`, with
32 indexed colors. Some Q3-derived games also use extended control codes such
as `^xRRGGBB` for HTML-style hex colors and `^B`/`^F`/`^f`/`^N` for text
effects; OpenASE strips those controls from display text.
"""

from __future__ import annotations

from dataclasses import dataclass
import re

HEX_COLOR_PATTERN = re.compile(r"\^[xX]([0-9a-fA-F]{6})")
CONTROL_PATTERN = re.compile(r"\^[BbFfNn]")
ANY_CARET_CODE_PATTERN = re.compile(r"\^.")
QUAKE_INDEX_COLORS = (
    "#000000",
    "#ff0000",
    "#00ff00",
    "#ffff00",
    "#0000ff",
    "#00ffff",
    "#ff00ff",
    "#ffffff",
    "#ff7f00",
    "#7f7f7f",
    "#bfbfbf",
    "#bfbfbf",
    "#007f00",
    "#7f7f00",
    "#00007f",
    "#7f0000",
    "#7f3f00",
    "#ff9919",
    "#007f7f",
    "#7f007f",
    "#007fff",
    "#7f00ff",
    "#3399cc",
    "#ccffcc",
    "#006633",
    "#ff0033",
    "#b21919",
    "#993300",
    "#cc9933",
    "#999933",
    "#ffffbf",
    "#ffff7f",
)


@dataclass(frozen=True, slots=True)
class TextSegment:
    text: str
    color: str


def strip_quake_colors(value: str) -> str:
    """Remove Quake color and text-effect markers from display text."""

    value = HEX_COLOR_PATTERN.sub("", value)
    value = CONTROL_PATTERN.sub("", value)
    return ANY_CARET_CODE_PATTERN.sub("", value)


def first_quake_color(value: str) -> str | None:
    """Return the first color represented by a Quake color marker."""

    index = 0
    while index < len(value) - 1:
        if value[index] != "^":
            index += 1
            continue

        code = value[index + 1]
        if code in {"x", "X"} and index + 7 < len(value):
            raw_hex = value[index + 2 : index + 8]
            if re.fullmatch(r"[0-9a-fA-F]{6}", raw_hex):
                return f"#{raw_hex.lower()}"
        if code not in {"B", "b", "F", "f", "N", "n"}:
            color_index = (ord(code) + 16) & 31
            return QUAKE_INDEX_COLORS[color_index]
        index += 2

    return None


def quake_color_segments(value: str, default_color: str = "#202020") -> list[TextSegment]:
    """Split Quake-colored text into renderable colored text segments."""

    segments: list[TextSegment] = []
    buffer: list[str] = []
    color = default_color
    index = 0

    def flush() -> None:
        if buffer:
            segments.append(TextSegment("".join(buffer), color))
            buffer.clear()

    while index < len(value):
        char = value[index]
        if char != "^" or index == len(value) - 1:
            buffer.append(char)
            index += 1
            continue

        code = value[index + 1]
        if code == "^":
            buffer.append("^")
            index += 2
            continue

        if code in {"x", "X"} and index + 7 < len(value):
            raw_hex = value[index + 2 : index + 8]
            if re.fullmatch(r"[0-9a-fA-F]{6}", raw_hex):
                flush()
                color = f"#{raw_hex.lower()}"
                index += 8
                continue

        if code in {"B", "b", "F", "f", "N", "n"}:
            index += 2
            continue

        flush()
        color = QUAKE_INDEX_COLORS[(ord(code) + 16) & 31]
        index += 2

    flush()
    return segments
