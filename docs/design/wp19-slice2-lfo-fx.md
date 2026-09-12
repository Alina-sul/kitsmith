# Design memo — WP19 slice 2: LFO page + FX track edits through EditKit

> Status: PARKED (2026-09-12). Direction call: full file/data management
> (rename, move, paths — WP20/WP21) ships before deeper kit-parameter editing.
> This memo is the approved design for when the slice resumes; re-check
> rytm-rs version and `docs/hardware-verification.md` rows before implementing.

## Chosen approach

Extend the existing `KitField`/`FieldValue` vocabulary with LFO and FX variants
plus one new value kind, `FieldValue::Enum(u8)` carrying the **device wire
code** (not a list index), with per-field `(code, label)` option tables in the
model. Rejected alternative: a parallel `FxEdit`/`FxField` type and a second
`OpStep` — it would duplicate the whole draft/plan/verify/describe pipeline for
zero safety gain, since FX settings live in the same kit object and the same
`EditKit` write.

Everything rides the existing pipeline unchanged: `Plan::edit_kit` →
`OpStep::EditKit` (fresh read + byte-identical drift guard) →
`OpStep::VerifyKitFields`.

## rytm-rs ground truth (checked in the vendored source, rytm-rs 0.1.3)

- Track LFO: `Sound::lfo()/lfo_mut()` (`sound/page/lfo.rs`) — `set_speed`/
  `set_fade` isize −64..=63, `set_start_phase` 0..=127, `set_depth` **f32
  −128.0..=127.99 scaled to u16 0..=32767 on the wire** (step ≈ 0.0078 — the
  existing `SAMPLE_POS_TOLERANCE = 0.005` is too tight for it), enums
  `LfoMultiplier` (24 variants), `LfoDestination` (32, incl. `Syn1..Syn8` +
  `Unset`), `LfoWaveform` (7), `LfoMode` (5).
- Kit FX: `Kit::fx_delay()/fx_delay_mut()` (+ `fx_reverb`, `fx_distortion`,
  `fx_compressor`, `fx_lfo`, all with const getters). Ranges: mostly 0..=127;
  **delay `feedback` is 0..=198**; `stereo_width`/`symmetry` isize −64..=63;
  comp enums `FxCompAttack`(7)/`FxCompRelease`(8)/`FxCompRatio`(4)/
  `FxCompSideChainEq`(4); FX LFO mirrors the track LFO but with
  `FxLfoDestination` (27 variants).
- **Wire codes are sparse**: `FxLfoDestination` has `Unset = 37` and gaps at
  9/17/20; `LfoDestination` likewise uses named constants. Hence `Enum(u8)` =
  wire code with membership-validated option tables, never `index < len`.
- All relevant enums have `TryFrom<u8>`/`Into<u8>` — `device/kit_edit.rs` uses
  those as the only conversion; the model tables must match their code order
  (test-guarded).
- Skip `FxDelay::set_time_on_grid` (write-only convenience, no getter —
  unverifiable by read-back). Raw 0–127 time only in v0.
- FW-drift caveat: if OS 1.72 added any enum value unknown to rytm-rs (FW
  1.70), **the kit query itself fails to decode** (`TryFrom<&ar_sound_t>`
  errors) before any write — safe failure mode, but must be probed per enum on
  hardware.

## Files and types

**`src/model/edit.rs`** (split into `src/model/edit/mod.rs` + `edit/fields.rs`
— ~44 new variants triple the match arms; per the one-concern rule):
- `KitField` — track-scoped (payload = track 0–11): `LfoSpeed, LfoMultiplier,
  LfoFade, LfoDestination, LfoWaveform, LfoStartPhase, LfoMode, LfoDepth`.
  Kit-scoped (no payload, `track() = None`): `FxDelayTime, FxDelayPingPong,
  FxDelayStereoWidth, FxDelayFeedback, FxDelayHpf, FxDelayLpf,
  FxDelayReverbSend, FxDelayVolume`; `FxReverbPreDelay, FxReverbDecay,
  FxReverbFreq, FxReverbGain, FxReverbHpf, FxReverbLpf, FxReverbVolume`;
  `FxDistAmount, FxDistSymmetry, FxDistDelayOverdrive, FxDistDelayPost,
  FxDistReverbPost`; `FxCompThreshold, FxCompAttack, FxCompRelease,
  FxCompRatio, FxCompSideChainEq, FxCompGain, FxCompMix, FxCompVolume`;
  `FxLfoSpeed, …, FxLfoDepth` (8, mirroring track LFO).
- `FieldValue::Enum(u8)` (wire code).
- `KitField::enum_options(self) -> Option<&'static [(u8, &'static str)]>` —
  device-order `(code, label)` tables in `fields.rs`. `Syn1..Syn8` labeled
  generically ("SRC P1"… — machine param names arrive with the SRC slice;
  hiding them would make existing kits un-displayable).
- `KitField::is_fx() -> bool`; `KitEdit::describe()` prefixes FX fields with
  `FX · `.
- `KitField::float_tolerance(self) -> f32` replacing the blanket constant in
  `value_matches`: `SAMPLE_POS_TOLERANCE` for start/end, new
  `LFO_DEPTH_TOLERANCE: f32 = 0.01` (> wire step 0.0078) for
  `LfoDepth`/`FxLfoDepth`.
- `validate()`: `Enum(code)` valid iff present in `enum_options`; new int
  ranges (`FxDelayFeedback` 0..=198, signed −64..=63 set); `Toggle` accepted
  for `FxDelayPingPong`/`FxDistDelayPost`/`FxDistReverbPost`; `Float`
  −128.0..=127.99 for the two depths.

**`src/model/mod.rs`**:
- `TrackParams` gains `#[serde(default)] pub lfo: Option<LfoParams>` (nested
  `Option`, same pattern as `SoundModel::params` — old caches deserialize to
  `None`, editor group gates until re-sync; never fabricate zeros).
- `KitModel` gains `#[serde(default)] pub fx: Option<FxParams>`; new
  `FxParams { delay, reverb, dist, comp, lfo }` populated in the sync path
  next to `TrackParams::from_sound`, with a `value(field) ->
  Option<FieldValue>` mirror for the GUI.

**`src/device/kit_edit.rs`**: extend `apply`/`extract`. LFO via
`sound.lfo_mut()`; FX via `kit.fx_*_mut()`. Enum arms:
`LfoDestination::try_from(code)` etc.; extract emits `Enum(u8::from(*getter))`.
Check whether `kit/lfo.rs` `speed()`/`fade()` getters sign-extend like the
`amp.pan()` bug slice 1 worked around — apply the same truncation if so.

**`src/ops/`**: no structural change. `OpStep::EditKit`/`VerifyKitFields`
already carry arbitrary `KitEdit`s; the drift guard in
`live.rs::apply_kit_edits` is untouched and already protects these regions
(fresh read, whole-kit re-encode compare **before** mutation — LFO/FX bytes are
inside the compared buffer). The only verify-path change is the per-field
tolerance in `KitEdit::value_matches`.

**GUI** (`src/gui/`):
- `app.rs`: replace `KitsState::selected_track: Option<usize>` with
  `Option<KitLane>` where `enum KitLane { Track(usize), Fx }` (rejected:
  sentinel `12`). Touches `views/kits.rs` (`pad_grid`, `pad_card` ~L628,
  toggle ~L741, `track_inspector`), `top_bar.rs` L138/141, `kit_editor.rs`.
- `views/kits.rs`: render a 13th "FX" chip after the 12-pad grid in
  `pad_grid` (visually distinct — it is not a pad and has no voice pairing),
  selecting `KitLane::Fx`.
- `kit_editor.rs`: `track_editor` gains an `LFO` inspector group after `FLTR`
  (hardware page order: speed, mult, fade, dest, wave, phase, mode, depth);
  new `fx_editor(ui, model, state)` with groups `DELAY`, `REVERB`, `DIST`,
  `COMP`, `LFO`; new `enum_field()` widget (egui `ComboBox` over
  `enum_options`, draft-aware like `int_field`). Group gating on
  `params.lfo.is_some()` / `kit.fx.is_some()`.
- `kit_editor.rs::apply_draft_to_model` extended for the new fields (FX
  fields write to `KitModel::fx`).

Scope note: FX LFO is included ("FX track settings" per PRD §WP19); cutting it
saves ~8 variants if a smaller diff is wanted.

## Hardware verification (rows to add to `docs/hardware-verification.md`)

Two new **pending** rows under the existing "Kit field edits" row, both gated
before the write path is enabled for these fields:
1. *EditKit — LFO page*: per-enum wire-code probe (set each multiplier/
   destination/waveform/mode value on the device UI → query → compare code),
   depth write→read within `LFO_DEPTH_TOLERANCE`, and drift-guard pass on a
   1.72 kit with non-default LFO destination (confirms no 1.70→1.72 enum
   additions break decode).
2. *EditKit — FX pages*: same probe for comp enums + FX LFO destination; delay
   feedback above 127 (0..=198 range is unusual — confirm on device); dist
   `delay_post`/`reverb_post` routing toggles; scratch-project single-field
   write → read-back per page.

## Test plan (`tests/kit_edit.rs`)

- Extend `every_field_with_value(track)` with the 8 LFO fields; add
  `every_fx_field_with_value()`; both feed the existing round-trip test.
- Enum exhaustiveness: for every enum field, `apply` each code in
  `enum_options` → `extract` → equal; a code **not** in the table (incl. gap
  codes 9/17/20/36 for FX LFO dest) is rejected by `validate`.
- Table-vs-rytm-rs consistency: every code in every `enum_options` table
  converts via the rytm enum's `TryFrom<u8>` and back.
- Depth quantization: `kit.as_sysex()` → fresh
  `RytmProject::update_from_sysex_response` → `extract` → `value_matches`
  within `LFO_DEPTH_TOLERANCE` (exercises the u16 wire scaling offline).
- Isolation: an FX edit leaves all 12 sounds byte-equal; a track-LFO edit
  leaves FX pages untouched.
- `describe()` snapshots for FX fields (`FX · delay time → 64`) and enum
  labels.
- Existing MockBackend plan tests inherit the new fields for free
  (`Plan::edit_kit` is field-agnostic).

## Constraint check

- Backup-first: inherited — `Plan::edit_kit` prepends `BackupProject`.
- Write gate: fields ship behind the two pending verification rows; same GUI
  gating as slice 1.
- Object sysex only; no file protocol involved. No live CC/NRPN — drafts only,
  per `docs/research/midi-control-map.md` rule.
