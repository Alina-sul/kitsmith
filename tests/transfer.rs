//! Cross-project pattern transfer: preflight matching, materialization and
//! plan structure — all offline against rytm-rs default objects.

use std::collections::BTreeMap;

use rytm_rs::prelude::*;
use rytm_rs::RytmProject;
use rytmdash::model::{ProjectModel, SampleSlotInfo};
use rytmdash::ops::{execute, OpStep, Plan};
use rytmdash::transfer::{materialize, plan_paste, PatternClipboard, SampleDep};

/// Builds a clipboard from locally-encoded default objects:
/// pattern 5 (kit link 5, sample plock slot 42, Sound Lock on pool 3),
/// kit 5 (track 0 → slot 42, track 1 → slot 7), pool sound 3 → slot 42.
fn test_clipboard() -> PatternClipboard {
    let mut rytm = RytmProject::try_default().expect("default project");
    rytm.patterns_mut()[5].set_kit_number(5).unwrap();
    rytm.patterns_mut()[5].tracks_mut()[0].trigs_mut()[0]
        .plock_set_sample_number(42)
        .unwrap();
    rytm.patterns_mut()[5].tracks_mut()[1].trigs_mut()[3]
        .set_sound_lock(3)
        .unwrap();
    rytm.kits_mut()[5].sounds_mut()[0]
        .sample_mut()
        .set_slice_number(42)
        .unwrap();
    rytm.kits_mut()[5].sounds_mut()[1]
        .sample_mut()
        .set_slice_number(7)
        .unwrap();
    // Default pool sounds have an unset machine, which rytm-rs cannot
    // encode; captured sounds always carry a real machine.
    rytm.pool_sounds_mut()[3]
        .set_machine_type(rytm_rs::object::sound::types::MachineType::BdHard)
        .unwrap();
    rytm.pool_sounds_mut()[3]
        .sample_mut()
        .set_slice_number(42)
        .unwrap();

    let mut pool_sounds = BTreeMap::new();
    pool_sounds.insert(3u8, rytm.pool_sounds()[3].as_sysex().unwrap());
    PatternClipboard {
        captured_at_unix: 0,
        source_hint: "A06 · Kit 06".into(),
        pattern_index: 5,
        kit_index: 5,
        kit_name: "TESTKIT".into(),
        pattern_sysex: rytm.patterns()[5].as_sysex().unwrap(),
        kit_sysex: rytm.kits()[5].as_sysex().unwrap(),
        pool_sounds,
        samples: vec![
            SampleDep {
                source_slot: 42,
                name: "Crash.wav".into(),
                path: "/crash".into(),
                size: 100,
                hash: 0xAA,
            },
            SampleDep {
                source_slot: 7,
                name: "Lost.wav".into(),
                path: String::new(),
                size: 0,
                hash: 0,
            },
        ],
    }
}

/// Target project: the same +Drive sample as slot 42 lives in its slot 9.
fn target_model() -> ProjectModel {
    ProjectModel {
        samples: vec![SampleSlotInfo {
            slot: 9,
            name: "Crash.wav".into(),
            path: "/crash".into(),
            size: 100,
            hash: 0xAA,
        }],
        ..ProjectModel::default()
    }
}

#[test]
fn preflight_matches_by_hash_and_reports_missing() {
    let clip = test_clipboard();
    let preview = plan_paste(&clip, &target_model());
    assert_eq!(preview.slot_remap.get(&42), Some(&9));
    assert_eq!(preview.missing.len(), 1);
    assert_eq!(preview.missing[0].source_slot, 7);
    assert_eq!(preview.overwrite_pattern, "A06");
    assert_eq!(preview.overwrite_pool, vec![3]);
}

#[test]
fn materialize_remaps_samples_and_round_trips() {
    let clip = test_clipboard();
    let preview = plan_paste(&clip, &target_model());
    let paste = materialize(&clip, &preview).expect("materialize");

    assert_eq!(paste.writes.len(), 3, "pool sound + kit + pattern");
    assert_eq!(paste.kit_index, 5);
    assert_eq!(paste.pattern_index, 5);
    assert_eq!(paste.expected_kit_slots[0], Some(9), "42 remapped to 9");
    assert_eq!(
        paste.expected_kit_slots[1], None,
        "7 had no match → SMP off"
    );

    // Decode the writes into a fresh project and verify the remaps landed.
    let mut verify = RytmProject::try_default().unwrap();
    // Pattern first so its plock pool links up (rytm-rs behavior).
    let pattern_write = &paste.writes[2];
    verify
        .update_from_sysex_response(&pattern_write.sysex)
        .unwrap();
    for write in &paste.writes[..2] {
        verify.update_from_sysex_response(&write.sysex).unwrap();
    }
    assert_eq!(verify.kits()[5].sounds()[0].sample().slice_number(), 9);
    assert_eq!(
        verify.pool_sounds()[3].sample().slice_number(),
        9,
        "pool sound sample remapped"
    );
    assert_eq!(
        verify.patterns()[5].tracks()[0].trigs()[0]
            .plock_get_sample_number()
            .unwrap(),
        Some(9),
        "sample plock remapped"
    );
    assert_eq!(
        verify.patterns()[5].tracks()[1].trigs()[3].sound_lock(),
        3,
        "sound lock unchanged (same-slot copy)"
    );
}

#[test]
fn paste_plan_backs_up_writes_then_verifies() {
    let clip = test_clipboard();
    let preview = plan_paste(&clip, &target_model());
    let paste = materialize(&clip, &preview).unwrap();
    let plan = Plan::paste_pattern(&paste);

    assert_eq!(plan.steps[0], OpStep::BackupProject, "backup always first");
    let sends = plan
        .steps
        .iter()
        .filter(|s| matches!(s, OpStep::SendObject { .. }))
        .count();
    assert_eq!(sends, 3);
    assert!(matches!(
        plan.steps[plan.steps.len() - 2],
        OpStep::VerifyKitSlots { .. }
    ));
    assert!(matches!(
        plan.steps[plan.steps.len() - 1],
        OpStep::VerifyPatternKitLink { .. }
    ));
}

#[test]
fn clipboard_serde_round_trips() {
    let clip = test_clipboard();
    let json = serde_json::to_string(&clip).unwrap();
    let back: PatternClipboard = serde_json::from_str(&json).unwrap();
    assert_eq!(back, clip);
}

/// Mock-executes a full paste plan: mismatching read-back must fail the
/// verification step, matching read-back must succeed.
#[test]
fn paste_execution_verifies_via_read_back() {
    use rytmdash::ops::{DeviceBackend, OpError};
    use std::path::PathBuf;

    struct Mock {
        kit_slots: Vec<Option<u8>>,
        kit_link: usize,
    }
    impl DeviceBackend for Mock {
        fn backup_project(&mut self) -> Result<PathBuf, OpError> {
            Ok(PathBuf::from("/tmp/mock.syx"))
        }
        fn write_kit_sample_slot(&mut self, _: usize, _: usize, _: u8) -> Result<(), OpError> {
            unreachable!("paste plans never use the single-slot write")
        }
        fn read_kit_sample_slot(&mut self, _: usize, _: usize) -> Result<Option<u8>, OpError> {
            unreachable!()
        }
        fn send_object(&mut self, _: &[u8]) -> Result<(), OpError> {
            Ok(())
        }
        fn read_kit_sample_slots(&mut self, _: usize) -> Result<Vec<Option<u8>>, OpError> {
            Ok(self.kit_slots.clone())
        }
        fn read_pattern_kit_link(&mut self, _: usize) -> Result<usize, OpError> {
            Ok(self.kit_link)
        }
    }

    let clip = test_clipboard();
    let preview = plan_paste(&clip, &target_model());
    let paste = materialize(&clip, &preview).unwrap();
    let plan = Plan::paste_pattern(&paste);

    let mut good = Mock {
        kit_slots: paste.expected_kit_slots.clone(),
        kit_link: 5,
    };
    assert!(execute(&plan, &mut good, |_, _| {}).succeeded());

    let mut bad = Mock {
        kit_slots: vec![None; 12],
        kit_link: 5,
    };
    let report = execute(&plan, &mut bad, |_, _| {});
    assert!(!report.succeeded());
    let (step, _) = report.failed.unwrap();
    assert!(step.contains("verify"), "failed step was: {step}");
}
