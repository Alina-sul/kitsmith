//! Device-recognition rule tests.

use rytmdash::device::detect::{is_rytm_port, DeviceInfo};
use rytmdash::midi::inquiry::parse_identity_reply;

#[test]
fn recognizes_rytm_port_names() {
    assert!(is_rytm_port("Analog Rytm MKII"));
    assert!(is_rytm_port("Elektron Analog Rytm MKII"));
    assert!(is_rytm_port("Analog Rytm")); // MKI
    assert!(is_rytm_port("ANALOG RYTM MKII MIDI 1")); // Linux ALSA style
}

#[test]
fn ignores_other_ports() {
    assert!(!is_rytm_port("IAC Driver Bus 1"));
    assert!(!is_rytm_port("Digitakt"));
    assert!(!is_rytm_port("Analog Four MKII"));
    assert!(!is_rytm_port(""));
}

#[test]
fn describe_covers_identified_and_name_only_devices() {
    let identified = DeviceInfo {
        port_name: "Analog Rytm MKII".into(),
        identity: parse_identity_reply(&[
            0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x20, 0x3C, 0x10, 0x00, 0x01, 0x00, b'1', b'.',
            b'7', b'2', 0xF7,
        ]),
    };
    let text = identified.describe();
    assert!(text.contains("Elektron"));
    assert!(text.contains("1.72"));

    let name_only = DeviceInfo {
        port_name: "Analog Rytm MKII".into(),
        identity: None,
    };
    assert!(name_only.describe().contains("port name"));
}
