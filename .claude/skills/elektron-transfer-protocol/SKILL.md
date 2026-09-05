---
name: elektron-transfer-protocol
description: Elektron file/sample transfer protocol (+Drive browse, sample up/download, .arprj/.arsnd) as spoken by Transfer and reverse-engineered in elektroid — load when implementing sample transfer or +Drive browsing.
---

# Elektron file protocol (samples & +Drive)

What Elektron **Transfer** speaks over USB-MIDI sysex. Clean-room reference:
**elektroid** (dagargo, GPLv3 — read `src/connectors/elektron.c` as a *spec*, never
port code; our implementation must be original).

## Capabilities (per elektroid's device map, AR MKII id = 16)

- `sample` filesystem: browse/create/rename/delete +Drive sample dirs; upload &
  download samples; **two storages: `+Drive` and `RAM`** (RAM = the 127 project
  slots — elektroid can even clear unused RAM slots remotely).
- `data` filesystem: generic data browse.
- `project` files: `.arprj` download/upload (whole project backup).
- `sound` files: `.arsnd`.
- Device identification: MIDI Device Inquiry; Elektron ids: AR MKI = 8, AR MKII = 16.

## Audio format rules (match Transfer's behavior)

- Native sample format: **16-bit, 48 kHz, mono** WAV. Transfer auto-converts
  anything dropped; we must too (resample + downmix + bit-depth on import).
- Per-project budget: ≤ 64 MB / ≤ 127 slots; +Drive total 1 GB. Max sample length
  33 s when recorded on device (not a transfer limit).

## Legacy fallback: SDS

The AR also accepts standard **MIDI Sample Dump Standard + Extended SDS** (the C6
path); uploads land in the +Drive `INCOMING/` dir (or the dir marked `UPLOAD HERE`
on-device). SDS is openly documented → a safe, if slow, first transport to validate
end-to-end sample upload before the proprietary protocol is fully implemented.
elektroid's `src/connectors/sds.c` implements it too.

## Practical notes

- Manual warns: MIDI+USB port config throttles USB transfer — advise USB-only.
- `FACTORY/` is write-protected; don't attempt writes there.
- When browsing +Drive, samples already loaded in the active project display their
  slot number — the protocol exposes the slot↔file relation for the *active*
  project; other projects' slot tables require reading their `.arprj`.
- elektroid exposes sample transfer by hash+size lookup (`elektron_get_sample_path_from_hash_size`)
  — the device tracks samples content-addressed; useful for dedup/"shared samples" UI.
