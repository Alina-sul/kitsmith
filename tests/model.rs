//! Project-model distillation and usage-index tests.
//!
//! Built against `RytmProject::try_default()` — the device defaults rytm-rs
//! ships — so they run without hardware.

use kitsmith::model::{pattern_display_name, ProjectModel, TRACK_COUNT};
use rytm_rs::RytmProject;

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

#[test]
fn sound_locks_are_scanned_into_the_dependency_graph() {
    let (mut rytm, _) = default_model();
    // Pattern 2, track 0, trig 4 sound-locks pool sound 7…
    rytm.patterns_mut()[2].tracks_mut()[0].trigs_mut()[4]
        .set_sound_lock(7)
        .unwrap();
    // …and pool sound 7 references sample slot 42.
    rytm.pool_sounds_mut()[7]
        .sample_mut()
        .set_slice_number(42)
        .unwrap();

    let model = ProjectModel::from_rytm(&rytm);
    assert!(model.dependency_scan);
    assert_eq!(model.patterns[2].sound_locks, vec![7]);
    assert_eq!(model.patterns_sound_locking(7), vec![2]);

    let refs = model.slot_references(42);
    assert!(refs.kit_uses.is_empty());
    assert_eq!(refs.pool_uses.len(), 1);
    assert_eq!(refs.pool_uses[0].pool_index, 7);
    assert_eq!(refs.pool_uses[0].patterns, vec![2]);
    // A slot used only through a Sound Lock chain is NOT safe to unload.
    assert_eq!(model.safe_to_unload(42), Some(false));
    assert_eq!(model.safe_to_unload(99), Some(true));
}

#[test]
fn sample_plocks_are_scanned_into_the_dependency_graph() {
    let (mut rytm, _) = default_model();
    rytm.patterns_mut()[5].tracks_mut()[3].trigs_mut()[0]
        .plock_set_sample_number(23)
        .unwrap();

    let model = ProjectModel::from_rytm(&rytm);
    assert_eq!(model.patterns[5].plocked_sample_slots, vec![23]);
    let refs = model.slot_references(23);
    assert_eq!(refs.plock_patterns, vec![5]);
    assert_eq!(model.safe_to_unload(23), Some(false));
}

#[test]
fn old_caches_report_incomplete_dependency_data() {
    let model = ProjectModel::default();
    assert!(!model.dependency_scan);
    assert_eq!(model.safe_to_unload(1), None);
}
