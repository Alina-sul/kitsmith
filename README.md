# rytmdash

A smart dashboard for the **Elektron Analog Rytm MKII**.

Plug in the Rytm and rytmdash recognizes it, reads its +Drive and projects, and lays
the whole machine out in one view — samples, sounds, kits, projects and how they
reference each other. Moving a sample (or later, any project entity) between projects
is a drag & drop, with the slot bookkeeping handled for you.

Why: on the device, sample management means juggling `GLOBAL SETTINGS → SAMPLES`,
127 numbered slots per project, and a file browser — with no view of what uses what,
and no way to move things *between* projects without breaking kit references.
Elektron Transfer moves files but knows nothing about slots, kits, or usage.

## Status

Early research & planning. No code yet.

- [Current usage flow](docs/research/current-usage-flow.md) — how AR sample/project
  management works today (mapped from the official manual) and where it hurts.
- [Protocol & ecosystem](docs/research/protocol-ecosystem.md) — the sysex protocols
  and prior art (rytm-rs, libanalogrytm, elektroid, Transfer).
- Software plan: `docs/plan.md`.

## Planned stack

Rust · USB-MIDI (sysex) · [rytm-rs](https://github.com/alisomay/rytm-rs) for object
data · clean-room Elektron file protocol for samples · native GUI (TBD in plan).
