# Device family landscape — how unique is the Rytm, how portable is rytmdash?

Question (2026-09-12): are there similar devices, and could rytmdash become
per-device software, or is the AR ecosystem too unique?

## The three layers port very differently

### 1. File protocol — shared across the whole Elektron family

elektroid drives every Elektron box through **one connector**: Analog Four
MKI/MKII/Keys, Analog Rytm MKI/MKII, Analog Heat, Digitakt I/II, Digitone
I/II/Keys, Syntakt, Model:Samples, Model:Cycles, Monomachine MKII — differing
only in a per-device capability table (which filesystems exist: sample / data /
project / sound / preset / raw). Elektron's own Transfer is likewise one app for
all devices. Everything rytmdash speaks over this protocol (+Drive browse,
sample up/download/rename/delete, project backup, 48k/16-bit conversion)
generalizes with device-ID + capability changes.

### 2. Object protocol — unique per device

Kit/sound/pattern binary formats are reverse-engineered per machine.
libanalogrytm/rytm-rs cover only the AR. Concepts don't even map: the Digitakt
has **no kits** (sounds live in the pattern), a Digitone sound is an FM patch,
Syntakt has its own machine set. Each device needs its own libanalogrytm-scale
effort; community coverage varies (Digitakt: decent — see Elk-Herd, the closest
existing cousin to rytmdash; Syntakt/Digitone II: thin).

### 3. Domain model & UX — the device's identity

12-pad grid, 8-voices/12-tracks pairing, machines-per-track, scenes/performance,
127-slot indirection: AR-specific product design. A second device would reuse
the shell (sidebar, project browser, plan modal, activity log, import queue)
but need its own workspace.

## Portability of current rytmdash modules

| Module | Ports to another Elektron device |
| --- | --- |
| `midi/` transport, inquiry, hotplug | ~100% |
| `device/files.rs` file-protocol client | ~90% (elektroid-proven pattern) |
| `ops/` plan/backup/verify, `cache/` | ~95% |
| GUI shell + import + activity | ~80% |
| `device/objects.rs` (rytm-rs), raw `.arprj` parsers | 0% — per-device |
| Kit editor, pad grid, scenes, `ar-sound-machines` knowledge | 0% — per-device |

Rule of thumb: a second Elektron device ≈ 40–50% of a from-scratch build
(object-format reverse-engineering + device workspace on the shared chassis);
sample-centric boxes (Digitakt) are the cheapest. Non-Elektron hardware (MPC,
Roland, Polyend) shares nothing — each would be a new product.

## Decision

Stay **AR-only as the product**. Keep the seam clean per PRD-v2 §49
(capability-driven device architecture): nothing outside `device/objects.rs`
and the raw parsers may assume "Rytm". Build no multi-device abstraction until
a second device is actually committed — the natural candidate then is the
Digitakt.
