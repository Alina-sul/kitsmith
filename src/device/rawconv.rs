//! Converts raw object records (as stored inside project payloads) into
//! object sysex via libanalogrytm — the bridge that lets Kitsmith copy a kit
//! straight out of a project backup without the device loading that project.

use rytm_sys::{
    ar_kit_raw_to_syx, ar_pattern_raw_to_syx, ar_sysex_id_t_AR_TYPE_KIT,
    ar_sysex_id_t_AR_TYPE_PATTERN, ar_sysex_meta_t, s_u16_t, s_u16_t__bindgen_ty_1,
};

/// Raw kit record size, FW1.70 (`AR_KIT_V5_SZ`, == the project-file record).
pub const KIT_RAW_LEN: usize = 0xA32;

/// Raw pattern record size, FW1.70 (`AR_PATTERN_V5_SZ`).
pub const PATTERN_RAW_LEN: usize = 0x332D;

/// Generous output buffer bound (encoded pattern sysex is ~15KB).
const SYSEX_BUF_LEN: usize = 20480;

fn make_meta(obj_type: u32, obj_nr: usize) -> ar_sysex_meta_t {
    let zero = s_u16_t {
        b: s_u16_t__bindgen_ty_1 { hi: 0, lo: 0 },
    };
    ar_sysex_meta_t {
        // Container version 0x0101, as every Rytm object message carries.
        container_version: s_u16_t {
            b: s_u16_t__bindgen_ty_1 { hi: 0x01, lo: 0x01 },
        },
        dev_id: 0,
        obj_type: obj_type as u8,
        obj_nr: obj_nr as u16,
        chksum: zero,
        data_size: zero,
    }
}

type RawToSyx =
    unsafe extern "C" fn(*mut u8, *const u8, u32, *mut u32, *const ar_sysex_meta_t) -> u32;

fn raw_to_sysex(
    convert: RawToSyx,
    raw: &[u8],
    expected_len: usize,
    obj_type: u32,
    index: usize,
    what: &str,
) -> Result<Vec<u8>, String> {
    if raw.len() != expected_len {
        return Err(format!(
            "raw {what} record must be {expected_len} bytes, got {}",
            raw.len()
        ));
    }
    if index > 127 {
        return Err(format!("{what} index {index} out of range"));
    }
    let meta = make_meta(obj_type, index);
    let mut out = vec![0u8; SYSEX_BUF_LEN];
    let mut out_len: u32 = 0;
    let err = unsafe {
        convert(
            out.as_mut_ptr(),
            raw.as_ptr(),
            raw.len() as u32,
            &mut out_len,
            &meta,
        )
    };
    if err != 0 {
        return Err(format!(
            "libanalogrytm raw→syx failed for {what} (error {err})"
        ));
    }
    let out_len = out_len as usize;
    if out_len == 0 || out_len > SYSEX_BUF_LEN {
        return Err(format!("implausible encoded {what} size {out_len}"));
    }
    out.truncate(out_len);
    Ok(out)
}

/// Encodes one raw kit record into a kit sysex message targeting `kit_index`.
pub fn kit_raw_to_sysex(raw: &[u8], kit_index: usize) -> Result<Vec<u8>, String> {
    raw_to_sysex(
        ar_kit_raw_to_syx,
        raw,
        KIT_RAW_LEN,
        ar_sysex_id_t_AR_TYPE_KIT,
        kit_index,
        "kit",
    )
}

/// Encodes one raw pattern record into a pattern sysex message.
pub fn pattern_raw_to_sysex(raw: &[u8], pattern_index: usize) -> Result<Vec<u8>, String> {
    raw_to_sysex(
        ar_pattern_raw_to_syx,
        raw,
        PATTERN_RAW_LEN,
        ar_sysex_id_t_AR_TYPE_PATTERN,
        pattern_index,
        "pattern",
    )
}
