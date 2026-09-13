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
| Data write (0x57–0x59) | **pending** | built (chunked upload with per-chunk CRC + echoed byte count); used by native project rename | — |
| Project move (`DATA_MOVE 0x5A`) | **pending** | framing spec-derived (opcode + two slot paths, mirrors verified 0x5B copy); flow: list preflight → download+backup src → move → re-list + download dst → byte-compare against the pre-move payload. Post-op listings are diffed against pre-op: any change to a slot the op never named fails the op (catches misrouted-path framing errors). On a damaged arrival the original is auto-restored to the source. Probe on scratch slots: framing accepted, refusal-byte position, src emptied, dst byte-identical, no third slot touched | modal warns first use should target scratch slots |
| Project swap (`DATA_SWAP 0x5D`) | **pending** | both slots downloaded+backed up first; verify = crosswise byte-compare. Probe: both payloads byte-identical crosswise | same warning |
| Project clear (`DATA_CLEAR 0x5C`) | **pending** | mandatory local backup, abort-before-send if the backup save fails; verify = slot lists as empty + untouched-slots diff. Probe: slot reads empty; a subsequent copy into the cleared slot succeeds (proves "empty" is real) | typed slot-number confirmation; **Restore from backup** re-uploads the payload (0x57–0x59) with a byte-compare verify |
| Project restore (upload 0x57–0x59 to an empty slot) | **pending** | the recovery path behind Clear: manifest checksum re-check before upload, empty-slot preflight, byte-compare read-back, untouched-slots diff. Uses the same upload path as native rename. Probe: cleared project restored byte-identical and loadable on-device | — |
| Download chunk-CRC enforcement | code-verified | `download_data` now rejects any chunk whose device-reported CRC disagrees with the computed CRC-32 (empty chunks skipped — their fields are uninitialized, HW-verified). A damaged download can no longer be saved as a "backup" | — |
| Danger probe: slot op on the LOADED project | **pending** | the active slot is unreadable, so the modals rely on user attestation + the active-project guess. Probe (sacrificial project only): move/clear the loaded project's slot, then PROJECT → SAVE on the device — document what actually happens; that result writes the final modal copy | attestation checkbox in every slot-op modal |
| +Drive create dir (0x11) | **pending** | opcode + framing pinned from the elektroid spec; MockBackend plan tests only. Probe: `mkdir /probe-a` → list-verify → cleanup | GUI action ships but every run is read-back verified |
| +Drive delete dir (0x12) | **pending** | plan refuses non-empty dirs on a fresh listing regardless of device behavior. Probe: rmdir empty → verify gone; rmdir NON-empty → record the device's refusal behavior | — |
| +Drive delete file (0x20) | **pending** | plan preflights hash+size identity, fresh RAM-slot check, and downloads a local WAV first (backup-first — nothing can re-upload yet). Probe on a scratch file only | refuse without local copy |
| +Drive rename file, same dir (0x21) | **pending** | body = src NUL + dst NUL. Gate: the matrix's "hash stays on rename" claim — verify hash+size unchanged in the re-listing and that 0x23 (hash→path) resolves to the NEW path. Also probe: rename onto a case-variant of an existing name (FS case semantics unresolved — Kitsmith conservatively refuses case-variant collisions until then) | — |
| +Drive move file, cross-dir (0x21) | **pending** | same opcode, dst path in another dir. Probe after same-dir rename passes | — |
| +Drive rename/move of a DIRECTORY (0x21) | **pending** | elektroid pins 0x21 for files; dir support is unknown. GUI hides dir rename/move until this row flips. Probe on scratch dirs only (`/probe-a/inner` → in-place, then cross-dir) | dirs delete-only in the GUI |
| Sample download (0x30–0x32) | **pending** | open reply carries id + total bytes; 64-byte header + BE PCM at payload offset 22; status 2 = factory refusal. Offline parse tested (tests/files.rs). Gate: byte totals + WAV plays | delete stays blocked (its backup needs this) |
| +Drive name length/charset limit | **pending** | Kitsmith caps at 32 printable ASCII (`config::DRIVE_NAME_MAX_LEN`); the device's true limit and charset are unprobed | conservative cap |
| USB audio: Rytm as CoreAudio input (USB AUDIO/MIDI mode) | **pending** | monitor built (`audio::monitor`); record the exact CoreAudio device name for the matcher corpus | monitor button reports NoRytmAudioInput |
| USB audio format is 48 kHz / 2-ch as assumed | **pending** | `pick_config` pins 48 kHz; mono/wide inputs are folded to stereo defensively | UnsupportedFormat error shown |
| Overbridge mode hides class-compliant audio, keeps MIDI | **pending** | validates the diagnostics hint heuristic (MIDI visible + audio absent → USB CONFIG help) | hint may show in a wrong mode; copy stays generic |
| Object sync + file transfer glitch-free while monitoring | **pending** | audio rides a separate USB interface from the serial sysex conversation (PRD §46) | stop monitoring during sync |
| Unplug mid-monitor lands in Failed(Disconnected), no hang | **pending** | cpal error callback → monitor thread drops both streams | — |
| Monitor latency acceptable at 1024-frame prefill | **pending** | ~21 ms prefill + device buffers; `config::MONITOR_PREFILL_FRAMES` is the knob | raise/lower prefill |
| macOS mic permission (TCC) covers USB audio input | **pending** | under `cargo run` the terminal gets the mic prompt; a future bundled app needs `NSMicrophoneUsageDescription` — a denied prompt looks like silent input, not an error | document in README |

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
