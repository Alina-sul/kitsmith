//! Downloads one project slot uncompressed and saves the raw payload for
//! format analysis. Read-only. Usage: `cargo run --example fetch_raw -- 1`

use kitsmith::device::detect::is_rytm_port;
use kitsmith::device::files::{FileClient, PROJECTS_PATH};
use kitsmith::midi::link::MidiLink;
use kitsmith::midi::ports;

fn main() {
    let slot: u32 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1);
    let snapshot = ports::snapshot();
    let in_port = snapshot
        .inputs
        .iter()
        .find(|n| is_rytm_port(n))
        .expect("no Rytm input");
    let out_port = snapshot
        .outputs
        .iter()
        .find(|n| is_rytm_port(n))
        .expect("no Rytm output");
    let mut link = MidiLink::open(in_port, out_port).expect("link");
    let mut client = FileClient::new();
    assert!(client.ping(&mut link), "no file-protocol ping");
    let path = format!("{PROJECTS_PATH}/{slot}");
    let download = client
        .download_data(&mut link, &path, false, |_, total| {
            if total.is_multiple_of(262_144) {
                println!("{total} bytes…");
            }
        })
        .expect("download failed");
    let out = format!("/tmp/rytm-project-{slot}.raw");
    std::fs::write(&out, &download.bytes).expect("write");
    println!("saved {} bytes to {out}", download.bytes.len());
}
