use super::*;
use crate::testutil::TempDir;

#[test]
fn atomic_write_and_json_read() {
    let dir = TempDir::new("paths");
    let file = dir.path("a.json");
    write_atomic(&file, b"[1,2]").unwrap();
    assert!(!dir.path("a.json.tmp").exists(), "Zwischendatei entfernt");
    assert_eq!(read_json::<Vec<u32>>(&file), Some(vec![1, 2]));
    // Überschreiben ersetzt den Inhalt vollständig.
    write_atomic(&file, b"[3]").unwrap();
    assert_eq!(read_json::<Vec<u32>>(&file), Some(vec![3]));
    // UTF-8-BOM (z. B. aus Windows-Editoren) wird ignoriert.
    std::fs::write(&file, b"\xEF\xBB\xBF[4]").unwrap();
    assert_eq!(read_json::<Vec<u32>>(&file), Some(vec![4]));
    std::fs::write(&file, b"kaputt").unwrap();
    assert_eq!(read_json::<Vec<u32>>(&file), None);
    assert_eq!(read_json::<Vec<u32>>(&dir.path("fehlt.json")), None);
    assert!(write_atomic(&dir.path("kein/ordner.json"), b"1").is_err());
}

#[test]
fn install_dir_from_log_head() {
    let head = "Initialize engine version\nMono path[0] = 'D:/MTG/MTGA/MTGA_Data/Managed'\nMono config path";
    assert_eq!(
        install_from_log_head(head),
        Some(PathBuf::from("D:/MTG/MTGA/MTGA_Data"))
    );
    assert_eq!(install_from_log_head("nichts hier"), None);
}

#[test]
fn finds_newest_card_database() {
    let dir = TempDir::new("paths");
    let raw = dir.0.join("MTGA_Data").join("Downloads").join("Raw");
    std::fs::create_dir_all(&raw).unwrap();
    assert_eq!(card_db_in(&dir.0.join("MTGA_Data")), None);
    std::fs::write(raw.join("Raw_CardDatabase_old.mtga"), b"").unwrap();
    std::fs::write(raw.join("Raw_ClientLocalization_x.mtga"), b"").unwrap();
    std::fs::write(raw.join("Raw_CardDatabase_new.txt"), b"").unwrap();
    let found = card_db_in(&dir.0.join("MTGA_Data")).unwrap();
    assert!(found.ends_with("Raw_CardDatabase_old.mtga"));

    // Eigene Angabe: Installationsordner oder direkt MTGA_Data.
    let install = dir.0.to_string_lossy().to_string();
    assert_eq!(find_card_db(Some(&install)).unwrap(), found);
    let data = dir.0.join("MTGA_Data").to_string_lossy().to_string();
    assert_eq!(find_card_db(Some(&data)).unwrap(), found);
    let candidates = mtga_data_candidates(Some("  "));
    assert!(!candidates.iter().any(|p| p.as_os_str().is_empty()));
}

#[test]
fn platform_locations() {
    let logs = player_log_candidates();
    assert!(!logs.is_empty());
    assert!(logs.iter().all(|p| p.ends_with("Player.log")));
    // Nur vorhandene Logs werden geliefert.
    assert!(player_logs().iter().all(|p| p.is_file()));
    let data = data_dir();
    assert!(data.is_dir());
}
