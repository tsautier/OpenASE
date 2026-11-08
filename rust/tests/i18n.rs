//! Interface translation: lookup rules and coverage of the source files.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use openase::i18n::{self, Language, fill, french, group_name, tr};

/// The language is a process-wide switch: tests that change it run one at a time.
static LANGUAGE: Mutex<()> = Mutex::new(());

fn with_language<T>(language: Language, run: impl FnOnce() -> T) -> T {
    let _guard = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    i18n::set_language(language);
    let result = run();
    i18n::set_language(Language::English);
    result
}

#[test]
fn french_on_translates_known_strings() {
    with_language(Language::French, || {
        assert_eq!(tr("Refresh"), "Actualiser");
        assert_eq!(tr("LAN servers"), "Serveurs LAN");
        assert_eq!(tr("Add server..."), "Ajouter un serveur…");
    });
}

#[test]
fn french_off_returns_the_input() {
    with_language(Language::English, || {
        assert_eq!(tr("Refresh"), "Refresh");
        assert_eq!(tr("Add server..."), "Add server...");
    });
}

#[test]
fn unknown_string_falls_back_to_english() {
    with_language(Language::French, || {
        assert_eq!(tr("No such interface text"), "No such interface text");
    });
}

#[test]
fn fill_replaces_placeholders_in_order() {
    assert_eq!(fill("{} of {}", &[&1, &"two"]), "1 of two");
    assert_eq!(fill("no values", &[]), "no values");
    assert_eq!(fill("{} and {}", &[&"a"]), "a and {}");
}

#[test]
fn group_names_follow_the_language() {
    with_language(Language::French, || {
        assert_eq!(group_name("Favorites3"), "Favoris3");
        assert_eq!(group_name("Friends1"), "Amis1");
        assert_eq!(group_name("Other"), "Other");
    });
    with_language(Language::English, || {
        assert_eq!(group_name("Favorites3"), "Favorites3");
    });
}

#[test]
fn language_setting_resolves() {
    assert!(Language::French.is_french());
    assert!(!Language::English.is_french());
    // System follows the OS; either answer is valid, it must just not panic.
    let _ = Language::System.is_french();
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Undo the escapes of a Rust string literal (the ones the sources use).
fn unescape(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn count_placeholders(text: &str) -> usize {
    text.matches("{}").count()
}

/// Every English text the code asks to translate: `tr("..")`, `noop("..")`,
/// `dialog("..")` / `window("..")` titles and the `SOUND_*`, `TOOLBAR_*` and
/// `COLORS_*` constants.
fn source_texts() -> Vec<(String, String)> {
    let literal = r#""((?:[^"\\]|\\.)*)""#;
    let calls =
        regex::Regex::new(&format!(r"\b(?:tr|noop|dialog|window)\(\s*{literal}\s*\)")).unwrap();
    let consts = regex::Regex::new(&format!(
        r"(?m)^\s*const (?:SOUND|TOOLBAR|COLORS)_[A-Z_]+: &str =\s*{literal};"
    ))
    .unwrap();
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut found = Vec::new();
    for file in files {
        if file.ends_with("i18n.rs") {
            continue;
        }
        let source = std::fs::read_to_string(&file).unwrap();
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        for re in [&calls, &consts] {
            for capture in re.captures_iter(&source) {
                found.push((name.clone(), unescape(&capture[1])));
            }
        }
    }
    found
}

#[test]
fn every_source_text_has_a_french_entry() {
    let texts = source_texts();
    assert!(
        texts.len() > 150,
        "scanner found only {} texts",
        texts.len()
    );
    let mut missing: Vec<String> = texts
        .iter()
        .filter(|(_, text)| french(text).is_none())
        .map(|(file, text)| format!("{file}: {text:?}"))
        .collect();
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "texts without a French entry in src/i18n.rs:\n{}",
        missing.join("\n")
    );
}

#[test]
fn french_entries_keep_their_placeholders() {
    let mut bad = Vec::new();
    for (_, text) in source_texts() {
        if let Some(fr) = french(&text)
            && count_placeholders(&text) != count_placeholders(fr)
        {
            bad.push(format!("{text:?} -> {fr:?}"));
        }
    }
    assert!(bad.is_empty(), "placeholder mismatch:\n{}", bad.join("\n"));
}

#[test]
fn old_settings_without_language_default_to_system() {
    let settings: openase::settings::Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(settings.language, Language::System);
    let french: openase::settings::Settings =
        serde_json::from_str(r#"{"language":"French"}"#).unwrap();
    assert_eq!(french.language, Language::French);
}
