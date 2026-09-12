//! Cross-project sample reference scan over synthetic raw payloads (same
//! documented offsets as tests/projfile.rs — never device dumps).

use std::collections::BTreeMap;

use kitsmith::analysis::refscan::scan;
use kitsmith::cache::{ProjectBackup, ProjectManifest};
use kitsmith::model::{ProjectInfo, ProjectModel, SampleSlotInfo};

const SAMPLES_OFFSET: usize = 0x1F94C9;

/// A minimal raw project payload with the given (slot, hash, size) samples.
fn raw_project(samples: &[(u8, u32, u32)]) -> Vec<u8> {
    let mut bytes = vec![0u8; SAMPLES_OFFSET + 128 * 16 + 64];
    bytes[..4].copy_from_slice(&[0xAC, 0x11, 0xD3, 0x03]);
    for &(slot, hash, size) in samples {
        let entry = SAMPLES_OFFSET + usize::from(slot) * 16;
        bytes[entry..entry + 4].copy_from_slice(&hash.to_be_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&size.to_be_bytes());
    }
    bytes
}

fn backup(name: &str, raw: bool) -> ProjectBackup {
    ProjectBackup {
        name: name.to_owned(),
        bytes: 0,
        crc: 0,
        taken_at: 0,
        raw,
    }
}

fn model_with(projects: &[u32], samples: &[(u8, u32, u32)]) -> ProjectModel {
    ProjectModel {
        projects: projects
            .iter()
            .map(|&id| ProjectInfo {
                id,
                name: format!("P{id}"),
            })
            .collect(),
        samples: samples
            .iter()
            .map(|&(slot, hash, size)| SampleSlotInfo {
                slot,
                name: format!("S{slot}"),
                path: format!("/S{slot}"),
                size,
                hash,
            })
            .collect(),
        ..ProjectModel::default()
    }
}

#[test]
fn finds_hits_in_active_model_and_backups() {
    let payloads = BTreeMap::from([
        (3u32, raw_project(&[(5, 0xAA, 100), (9, 0xAA, 100)])),
        (7u32, raw_project(&[(1, 0xBB, 200)])),
    ]);
    let mut manifest = ProjectManifest::new();
    manifest.insert(3, backup("LIVE", true));
    manifest.insert(7, backup("STUDIO", true));

    let model = model_with(&[3, 7, 9], &[(2, 0xAA, 100)]);
    let found = scan(
        &model,
        &manifest,
        |slot| payloads.get(&slot).cloned(),
        0xAA,
        100,
    );
    assert_eq!(found.active_slots, vec![2]);
    assert_eq!(found.backed_up.len(), 1);
    assert_eq!(found.backed_up[0].slot_id, 3);
    assert_eq!(found.backed_up[0].name, "LIVE");
    assert_eq!(found.backed_up[0].sample_slots, vec![5, 9]);
    assert!(found.has_references());
    assert_eq!(found.reference_count(), 3);
    // Project 9 has no backup — reported as the blind spot.
    assert_eq!(found.unscanned.len(), 1);
    assert_eq!(found.unscanned[0].id, 9);
}

#[test]
fn hash_match_with_size_mismatch_is_not_a_hit() {
    let payloads = BTreeMap::from([(1u32, raw_project(&[(4, 0xAA, 999)]))]);
    let mut manifest = ProjectManifest::new();
    manifest.insert(1, backup("P", true));
    let model = model_with(&[1], &[(2, 0xAA, 999)]);
    let found = scan(
        &model,
        &manifest,
        |slot| payloads.get(&slot).cloned(),
        0xAA,
        100,
    );
    assert!(!found.has_references());
}

#[test]
fn compressed_backups_count_as_unscanned() {
    // A pre-raw-era backup exists but cannot be parsed — the project must
    // land in the blind spot, never in "no references".
    let mut manifest = ProjectManifest::new();
    manifest.insert(4, backup("OLD", false));
    let model = model_with(&[4], &[]);
    let found = scan(&model, &manifest, |_| None, 0xAA, 100);
    assert!(!found.has_references());
    assert_eq!(found.unscanned.len(), 1);
    assert_eq!(found.unscanned[0].id, 4);
}

#[test]
fn empty_world_reports_nothing() {
    let found = scan(
        &ProjectModel::default(),
        &ProjectManifest::new(),
        |_| None,
        0xAA,
        100,
    );
    assert!(!found.has_references());
    assert!(found.unscanned.is_empty());
    assert_eq!(found.reference_count(), 0);
}
