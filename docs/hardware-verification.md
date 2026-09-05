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
| Data chunk CRC | **verified** | device CRC = CRC-32 (IEEE) with running init `0xFFFFFFFF` over the chunk payload; chunk 1 reported `c294465e` = computed | ignore CRC, trust `last`+length |
| Data payload compression | **verified** | `compressed=true`; payload magic `ac 11 d3 03 02 00 02 00 05 30 31 36 32 ...` — Elektron container, not zlib/gzip; listed size is the fixed 2 MB slot, wire size is compressed | treat payload as opaque (backup/fingerprint) |
| Sample-off wire values (SMP OFF = 0x00/0xFF) | unverified | — | display-only assumption, blocks nothing |
| SDS sample upload | unverified live | code path exists (M4); needs a listen test on device | open-loop mode |
| Kit write + read-back (AssignSampleSlot) | **pending** | needs a scratch project loaded on the device first | MockBackend tests only |
| Data write (0x57–0x59), move/copy/clear (0x5a–0x5c) | not implemented | write side gated on WP0 write verification | — |

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
