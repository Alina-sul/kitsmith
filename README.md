# Kitsmith

**A full management suite for the Elektron Analog Rytm MKII.**

Plug in the Rytm and Kitsmith recognizes it, reads its +Drive and projects, and
lays the whole machine out in one view — samples, Sounds, kits, patterns,
projects and how they reference each other. From there, everything the device
allows becomes manageable from the desktop:

- **Move & copy** samples, kits and patterns between projects by drag & drop,
  with the slot/reference bookkeeping handled for you.
- **Create & edit** kits — and later scenes and performance macros — then save
  them to the device.
- **Play** pads and kits from the app (MIDI triggering through the machine's
  analog engine, plus local sample preview).
- **Organize**: rename projects/kits/samples, bulk upload & delete, reorder
  what the device lets you reorder.
- **Analyze**: dependency graph, duplicate and unused samples across projects,
  storage budgets.

Why: on the device, sample management means juggling `GLOBAL SETTINGS →
SAMPLES`, 127 numbered slots per project, and a file browser — with no view of
what uses what, and no way to move things between projects without breaking
kit references. Elektron Transfer moves files but knows nothing about slots,
kits, or usage.

**Every device write is a previewed, backup-protected, read-back-verified
transaction.** Reads are free; writes are explicit.

## Status

Read side hardware-verified on OS 1.72 (device recognition, object sync,
+Drive/project listing, raw project download + parsed layout). Write paths are
built but gated on per-format hardware verification —
see `docs/hardware-verification.md` and the roadmap in
`docs/management-matrix.md`.

## Run

`cargo run --release`. On the Rytm, set `GLOBAL SETTINGS → SYSTEM → USB CONFIG`
to USB-MIDI and `MIDI CONFIG → PORT CONFIG` input/output to USB.

## Documentation

- `docs/PRD-v2.md` — product/UX spec and work packages.
- `docs/management-matrix.md` — every entity × operation: what's possible,
  over which protocol, and its status.
- `docs/research/` — protocol and device research (index in its README).

## Legal

Kitsmith is an independent project, **not affiliated with, endorsed or
sponsored by Elektron**. Elektron, Analog Rytm and +Drive are trademarks of
Elektron Music Machines MAV AB, referenced only to state compatibility.
Kitsmith never distributes Elektron content; it only moves your own data to
and from your own device. The device protocols are community
reverse-engineered for interoperability; all writes are backup-protected, but
you use this software **at your own risk** (see `LICENSE`).

Third-party attributions: `NOTICE.md`. License: MIT.
