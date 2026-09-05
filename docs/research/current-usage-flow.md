# Current usage flow — Analog Rytm MKII sample & project management

How sample/kit/project management works *today*, mapped from the official
[Analog Rytm MKII User Manual (OS 1.72)](https://www.elektron.se/wp-content/uploads/2025/01/Analog-Rytm-MKII-User-Manual_ENG_OS1.72_250130.pdf)
and the Elektron Transfer documentation. This is the baseline we are rethinking.

## 1. The data model (manual §5)

```
+Drive (non-volatile, 1 GB)
├── 128 project slots
│     └── PROJECT = 128 patterns · 128 kits · 16 songs · 4 globals
│                  · 128-slot Sound pool · 127 sample slots (≤ 64 MB RAM)
├── Sound library: 4096 Sounds (16 banks A–P × 256)
└── Sample bank: directory tree of mono 16-bit/48 kHz WAV
      ├── FACTORY/   (write-protected presets)
      ├── INCOMING/  (default target for computer transfers)
      └── RECORDED/  (default target for on-device sampling)

Active project (working RAM state)
├── pattern ──links to──► kit (1 of 128)
├── kit = 12 track Sounds + FX track settings (+ scenes, perf macros)
├── Sound = sample slot reference + SRC/SMPL/FLTR/AMP/LFO settings
└── sample slots 1–127 ──reference──► +Drive sample files
```

Key indirections (the source of most management pain):

- A **pattern** doesn't own its sounds — it links to a **kit**.
- A **kit** doesn't own samples — its Sounds reference **project sample slots**.
- A **sample slot** doesn't own audio — it references a **+Drive file**.
- Nothing persists to the +Drive project slot until the user **manually saves
  the project**; only the *active kit* survives power-off unsaved.

## 2. Device-side flows (what the user does on hardware)

### Load samples into a project (manual §14.2.1)
`GLOBAL SETTINGS → SAMPLES` → browse +Drive dirs → tick samples → `[RIGHT]` menu
→ `LOAD TO PROJ` → samples land in the *first free* slots of the 127.
Slot assignment is not chosen by the user; a sample already in the project shows
its slot number in the browser.

### View / unload / replace project samples
`SAMPLES → [LEFT] → VIEW RAM` → 127-slot list (○ = unused in any pattern) →
tick → `UNLOAD`, or highlight → `REPLACE` → browse +Drive → pick a file
(slot number preserved — the only way to keep Sound references intact).

### Purge unused
`SAMPLES → VIEW RAM → SELECT UNUSED → UNLOAD` (samples stay on +Drive), or
`PROJECT MANAGER → PURGE ALL` for sounds + samples at once.

### Assign a sample to a track Sound (manual §10.8)
`[TRK]+[PAD]` → `[SMPL]` page → knob D scrolls the **127 project slots** only.
`[FUNC]+[YES]` jumps to +Drive browse to fill/replace a slot in place.

### Kits & Sounds (manual §10)
- `SOUND BROWSER` / `SOUND MANAGER`: browse Sound pool ⇄ +Drive library,
  `COPY TO POOL / +DRIVE / BANK`, tag, rename, `SEND SYSEX`.
- Kits are saved per project; **there is no on-device way to copy a kit to
  another project.**

### Project management (manual §9.1)
`PROJECT MANAGER`: load/save/clear/rename/write-protect project slots.
Everything is whole-project granularity.

## 3. Computer-side flows (Elektron Transfer)

Transfer ([elektron.se/support-downloads/transfer](https://www.elektron.se/support-downloads/transfer))
connects over **USB-MIDI** (SysEx) — not mass storage; the +Drive never mounts
as a disk.

- **DROP page**: drag audio files → auto-converted to 16-bit/48 kHz mono →
  written to the +Drive sample bank.
- **EXPLORE page**: two-pane browser (My Computer ⇄ Device). Drag samples both
  directions, create/rename/delete +Drive directories, drag whole **projects**
  (`.arprj`) and **Sounds** (`.arsnd`) off/onto the device for backup.
- Firmware updates and sound-pack installs.

Legacy path: **C6** utility + MIDI **SDS / Extended SDS** sample dump into the
`INCOMING/` directory (manual §14.2.4). Still supported by the device.

SysEx dump (manual §14.5): kits, Sounds, patterns, songs, globals — individually
or "whole project" — over MIDI/USB. **Samples are explicitly excluded** from
sysex dump; they only move via the SAMPLES menu protocol (Transfer/C6).

## 4. The pain points this project addresses

Documented across Elektronauts threads
([workflow](https://www.elektronauts.com/t/question-your-workflow-with-samples-projects/11442),
[consolidating projects](https://www.elektronauts.com/t/consolidating-patterns-kits-samples-between-projects/49405),
[sample management](https://www.elektronauts.com/t/sample-management-issues-1-30/18858),
[noob's guide](https://www.elektronauts.com/t/a-noobs-guide-to-sample-management-on-the-analog-rytm/17979)):

1. **No cross-project view.** The device shows one project at a time; Transfer
   shows files, not what uses them. Nobody shows "project A uses these 40
   samples, project B these 25, and they share 12."
2. **Moving anything between projects is manual and lossy.** Copying a kit to
   another project means sysex-dumping it, loading the other project, receiving
   it — and its Sounds' *sample-slot references* break unless the same samples
   sit in the same slots.
3. **Slot bookkeeping is invisible.** Sounds reference slots by number; replace
   or reorder slots and kits silently play the wrong sample.
4. **Three-level indirection is opaque.** Pattern → kit → Sound → slot → file:
   answering "which patterns break if I unload this sample?" is impossible
   on-device.
5. **Transfer is a file manager, not a project manager.** It moves files but has
   no notion of slots, kits, or usage.

## 5. Flow summary diagram

```
                 ┌───────────────  computer  ───────────────┐
   WAV/AIFF ──► Transfer (convert 48k/16b mono) ──► +Drive sample bank
                 C6 + SDS (legacy)           ──► +Drive INCOMING/
                 Transfer/C6 sysex  ◄──► kits · sounds · patterns · projects
                 └──────────────────────────────────────────┘
                                   USB-MIDI (sysex)
                 ┌───────────────  device  ─────────────────┐
   +Drive bank ──LOAD TO PROJ──► project sample slots (1–127, 64 MB RAM)
   slot ──SMPL page──► Sound ──save──► kit ──link──► pattern ──chain──► song
   project (working state) ──manual SAVE──► +Drive project slot (×128)
                 └──────────────────────────────────────────┘
```
