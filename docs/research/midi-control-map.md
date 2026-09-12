# AR MKII MIDI control map (manual Appendix C, OS 1.72)

The Rytm is fully playable and largely controllable over **plain MIDI** — no sysex,
no write risk. This is the transport for kitsmith's "play pads / play kits /
audition scenes & performances" features. Source: manual pp. 85–95.

## Note triggers (play)

Tracks must be on their default channels 1–12 (device MIDI CONFIG → CHANNELS).

| MIDI note | Function |
| --- | --- |
| 0–11 (C0–B0) | Trigger Sound on track 1–12 |
| 12–59 (C1–B4) | Play the **active track** chromatically |

Velocity is honored. Implication: "play this pad" = one NoteOn/NoteOff on the
track's channel; "audition a kit" = a scheduled note sequence; "chromatic preview
of a Sound" needs the track made active first.

## Control changes worth building on (per-track channel unless noted)

| CC | Parameter | Use in kitsmith |
| --- | --- | --- |
| 92 | **Active Scene** | Switch scenes from the app (audition scenes live) |
| 35–47 | Performance params 1–12 | Drive performance macros from the app (sliders = pad pressure) |
| 95 / 94 / 93 | Track level / mute / solo | Mixer-style kit view |
| 15 | Track Machine Type | Live machine switching (careful: changes the Sound) |
| 27 | Sample Slot | Live sample-slot audition on a track |
| 16–23 | Synth params 1–8 (meaning depends on machine — see below) | Live sound tweaking |
| 24–31 | Sample page (tune, fine, BR, slot, start, end, loop, level) | |
| 70–77 | Filter (ADSR, freq, res, mode, env depth) | |
| 78–83, 10, 7 | Amp (atk, hold, dec, OD, delay send, rev send, pan, vol) | |
| 102–109 | LFO (speed, mult, fade, dest, wave, phase, mode, depth MSB; 118 = depth LSB) | |

FX track (its own channel, default 13 in MIDI CONFIG): delay CC 16–23, reverb
CC 24–31, dist CC 70–72/76/77, compressor CC 78–85.

NRPN equivalents exist for everything (MSB.LSB): trig params 3.x, kit/track 1.100–104,
performance 0.0–11, synth 1.0–7, sample 1.8–15, filter 1.16–23, amp 1.24–31,
LFO 1.32–39, FX delay 2.0–7, reverb 2.8–15, dist 2.16–23, comp 2.24–31.

## Per-machine synth CC meaning (CC 16–23)

CC 16 is always **Level**; 17 is always **Tune** (except CP CLASSIC: Noise Tone;
UT NOISE: LP Freq; UT IMPULSE: Attack). CC 18 is Decay for every machine. The
remaining CCs 19–23 map to each machine's specific params — full table in the
manual pp. 88–95; keep the app's mapping table generated from one constants module.

## Caveats

- CC values are 0–127 while device params are often 0–127 halves or bipolar —
  display mapping must match the device screen, not the raw CC.
- Sending CC changes the **working kit state** (unsaved edits on device). Treat
  live-control features as "performance, not persistence": they never replace the
  save path (object sysex write of the kit), and the UI must not pretend a CC
  tweak was saved.
- MIDI note triggering does not require Overbridge; plain USB-MIDI is enough.
