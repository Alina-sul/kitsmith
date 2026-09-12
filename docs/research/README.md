# docs/research — ground-truth research for rytmdash

Every piece of protocol/device/UX research lands in this folder as a markdown file.
Rule: **research before implementation** — a feature that touches the device gets a
research doc (or an update to one) first, and the doc cites its sources.

## The manual

The authoritative device reference is the **Analog Rytm MKII User Manual, OS 1.72**
— the latest firmware as of 2026-09 (Elektron's release notes were last updated
2025-01-30 and 1.72 remains current).

- Local copy: `reference/Analog-Rytm-MKII-User-Manual_ENG_OS1.72.pdf` (gitignored —
  Elektron's copyright; do not commit).
- Download: <https://www.elektron.se/support-downloads/analog-rytm-mkii>
- Release notes: <https://www.elektron.se/release-notes/analog-rytm-mkii-os-release-notes>

If Elektron ships a firmware newer than 1.72: update the manual copy, re-run the
hardware verification matrix (`docs/hardware-verification.md`), and re-check every
skill in `.claude/skills/` before trusting any write path.

## Index

| Doc | What it answers |
| --- | --- |
| `current-usage-flow.md` | The AR data model and how users manage it today — the problem statement. |
| `protocol-ecosystem.md` | The two sysex protocols, prior art (Transfer, elektroid, libanalogrytm, rytm-rs), licenses, risks. |
| `midi-control-map.md` | Complete MIDI note/CC/NRPN control surface (manual Appendix C) — the basis for playing pads/kits and live parameter control from the app. |
| `../hardware-verification.md` | What is *verified on real hardware* vs assumed — the write-gate ledger. |
| `../management-matrix.md` | Every device entity × every management operation: what's possible, over which protocol, and its status. |

## Related context skills (`.claude/skills/`)

- `ar-data-model` — structure + reference invariants (slot indirection).
- `ar-sysex-protocol` — object sysex (kits/sounds/patterns/…).
- `elektron-transfer-protocol` — file protocol (+Drive, samples, .arprj).
- `ar-sound-machines` — machines, parameter pages, scenes/performance, kit
  construction and MIDI play/control (the sound-design layer).
