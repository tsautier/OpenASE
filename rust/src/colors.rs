//! Quake-style color code helpers.
//!
//! WolfWiki documents the Quake III color lookup as `(*char + 16) & 31`, with
//! 32 indexed colors. Some Q3-derived games also use extended control codes
//! such as `^xRRGGBB` for HTML-style hex colors and `^B`/`^F`/`^f`/`^N` for
//! text effects; OpenASE strips those controls from display text.

use std::sync::LazyLock;

use regex::Regex;

static HEX_COLOR_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^[xX][0-9a-fA-F]{6}").unwrap());
static CONTROL_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^[BbFfNn]").unwrap());
static ANY_CARET_CODE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^.").unwrap());

/// RGB colors indexed by `(code + 16) & 31`.
pub const QUAKE_INDEX_COLORS: [[u8; 3]; 32] = [
    [0x00, 0x00, 0x00],
    [0xff, 0x00, 0x00],
    [0x00, 0xff, 0x00],
    [0xff, 0xff, 0x00],
    [0x00, 0x00, 0xff],
    [0x00, 0xff, 0xff],
    [0xff, 0x00, 0xff],
    [0xff, 0xff, 0xff],
    [0xff, 0x7f, 0x00],
    [0x7f, 0x7f, 0x7f],
    [0xbf, 0xbf, 0xbf],
    [0xbf, 0xbf, 0xbf],
    [0x00, 0x7f, 0x00],
    [0x7f, 0x7f, 0x00],
    [0x00, 0x00, 0x7f],
    [0x7f, 0x00, 0x00],
    [0x7f, 0x3f, 0x00],
    [0xff, 0x99, 0x19],
    [0x00, 0x7f, 0x7f],
    [0x7f, 0x00, 0x7f],
    [0x00, 0x7f, 0xff],
    [0x7f, 0x00, 0xff],
    [0x33, 0x99, 0xcc],
    [0xcc, 0xff, 0xcc],
    [0x00, 0x66, 0x33],
    [0xff, 0x00, 0x33],
    [0xb2, 0x19, 0x19],
    [0x99, 0x33, 0x00],
    [0xcc, 0x99, 0x33],
    [0x99, 0x99, 0x33],
    [0xff, 0xff, 0xbf],
    [0xff, 0xff, 0x7f],
];

/// A run of text drawn in one color, as `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSegment {
    pub text: String,
    pub color: String,
}

/// Format an RGB triple as a lowercase `#rrggbb` string.
pub fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// Parse `#rrggbb` into RGB, or return `None`.
pub fn parse_hex(color: &str) -> Option<[u8; 3]> {
    let raw = color.strip_prefix('#')?;
    if raw.len() != 6 {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(raw.get(range)?, 16).ok();
    Some([channel(0..2)?, channel(2..4)?, channel(4..6)?])
}

fn index_color(code: char) -> String {
    hex(QUAKE_INDEX_COLORS[((code as u32 + 16) & 31) as usize])
}

fn is_effect(code: char) -> bool {
    matches!(code, 'B' | 'b' | 'F' | 'f' | 'N' | 'n')
}

/// Return the `^xRRGGBB` color starting at `index`, if present.
fn hex_at(chars: &[char], index: usize) -> Option<String> {
    if !matches!(chars[index + 1], 'x' | 'X') || index + 7 >= chars.len() {
        return None;
    }
    let raw: String = chars[index + 2..index + 8].iter().collect();
    raw.chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| format!("#{}", raw.to_lowercase()))
}

/// Remove Quake color and text-effect markers from display text.
pub fn strip_quake_colors(value: &str) -> String {
    let value = HEX_COLOR_PATTERN.replace_all(value, "");
    let value = CONTROL_PATTERN.replace_all(&value, "");
    ANY_CARET_CODE_PATTERN.replace_all(&value, "").into_owned()
}

/// Return the first color represented by a Quake color marker.
pub fn first_quake_color(value: &str) -> Option<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut index = 0;
    while index + 1 < chars.len() {
        if chars[index] != '^' {
            index += 1;
            continue;
        }
        if let Some(color) = hex_at(&chars, index) {
            return Some(color);
        }
        let code = chars[index + 1];
        if !is_effect(code) {
            return Some(index_color(code));
        }
        index += 2;
    }
    None
}

/// Split Quake-colored text into renderable colored text segments.
pub fn quake_color_segments(value: &str, default_color: &str) -> Vec<TextSegment> {
    let chars: Vec<char> = value.chars().collect();
    let mut segments = Vec::new();
    let mut buffer = String::new();
    let mut color = default_color.to_string();
    let mut index = 0;

    let mut flush = |buffer: &mut String, color: &str| {
        if !buffer.is_empty() {
            segments.push(TextSegment {
                text: std::mem::take(buffer),
                color: color.to_string(),
            });
        }
    };

    while index < chars.len() {
        let current = chars[index];
        if current != '^' || index == chars.len() - 1 {
            buffer.push(current);
            index += 1;
            continue;
        }

        let code = chars[index + 1];
        if code == '^' {
            buffer.push('^');
            index += 2;
            continue;
        }
        if let Some(hex_color) = hex_at(&chars, index) {
            flush(&mut buffer, &color);
            color = hex_color;
            index += 8;
            continue;
        }
        if is_effect(code) {
            index += 2;
            continue;
        }
        flush(&mut buffer, &color);
        color = index_color(code);
        index += 2;
    }

    flush(&mut buffer, &color);
    segments
}
