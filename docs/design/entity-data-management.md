# Design memo — entity data management (names & locations), part 2 of 2

> Approved 2026-09-12 (rytm-architect). Companion to the +Drive file-management
> slice on `feature/drive-file-management`. Governs the follow-up slices:
> `feature/project-slot-ops` (A), quick-rename (B), pool-sound rename (C).

Chosen approach: extend the two existing transaction rails exactly as they are
— file-protocol slot ops ride `FileOp`/`FileOpController` (`src/gui/fileops.rs`),
object renames ride `Plan`/`OpStep` (`src/ops/mod.rs`). Rejected alternative:
unifying both into one Plan system now — it would force `ops/` to own a
`FileClient` and re-plumb the worker thread for zero user-visible gain; revisit
only when a single gesture needs both protocols (kit slot move, deferred below).

## 1. Project slot management — move / swap / clear (FileOp rail)

**Protocol layer — `src/device/files.rs`:**
- Constants next to `OP_DATA_COPY`: `OP_DATA_MOVE: u8 = 0x5A`,
  `OP_DATA_CLEAR: u8 = 0x5C`, `OP_DATA_SWAP: u8 = 0x5D`.
- Methods mirroring `copy_data` framing and the `reply.get(5) != Some(&1)`
  refusal check: `move_data(src, dst)`, `swap_data(a, b)`, `clear_data(path)`.
- Wire framing is elektroid-spec-derived, **unverified**; the refusal-byte
  position for these three opcodes is assumed from `copy_data` and must be
  confirmed on hardware.

**Worker — `src/gui/fileops.rs`:** `FileOp::MoveProject { src, dst }` /
`SwapProjects { a, b }` / `ClearProject { slot }` with outcomes
`Moved`/`Swapped`/`Cleared`.

Flow, mirroring `copy_project` + `rename_project`:
- `move_project`: list preflight (src valid, dst empty). Download src →
  `save_project_backup(src, "(pre-move)")` — backup and verify oracle. Send
  0x5A. Verify: re-list (src empty, dst valid) **and** download dst,
  byte-compare against pre-move bytes; on mismatch report with pointer to the
  local backup (no auto-restore — a failed move's device state is ambiguous).
- `swap_projects`: preflight both valid (one empty → refuse, "use Move").
  Download + backup **both**. Send 0x5D. Verify: download both, compare
  crosswise.
- `clear_project`: preflight valid. Mandatory download →
  `save_project_backup(slot, "(pre-clear)")`; if the backup save fails,
  **abort before sending 0x5C**. Verify: re-list, `has_valid_data == false`.

**GUI — `project_modals.rs` + `sidebar.rs`:** `move_modal`, `swap_modal`,
`clear_modal`; `UiState` fields `move_modal: Option<(u32, u32)>`,
`swap_modal: Option<(u32, u32)>`, `clear_modal_slot: Option<u32>` +
`clear_confirm_draft`. Sidebar context menu adds "Move to empty slot…",
"Swap with…", "Clear slot…" behind `ops_busy`. Rearrange-mode drop on
`empty_slot_row` defaults to Move with Copy as the modal's secondary button.

**Active-project guard — the honest version:** the active slot is *not
readable* (hardware-verification.md), so a hard refuse is impossible. Guard =
(a) `active_guess` warning banner worded for the hazard ("saving the project
on the device afterwards writes to its old slot number"); (b) required
checkbox "The project in slot NNN is not currently loaded on the device";
(c) Clear additionally requires typing the slot number and lists the native
name + backup path. Side effects: neither Settings nor Global carries a
project-slot reference; the only referrer is the device's internal
current-project pointer, which is unreadable → user attestation is the only
possible guard (record in the matrix row).

**Hardware gate (new rows):** (1) 0x5A on scratch slots: framing, reply code
position, src emptied, dst byte-identical. (2) 0x5D: both payloads
byte-identical crosswise. (3) 0x5C: slot reads empty; subsequent copy into it
succeeds. (4) Danger probe on a sacrificial project: move/clear the *loaded*
project's slot, then Save Project on-device — document what happens; this
writes the modal copy.

**Tests:** `tests/files.rs` wire framing (opcode, path order, NUL
termination), refusal-byte handling; extract pure preflight/verify helpers in
`fileops.rs` for coverage.

## 2. Quick-rename for kits and Sounds (Plan rail, zero new ops types)

GUI plumbing only; `Plan::edit_kit` + `OpStep::EditKit`/`VerifyKitFields`
already carry it, validation included.

- New `src/gui/kit_modals.rs` (pattern: `project_modals.rs`): one
  `rename_modal` driven by `UiState.kit_rename: Option<KitRenameTarget>`,
  `enum KitRenameTarget { Kit(usize), Sound { kit: usize, track: usize } }`
  (lives in `gui/app.rs` — pure UI intent).
- Wire-up: `views/kits.rs` kit-list context menu → "Rename…"; `pad_card`
  context menu → "Rename Sound…". On confirm: one-edit `KitEditDraft` →
  `Plan::edit_kit` → `executor.launch`.
- Backup-first from `Plan::edit_kit`; EditKit drift guard protects
  scenes/perf. **Same write gate as kit editor v0** — no new hardware rows.
- Tests: one-edit rename draft compiles to `[EditKit, VerifyKitFields]`
  (+ backup); add the Sound-name case if absent.

## 3. Pattern rename — not possible; matrix correction

Verified against vendored sources: rytm-rs `pattern.rs` has no
`name()`/`set_name`; `ar_pattern_t` (rytm-sys/libanalogrytm) is fully
offset-mapped with **no name field** and no unmapped gap large enough. The
device identifies patterns as bank+number (A01–H16) only.

Action: flip the matrix "Rename pattern" row 🧭 → 🚫 with the note "no name
field in pattern object (ar_pattern_t fully mapped; rytm-rs exposes none) —
device identifies patterns as bank+number only". The row was wrong; this memo
corrects it rather than silently deviating.

## 4. Sound Pool rename — Plan rail, ships in slice C

Feasible: rytm-rs `Sound::set_name`/`name()` exist; pool sounds are already
read and written (pattern-paste path). Design:

- `ops/mod.rs`: `OpStep::RenamePoolSound { pool_index: u8, name: String }` +
  `OpStep::VerifyPoolSoundName { pool_index, expected }`;
  `Plan::rename_pool_sound(pool_index, name, backup_taken)` validating via an
  extracted `model::edit::validate_name(&str)` (one source of truth shared
  with `KitEdit::validate`).
- `ops/backend.rs`: `rename_pool_sound` + `read_pool_sound_name`;
  `LiveBackend` = fresh `SoundQuery` read → drift guard (byte-identical
  re-encode, same policy as EditKit) → `set_name` → encode → send.
- GUI: `views/pool.rs` context menu → the `kit_modals.rs` rename modal
  generalized (`RenameTarget::PoolSound(u8)`).
- **Two hardware unknowns gate this** (new rows): (a) sound object encode
  round-trip byte-identical on OS 1.72 (shared with pattern-paste pool-sound
  writes); (b) whether a sound sysex dump with obj_nr = pool index reliably
  lands in that pool slot rather than being routed by SYSEX RECEIVE to
  +Drive — read-back-verify catches misrouting, but the probe must establish
  it works at all.

## 5. Kit slot move — deferred, prerequisites named

rytm-rs exposes `Pattern::kit_number()`/`set_kit_number(0..=127)` — the
referrer rewrite is expressible. Minimal plan would be: fresh kit read
(drift-guarded) → re-encode with dst obj_nr → `SendObject` →
`VerifyKitSlots(dst)` → per referring pattern: read, `set_kit_number(dst)`,
write, `VerifyPatternKitLink` → optionally init src.

Deferred because: (1) kit object HW write round-trip still pending; (2)
pattern object HW write round-trip still pending — a pattern write rewrites
the entire pattern incl. trigs/plocks, and 1.70↔1.72 drift on patterns is
unproven; (3) there is no "empty kit slot" — all 128 kits exist; "move" is
really copy + relink + overwrite src with an init kit, and no verified
init-kit payload exists; (4) whether device RAM follows the rewrite when the
active pattern references src is unknown.

Matrix note (keep 🔬): "blocked on kit + pattern object HW write verification
and an init-kit payload; interim: kit copy-to-slot + pattern relink as
separate verified ops." Do not build a partial version now.

## Sequencing

- **`feature/drive-file-management`** (this branch): +Drive sample file ops
  only (sibling memo).
- **Slice A — `feature/project-slot-ops`** (stacked): §1 complete; ships
  UI-complete but the three ops stay disabled until the four hardware rows
  flip.
- **Slice B — quick-rename**: §2 + the §3 matrix correction in one PR.
- **Slice C — pool-sound rename**: §4, piggybacking the pattern-paste
  hardware probe session.
- Kit slot move: matrix note only, no branch.

## Constraint check

- Two clients, one model: slot ops touch only `FileClient`; renames only
  object sysex. ✓
- Every write transactional; backup-first (both slots for swap;
  abort-before-send if the clear backup fails). ✓
- Firmware gate: seven new/updated hardware-verification rows; 0x5A/0x5C/0x5D
  framing explicitly marked spec-unverified. ✓
- GPL hygiene: opcodes/framing from protocol research notes; no elektroid
  code. ✓
- PRD flag: WP21 says "never raw swaps"; project-slot swap **is** a raw device
  swap and is safe *only because* nothing readable references project slots.
  State this in the matrix row when flipping to 🔶 (satisfied vacuously, not
  violated).
