# kitsmith — software plan

> **Direction update (2026-09):** the goal has grown from "move samples/kits
> between projects" to **full Rytm management** — create/edit/save kits, scenes
> and performance macros; play pads and kits from the app; bulk sample
> upload/delete; rename and reorder everything renamable/reorderable; a
> dedicated analysis area. The per-operation roadmap now lives in
> **`docs/management-matrix.md`**; UX spec in **`docs/PRD-v2.md`** (WP0–WP17).
> The plan below is the original v1 record — its architecture and safety rules
> still stand.

Goal (v1): connect an Analog Rytm MKII over USB, auto-recognize it, read its state,
and present a **dashboard** where samples (first) and other project entities (next)
move between projects and the +Drive by drag & drop.

## Architecture

Library + thin binary, mirroring the flows in `docs/research/current-usage-flow.md`:

```
kitsmith/
├── src/main.rs            # bootstrap only
├── src/lib.rs             # wiring
├── src/midi/              # transport: port discovery, device inquiry, sysex framing,
│                          #   request/response correlation, retry/backoff
├── src/device/            # "the Rytm as a data source"
│   ├── detect.rs          # hotplug watch + MIDI Device Inquiry (AR MKII id 16)
│   ├── objects.rs         # kits/sounds/patterns via rytm-rs (object sysex)
│   └── files.rs           # +Drive/RAM sample FS, .arprj/.arsnd (clean-room file protocol;
│                          #   SDS fallback first)
├── src/model/             # the dashboard's own graph: Project, Kit, Sound, SampleSlot,
│                          #   DriveSample — with resolved cross-references + usage index
├── src/ops/               # user operations as transactions: move/copy sample,
│                          #   plan → preview → execute → verify; backup-before-write
├── src/audio/             # WAV import conversion (→ 16-bit/48 kHz mono), preview playback
├── src/cache/             # local mirror of device state (pull is slow; sync incrementally)
└── src/gui/               # egui/eframe dashboard: device tree, project columns,
                           #   drag & drop, usage highlights
└── tests/                 # per-module integration tests; protocol round-trip fixtures
```

### Key design decisions

1. **Two protocol clients, one model.** Object sysex (rytm-rs) and the file protocol
   are separate modules; `model/` merges both into one reference graph
   (pattern → kit → Sound → slot → file). All UI reads the model, never a protocol.
2. **Everything through `ops/` transactions.** A drag & drop compiles to a plan
   (e.g. *copy file ref, allocate slot 12 in project B, rewrite 3 Sounds*), shown
   to the user, executed stepwise, verified by read-back. No direct writes from GUI.
3. **Cache-first.** On connect: identify device → pull inventory into `cache/` →
   dashboard renders instantly from cache, refreshes incrementally. Sysex is slow;
   never block the UI on the wire.
4. **Read-only first, writes gated.** Milestones 1–3 never write to the device.
   Each write path unlocks only after byte-level round-trip verification on OS 1.72.

### Crate choices

| Concern | Crate | Why |
| --- | --- | --- |
| MIDI I/O | `midir` | Cross-platform CoreMIDI backend, sysex capable, de-facto standard |
| AR objects | `rytm-rs` | Only high-level AR SDK; Rust; we contribute fixes if 1.72 drifted |
| GUI | `eframe`/`egui` | Proven in cymatik; first-class drag & drop; immediate-mode fits a live dashboard |
| Audio decode/resample | `symphonia` + `rubato` | Import any format → 48 kHz/16-bit mono like Transfer |
| WAV write | `hound` | Simple, standard |
| Cache | `serde` + JSON/`redb` | Decide at milestone 2; start with serde-JSON files |

(If egui drag & drop between complex panes feels limiting we revisit — Tauri/iced are
the fallbacks — but we prototype in egui first.)

## Milestones

Branch + PR per milestone, per global workflow.

- **M0 — skeleton.** Cargo workspace, CI (fmt/clippy/test, warnings=errors), thin
  bin, `midir` port listing. *Accept: `cargo run` lists MIDI ports.*
- **M1 — recognize the Rytm.** Device Inquiry over USB-MIDI, hotplug watch,
  "Analog Rytm MKII (OS x.xx) connected" in a minimal egui shell.
  *Accept: plug/unplug reflects live in the UI.*
- **M2 — read the machine (read-only dashboard v0).** Pull sample-slot list +
  kits + sounds of the **active project** via rytm-rs; pull +Drive sample tree
  (start with what the file protocol/SDS lets us read); build the model + cache;
  render device tree and slot/usage table. *Accept: dashboard shows real slots,
  kit names, and which slots each kit uses, offline-refreshable from cache.*
- **M3 — cross-project view.** Download `.arprj` for all 128 slots (or on demand),
  parse slot tables, render projects side by side with shared-sample highlighting.
  *Accept: "these two projects share these N samples" is visible.*
- **M4 — first write: sample upload.** Drag WAV from Finder → convert → upload to
  +Drive dir (SDS first, file protocol after verification). *Accept: sample audible
  on device from INCOMING/.*
- **M5 — the headline feature.** Drag a sample between project columns → transaction
  plan preview → slot allocation + (when needed) Sound reference rewrite → verify.
  Backup pull enforced before the session's first write. *Accept: sample moved
  between two projects with kit references intact, confirmed on hardware.*
- **M6+ — more entities.** Superseded by the full-management direction: see
  `docs/management-matrix.md` (roadmap ledger) and `docs/PRD-v2.md` WP0–WP17.

## Open questions (resolve during M2–M3 on real hardware)

- Does OS 1.72 change any object format vs rytm-rs's 1.70 target? (round-trip test)
- Can the file protocol read `.arprj` of *non-active* projects without loading them?
  (elektroid suggests yes — verify.)
- Slot table location inside `.arprj` (needed for M3 without object-sysex per project).
- Whether RAM-slot writes via file protocol update the active project cleanly.
