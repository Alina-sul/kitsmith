//! Parses a raw project payload from disk and prints a summary.
//! Usage: `cargo run --example parse_raw -- /path/to/project.raw`

use rytmdash::device::projfile::parse_raw_project;

fn main() {
    let path = std::env::args().nth(1).expect("usage: parse_raw <file>");
    let bytes = std::fs::read(&path).expect("read file");
    let Some(snapshot) = parse_raw_project(&bytes) else {
        eprintln!("not a raw project payload ({} bytes)", bytes.len());
        std::process::exit(1);
    };
    println!("project name: '{}'", snapshot.name);
    let linked = snapshot.linked_kits();
    println!("patterns with saved kit link: {}", linked.len());
    for kit in snapshot.kits.iter().filter(|k| !k.name.is_empty()).take(12) {
        println!(
            "kit {:02} '{}' sounds: {}",
            kit.index + 1,
            kit.name,
            kit.sound_names
                .iter()
                .filter(|n| !n.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let linked_patterns: Vec<String> = snapshot
        .patterns
        .iter()
        .filter_map(|p| {
            p.kit_link.map(|k| {
                format!(
                    "{}→kit{:02}",
                    rytmdash::model::pattern_display_name(p.index),
                    k + 1
                )
            })
        })
        .collect();
    println!("pattern links: {}", linked_patterns.join(" "));
    println!(
        "sample slots loaded: {} — {:?}",
        snapshot.samples.len(),
        snapshot
            .samples
            .iter()
            .take(6)
            .map(|s| format!("slot{} {}KB {:08x}", s.slot, s.size / 1024, s.hash))
            .collect::<Vec<_>>()
    );

    // End-to-end check of copy-from-backup: convert the first in-use kit.
    if let Some(kit) = snapshot.kits.iter().find(|k| snapshot.kit_in_use(k.index)) {
        match rytmdash::transfer::capture_kit_from_backup(&bytes, &snapshot, "backup", kit.index) {
            Ok(clip) => println!(
                "capture_kit_from_backup OK: '{}' · {} sample dep(s) · {} bytes sysex",
                clip.source_hint,
                clip.samples.len(),
                clip.kit_sysex.len()
            ),
            Err(e) => println!("capture_kit_from_backup FAILED: {e}"),
        }
    }
}
