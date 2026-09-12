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
| Kit field edits (EditKit: rename, track level, SMPL/AMP/FLTR) | **pending** | apply→extract round-trips offline (tests/kit_edit.rs); a drift guard refuses the write unless the fresh read re-encodes byte-identically (protects scenes/perf/unmodeled bytes from FW 1.70↔1.72 encode drift — the first hardware run may surface legitimate don't-care bytes to relax, never skip). Gate: single-field edit → write → read-back on a scratch project, then 15-char rename NUL-padding, then write to the loaded kit's slot (does the working state refresh or need a manual kit reload?) | MockBackend tests only |
| Pattern/kit/pool-sound object writes (pattern paste) | **pending** | encode→decode round-trips offline (tests/transfer.rs); first hardware run should target a scratch project | MockBackend tests only |
| Data write (0x57–0x59), move/copy/clear (0x5a–0x5c) | not implemented | write side gated on WP0 write verification | — |
| +Drive create dir (0x11) | **pending** | opcode + framing pinned from the elektroid spec; MockBackend plan tests only. Probe: `mkdir /probe-a` → list-verify → cleanup | GUI action ships but every run is read-back verified |
| +Drive delete dir (0x12) | **pending** | plan refuses non-empty dirs on a fresh listing regardless of device behavior. Probe: rmdir empty → verify gone; rmdir NON-empty → record the device's refusal behavior | — |
| +Drive delete file (0x20) | **pending** | plan preflights hash+size identity, fresh RAM-slot check, and downloads a local WAV first (backup-first — nothing can re-upload yet). Probe on a scratch file only | refuse without local copy |
| +Drive rename file, same dir (0x21) | **pending** | body = src NUL + dst NUL. Gate: the matrix's "hash stays on rename" claim — verify hash+size unchanged in the re-listing and that 0x23 (hash→path) resolves to the NEW path. Also probe: rename onto a case-variant of an existing name (FS case semantics unresolved — Kitsmith conservatively refuses case-variant collisions until then) | — |
| +Drive move file, cross-dir (0x21) | **pending** | same opcode, dst path in another dir. Probe after same-dir rename passes | — |
| +Drive rename/move of a DIRECTORY (0x21) | **pending** | elektroid pins 0x21 for files; dir support is unknown. GUI hides dir rename/move until this row flips. Probe on scratch dirs only (`/probe-a/inner` → in-place, then cross-dir) | dirs delete-only in the GUI |
| Sample download (0x30–0x32) | **pending** | open reply carries id + total bytes; 64-byte header + BE PCM at payload offset 22; status 2 = factory refusal. Offline parse tested (tests/files.rs). Gate: byte totals + WAV plays | delete stays blocked (its backup needs this) |
| +Drive name length/charset limit | **pending** | Kitsmith caps at 32 printable ASCII (`config::DRIVE_NAME_MAX_LEN`); the device's true limit and charset are unprobed | conservative cap |

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
- +Drive write preflights leave a one-transact TOCTOU window: a file replaced
  on the device between the delete preflight and the 0x20 send deletes the new
  content while the local backup holds the old. Unavoidable without a
  device-side compare-and-delete; every other interleaving fails safe (refused
  preflight or failed verify).
- The delete reference scan checks backed-up projects as of their last backup;
  a project modified on the device afterwards can reference a sample unseen.
  The delete modal states this.
