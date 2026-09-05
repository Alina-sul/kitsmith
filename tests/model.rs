//! Project-model distillation and usage-index tests.
//!
//! Built against `RytmProject::try_default()` — the device defaults rytm-rs
//! ships — so they run without hardware.

use rytm_rs::RytmProject;
use rytmdash::model::{pattern_display_name, ProjectModel, TRACK_COUNT};

fn default_model() -> (RytmProject, ProjectModel) {
    let rytm = RytmProject::try_default().expect("default project");
    let model = ProjectModel::from_rytm(&rytm);
    (rytm, model)
}

#[test]
fn distills_full_project_shape() {
    let (_, model) = default_model();
    assert_eq!(model.kits.len(), 128);
    assert_eq!(model.patterns.len(), 128);
    assert_eq!(model.pool_sounds.len(), 128);
    for kit in &model.kits {
        assert_eq!(kit.sounds.len(), TRACK_COUNT);
    }
}

#[test]
fn default_project_has_no_sample_usage() {
    // Factory defaults are synth-only (SMP off) — the usage index must be empty.
    let (_, model) = default_model();
    assert!(model.slot_usage().is_empty());
}

#[test]
fn assigned_sample_slots_appear_in_usage_index() {
    let (mut rytm, _) = default_model();
    // Point kit 3 / track 0 and kit 7 / track 5 at slot 42, kit 3 / track 1 at 7.
    rytm.kits_mut()[3].sounds_mut()[0]
        .sample_mut()
        .set_slice_number(42)
        .unwrap();
    rytm.kits_mut()[7].sounds_mut()[5]
        .sample_mut()
        .set_slice_number(42)
        .unwrap();
    rytm.kits_mut()[3].sounds_mut()[1]
        .sample_mut()
        .set_slice_number(7)
        .unwrap();

    let model = ProjectModel::from_rytm(&rytm);
    let usage = model.slot_usage();
    assert_eq!(usage.len(), 2);
    let slot42 = &usage[&42];
    assert_eq!(slot42.len(), 2);
    assert_eq!((slot42[0].kit_index, slot42[0].track), (3, 0));
    assert_eq!((slot42[1].kit_index, slot42[1].track), (7, 5));
    assert_eq!(usage[&7].len(), 1);
}

#[test]
fn smp_off_values_do_not_count_as_usage() {
    let (mut rytm, _) = default_model();
    rytm.kits_mut()[0].sounds_mut()[0]
        .sample_mut()
        .unset_slice(); // 0xFF
    let model = ProjectModel::from_rytm(&rytm);
    assert!(model.slot_usage().is_empty());
    assert_eq!(model.kits[0].sounds[0].sample_slot, None);
}

#[test]
fn linked_kits_follow_pattern_links() {
    let (mut rytm, _) = default_model();
    rytm.patterns_mut()[0].set_kit_number(5).unwrap();
    rytm.patterns_mut()[1].set_kit_number(5).unwrap();
    rytm.patterns_mut()[2].set_kit_number(9).unwrap();
    let model = ProjectModel::from_rytm(&rytm);
    let linked = model.linked_kit_indices();
    assert!(linked.contains(&5));
    assert!(linked.contains(&9));
    // Deduplicated.
    assert_eq!(linked.iter().filter(|&&k| k == 5).count(), 1);
}

#[test]
fn pattern_names_match_device_banks() {
    assert_eq!(pattern_display_name(0), "A01");
    assert_eq!(pattern_display_name(15), "A16");
    assert_eq!(pattern_display_name(16), "B01");
    assert_eq!(pattern_display_name(127), "H16");
}
