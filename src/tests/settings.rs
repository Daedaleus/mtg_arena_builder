use super::*;
use crate::testutil::TempDir;

#[test]
fn defaults_when_missing_or_broken() {
    let dir = TempDir::new("settings");
    let (_, s) = SettingsFile::load(&dir.0);
    assert_eq!(s, Settings::default());
    std::fs::write(dir.path("settings.json"), b"{ kaputt").unwrap();
    let (_, s) = SettingsFile::load(&dir.0);
    assert_eq!(s, Settings::default());
}

#[test]
fn roundtrip_and_change_detection() {
    let dir = TempDir::new("settings");
    let (mut file, mut s) = SettingsFile::load(&dir.0);
    // Unverändert: nichts schreiben.
    assert!(!file.save(&s));
    assert!(!dir.path("settings.json").exists());

    s.companion = true;
    s.tab = 3;
    s.paste_text = "4 Shock".into();
    assert!(file.save(&s));
    assert!(!file.save(&s), "zweites Speichern ohne Änderung");

    let (_, loaded) = SettingsFile::load(&dir.0);
    assert_eq!(loaded, s);
}

#[test]
fn reads_bom_and_partial_files() {
    let dir = TempDir::new("settings");
    let mut body = b"\xEF\xBB\xBF".to_vec();
    body.extend_from_slice(br#"{ "companion": true, "zoom": 9.0 }"#);
    std::fs::write(dir.path("settings.json"), body).unwrap();
    let (_, s) = SettingsFile::load(&dir.0);
    assert!(s.companion);
    // Fehlende Felder bekommen Standardwerte, der Zoom wird begrenzt.
    assert!(s.include_side);
    assert_eq!(s.zoom(), ZOOM_RANGE.1);
}

#[test]
fn replacements_map_roundtrip() {
    let mut s = Settings::default();
    let mut map = HashMap::new();
    map.insert((7, "b".to_string()), "x".to_string());
    map.insert((7, "a".to_string()), "y".to_string());
    s.set_replacements(&map);
    assert_eq!(s.replacements[0].1, "a", "sortiert");
    assert_eq!(s.replacement_map(), map);
}
