//! Display-name resolution rules (PRD v2 §33): native names win, fallbacks
//! are deterministic, unresolved names are never fabricated.

use kitsmith::model::names::{
    kit_display_name, project_display_name, sample_display_name, sound_display_name, track_label,
    ResolvedName,
};

#[test]
fn native_names_pass_through_trimmed() {
    let name = sound_display_name("  HARD KICK  ", 0);
    assert_eq!(name, ResolvedName::Native("HARD KICK".to_owned()));
    assert!(name.is_native());
}

#[test]
fn blank_sound_name_falls_back_to_track_semantic() {
    assert_eq!(
        sound_display_name("   ", 0),
        ResolvedName::Fallback("BD sound".to_owned())
    );
    assert_eq!(
        sound_display_name("", 11),
        ResolvedName::Fallback("CB sound".to_owned())
    );
}

#[test]
fn kit_and_project_fallbacks_are_one_based_and_zero_padded() {
    assert_eq!(
        kit_display_name("", 13),
        ResolvedName::Fallback("Kit 014".to_owned())
    );
    assert_eq!(
        project_display_name("", 42),
        ResolvedName::Fallback("Project 042".to_owned())
    );
}

#[test]
fn unresolved_sample_name_is_unavailable_not_invented() {
    assert_eq!(sample_display_name(None), ResolvedName::Unavailable);
    assert_eq!(sample_display_name(Some("  ")), ResolvedName::Unavailable);
    assert_eq!(sample_display_name(None).text(), "Name unavailable");
    assert!(!sample_display_name(None).is_native());
}

#[test]
fn resolved_sample_name_is_native() {
    assert_eq!(
        sample_display_name(Some("Crash.wav")),
        ResolvedName::Native("Crash.wav".to_owned())
    );
}

#[test]
fn track_label_clamps_out_of_range() {
    assert_eq!(track_label(0), "BD");
    assert_eq!(track_label(11), "CB");
    assert_eq!(track_label(99), "CB");
}
