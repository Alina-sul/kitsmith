//! Local metadata store (PRD v2 §32–§34): aliases persist, never attach to
//! changed content, and unknown store versions are ignored rather than
//! misread.

use kitsmith::localmeta::{load_from, save_in, LocalMeta};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kitsmith-localmeta-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn alias_round_trips_through_disk() {
    let dir = temp_dir("roundtrip");
    let mut meta = LocalMeta::default();
    meta.set_project_alias(17, "Live set A", "DEUIL B");
    save_in(&dir, &meta).expect("save failed");

    let loaded = load_from(&dir);
    assert_eq!(loaded, meta);
    assert_eq!(loaded.project_alias(17, "DEUIL B"), Some("Live set A"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn alias_detaches_when_native_name_changes() {
    let mut meta = LocalMeta::default();
    meta.set_project_alias(17, "Live set A", "DEUIL B");
    // Slot 17 was overwritten with a different project: alias must not apply.
    assert_eq!(meta.project_alias(17, "ACID KIT"), None);
    // The original content still resolves.
    assert_eq!(meta.project_alias(17, "DEUIL B"), Some("Live set A"));
}

#[test]
fn empty_or_whitespace_alias_clears() {
    let mut meta = LocalMeta::default();
    meta.set_project_alias(3, "Keeper", "NAME");
    meta.set_project_alias(3, "   ", "NAME");
    assert_eq!(meta.project_alias(3, "NAME"), None);
}

#[test]
fn clear_removes_alias() {
    let mut meta = LocalMeta::default();
    meta.set_project_alias(3, "Keeper", "NAME");
    meta.clear_project_alias(3);
    assert_eq!(meta.project_alias(3, "NAME"), None);
}

#[test]
fn alias_is_trimmed_and_replaces_previous() {
    let mut meta = LocalMeta::default();
    meta.set_project_alias(5, "  First  ", "NAME");
    assert_eq!(meta.project_alias(5, "NAME"), Some("First"));
    meta.set_project_alias(5, "Second", "NAME");
    assert_eq!(meta.project_alias(5, "NAME"), Some("Second"));
}

#[test]
fn missing_store_loads_default() {
    let dir = temp_dir("missing");
    assert_eq!(load_from(&dir), LocalMeta::default());
}

#[test]
fn newer_store_version_is_not_misread() {
    let dir = temp_dir("newer");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("local_meta.json"),
        // A future format: version bumped, unknown fields.
        r#"{"version": 99, "projects": {"1": {"alias": "X", "native_name": "Y"}}}"#,
    )
    .unwrap();
    assert_eq!(load_from(&dir), LocalMeta::default());
    let _ = std::fs::remove_dir_all(&dir);
}
