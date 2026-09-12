//! Apply/extract round-trips for the kit-edit vocabulary against an
//! in-memory rytm-rs kit — no device involved. Every editable field must
//! survive apply → extract with `value_matches`, or read-back verification
//! would reject correct writes.

use kitsmith::device::kit_edit::{apply, extract};
use kitsmith::model::edit::{FieldValue, KitEdit, KitField};
use rytm_rs::object::Kit;

/// One representative in-range value per field kind.
fn every_field_with_value(track: usize) -> Vec<KitEdit> {
    use FieldValue::{Float, Int, Text, Toggle};
    use KitField as F;
    vec![
        KitEdit {
            field: F::KitName,
            value: Text("ROUNDTRIP".into()),
        },
        KitEdit {
            field: F::SoundName(track),
            value: Text("MY SOUND".into()),
        },
        KitEdit {
            field: F::TrackLevel(track),
            value: Int(101),
        },
        KitEdit {
            field: F::SampleTune(track),
            value: Int(-24),
        },
        KitEdit {
            field: F::SampleFineTune(track),
            value: Int(-64),
        },
        KitEdit {
            field: F::SampleBitReduction(track),
            value: Int(17),
        },
        KitEdit {
            field: F::SampleStart(track),
            value: Float(30.25),
        },
        KitEdit {
            field: F::SampleEnd(track),
            value: Float(119.5),
        },
        KitEdit {
            field: F::SampleLoop(track),
            value: Toggle(true),
        },
        KitEdit {
            field: F::SampleLevel(track),
            value: Int(88),
        },
        KitEdit {
            field: F::AmpAttack(track),
            value: Int(1),
        },
        KitEdit {
            field: F::AmpHold(track),
            value: Int(2),
        },
        KitEdit {
            field: F::AmpDecay(track),
            value: Int(3),
        },
        KitEdit {
            field: F::AmpOverdrive(track),
            value: Int(4),
        },
        KitEdit {
            field: F::AmpDelaySend(track),
            value: Int(5),
        },
        KitEdit {
            field: F::AmpReverbSend(track),
            value: Int(6),
        },
        KitEdit {
            field: F::AmpPan(track),
            value: Int(-33),
        },
        KitEdit {
            field: F::AmpVolume(track),
            value: Int(7),
        },
        KitEdit {
            field: F::FltAttack(track),
            value: Int(8),
        },
        KitEdit {
            field: F::FltSustain(track),
            value: Int(9),
        },
        KitEdit {
            field: F::FltDecay(track),
            value: Int(10),
        },
        KitEdit {
            field: F::FltRelease(track),
            value: Int(11),
        },
        KitEdit {
            field: F::FltCutoff(track),
            value: Int(12),
        },
        KitEdit {
            field: F::FltResonance(track),
            value: Int(13),
        },
        KitEdit {
            field: F::FltEnvAmount(track),
            value: Int(-15),
        },
    ]
}

#[test]
fn every_field_round_trips_through_apply_and_extract() {
    let mut kit = Kit::try_default(0).expect("default kit");
    for edit in every_field_with_value(4) {
        apply(&edit, &mut kit).unwrap_or_else(|e| panic!("{:?}: {e}", edit.field));
        let read = extract(edit.field, &kit).unwrap_or_else(|e| panic!("{:?}: {e}", edit.field));
        assert!(
            edit.value_matches(&read),
            "{:?}: wrote {}, extracted {read}",
            edit.field,
            edit.value
        );
    }
}

#[test]
fn signed_values_survive_the_negative_range() {
    // AMP pan is the field rytm-rs's getter mishandles (i8 → usize
    // sign-extension); the extract path must recover the signed value.
    let mut kit = Kit::try_default(0).expect("default kit");
    for pan in [-64, -1, 0, 63] {
        let edit = KitEdit {
            field: KitField::AmpPan(2),
            value: FieldValue::Int(pan),
        };
        apply(&edit, &mut kit).expect("apply pan");
        assert_eq!(
            extract(KitField::AmpPan(2), &kit).expect("extract pan"),
            FieldValue::Int(pan)
        );
    }
}

#[test]
fn apply_rejects_out_of_range_and_wrong_types() {
    let mut kit = Kit::try_default(0).expect("default kit");
    let too_high = KitEdit {
        field: KitField::FltCutoff(0),
        value: FieldValue::Int(128),
    };
    assert!(apply(&too_high, &mut kit).is_err());

    let wrong_type = KitEdit {
        field: KitField::FltCutoff(0),
        value: FieldValue::Toggle(true),
    };
    assert!(apply(&wrong_type, &mut kit).is_err());

    let long_name = KitEdit {
        field: KitField::KitName,
        value: FieldValue::Text("0123456789ABCDEF".into()),
    };
    assert!(apply(&long_name, &mut kit).is_err());

    let no_track = KitEdit {
        field: KitField::AmpDecay(12),
        value: FieldValue::Int(1),
    };
    assert!(
        apply(&no_track, &mut kit).is_err(),
        "track 12 must not exist"
    );
}

#[test]
fn edits_only_touch_their_own_track() {
    let mut kit = Kit::try_default(0).expect("default kit");
    let before: Vec<FieldValue> = (0..12)
        .map(|t| extract(KitField::AmpDecay(t), &kit).expect("extract"))
        .collect();
    apply(
        &KitEdit {
            field: KitField::AmpDecay(4),
            value: FieldValue::Int(99),
        },
        &mut kit,
    )
    .expect("apply");
    for (t, old) in before.iter().enumerate() {
        let now = extract(KitField::AmpDecay(t), &kit).expect("extract");
        if t == 4 {
            assert_eq!(now, FieldValue::Int(99));
        } else {
            assert_eq!(&now, old, "track {t} changed");
        }
    }
}
