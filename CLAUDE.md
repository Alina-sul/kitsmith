# CLAUDE.md — kitsmith

**Full management suite for the Elektron Analog Rytm MKII.** A native app that
recognizes the Rytm over USB and makes everything manageable that the device
allows: browse and move samples/kits/patterns across projects by drag & drop,
**create/edit/save kits, scenes and performance macros**, play pads and kits from
the app (MIDI + local preview), bulk upload/delete samples, rename and reorder
every renamable/reorderable entity, and analyze the data (dependencies, duplicates,
unused, storage) in a dedicated area. It bridges the analog hands-on experience
with fast digital editing — replacing device-menu + Elektron Transfer juggling.

Language: **Rust**. Status: read side hardware-verified on OS 1.72 (recognition,
object sync, +Drive/project listing, raw `.arprj` download + parsed layout); write
paths built but gated on hardware verification (`docs/hardware-verification.md`).
Modules: `midi/`, `device/`, `model/` + `cache/`, `audio/`, `ops/` (transaction
plans, backup-first, read-back verify), `gui/` (PRD-v2 desktop shell). The UX spec
and work packages (WP0–WP17) live in **`docs/PRD-v2.md`**; the entity × operation
roadmap in **`docs/management-matrix.md`**; the v1 record in `docs/plan.md`.

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
- `docs/management-matrix.md` — every entity × operation: protocol, status,
  safety notes. **The roadmap ledger for the full-management direction.**
- `docs/hardware-verification.md` — what is verified vs pending on OS 1.72;
  the write-gate ledger.
- `docs/research/midi-control-map.md` — the MIDI note/CC/NRPN control surface
  (playing pads/kits, scenes, performance from the app).
- `docs/plan.md` — the v1 software plan (architecture, crates, phases).
- `reference/` (gitignored) — local copy of the AR MKII manual PDF (OS 1.72, the
  latest firmware); index + download link in `docs/research/README.md`.

## Skills

`.claude/skills/` holds context skills — load them when working on the matching area:

- `ar-data-model` — the device's data structure and the reference invariants
  (slot indirection, what breaks what).
- `ar-sysex-protocol` — object sysex (kits/sounds/patterns/…): rytm-rs usage, caveats.
- `elektron-transfer-protocol` — sample/+Drive file transfer: how Transfer/elektroid
  do it, SDS fallback, audio format rules.
- `ar-sound-machines` — machines per track, kit anatomy, scenes/performance
  semantics, voice pairing, MIDI play/control surface. Load for kit creation,
  scene/performance features, or anything sound-facing.

## Subagents (consult them — that's what they're for)

`.claude/agents/` holds project reviewers; each loads the right skills itself:

- `rytm-architect` — design gate **before** implementing any feature touching
  device/model/ops. Produces the design memo (modules, types, flow, test plan).
- `rytm-write-safety` — adversarial review of **every diff that writes to the
  device** (object sysex, file protocol, bulk ops) before the PR opens.
- `rytm-sound-designer` — musical/domain review for kit creation, scenes,
  performance, play features, parameter defaults.
- `rytm-ux-reviewer` — GUI diffs reviewed against PRD-v2's interaction spec and
  the hardware mental model.

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

## Legal & licensing (see docs/research/legal-shipping.md)

- **No Elektron marks in product identity.** The product is **Kitsmith** — never put
  "Elektron", "Rytm" or "+Drive" in the app name, binary/crate name, branding, or
  domain. "Works with Elektron Analog Rytm MKII" in descriptions is fine (nominative
  use) and must sit next to the non-affiliation disclaimer (`config::LEGAL_DISCLAIMER`
  — the single source of that text; README/NOTICE mirror it).
- **Never commit or distribute Elektron content**: no manual PDFs (keep `reference/`
  gitignored), no factory samples/Sounds, no firmware, no device dumps in fixtures —
  test fixtures must be synthetic or from the developer's own data.
- **elektroid is GPLv3**: protocol spec only, never ported/derived code (also listed
  under hard constraints — it is both a legal and a licensing rule).
- **New dependency ⇒ NOTICE.md row.** Every crate or protocol reference added gets an
  attribution entry in `NOTICE.md` in the same PR.
- **Interop must stay clean-room**: implement from wire observation and community
  specs; never decompile Elektron software or firmware.

## Conventions

Global `~/.claude/CLAUDE.md` applies (branch + PR per unit of work, never commit to
`main`, tests in `tests/`, CI on every PR, thin entry points, constants centralized).
