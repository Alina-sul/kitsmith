//! Raw-kit → sysex conversion: what `capture_kit_from_backup` relies on.
//! Round-trips through libanalogrytm's own decoder to prove byte fidelity.

use kitsmith::device::rawconv::{kit_raw_to_sysex, KIT_RAW_LEN};
use rytm_rs::prelude::*;
use rytm_rs::RytmProject;

#[test]
fn kit_survives_syx_to_raw_to_syx() {
    let mut rytm = RytmProject::try_default().expect("default project");
    rytm.kits_mut()[5].set_name("RTTEST").unwrap();
    rytm.kits_mut()[5].sounds_mut()[0]
        .sample_mut()
        .set_slice_number(42)
        .unwrap();
    let syx = rytm.kits()[5].as_sysex().expect("encode");

    // Decode the sysex into a raw record with libanalogrytm itself.
    let mut raw = vec![0u8; KIT_RAW_LEN + 64];
    let mut raw_len: u32 = 0;
    let mut meta = rytm_sys::ar_sysex_meta_t::default();
    let err = unsafe {
        rytm_sys::ar_kit_syx_to_raw(
            raw.as_mut_ptr(),
            syx.as_ptr(),
            syx.len() as u32,
            &mut raw_len,
            &mut meta,
        )
    };
    assert_eq!(err, 0, "syx→raw failed");
    assert_eq!(raw_len as usize, KIT_RAW_LEN, "raw kit record size");
    raw.truncate(raw_len as usize);

    // Re-encode with our converter (as done when copying from a backup)…
    let syx2 = kit_raw_to_sysex(&raw, 5).expect("raw→syx");

    // …and prove a fresh project decodes it with everything intact.
    let mut verify = RytmProject::try_default().unwrap();
    verify.update_from_sysex_response(&syx2).expect("decode");
    assert_eq!(verify.kits()[5].name().trim_matches('\0'), "RTTEST");
    assert_eq!(verify.kits()[5].sounds()[0].sample().slice_number(), 42);
}

#[test]
fn retargeting_kit_index_via_meta_works() {
    let mut rytm = RytmProject::try_default().expect("default project");
    rytm.kits_mut()[5].set_name("MOVEME").unwrap();
    let syx = rytm.kits()[5].as_sysex().unwrap();
    let mut raw = vec![0u8; KIT_RAW_LEN + 64];
    let mut raw_len: u32 = 0;
    let mut meta = rytm_sys::ar_sysex_meta_t::default();
    unsafe {
        rytm_sys::ar_kit_syx_to_raw(
            raw.as_mut_ptr(),
            syx.as_ptr(),
            syx.len() as u32,
            &mut raw_len,
            &mut meta,
        );
    }
    raw.truncate(raw_len as usize);

    // Encode the same record targeting a different kit slot.
    let syx11 = kit_raw_to_sysex(&raw, 11).expect("raw→syx");
    let mut verify = RytmProject::try_default().unwrap();
    verify.update_from_sysex_response(&syx11).expect("decode");
    assert_eq!(
        verify.kits()[11].name().trim_matches('\0'),
        "MOVEME",
        "landed in slot 12"
    );
}

#[test]
fn wrong_record_size_is_rejected() {
    assert!(kit_raw_to_sysex(&[0u8; 100], 0).is_err());
}
