//! Cache round-trip tests (against a temp directory, not the user cache).

use rytm_rs::RytmProject;
use rytmdash::cache::{load_from, save_in};
use rytmdash::model::ProjectModel;

#[test]
fn save_and_load_round_trips() {
    let dir = std::env::temp_dir().join(format!("rytmdash-cache-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let mut rytm = RytmProject::try_default().expect("default project");
    rytm.kits_mut()[2].sounds_mut()[3]
        .sample_mut()
        .set_slice_number(99)
        .unwrap();
    let model = ProjectModel::from_rytm(&rytm);

    let path = save_in(&dir, &model).expect("save");
    assert!(path.exists());
    let loaded = load_from(&dir).expect("load");
    assert_eq!(loaded, model);
    assert_eq!(loaded.kits[2].sounds[3].sample_slot, Some(99));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_from_missing_dir_is_none() {
    assert!(load_from(std::path::Path::new("/nonexistent/rytmdash-nowhere")).is_none());
}
