# CLAUDE.md — rytmdash

**Smart dashboard for the Elektron Analog Rytm MKII.** A native app that recognizes the
Rytm over USB, reads its +Drive/project state, and displays it as a dashboard where
samples and other project entities (sounds, kits — later patterns) move between projects
by **drag & drop**. It replaces the tedious device-menu + Elektron Transfer juggling.

Language: **Rust**. Status: milestones M0–M5 built, **none hardware-verified yet**:
`midi/` (inquiry, duplex link, SDS), `device/` (watcher, object sync, SDS upload,
file-protocol reads), `model/` + `cache/`, `audio/` (WAV→48k/16bit mono), `ops/`
(transaction plans, backup-first, read-back verify), `gui/` (desktop shell per
PRD v2: app bar + sidebar navigation, project workspace with Kits/Samples/Patterns/
Overview tabs, inspectors, import queue, activity log, plan modal). The product
direction and remaining work packages (WP0–WP17) live in **`docs/PRD-v2.md`**;
earlier milestones in `docs/plan.md`.

## Commands

| Task | Command |
| --- | --- |
| Build | `cargo build --release` |
| Run (dashboard) | `cargo run --release` |
| Lint (as CI) | `cargo clippy --all-targets -- -D warnings` |
| Format | `cargo fmt` |
| Test | `cargo test --all-targets` |

## Ground truth documents (read before touching device I/O)

- `docs/research/current-usage-flow.md` — the AR data model (+Drive → project →
  kit → Sound → sample slot → file) and how users manage it today. **The problem
  statement lives here.**
- `docs/research/protocol-ecosystem.md` — the two sysex protocols (Elektron file
  protocol à la Transfer/elektroid; AR object protocol à la libanalogrytm/rytm-rs),
  prior art, licenses, risks.
- `docs/PRD-v2.md` — the product/UX/architecture spec driving the redesign and
  the WP0–WP17 work packages. **Read before UI or cross-project work.**
- `docs/plan.md` — the software plan (architecture, crates, phases).
- `reference/` (gitignored) — local copy of the AR MKII manual PDF (OS 1.72).

## Skills

`.claude/skills/` holds context skills — load them when working on the matching area:

- `ar-data-model` — the device's data structure and the reference invariants
  (slot indirection, what breaks what).
- `ar-sysex-protocol` — object sysex (kits/sounds/patterns/…): rytm-rs usage, caveats.
- `elektron-transfer-protocol` — sample/+Drive file transfer: how Transfer/elektroid
  do it, SDS fallback, audio format rules.

## Hard constraints

- **Never write to the device without a fresh backup pull first.** All protocols are
  reverse-engineered; a bad write can corrupt a project. Reads are free; writes are
  transactional and explicit.
- **Firmware-gate writes.** Formats verified against AR MKII OS 1.72 (rytm-rs targets
  1.70 — differences must be checked on real hardware before trusting a write path).
- **elektroid is GPLv3** — use it as a protocol spec only; never port its code.
- Samples move only via the file protocol; kit/sound/pattern objects only via object
  sysex. Don't conflate the two.
- AR native sample format: **16-bit, 48 kHz, mono** — convert on import like Transfer does.

## Conventions

Global `~/.claude/CLAUDE.md` applies (branch + PR per unit of work, never commit to
`main`, tests in `tests/`, CI on every PR, thin entry points, constants centralized).
