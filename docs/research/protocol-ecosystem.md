# Protocol & ecosystem research

What exists for talking to an Analog Rytm from a computer, and what we can build on.
Everything below is community reverse-engineering — Elektron publishes **no** protocol docs;
the AR's sysex format is undocumented and unofficial.

## Communication channels

| Channel | What flows over it | Notes |
| --- | --- | --- |
| USB-MIDI (sysex) | Samples, projects, sounds, kits, patterns, songs, globals | The only management channel. The +Drive never mounts as a disk. |
| USB audio (Overbridge) | Multitrack audio + plugin control | Proprietary; irrelevant for sample management (v1 non-goal). |
| DIN MIDI | Same sysex, slower | Fallback; Turbo-MIDI supported. |

Device identification: standard MIDI Device Inquiry (`F0 7E .. 06 01 F7`); Elektron
device IDs seen in elektroid: AR MKI = 8, **AR MKII = 16**, Digitakt = 12.

## Two distinct protocols matter

1. **The Elektron "file" protocol** (what Transfer speaks): sysex-wrapped RPC for
   browsing/reading/writing the +Drive — sample bank directories, `.arprj` project
   files, `.arsnd` sound files, RAM sample slots. Reverse-engineered and
   implemented in **elektroid** (`src/connectors/elektron.c`). For the AR it
   exposes: `sample` FS (+Drive **and RAM** storage), `data` FS, `project`
   (`.arprj`), `sound` (`.arsnd`).
2. **The AR object sysex protocol** (what the SYSEX DUMP menu speaks): encoded
   dumps of kit / sound / pattern / song / settings / global objects, plus
   query/send of the *working state*. Reverse-engineered in **libanalogrytm**,
   wrapped for Rust by **rytm-sys / rytm-rs**.

Samples cannot move over protocol 2 (manual §14.5 explicitly says so); slot
metadata and kit/sound structure cannot be edited over protocol 1 alone.
**A dashboard that moves "project entities" needs both.**

## Prior art

| Project | Language / license | What it gives us |
| --- | --- | --- |
| [rytm-rs](https://github.com/alisomay/rytm-rs) ([docs](https://docs.rs/rytm-rs)) | Rust, over `rytm-sys` | High-level SDK for AR **FW 1.70**: query/modify patterns, kits, sounds, settings, globals (no songs) via `RytmProject`. Sysex encode/decode abstracted; range-validated setters. No sample transfer. |
| [libanalogrytm](https://github.com/bsp2/libanalogrytm) | C, headers | The underlying reverse-engineered object formats (kit/sound/pattern/song/settings/global), whole-project `.syx` ⇄ raw. No MIDI I/O. |
| [rytm-external](https://github.com/alisomay/rytm-external) | Rust (Max/MSP) | Proof rytm-rs works against real MKII hardware. |
| [elektroid](https://github.com/dagargo/elektroid) | C, GPLv3 | The Transfer-equivalent protocol: +Drive/RAM sample FS, project/sound files, SDS. GUI + CLI. **GPL: read for protocol understanding; do not copy code into a non-GPL app.** |
| Elektron Transfer | Closed | Official reference behavior: auto-conversion to 16-bit/48 kHz mono, EXPLORE two-pane drag & drop. |
| Elektron C6 + SDS | Closed / MIDI standard | Legacy sample dump into `INCOMING/`; standard Extended SDS is documented and safe to implement. |

## Constraints & risks

- **Firmware coupling.** rytm-rs targets FW 1.70; current OS is 1.72 (2025-01).
  Object formats can shift between firmware versions — must verify against real
  hardware early, and version-gate anything that writes.
- **Sysex speed is the bottleneck** (per rytm-rs author). Whole-project pulls are
  seconds-to-minutes; the dashboard needs caching + incremental sync, and the
  manual warns to set PORT CONFIG to USB-only for bulk transfer.
- **License wall.** elektroid is GPLv3 → treat as spec, reimplement cleanly.
  libanalogrytm/rytm-rs are permissive enough to build on (verify exact terms
  when we vendor).
- **Write safety.** Everything is undocumented; a bad write can corrupt a
  project slot. Design rule: *read freely, write transactionally, always after
  a backup pull.*
- **No native project rename exists in the spec** (checked 2026-09-05 against
  elektroid's connector). The data filesystem has `DATA_LIST 0x53`,
  read `0x54–0x56`, write `0x57–0x59`, `DATA_MOVE 0x5a`, `DATA_COPY 0x5b`,
  `DATA_CLEAR 0x5c`, `DATA_SWAP 0x5d` — move/copy/swap address *slots*
  (`/projects/<id>` → `/projects/<id>`), never names. The project name is
  metadata inside the `.arprj` payload, and rytm-rs's settings object carries
  no name field either. Renaming natively would mean download → patch
  unmapped bytes → re-upload of a whole project: out of bounds until the
  `.arprj` format is mapped and hardware-verified (WP11+). Until then,
  Kitsmith offers *local labels* only (`localmeta`), per PRD §32.

## Sources

- [AR MKII manual OS 1.72 (PDF)](https://www.elektron.se/wp-content/uploads/2025/01/Analog-Rytm-MKII-User-Manual_ENG_OS1.72_250130.pdf) — local copy: `reference/` (gitignored)
- [AR MKII support page](https://www.elektron.se/support-downloads/analog-rytm-mkii)
- [Transfer download](https://www.elektron.se/support-downloads/transfer)
- Repos linked in the table above
