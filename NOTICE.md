# Notices and attributions

Kitsmith is an independent project, **not affiliated with, endorsed or
sponsored by Elektron**. Elektron, Analog Rytm and +Drive are trademarks of
Elektron Music Machines MAV AB, used here only to state compatibility
(nominative use). Kitsmith never distributes Elektron content — no factory
samples, Sounds, firmware or manuals; it only moves the user's own data
between their computer and their own device.

## Third-party software

| Project | Author | License | Use |
| --- | --- | --- | --- |
| [rytm-rs](https://github.com/alisomay/rytm-rs) / rytm-sys | alisomay | MIT | Analog Rytm object sysex SDK |
| [libanalogrytm](https://github.com/bsp2/libanalogrytm) | bsp2 | MIT | Object format ground truth (via rytm-sys) |
| [midir](https://github.com/Boddlnagg/midir) | Boddlnagg | MIT | Cross-platform MIDI I/O |
| [egui / eframe](https://github.com/emilk/egui) | emilk | MIT/Apache-2.0 | GUI framework |
| [hound](https://github.com/ruuda/hound) | ruuda | Apache-2.0 | WAV read/write |
| [serde](https://serde.rs) | serde-rs | MIT/Apache-2.0 | Serialization |

## Protocol references (no code used)

- [elektroid](https://github.com/dagargo/elektroid) (GPLv3) — studied strictly
  as a protocol *specification* for the Elektron file protocol. **No elektroid
  code is included, ported or derived from** — Kitsmith's implementation is
  original (see the GPL-hygiene rule in `CLAUDE.md`).
- MIDI Sample Dump Standard — openly documented MIDI standard.
- Elektron Analog Rytm MKII User Manual (OS 1.72) — factual reference
  (parameter names, CC numbers, data-structure facts). The manual itself is
  Elektron's copyright and is never redistributed with this project.
