# Hardware verification matrix (WP0)

Device: Analog Rytm MKII, OS 1.72, USB-MIDI. Probe: `cargo run --example probe`
(read-only). Updated 2026-09-05.

| Feature | Status | Observed behavior | Fallback |
| --- | --- | --- | --- |
| Device recognition (port name + inquiry) | **verified** | Rytm ports found, watcher hotplug works | port-name match only |
| Object sync: kits/patterns/pool sounds | **verified** | full active-project sync renders real kit/sound names in the app | partial sync tolerated, missed objects counted |
| File protocol ping (0x01) | **verified** | answers | file-protocol features disabled |
| Project list (`DATA_LIST 0x53` on `/projects`) | **verified** | ~50 slots with names, ids, `size=2097152` fixed, `has_valid_data` | — |
| +Drive sample dir listing (0x10) | **verified** | 33 root entries, dirs+files, hash/size/write-protect flags | — |
| `/incoming` listing | **failed** | no reply for lowercase `/incoming`; root shows other names — path likely case-sensitive (`INCOMING`) | list root first, use listed names verbatim |
| RAM slot table (0x17) | **verified** | 127 slots, 6 used, hash+size per slot | — |
| Hash+size → +Drive path (0x23) | **verified** | resolves used slots incl. `/factory/...`, `/recorded/...` | `Slot N · name unavailable` |
| Data download open/partial/close (0x54–0x56) | **verified** | `/projects/1` → 119,563 bytes in 16 chunks, ~68 KB/s; empty first chunk; `last` flag terminates | — |
| Data chunk CRC | **verified** | device CRC = CRC-32 (IEEE) with running init `0xFFFFFFFF` over the chunk payload; chunk 1 reported `c294465e` = computed | downloads now reject a chunk whose CRC does not match (a silently corrupt backup is worse than a failed one); fall back to trusting `last`+length if this ever misfires |
| Data payload compression | **verified** | `compressed=true`; payload magic `ac 11 d3 03 02 00 02 00 05 30 31 36 32 ...` — Elektron container, not zlib/gzip; listed size is the fixed 2 MB slot, wire size is compressed | treat payload as opaque (backup/fingerprint) |
| Object reply correlation (obj_type @6, obj_nr @9) | unverified live | offsets read off rytm-rs/libanalogrytm encodings: query cmd is the response type + 0x10 (kit `0x62`→`0x52`, sound `0x63`→`0x53`, pattern `0x64`→`0x54`, settings `0x66`→`0x56`, global `0x67`→`0x57`) | if a sync reports every object missed, drop the `is_reply_to` check in `await_reply`/`accept_reply` and re-probe the real response header |
| Sample-off wire values (SMP OFF = 0x00/0xFF) | unverified | — | display-only assumption, blocks nothing |
| SDS sample upload | unverified live | code path exists (M4); needs a listen test on device | open-loop mode |
| Kit write + read-back (AssignSampleSlot) | **pending** | needs a scratch project loaded on the device first | MockBackend tests only |
| Pattern/kit/pool-sound object writes (pattern paste) | **pending** | encode→decode round-trips offline (tests/transfer.rs); first hardware run should target a scratch project | MockBackend tests only |
| Data write (0x57–0x59), copy (0x5b), RAM slot load (0x19) | **implemented, unverified live** | wire formats derived from elektroid/Transfer, never run against hardware. Exposed as *Rename on device*, *Copy to empty slot* and *Load into project slot*. Every path pulls a fresh backup first and reads back to verify; the rename rolls the original payload back when the write fails or does not verify | none — treat the first hardware run as a test on a scratch slot |
| Data move/clear (0x5a, 0x5c) | not implemented | no caller yet | — |

## Raw project format (verified live, 2026-09-05)

- `DATA_READ_OPEN` with compression flag **0** is honored: the device sends
  the raw ~2,095,830-byte project payload (~8.7 s vs ~1.7 s compressed).
- Layout (validated against a real project + libanalogrytm FW1.70 sizes,
  parsed by `device/projfile.rs`): header with name at 0x27 · work pattern
  at 0x37 · 128 patterns at 0x3364 (0x332D each, kit link at +0x3325) ·
  work kit at 0x19C9E4 (0xA36) · 128 kits at 0x19D41A (0xA32 each, name at
  +0x04, 12 sounds from +0x2E stride 162, sound name at +0x0C).
- **Sample-slot table** found at 0x1F94C9: 16-byte entries, slot i at
  base + i·16 (i = 1..127), fields hash u32be + size u32be + 8 reserved;
  a header entry carrying the 0x7F count precedes slot 1. Hashes are the
  same content identity as the RAM-slot table → cross-project matching.
- **Raw kit records re-encode to writable sysex** via libanalogrytm
  (`ar_kit_raw_to_syx`, meta obj_type = AR_TYPE_KIT, container 0x0101) —
  round-trip proven in tests and against a real backup. Note: names decoded
  from raw records are NUL-padded.
- Settings/songs/pool-sounds regions not yet mapped.
- The compression used for compressed transfers remains unknown (not zlib/
  lzma; elektroid stores it opaquely) — moot now that raw works.

## Sysex transport finding

- CoreMIDI fragments large sysex across packets; the original link callback
  dropped continuations → consistent "reply received but not understood"
  partial syncs on plock-heavy kits/patterns. Fixed by reassembly in
  `midi/link.rs` (`SysexAssembler`, tested).

## Object-API findings (rytm-rs 0.1.3, source-verified)

- **Sound Locks readable**: `Trig::sound_lock()` (255 = none, 0–127 = Sound
  Pool index) and per-trig sample plocks via `plock_get_sample_number()` —
  the full sample dependency graph needs both (implemented in the model scan).
- **Scenes/Performance NOT exposed**: `Kit.scene_ctl`/`perf_ctl`/`current_scene_id`
  are private raw bytes ("TODO" upstream). WP15 requires our own decoding
  against libanalogrytm's `ar_kit_t` layout.
- **Active project slot is NOT readable**: neither Settings nor Global carries
  the active project slot or name; the file protocol has no equivalent either.
  The UI therefore cannot name the active project — it says "Active project"
  and does not guess.
- Single-object queries exist (`KitQuery`, `PatternQuery`, `SoundQuery`,
  `SettingsQuery`, `GlobalQuery`) with `RytmProject::update_from_sysex_response`
  — the building blocks for incremental sync later.

## Notes

- Project download speed ⇒ full-device project indexing (~50 slots) ≈ 1–2 min;
  fine as a background Activity task with per-project progress.
- Project slots report a fixed 2 MB `size`; the real payload is the compressed
  wire size. Don't show the listed size as "project size" in UI.
- Two project names came back mojibake on the wire (`LDV~OQNIDBR`) — present on
  the device itself; render as-is, never "fix" silently.
