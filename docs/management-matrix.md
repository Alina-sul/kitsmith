# Full-management matrix — every entity × every operation

The product goal (2026-09 direction): **manage everything on the Rytm that is
manageable** — create, rename, reorder, move, bulk-edit, play, and analyze — from
rytmdash. This document is the single roadmap ledger for that goal: what each
operation means, which protocol carries it, and where it stands.

Protocols: **FP** = Elektron file protocol (samples, +Drive, project/sound files);
**OS** = object sysex via rytm-rs/libanalogrytm (kits, sounds, patterns, …);
**MIDI** = plain notes/CC/NRPN (play & live control, no write risk);
**RAW** = our own parsing of the raw `.arprj` payload (verified layout, see
`docs/hardware-verification.md`).

Status: ✅ built+hardware-verified · 🔶 built, needs hardware verification ·
🧭 designed/verified feasible, not built · 🔬 needs research/reverse-engineering ·
🚫 not possible remotely (device-only).

## Samples & +Drive

| Operation | Protocol | Status | Notes |
| --- | --- | --- | --- |
| Browse +Drive tree | FP 0x10 | ✅ | dirs+files, hash/size/write-protect |
| Download sample | FP | 🧭 | elektroid-proven; needed for preview + backup |
| Upload sample (single) | FP / SDS | 🔶 | SDS path built (M4), FP upload preferred; conversion to 16-bit/48k mono built |
| **Bulk upload** | FP | 🧭 | queue through ops/ serially; per-file progress + abort |
| **Delete sample file** | FP | 🧭 | elektroid-proven; must check no project references the hash first (cross-project scan) |
| **Bulk delete** | FP | 🧭 | same + one plan listing every file and every affected project |
| **Rename sample file / dir** | FP | 🧭 | elektroid-proven; hash stays → project slot tables unaffected (verify on HW) |
| Create/delete +Drive dir | FP | 🧭 | `FACTORY/` write-protected |
| Load sample → project slot | FP 0x17 write / ram_load | 🔶 | built (`ram_load_slot`), pending HW verify |
| Unload slot / purge unused | FP | 🧭 | elektroid can clear RAM slots remotely |
| **Reorder project slots** | FP + OS | 🔬 | slot numbers are referenced by Sounds/trigs — reorder = rewrite every referrer; only via full transaction |
| Local audio preview (computer) | — | 🧭 | decode WAV locally; no device needed |
| Audition slot on device | MIDI | 🧭 | CC 27 (sample slot) on a scratch track + note trig |

## Projects

| Operation | Protocol | Status | Notes |
| --- | --- | --- | --- |
| List 128 project slots | FP 0x53 | ✅ | |
| Download/backup project | FP 0x54–56 | ✅ | raw uncompressed layout parsed (kits, patterns, slot table) |
| Upload/restore project | FP 0x57–59 | 🧭 | write side gated on WP0 write verification |
| **Rename project** | FP | 🔶 | built in PR #8 (rename + verify); pending merge |
| Copy project → slot | FP 0x5a–5c or down/up | 🔶 | copy_data built; pending HW verify |
| Delete/clear project slot | FP 0x5c | 🧭 | destructive — plan + typed confirmation |
| **Reorder projects** (slot swap) | FP | 🧭 | move/copy/clear ops exist in protocol; verify slot-referencing side effects (globals? none known) |
| Cross-project compare | RAW | 🔶 | slot-table hashes enable shared-sample view |

## Kits & Sounds

| Operation | Protocol | Status | Notes |
| --- | --- | --- | --- |
| Read kits/sounds (active project) | OS | ✅ | incl. sound locks for dependency graph |
| Read kits of *backed-up* projects | RAW | ✅ | parsed from .arprj |
| **Edit kit params** (levels, sounds, FX) | OS | 🔶 | kit raw→syx round-trip proven offline; HW write pending |
| **Create kit from scratch → save to slot** | OS | 🧭 | the headline write; machine-per-track validity per `ar-sound-machines` skill |
| **Rename kit / Sound** | OS | 🧭 | name field in kit/sound objects (NUL-padded); round-trip then write |
| Copy kit across projects | RAW→OS | 🔶 | built in PR #8 (capture→switch→paste); slot remap in plan |
| **Reorder kits** (slot moves) | OS | 🔬 | patterns reference kits by slot — reorder must rewrite pattern links |
| Sound pool management (load/unload/rename) | OS | 🧭 | pool = 128 slots; sound locks reference pool indices |
| +Drive Sound library (4096) | FP `.arsnd` / OS | 🔬 | which protocol wins for bulk library ops — research |
| **Play a pad / audition a kit** | MIDI | 🧭 | notes 0–11 ch 1–12; audition sequence respects voice-pair chokes |
| Live param control (mixer, tweaks) | MIDI CC/NRPN | 🧭 | working-state only; never presented as saved |

## Scenes & Performance (per kit)

| Operation | Protocol | Status | Notes |
| --- | --- | --- | --- |
| Read scenes/perf macros | RAW/OS | 🔬 | rytm-rs keeps these private (upstream TODO) — decode `ar_kit_t` regions ourselves (WP15) |
| **Create/edit scene** (fixed-value locks) | OS | 🔬 | 48-lock budget across 12 scenes; after decode verified |
| **Create/edit performance macro** | OS | 🔬 | offsets, not values; same budget |
| Audition scene / drive macro live | MIDI | 🧭 | CC 92 = active scene; CC 35–46 = macros 1–12 |

## Patterns & Songs

| Operation | Protocol | Status | Notes |
| --- | --- | --- | --- |
| Read patterns (trigs, locks, kit link) | OS/RAW | ✅ | |
| Copy pattern across projects | OS | 🔶 | built in PR #8; pending HW verify |
| Rename pattern | OS | 🧭 | |
| Reorder patterns | OS | 🔬 | chains/songs reference pattern slots |
| Songs | — | 🔬 | rytm-rs has no song support; libanalogrytm has the type — research |

## Analysis area (read-only, zero device risk)

| Feature | Basis | Status |
| --- | --- | --- |
| Dependency graph (pattern→kit→Sound→slot→file) | OS+RAW | ✅ core built (WP10) |
| Cross-project duplicate/shared samples | slot-table hashes | 🔶 |
| Unused-sample finder (project + +Drive-wide) | graph | 🧭 |
| Storage breakdown (+Drive, per-project 64 MB budget) | FP sizes | 🧭 |
| Project health (broken refs, empty slots, name gaps) | graph | 🧭 |

## Standing rules (apply to every row)

1. Writes ship only after byte-level round-trip verification on OS 1.72, logged
   in `docs/hardware-verification.md`.
2. Every write — including each item of a bulk operation — is an `ops/` Plan
   step: preview, backup-first, execute, read-back verify, abortable.
3. Reorder operations are reference-rewriting transactions, never raw slot swaps.
4. Bulk destructive plans list every affected entity by name and every project
   that references it.
