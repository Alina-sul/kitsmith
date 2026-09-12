---
name: ar-sysex-protocol
description: Analog Rytm object sysex (kits, sounds, patterns, songs, globals, settings) via rytm-rs/libanalogrytm — load when implementing device object read/write or debugging sysex.
---

# AR object sysex protocol (kits/sounds/patterns/…)

Undocumented, community reverse-engineered. Two layers of prior art:

- **libanalogrytm** (C, bsp2): the format ground truth. Object types: kit, sound,
  pattern, song, settings, global. Container framing + 7-bit encode/decode in
  `sysex.h`/`sysex.c`. Converts whole-project `.syx` ⇄ raw and back. No MIDI I/O.
- **rytm-rs** (Rust, alisomay, over `rytm-sys` bindings to libanalogrytm): SDK-style
  API. `RytmProject` holds patterns, kits, sounds, settings, globals (**no songs**).
  Query types per object; setters range-validated. Targets **FW 1.70**.

## Using rytm-rs

- Flow: build query → send its sysex via any MIDI crate (e.g. `midir`) → feed device
  response bytes into `RytmProject::update_from_sysex_response` → read/mutate structs
  → encode back to sysex → send to device.
- It does MIDI I/O through *you* — bring your own transport. Good: we control
  scheduling/backpressure.
- Work-buffer vs saved-slot queries both exist (query the working state or a slot).

## Caveats (all verified-by-community, not by Elektron)

- **Samples never move over this protocol** (manual §14.5) — file protocol only
  (see `elektron-transfer-protocol` skill).
- Firmware drift: rytm-rs = FW 1.70; device OS 1.72 current. Diff-test on real
  hardware (round-trip: query → re-encode → byte-compare) before any write feature.
- Sysex is slow; the device is the bottleneck. Whole-project pull is heavy — cache
  and query incrementally. Set device PORT CONFIG to USB-only (not MIDI+USB) for bulk.
- Sound objects exist in three homes: track Sound (inside kit), project Sound pool
  (128), +Drive library (4096, banks A–P). `SEND SYSEX` on device can export sounds;
  received sounds land in pool/+Drive per the receive menu.
- Device sysex receive can run regardless of active menu; data saved to the slot
  specified in the dump, or first free if unspecified.

## Safety rules for kitsmith

1. Pull a full backup (`.syx`/`.arprj`) before the first write of a session.
2. Byte-level round-trip test any object type on this firmware before enabling
   its write path.
3. Writes are explicit user actions, never background sync.
