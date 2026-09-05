---
name: ar-data-model
description: Analog Rytm MKII data structure and reference invariants — load when modeling projects/kits/sounds/sample slots or reasoning about what a move/unload operation breaks.
---

# Analog Rytm MKII data model

Source: AR MKII manual OS 1.72 §5, §9, §10, §14.2 (local copy in `reference/`).
Fuller narrative: `docs/research/current-usage-flow.md`.

## Structure

- **+Drive** (1 GB, non-volatile): 128 project slots · Sound library (4096 Sounds,
  16 banks A–P × 256) · sample bank (directory tree: `FACTORY/` write-protected,
  `INCOMING/` computer-transfer target, `RECORDED/` sampling target, user dirs).
- **Project**: 128 patterns · 128 kits · 16 songs · 4 globals · Sound pool
  (128 slots) · **127 sample slots**, ≤ 64 MB total sample RAM.
- **Kit**: 12 drum-track Sounds + FX track settings + 12 scenes + 12 performance
  macros (48 param locks each pool).
- **Sound**: machine choice + SRC/SMPL/FLTR/AMP/LFO parameter pages + a
  **sample slot number** (not a file path).
- **Pattern**: sequencer data + a **link to one kit** (many patterns may share a kit).
- Tracks: 12 drum tracks + 1 FX track; 8 voices (4 pads share 2×2).

## Reference chain (the invariant that matters)

```
pattern → kit (by slot) → Sound (in kit) → sample slot (1–127, by number) → +Drive file
```

- Slots are referenced **by number**. Replacing a slot's file silently changes every
  Sound/kit/pattern that uses that slot. Moving a sample to a different slot breaks
  those references unless every referring Sound is rewritten.
- `LOAD TO PROJ` fills *first free* slots — slot numbers are not user-chosen.
- The manual's `REPLACE` flow (swap file, keep slot number) is the only on-device
  reference-preserving operation.
- Sysex-received kits/sounds carry slot numbers from their **source** project;
  importing a kit without aligning sample slots = wrong audio, no error.

## Persistence rules

- Active project = working RAM state; changes are NOT auto-saved to the +Drive slot.
  On power-off only the *active kit's* unsaved edits survive among kits.
- "Unload" removes a sample from a project slot only; the +Drive file stays.
  `PURGE ALL` / `SELECT UNUSED → UNLOAD` clear pool/slots, never +Drive files.

## Implications for rytmdash

- Model slots as first-class entities: `SampleSlot { index, drive_path, used_by: [Sound] }`.
- Any cross-project move = copy file reference **and** allocate a slot **and**
  rewrite referring Sounds' slot numbers atomically.
- Usage analysis (which patterns/kits break?) requires reading kits + patterns via
  object sysex, not just the file listing.
