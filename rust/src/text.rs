//! Text from game servers: most send UTF-8 today, older ones a legacy 8-bit
//! charset (Latin-1 or the Quake font). Decode both without showing mojibake
//! such as "CafÃ©" for "Café".

/// Decode bytes as UTF-8 when they are valid UTF-8, else as Latin-1.
pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&byte| char::from(byte)).collect(),
    }
}

/// Fix a field that was decoded as Latin-1 but holds UTF-8 bytes.
///
/// Packets are decoded as Latin-1 as a whole so the parsers never fail, but
/// a server name or player name inside may be UTF-8. Each field is re-read as
/// UTF-8 when its bytes are valid UTF-8; anything else (true Latin-1 accents,
/// Quake font glyphs) is left as it was.
pub fn repair(text: &str) -> String {
    if text.is_ascii() || text.chars().any(|c| u32::from(c) > 0xFF) {
        return text.to_string();
    }
    let bytes: Vec<u8> = text.chars().map(|c| c as u8).collect();
    match String::from_utf8(bytes) {
        Ok(utf8) => utf8,
        Err(error) if looks_cyrillic(error.as_bytes()) => cp1251(error.as_bytes()),
        Err(_) => text.to_string(),
    }
}

/// Russian servers often send Windows-1251: their names are mostly bytes in
/// 0xC0..=0xFF (А..я), where a Latin-1 name has a few accents among ASCII.
fn looks_cyrillic(bytes: &[u8]) -> bool {
    let letters = bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    let cyrillic = bytes
        .iter()
        .filter(|&&b| b >= 0xC0 || b == 0xA8 || b == 0xB8)
        .count();
    cyrillic >= 4 && cyrillic > letters
}

/// Decode Windows-1251 letters; other bytes keep their Latin-1 meaning.
fn cp1251(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0xC0..=0xFF => char::from_u32(0x0410 + u32::from(b - 0xC0)).unwrap_or('?'),
            0xA8 => 'Ё',
            0xB8 => 'ё',
            _ => char::from(b),
        })
        .collect()
}
