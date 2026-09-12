---
name: ar-sound-machines
description: AR MKII machines, parameter pages, scenes/performance semantics, kit construction rules, and the MIDI play/control surface — load when building kit creation/editing, scene/performance features, playing pads from the app, or judging whether a generated kit/sound makes musical sense.
---

# AR sound architecture: machines, kits, scenes, performance

Source: AR MKII manual OS 1.72 — §4 (voices), §8.2 (machines), §10 (kits/sounds/
scenes/performance), Appendix A/B (params), C (MIDI), D (machines). Local copy in
`reference/`. MIDI detail: `docs/research/midi-control-map.md`.

## Voices and tracks

- 12 drum tracks (BD, SD, RS, CP, BT, LT, MT, HT, CH, OH, CY, CB) + 1 FX track.
- **8 analog voices for 12 tracks**: 4 pad pairs share circuits — BT/LT, MT/HT,
  CH/OH, CY/CB. Triggering one chokes the other in the pair. A "kit" that expects
  CH and OH simultaneously is physically impossible; sound-design features must
  respect the pairing.
- Every drum track = analog synthesis (machine) + sample layer + analog filter +
  amp + LFO. Machine and sample can be used together or alone.

## Machines (what can run on which track)

A machine is the synthesis model on a track's SRC page. Availability is
track-constrained — this is a hard validity rule for kit creation:

| Machines | Allowed tracks |
| --- | --- |
| DISABLE, UT NOISE, UT IMPULSE | all 12 tracks |
| BD HARD/CLASSIC/FM/PLASTIC/SILKY/SHARP/ACOUSTIC | BD, SD, RS, CP |
| SD HARD/CLASSIC/FM/NATURAL/ACOUSTIC | BD, SD, RS, CP |
| SY DUAL VCO, SY CHIP, SY RAW | BD, SD, RS, CP |
| RS HARD, RS CLASSIC | RS, CP |
| CP CLASSIC | RS, CP |
| BT CLASSIC | BT |
| XT CLASSIC (low/mid/high tom) | LT, MT, HT |
| HH BASIC, HH LAB, CH CLASSIC, CH METALLIC | CH, OH |
| OH CLASSIC, OH METALLIC | CH, OH |
| CY CLASSIC, CY METALLIC, CY RIDE | CY, CB |
| CB CLASSIC, CB METALLIC | CY, CB |

- `DISABLE` turns synthesis off → pure sample track (and makes any sample Sound
  loadable/lockable on that track).
- Each machine exposes ≤ 8 SRC params; **param 1 is always LEV, TUN is almost
  always param 2, DEC always exists**. Full per-machine catalogs: manual App. D
  (names + meaning) and App. C (CC/NRPN order — differs from screen order!).
- The Sound Browser only shows Sounds whose machine fits the active track —
  mirror that filter in any "assign Sound to pad" UI.

## Sound anatomy (per drum track, stored in kit)

Parameter pages: **SRC** (machine params) · **SMPL** (tune, fine, bit reduction,
slot, start, end, loop, level) · **FLTR** (1 multimode filter: ADSR + freq/res/
mode/env-depth) · **AMP** (attack, hold, decay, overdrive, delay send, reverb
send, pan, volume) · **LFO** (1 LFO: speed, multiplier, fade, destination, wave,
phase, mode, depth). Plus per-Sound settings: machine choice, chromatic mode,
env/LFO reset, velocity mod, aftertouch.

FX track pages: delay, reverb, distortion, compressor, LFO — settings live in the
kit (Appendix B).

## Kit anatomy (what "create a kit" must produce)

A kit = 12 track Sounds + FX track settings + 12 scenes + 12 performance macros
+ track levels + routing (+ ctrl-in mods). Validity rules for generated kits:

1. Every track's machine must be legal for that track (table above).
2. Sample slots referenced by SMPL pages must exist in the project (1–127);
   slot 0/0xFF = no sample.
3. Voice pairs: don't design kits that require both halves of a shared voice at
   once; prefer complementary settings (e.g. short CH decay + longer OH).
4. Scenes/performance lock budgets (below) must not exceed 48 each.
5. A kit saved to a slot does not auto-link to any pattern — patterns reference
   kits by slot number; creating a kit is safe, *replacing* one affects every
   pattern linked to that slot.

## Scenes vs performance (both stored per kit, 12 each — one per pad)

- **Scene** = a set of *fixed-value* parameter locks, any params from any of the
  13 tracks. Activating the scene applies those values. Budget: **48 locks total
  across the 12 scenes of a kit** (any distribution).
- **Performance macro** = a set of *modulation-depth* locks (relative offsets,
  ±); pad pressure scales the offset continuously from 0 → full depth. Same
  48-lock budget across the 12 macros. Quick Performance maps chosen macros to
  the QUICK PERF knob.
- Neither can be parameter-locked into the sequencer; they are live controls.
- On-device: scene/perf copy/paste/clear per pad; stored only when the kit is
  saved.
- **rytm-rs does NOT expose scenes/performance** (`Kit.scene_ctl`/`perf_ctl` are
  private raw bytes, upstream TODO). Editing them requires our own decoding of
  the kit's raw regions against libanalogrytm's `ar_kit_t` layout, verified
  byte-for-byte on OS 1.72 before any write (see docs/hardware-verification.md).

## Playing from the computer (no sysex, no risk)

- Note 0–11 on channels 1–12 triggers tracks 1–12; notes 12–59 play the active
  track chromatically. Velocity honored.
- CC 92 = active scene; CC 35–47 = performance macros 1–12 (slider = pressure);
  CC 93/94/95 = track solo/mute/level. Full map: `docs/research/midi-control-map.md`.
- CC/NRPN edits change the device's **working state only** (unsaved kit edits).
  Persistence still goes through the object-sysex kit write + save. Never present
  a CC tweak as saved.

## Musical defaults (for generated kits — sound-designer agent territory)

- Start from the machine's namesake track (BD machine on BD track, etc.);
  cross-assignments (SY RAW bass on SD track) are a feature, not a default.
- Keep AMP volume headroom: per-track defaults ≈ 100, overdrive 0.
- Filter wide open (freq max, env 0) unless the design asks otherwise.
- Sends at 0; let the user push delay/reverb deliberately.
- LFO depth 0 by default — a kit with hidden LFO motion confuses more than it
  delights.
