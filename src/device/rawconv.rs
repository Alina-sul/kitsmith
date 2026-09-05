//! Converts raw object records (as stored inside project payloads) into
//! object sysex via libanalogrytm — the bridge that lets Rytmdash copy a kit
//! straight out of a project backup without the device loading that project.

use rytm_sys::{
    ar_kit_raw_to_syx, ar_sysex_id_t_AR_TYPE_KIT, ar_sysex_meta_t, s_u16_t, s_u16_t__bindgen_ty_1,
};

/// Raw kit record size, FW1.70 (`AR_KIT_V5_SZ`, == the project-file record).
pub const KIT_RAW_LEN: usize = 0xA32;

/// Generous output buffer bound (encoded kit sysex is 2998 bytes).
const SYSEX_BUF_LEN: usize = 8192;

/// Encodes one raw kit record into a kit sysex message targeting `kit_index`.
pub fn kit_raw_to_sysex(raw: &[u8], kit_index: usize) -> Result<Vec<u8>, String> {
    if raw.len() != KIT_RAW_LEN {
        return Err(format!(
            "raw kit record must be {KIT_RAW_LEN} bytes, got {}",
            raw.len()
        ));
    }
    if kit_index > 127 {
        return Err(format!("kit index {kit_index} out of range"));
    }
    let zero = s_u16_t {
        b: s_u16_t__bindgen_ty_1 { hi: 0, lo: 0 },
    };
    let meta = ar_sysex_meta_t {
        // Container version 0x0101, as every Rytm object message carries.
        container_version: s_u16_t {
            b: s_u16_t__bindgen_ty_1 { hi: 0x01, lo: 0x01 },
        },
        dev_id: 0,
        obj_type: ar_sysex_id_t_AR_TYPE_KIT as u8,
        obj_nr: kit_index as u16,
        chksum: zero,
        data_size: zero,
    };
    let mut out = vec![0u8; SYSEX_BUF_LEN];
    let mut out_len: u32 = 0;
    let err = unsafe {
        ar_kit_raw_to_syx(
            out.as_mut_ptr(),
            raw.as_ptr(),
            raw.len() as u32,
            &mut out_len,
            &meta,
        )
    };
    if err != 0 {
        return Err(format!("libanalogrytm raw→syx failed (error {err})"));
    }
    let out_len = out_len as usize;
    if out_len == 0 || out_len > SYSEX_BUF_LEN {
        return Err(format!("implausible encoded size {out_len}"));
    }
    out.truncate(out_len);
    Ok(out)
}
