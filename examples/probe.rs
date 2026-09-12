//! WP0 hardware probe — **read-only**. Exercises every file-protocol read
//! path against a live Analog Rytm and prints observed behavior for
//! `docs/hardware-verification.md`. Sends no write of any kind.
//!
//! Run with the device connected: `cargo run --example probe`

use std::time::Instant;

use rytmdash::device::detect::is_rytm_port;
use rytmdash::device::files::{crc32, DataChunk, FileClient, PROJECTS_PATH};
use rytmdash::midi::link::MidiLink;
use rytmdash::midi::ports;

fn main() {
    let snapshot = ports::snapshot();
    let Some(in_port) = snapshot.inputs.iter().find(|n| is_rytm_port(n)) else {
        eprintln!("no Rytm MIDI input port found");
        std::process::exit(1);
    };
    let Some(out_port) = snapshot.outputs.iter().find(|n| is_rytm_port(n)) else {
        eprintln!("no Rytm MIDI output port found");
        std::process::exit(1);
    };
    println!("ports: in='{in_port}' out='{out_port}'");
    let mut link = match MidiLink::open(in_port, out_port) {
        Ok(link) => link,
        Err(e) => {
            eprintln!("link open failed: {e}");
            std::process::exit(1);
        }
    };

    let mut client = FileClient::new();
    println!("ping: {}", client.ping(&mut link));

    let projects = client
        .list_data_dir(&mut link, PROJECTS_PATH)
        .unwrap_or_default();
    println!("projects listed: {}", projects.len());
    for p in &projects {
        println!(
            "  id={:?} size={} valid={} dir={} name='{}'",
            p.id, p.size, p.has_valid_data, p.is_dir, p.name
        );
    }

    for path in ["/", "/incoming"] {
        match client.list_sample_dir(&mut link, path) {
            Some(entries) => {
                println!("sample dir '{path}': {} entries", entries.len());
                for e in entries.iter().take(8) {
                    println!(
                        "  {} '{}' size={} hash={:08x} wp={}",
                        if e.is_dir { "dir " } else { "file" },
                        e.name,
                        e.size,
                        e.hash,
                        e.write_protected
                    );
                }
            }
            None => println!("sample dir '{path}': no reply"),
        }
    }

    let ram = client.ram_slots(&mut link).unwrap_or_default();
    let used: Vec<_> = ram.iter().filter(|s| s.used).collect();
    println!("ram slots: {} total, {} used", ram.len(), used.len());
    for slot in used.iter().take(4) {
        let path = client.sample_path_from_hash_size(&mut link, slot.hash, slot.size);
        println!("  slot {} -> {:?}", slot.id, path);
    }

    // Download the smallest valid project file — read-only.
    let Some(target) = projects
        .iter()
        .filter(|p| !p.is_dir && p.has_valid_data && p.size > 0 && p.id.is_some())
        .min_by_key(|p| p.size)
    else {
        println!("no downloadable project found; skipping data-read probe");
        return;
    };
    let path = format!("{}/{}", PROJECTS_PATH, target.id.unwrap_or_default());
    println!(
        "downloading '{path}' (name='{}', listed size={})…",
        target.name, target.size
    );
    let started = Instant::now();
    let mut chunks: Vec<DataChunk> = Vec::new();
    let result = client.download_data(&mut link, &path, true, |chunk, total| {
        if chunks.len().is_multiple_of(8) {
            println!("  chunk {} · {total} bytes so far", chunks.len());
        }
        chunks.push(chunk.clone());
    });
    match result {
        Some(download) => {
            let secs = started.elapsed().as_secs_f64();
            println!(
                "downloaded {} bytes in {} chunks, {:.1}s ({:.1} KB/s), compressed={}",
                download.bytes.len(),
                chunks.len(),
                secs,
                download.bytes.len() as f64 / 1024.0 / secs,
                download.compressed
            );
            let head: Vec<String> = download
                .bytes
                .iter()
                .take(16)
                .map(|b| format!("{b:02x}"))
                .collect();
            println!("first 16 bytes: {}", head.join(" "));
            for (i, chunk) in chunks.iter().take(2).enumerate() {
                println!(
                    "chunk {i}: reported crc={:08x} · crc32(0)={:08x} · crc32(ff..)={:08x}",
                    chunk.crc,
                    crc32(0, &chunk.data),
                    crc32(0xFFFF_FFFF, &chunk.data)
                );
            }
            println!(
                "listed size {} vs downloaded {} (mismatch = compression on the wire)",
                target.size,
                download.bytes.len()
            );
        }
        None => println!("download failed / no reply"),
    }

    // Does the device honor compression=0 and send the raw payload?
    println!("downloading '{path}' UNCOMPRESSED…");
    let started = Instant::now();
    let raw = client.download_data(&mut link, &path, false, |_, _| {});
    match raw {
        Some(download) => {
            println!(
                "raw download: {} bytes in {:.1}s, compressed flag echoed = {}",
                download.bytes.len(),
                started.elapsed().as_secs_f64(),
                download.compressed
            );
            let head: Vec<String> = download
                .bytes
                .iter()
                .take(32)
                .map(|b| format!("{b:02x}"))
                .collect();
            println!("first 32 bytes: {}", head.join(" "));
            // Scan for printable ASCII runs (kit/project names live somewhere).
            let mut runs: Vec<(usize, String)> = Vec::new();
            let mut start = None;
            for (i, &b) in download.bytes.iter().enumerate() {
                if (0x20..0x7F).contains(&b) {
                    if start.is_none() {
                        start = Some(i);
                    }
                } else if let Some(s) = start.take() {
                    if i - s >= 5 {
                        let text: String =
                            download.bytes[s..i].iter().map(|&b| b as char).collect();
                        runs.push((s, text));
                    }
                }
            }
            println!("ASCII runs (≥5 chars), first 25 of {}:", runs.len());
            for (offset, text) in runs.iter().take(25) {
                println!("  0x{offset:06x}: '{text}'");
            }
        }
        None => println!("uncompressed download failed / no reply"),
    }
}
