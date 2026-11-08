//! Toolbar, friend sound and color settings.

use openase::settings::{
    ColorSettings, FriendSound, Settings, TOOLBAR_DEFAULT, ToolbarItem, ToolbarSettings,
};

#[test]
fn defaults_show_every_button_in_order() {
    let toolbar = ToolbarSettings::default();
    assert_eq!(toolbar.visible_ids(), TOOLBAR_DEFAULT);
    let colors = ColorSettings::default();
    assert_eq!(colors.list_bg, [0xff; 3]);
    assert_eq!(colors.selection, [0x31, 0x6a, 0xc5]);
    assert!(colors.quake_colors);
    let settings = Settings::default();
    assert_eq!(settings.colors, colors);
    assert!(settings.friend_sounds.is_empty());
    assert_eq!(settings.friend_sound("Anyone"), FriendSound::Default);
}

#[test]
fn old_settings_files_load_with_defaults() {
    let settings: Settings =
        serde_json::from_str(r#"{"friends":["Bob"],"wizard_done":true}"#).expect("old file loads");
    assert_eq!(settings.friends, ["Bob"]);
    assert_eq!(settings.toolbar, ToolbarSettings::default());
    assert_eq!(settings.colors, ColorSettings::default());
    assert!(settings.friend_sounds.is_empty());
}

#[test]
fn partial_color_object_keeps_other_defaults() {
    let settings: Settings =
        serde_json::from_str(r#"{"colors":{"selection":[1,2,3]}}"#).expect("loads");
    assert_eq!(settings.colors.selection, [1, 2, 3]);
    assert_eq!(settings.colors.list_bg, ColorSettings::default().list_bg);
    assert!(settings.colors.quake_colors);
}

#[test]
fn round_trip_keeps_every_new_field() {
    let mut settings = Settings::default();
    settings.toolbar.items[0].visible = false;
    settings.toolbar.move_item(1, 1);
    settings.colors.list_bg = [10, 20, 30];
    settings.colors.quake_colors = false;
    settings
        .friend_sounds
        .insert("bob".into(), FriendSound::File("C:\\a\\b.wav".into()));
    settings
        .friend_sounds
        .insert("eve".into(), FriendSound::Silent);
    let text = serde_json::to_string_pretty(&settings).unwrap();
    let back: Settings = serde_json::from_str(&text).unwrap();
    assert_eq!(back.toolbar, settings.toolbar);
    assert_eq!(back.colors, settings.colors);
    assert_eq!(back.friend_sounds, settings.friend_sounds);
    assert_eq!(
        back.friend_sound("BOB"),
        FriendSound::File("C:\\a\\b.wav".into())
    );
    assert_eq!(back.friend_sound("eve"), FriendSound::Silent);
}

#[test]
fn toolbar_normalize_drops_unknown_and_restores_missing() {
    let mut toolbar = ToolbarSettings {
        items: vec![
            ToolbarItem {
                id: "options".into(),
                visible: true,
            },
            ToolbarItem {
                id: "bogus".into(),
                visible: true,
            },
            ToolbarItem {
                id: "options".into(),
                visible: false,
            },
            ToolbarItem {
                id: "refresh".into(),
                visible: false,
            },
        ],
    };
    toolbar.normalize();
    assert_eq!(toolbar.items.len(), TOOLBAR_DEFAULT.len());
    assert_eq!(toolbar.items[0].id, "options");
    assert!(toolbar.items[0].visible);
    assert!(
        toolbar
            .items
            .iter()
            .all(|item| TOOLBAR_DEFAULT.contains(&item.id.as_str()))
    );
    assert!(!toolbar.visible_ids().contains(&"refresh"));
}

#[test]
fn toolbar_move_item_stays_in_bounds() {
    let mut toolbar = ToolbarSettings::default();
    toolbar.move_item(0, -1);
    toolbar.move_item(TOOLBAR_DEFAULT.len() - 1, 1);
    assert_eq!(toolbar, ToolbarSettings::default());
    toolbar.move_item(0, 1);
    assert_eq!(toolbar.items[0].id, "stop");
    assert_eq!(toolbar.items[1].id, "refresh");
}

#[test]
fn import_export_normalizes_the_toolbar() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile.json");
    std::fs::write(
        &path,
        r#"{"toolbar":{"items":[{"id":"find","visible":true}]}}"#,
    )
    .unwrap();
    let settings = Settings::import_from(&path).unwrap();
    assert_eq!(settings.toolbar.items.len(), TOOLBAR_DEFAULT.len());
    assert_eq!(settings.toolbar.items[0].id, "find");
}
