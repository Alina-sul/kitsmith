# KITSMITH — Product, UX and Architecture PRD v2

**Status:** Product direction + implementation specification  
**Date:** 2026-09-05  
**Audience:** Claude Code / implementation agent / product & engineering review  
**Primary device for this phase:** Elektron Analog Rytm MKII, OS 1.72  
**Implementation stack today:** Rust + eframe/egui on macOS  
**Long-term direction:** one desktop management environment for supported Elektron devices  

---

## 0. Executive mandate

Kitsmith should become the place where a user manages an Elektron device instead of mentally translating slot numbers, hidden references, project boundaries, and multiple Elektron utilities.

The product is **not** a software clone of the hardware UI. It should preserve the hardware's useful mental model — especially the Analog Rytm's 12-pad layout — while making the device's hidden data relationships visible, searchable, transferable, and safe to edit.

The product must optimize for this real workflow:

1. The hardware is physically next to the computer.
2. The user wants to understand what is in the device at a glance.
3. The user wants to move between projects, kits, sounds and samples without remembering slot numbers.
4. The user wants to drag an item to where it should go and let Kitsmith calculate the reference bookkeeping.
5. The user wants batch operations for samples and naming.
6. The user must never be surprised by a destructive write.

The current pilot already proves several important foundations: device recognition, active-project reading, a local project model, a 12-pad representation, sample-slot tracing, WAV conversion/upload, and a backup-first transaction framework. Those are the foundations to keep. The next phase is primarily a **product-structure and interaction redesign**, followed by the missing protocol/indexing work that makes cross-project management possible.

### North-star product promise

> **“I can see what is on my Elektron, understand what depends on what, and move/copy/rename things safely without doing slot bookkeeping.”**

---

# PART I — PRODUCT MODEL

## 1. Current baseline that must be preserved

The existing implementation already contains valuable engineering that should not be thrown away during the UI redesign:

- Automatic Analog Rytm MKII discovery over USB-MIDI.
- Device inquiry and connected/disconnected state.
- Active-project sync into a small serializable model.
- Cached project data for fast launch/offline rendering.
- Read path for kits, patterns, project Sound pool and sample-slot metadata.
- File-protocol work for sample names and project slot names.
- 12-pad hardware-oriented grid.
- A sample usage index and reverse trace interaction.
- WAV conversion to 16-bit / 48 kHz / mono.
- Sample upload pipeline.
- A transaction abstraction (`Plan -> preview -> execute -> verify`).
- Automatic first-write project backup.
- Read-back verification for writes.
- Mock backend tests for write safety.

The redesign should **reuse the current backend safety model** and replace the current “everything on one canvas” UI with a clearer information architecture.

---

## 2. Problems in the current pilot

The screenshot and current implementation reveal a number of product problems that are independent of protocol correctness.

### 2.1 The hierarchy is visually unclear

The current screen contains a row of kit names/pattern labels, a pad grid, a sample rail, a project counter, device status, and sync status, but the user has to infer what each area represents. The distinction between **Project**, **Kit**, **Pattern**, **Track Sound**, **Sample Slot**, and **+Drive Sample File** is not strong enough.

### 2.2 The main navigation is not a project manager

A small `projects (N)` control is not sufficient for a device with 128 project slots. Projects are a primary object in the Analog Rytm workflow and should be persistently visible, searchable, comparable, and usable as drag-and-drop destinations.

### 2.3 The pad cards are useful, but under-designed

The 12-card representation is the strongest part of the current UI because it maps directly to the physical machine. Keep it. Improve it.

Current issues:

- Cards are visually generic rather than clearly behaving like tracks/pads.
- Text is too small.
- Too much metadata competes for attention.
- The sample slot badge is visually unexplained.
- The `synth` label is misleading.
- The cards do not expose obvious drag affordances.
- The layout does not take advantage of wide desktop windows.

### 2.4 `synth` is the wrong state label

On Analog Rytm, a track Sound can combine the analog/digital machine layer **and** a sample layer. “No sample assigned” does not mean “this is a synth track.” The machine name already explains the synthesis engine.

**Required change:** remove the `synth` chip entirely.

If a sample is assigned, show a sample pill. If no sample is assigned, either show a quiet `No sample` line or show no sample pill at all. The user should never see `synth` as the opposite of `sample`.

### 2.5 Names and fallbacks are confusing

The app currently leaks states such as generic sound names and unresolved sample names into the main view. Kitsmith must distinguish:

- a real native device name,
- an intentionally unnamed object,
- a name that exists but has not been resolved because a protocol phase failed,
- a local Kitsmith alias.

These are not the same state and must not share the same presentation.

### 2.6 The light presentation conflicts with the intended tool

The current screenshot is light, low-contrast and sparse even though the intended product direction is a dark hardware-adjacent utility. Theme behavior must be controlled by Kitsmith rather than relying on an implicit egui/system default.

**Required change:** Kitsmith dark theme is the default and is explicitly applied on startup. Light theme may exist as an option later, but it must not be the accidental default.

### 2.7 The screen has too much empty canvas and too little useful structure

At large window sizes, the central content occupies only a small portion of the available width while important lists remain compressed. The redesign should use a real desktop layout: persistent sidebar, main workspace, optional inspector, responsive panes.

### 2.8 Errors and partial sync are too technical and too small

A status such as `sync complete — 4 object(s) did not answer` is important. It should not be tiny gray text in the toolbar. Main UI should say something understandable such as `Partial sync · 4 items unavailable` with a clickable detail view. Raw protocol details belong in Diagnostics.

---

## 3. Analog Rytm facts that should shape the product

The UI and dependency engine should be designed around actual Analog Rytm concepts rather than generic file-manager assumptions.

### 3.1 Device-wide storage

The +Drive contains:

- up to 128 saved projects,
- a global sample bank available to all projects,
- a +Drive Sound library with up to 4096 Sounds.

A sample file existing in the +Drive is **not the same thing** as that sample being loaded into a project's sample slot.

### 3.2 Per-project structure

A project contains:

- 128 patterns,
- 128 kits,
- 16 songs,
- 4 global slots,
- a Sound pool of up to 128 Sounds,
- 127 project sample slots,
- up to 64 MB of loaded sample RAM.

### 3.3 Kit structure

A kit contains:

- 12 drum-track Sounds,
- FX track settings,
- levels/retrig/general Sound settings,
- Scene definitions,
- Performance macro definitions.

A pattern links to a kit. More than one pattern can link to the same kit, so editing a kit can affect several patterns.

### 3.4 Scene structure

There are 12 Scenes per kit, mapped to the 12 pads. A Scene applies fixed parameter values. Only one Scene can be active at a time. The hardware uses blue to communicate Scene mode.

The manual documents a **48-parameter-lock total budget across the 12 Scenes in a kit**. This is valuable information to visualize in Kitsmith.

### 3.5 Performance structure

There are 12 Performance macros per kit, mapped to the 12 pads. Unlike Scenes, Performance values are relative modulation depths controlled by pressure. The hardware uses green to communicate Performance mode.

### 3.6 Sound Pool and Sound Locks matter for dependency analysis

The active project Sound pool can contain up to 128 Sounds. A sequencer trig can use a Sound Lock to select a Sound Pool Sound for that step.

This matters because a Sound can reference a sample. Therefore, a robust “what uses this sample?” graph must not stop at `kit -> track sound -> sample slot`. It must eventually include:

`pattern -> sound lock -> Sound Pool sound -> sample slot`.

**Product consequence:** any feature called `Unused`, `Safe to unload`, `Purge`, or `Cleanup` must use the full dependency graph. The current kit-only sample usage index is useful for display but is not sufficient to prove that a sample is globally unused by the project.

### 3.7 Useful hardware-native management operations

The device itself conceptually supports operations that Kitsmith can make far easier:

Projects:

- load,
- save to slot,
- rename,
- delete/clear,
- initialize new,
- toggle write protection,
- purge unused project resources.

Project samples:

- load a +Drive sample into a project slot,
- view 127 RAM slots,
- unload samples,
- replace a slot,
- select/remove unused samples.

Sounds:

- copy/store to Sound Pool or +Drive library,
- rename,
- tag,
- delete,
- select unused,
- write-protect.

These capabilities should inform the roadmap, but **must only become write actions after the relevant protocol path is verified on real hardware**.

---

## 4. Product principles

### P1 — Show nouns before numbers

The user should see `Crash`, `DEUIL B`, `BD`, and `A01` before they see `slot 3`, `kit 2`, or protocol IDs. Slot numbers remain available because they are important for debugging and hardware correlation, but they are secondary.

### P2 — Preserve the hardware mental model where it helps

The 12-pad layout, track abbreviations, Scene blue and Performance green are useful because they match the machine sitting next to the user.

Do not copy the hardware's menu limitations into the desktop application.

### P3 — Drag-and-drop should express user intent, not protocol mechanics

The user drags `Crash` onto `BD`. Kitsmith decides whether that means:

- reuse an already-loaded project sample slot,
- allocate a new project sample slot,
- load a +Drive file into RAM,
- upload an external WAV first,
- update the kit Sound reference,
- verify the result.

The preview explains the generated plan before anything changes.

### P4 — Copy is safer than move

Cross-project drag-and-drop should default to **Copy**. `Move` requires an explicit second step because deleting/cleaning the source can have hidden dependencies.

### P5 — Never silently overwrite a fixed slot

Kits, projects, Sound Pool entries and sample slots are slot-based. If a destination is occupied, show the collision and require a resolution. Do not interpret a drop as permission to destroy an occupied destination.

### P6 — Separate selection from device activation

Viewing Project 42 in Kitsmith is not the same as loading Project 42 on the hardware. The UI must always distinguish:

- selected in the app,
- currently active on the device,
- cached/offline,
- write-protected.

### P7 — Partial data is a first-class state

If names or objects cannot be read, the app should stay usable and visibly mark incomplete data. It should not fill missing data with plausible labels.

### P8 — Every destructive action is reversible in practice or clearly not reversible

Mutating plans must use the existing backup-first framework. Where a true automatic rollback is not available, the confirmation must say so. Never present “Undo” unless Kitsmith can actually guarantee an inverse write.

### P9 — Keep the main view musical, keep protocol detail in Diagnostics

Users should see `Sample name unavailable`, not `opcode 0x23 failed`. Diagnostics can expose transport details for development and support.

### P10 — Multi-device support should be capability-driven

Do not redesign the whole application around an imaginary universal 12-pad Elektron. Keep a shared shell and shared transfer concepts, but allow device-specific workspaces.

---

# PART II — INFORMATION ARCHITECTURE

## 5. New top-level application structure

Replace the current single dashboard with a desktop shell containing four top-level destinations:

1. **Projects** — device project browser and project workspaces.
2. **+Drive** — global sample browser and, later, +Drive Sound library.
3. **Import** — external audio preparation and batch upload queue.
4. **Activity** — sync, upload, transfer, backup and transaction history/status.

`Projects` is the default destination after startup.

### 5.1 Persistent app shell

Use a three-part desktop layout:

- **Top app bar** — device status, sync, global actions.
- **Left sidebar** — top-level navigation plus the project list when Projects is selected.
- **Main workspace** — selected project or selected global library view.
- **Right inspector** — context-sensitive details; collapsible.

At normal desktop widths the user should be able to keep the project list visible while working on a kit. This is what makes project-to-project drag-and-drop discoverable.

---

## 6. Top app bar specification

**Target height:** 52 px.  
**Always visible.**

### Left section

- `kitsmith` wordmark.
- Connected device pill:
  - green status dot,
  - `Analog Rytm MKII`,
  - optional `OS 1.72` in tooltip/details.
- If disconnected: neutral dot + `No device`.

The device pill opens **Device & Diagnostics**, not a tiny raw MIDI-port menu.

### Center / flexible section

Show a breadcrumb when inside a project:

`Projects / 002 DEUIL B / Kits / Kit 02`

Breadcrumb segments are clickable.

### Right section

Primary controls:

- **Sync** icon + text.
- **Import** icon + text.
- Activity indicator if work is running.
- Settings button.

Do not hide primary controls behind `...`.

### Sync states

Idle:

- `Sync`
- tooltip: `Refresh device data`

Running:

- spinner,
- short human label, e.g. `Reading kits 87/128`,
- cancel icon.

Partial completion:

- amber badge `Partial sync · 4 unavailable`
- clicking opens Activity/Diagnostics details.

Success:

- brief `Synced just now` status that fades to a timestamp in project header metadata.

---

## 7. Left sidebar specification

**Normal width:** 240–280 px.  
**Minimum collapsed width:** ~64 px.  
**Scrollable independent of main workspace.**

### 7.1 Top-level navigation

Use icon + label rows:

- Projects
- +Drive
- Import
- Activity

Settings/device diagnostics stay at the bottom.

### 7.2 Project browser inside sidebar

When `Projects` is selected, show:

- section title `PROJECTS`,
- search field,
- filter button,
- `Compare` toggle/action,
- scrollable list of project slots.

Each project row should show:

- slot number, fixed width: `002`,
- native project name, strong text,
- active-device marker if this is the active project,
- lock icon if write-protected,
- index state if contents are not available locally.

Example:

`002  DEUIL B        ACTIVE`

A project row must never look like a kit row or pattern chip.

### 7.3 Project row visual states

- **Selected in app:** elevated/filled neutral highlight.
- **Active on device:** small green `ACTIVE` badge or green left accent.
- **Write-protected:** lock icon.
- **Cached but device disconnected:** `CACHED` badge when relevant.
- **Not indexed:** subtle `Not indexed` secondary text, not an error.
- **Empty project slot:** `042  Empty` with reduced emphasis.

### 7.4 Project drag-and-drop behavior

Project rows are important **drop targets**.

When the user drags a compatible object, destination project rows should visibly change state:

- compatible target: highlighted outline + `Copy to project` hint,
- source/current project: disabled if the operation would be a no-op,
- write-protected target: lock + `Read only`, no drop,
- unknown/unindexed target: allow only if preflight can obtain required data first.

Examples:

- drag a sample onto another project -> add/reuse that sample in the target project sample table.
- drag a kit onto another project -> copy kit plus required sample-slot mappings.

Dropping never writes immediately. It opens a Transfer Plan.

---

# PART III — PROJECT WORKSPACE

## 8. Project header

The project workspace should begin with a clear header, not a strip of ambiguous chips.

### Header content

Left:

- `Project 002`
- project name, large: `DEUIL B`
- active/cached/partial state
- optional local favorite star

Right:

- `Compare`
- `Project actions`
- sync timestamp

### Project actions menu

Only show actions whose protocol capability is implemented and verified. Future actions include:

- Rename project
- Save active project to this slot
- Load on device
- Duplicate / Copy to slot
- Toggle write protection
- Initialize empty project
- Delete / Clear
- Purge unused resources

Dangerous operations must be visually separated at the bottom of the menu.

### Important wording

Never use `Open project` to mean `Load on hardware`.

Use:

- `View in Kitsmith`
- `Load on device`

as separate concepts.

---

## 9. Project tabs

Under the project header use a stable tab row:

1. **Kits** — default.
2. **Samples**.
3. **Patterns**.
4. **Sound Pool**.
5. **Overview**.

`Scenes` and `Performance` belong inside a selected Kit, not as project-level tabs.

Songs and Globals can be added later when they have a clear management use case.

---

# PART IV — KITS EXPERIENCE

## 10. Kits page layout

The Kits page is the primary musical workspace.

Use a two- or three-pane layout:

- **Kit list**: 240–280 px.
- **Pad workspace**: flexible center.
- **Inspector**: 300–340 px, collapsible.

At narrower widths, the inspector collapses into a drawer. The kit list may collapse after that. The pad grid should remain the visual priority.

---

## 11. Kit list

The current implementation shows only kits linked by patterns. That is useful as a filter, but it hides exactly the empty/unused kit slots needed for management.

### Required filters

- `In use` — default.
- `All 128`.
- `Unused`.

### Kit row

Each row:

- `02` slot number.
- kit name.
- pattern count or small pattern badges.
- optional warning if the kit data is partial.

Selected row is clearly highlighted.

Example:

`02  DEUIL B`  
`    B01 · B02 · B03`

If many patterns use the kit, do not show a wall of badges. Show first 3–4 and `+N`.

### Search

Search by:

- kit name,
- slot number,
- linked pattern name such as `B03`.

---

## 12. Kit header

Above the pad grid show:

- `Kit 02 · DEUIL B`
- linked pattern summary: `Used by B01, B02, B03`
- local note/favorite if present
- kit actions.

Kit actions may include, when supported:

- Rename / Save as
- Copy to another kit slot
- Copy to another project
- Compare with another kit
- Reload saved kit

A small `Copied from...` provenance line may be local metadata, not device data.

---

## 13. Kit mode switch

Directly above the 12-pad area use a segmented control:

- **Tracks**
- **Scenes**
- **Performance**
- **FX & Routing**

This is a strong mapping to the hardware while avoiding separate disconnected screens.

Default: `Tracks`.

---

## 14. Tracks mode: 12-pad grid

Preserve the Analog Rytm physical orientation:

Top row: `CH  OH  CY  CB`  
Middle row: `BT  LT  MT  HT`  
Bottom row: `BD  SD  RS  CP`

### 14.1 Responsive dimensions

At a normal 1280+ width:

- card width: ~170–220 px depending on available space,
- card height: ~108–128 px,
- gap: 10–12 px.

Do not shrink important text simply to preserve fixed card sizes.

At minimum window width the grid may become horizontally tighter, but should remain 4 columns to preserve the hardware mapping.

### 14.2 Card anatomy

Each pad card should contain, in order:

**Top row**

- colored track label: `BD`.
- optional tiny track index only in tooltip.
- six-dot drag handle at top-right on hover/focus.

**Primary text**

- Track Sound name, 15–16 px equivalent, strong.
- If no native name is available: `Unnamed BD sound` or `BD sound`, not a fake numbered name.

**Secondary text**

- Machine name, e.g. `BdHard`.

**Sample row**

If a sample is assigned:

- purple sample pill with waveform/sample icon,
- slot number `03`,
- file name `Crash`.

If sample name is unresolved:

- `03 · name unavailable`
- small question/info icon with tooltip explaining that file metadata could not be resolved.

If sample is off:

- show `No sample` in muted text **or omit the row**.
- never show `synth`.

### 14.3 Pad card interactions

Click:

- select track and open track inspector.

Double-click:

- no destructive shortcut.
- optional future behavior: open detailed Sound view.

Drag from card:

- means `Copy this Track Sound` only in contexts with an unambiguous destination, such as Compare Kits.
- drag ghost must include track label + Sound name.

Drop sample onto card:

- means `Assign this sample to this track`.

Drop compatible Track Sound onto card:

- means `Copy/replace Track Sound` and requires a Plan preview.

### 14.4 Drag affordance

A visible drag handle is mandatory for draggable entities.

Use a six-dot / 2x3-dot handle roughly 14x18 px.

Behavior:

- whole card may be draggable for usability,
- handle communicates discoverability,
- handle appears on hover/focus,
- keyboard alternative must exist in contextual menu (`Copy to...`).

Do not make the user guess that text chips are draggable.

---

## 15. Track inspector

Selecting a pad opens a right inspector with clear groups.

### Header

`BD · SOUND 1` or real Sound name  
`Machine: BdHard`

### Sample group

- `Sample`
- `Slot 03`
- `Crash.wav`
- full +Drive path on a second line or tooltip
- size
- usage link: `Used in 6 places`
- `Trace usage` button
- `Replace sample...` action when supported

### Sound group

Read-only initially:

- machine
- native Sound name
- source: `Kit track`, `Sound Pool`, or `+Drive Sound` when knowable

Do not expose all synth parameters in v1 of the redesign. Parameter editing remains out of scope.

### Usage group

List actual references known to the dependency engine:

- current kit/track,
- Sound Pool references,
- pattern Sound Locks if parsed.

Use plain language.

---

## 16. Sample tracing inside a kit

The existing purple cross-highlight concept is worth keeping, but it should be integrated with the inspector instead of relying on a permanently dense right rail.

When a sample is selected or `Trace usage` is active:

- all pads in the selected kit using that sample receive a 2 px purple ring,
- inspector shows project-wide usages,
- the Samples tab can be opened with the sample preselected.

Add `Esc` / close control to exit trace state.

Do not leave the user in an invisible persistent trace mode.

---

## 17. Compare Kits mode

This is the recommended solution for “easy transfer between kits.”

### Entry points

- `Compare with another kit` from kit actions.
- Select two kits from kit list in Compare mode.

### Layout

Two complete kit pad grids side by side:

`Source Kit` | `Target Kit`

At normal widths, each grid may use compact cards but must retain the 3x4 orientation.

### Core interaction

Drag a source pad onto a target pad to copy the Track Sound.

The target pad must be compatible with the source machine/Sound. If not compatible:

- disable drop,
- show a short reason.

After drop, open a Plan preview showing:

- target kit/track,
- Sound/sample dependencies,
- sample-slot reuse/allocation,
- whether target Sound will be replaced,
- backup step,
- verification step.

### Important rule

A pad-to-pad copy is a copy. The source kit is unchanged.

---

# PART V — SCENES AND PERFORMANCE

## 18. Scenes mode

Scenes are part of the selected kit and should use the same 12-pad spatial layout.

Use **blue** as the mode accent because that matches the hardware.

### Scene card

Each of the 12 cards shows:

- `Scene 1` … `Scene 12`,
- optional local Kitsmith alias, e.g. `Breakdown`,
- `N locks`,
- compact affected-track badges such as `BD · OH · FX`.

Empty Scene:

- `Scene 4`
- `Empty`

### Scene page header

Show a kit-wide lock budget:

`Scene locks: 18 / 48`

This is substantially more understandable than discovering the limit while editing on hardware.

### Scene inspector

On selection:

- local alias field,
- native scene number,
- lock count,
- parameter locks grouped by track,
- fixed value for each lock,
- affected tracks.

Example:

`BD`  
`Filter cutoff → 32`  
`Sample level → 92`

`FX`  
`Reverb send → 64`

### Write scope

Phase 1: read-only visualization + local aliases/notes.  
Phase 2: copy/paste/clear/edit only after Scene object mapping and hardware write round-trip are proven.

Do not implement Scene writes based solely on guessed struct offsets.

---

## 19. Performance mode

Performance macros should use the same 12-pad grid with a **green** mode accent.

### Performance card

- `Perf 1` … `Perf 12`,
- optional local alias, e.g. `Kick Pressure`,
- `N locks`,
- affected-track chips.

### Performance inspector

Show relative modulation depths, not fixed values.

Example:

`BD · Tune   0 → +24`  
`OH · Filter 0 → -36`

Add a local **Preview amount** slider from `0%` to `100%` that calculates/display the interpolated values in the UI. This is a visualization tool only unless a separate verified live-control feature is implemented.

The UI must not imply that a mouse slider has the same physical pressure behavior as the Rytm pad.

### Quick Performance

If the active Quick Performance assignment can be read reliably, show it as metadata:

`Quick Performance: Perf 3`

Do not prioritize write control for this in the first redesign phase.

---

# PART VI — SAMPLES

## 20. Project Samples page

The Samples page should replace the tiny always-visible sample rail with a full management workspace.

Use three sub-tabs:

1. **Project Slots**
2. **+Drive Browser**
3. **Unused & Cleanup**

Import remains a global top-level destination but can be opened with the current project preselected as the destination.

---

## 21. Project Slots view

Show all 127 project sample slots, not only used slots.

Default presentation: a dense but readable table/list, because 127 items are better managed as rows than as cards.

### Columns

- drag handle
- Slot
- Sample name
- Size
- Uses
- Status
- Actions

Example:

`⠿  003  Crash.wav        84 KB   6 uses   In use`

`⠿  006  Tonal.wav       41 KB   1 use    In use`

`   007  —                —       —        Empty`

### Usage click

Click `6 uses` to open the dependency inspector with all known references.

### Multi-select

Checkboxes appear when selection mode is active. Batch actions may include:

- Unload selected
- Replace
- Copy to project
- Export metadata

Only expose write actions when supported and verified.

### Slot colors

Purple remains the sample semantic color, but rows should not all be purple. Use purple only for icons, selected state, sample pills and traces.

---

## 22. Safe “Unused” semantics

This is a critical product requirement.

Kitsmith must have two distinct terms:

- **No kit-track uses** — the current kit-oriented graph found no direct kit track references.
- **Safe to unload** — the full dependency engine found no references including Sound Locks / Sound Pool paths that can use the sample.

Never label a sample `Unused` in a destructive cleanup flow unless the full dependency scan is complete enough to justify it.

If pattern Sound Locks are not yet parsed, the Cleanup view should say:

`Cleanup safety incomplete — Sound Lock dependencies are not indexed yet.`

and disable `Unload all unused`.

This is better than a fast but unsafe cleanup feature.

---

## 23. +Drive Browser

This view replaces the need to open Elektron Transfer for ordinary sample browsing once file-protocol write support is ready.

### Layout

Two panes:

- folder tree: 220–260 px,
- file list: flexible.

### Header

- current path breadcrumb,
- search,
- `New folder` when supported,
- upload destination marker.

### File row

- drag handle,
- sample icon,
- file name,
- size,
- loaded-in-project indicator when relevant,
- usage/project count if indexed.

### Required drag behaviors

**+Drive sample -> Project sample slot**  
Load/reuse the sample in the exact target slot.

**+Drive sample -> empty area of Project Slots**  
Allocate first available slot after confirmation.

**+Drive sample -> pad**  
Generate a combined Plan:

1. find existing matching project sample slot;
2. if absent, allocate/load one;
3. assign resulting slot to the target Track Sound;
4. read back and verify.

The user should not have to perform `Load to Project` first and then assign to a pad as two unrelated operations.

### File operations roadmap

When protocol support is verified:

- Rename
- Move
- Delete
- Create directory
- Set upload destination

Destructive filesystem actions must show affected project references if known.

---

# PART VII — IMPORT AND SAMPLE PREPARATION

## 24. Import workspace

The current bottom tray is suitable for a proof of concept but not for batch work. Move imports to a dedicated workspace while keeping drag-anywhere as an entry shortcut.

### 24.1 Entry

Dropping one or more audio files anywhere on Kitsmith should:

- add them to the Import queue,
- show a compact toast `4 files added to Import`,
- not cover the project workspace with a permanent tray.

The user may click the toast or Import nav item to open the queue.

### 24.2 Queue table

Columns:

- checkbox,
- file name,
- duration,
- source format,
- converted size,
- target folder,
- destination behavior,
- status.

### 24.3 Batch controls

Top toolbar:

- Select all
- Target +Drive folder
- Naming rule
- `Upload selected`

Destination behavior per batch:

- `Upload only`
- `Upload + add to active project`
- `Upload + assign to selected pad` (only when exactly one compatible target is selected)

### 24.4 Conversion wording

Show:

`Converted for Rytm: 48 kHz · 16-bit · mono`

Keep source metadata available in tooltip/details.

Do not expose resampler implementation detail in the normal UI.

### 24.5 Sample Prep — later sub-phase

A lightweight sample-preparation editor can be added after reliable upload/file management:

- waveform view,
- trim start/end,
- short fades,
- optional normalize,
- rename,
- batch export/upload.

This should remain intentionally small. Kitsmith should not become a DAW.

---

# PART VIII — SOUND POOL AND PATTERNS

## 25. Sound Pool page

The Sound Pool is important because Sound Locks can create dependencies invisible in the kit view.

### Initial scope

Read-only inventory first:

- slot 1–128,
- Sound name,
- machine,
- referenced sample slot,
- usage count in patterns,
- tags if available,
- compatible track family if it can be derived reliably.

### Later actions

After verified write support:

- rename,
- copy Track Sound into pool,
- copy pool Sound to +Drive Sound library,
- delete unused,
- edit tags,
- toggle write protection.

### Important dependency behavior

Clicking a Sound must show:

- patterns/steps that Sound-lock it, if parsed,
- sample it references,
- whether deleting it would affect patterns.

---

## 26. Patterns page

Pattern sequence editing remains out of scope, but pattern relationships should be visible.

### Pattern map

Show 8 banks x 16 patterns in a compact grid.

Each cell:

- `A01`,
- linked kit slot/name,
- Sound Lock count if parsed,
- partial/error indicator if needed.

Clicking a pattern opens an inspector with:

- linked kit,
- tracks containing trigs if known,
- Sound Pool Sounds used through Sound Locks,
- derived sample dependencies.

### Why this page exists

It explains why a kit or sample is “in use” without becoming a sequencer editor.

### Write scope

Pattern-to-kit relinking, pattern copy and sequence editing are explicitly postponed until a safe, verified object write path is designed.

---

# PART IX — CROSS-PROJECT MANAGEMENT

## 27. Cross-project indexing prerequisite

The headline product cannot exist until Kitsmith can inspect non-active projects without manually loading each one on the device.

Required backend milestone:

- download/read `.arprj` or equivalent project data for stored slots,
- parse each project's sample-slot table and the minimum entities required for transfer planning,
- cache a per-project snapshot with completeness metadata.

Until this exists, the UI must not fake cross-project contents.

---

## 28. Project Compare mode

Project Compare is the dedicated high-power workflow for transfer.

### Entry

- Click `Compare` in project header and choose another project.
- Or multi-select exactly two projects in sidebar and choose `Compare`.

### Layout

Two side-by-side project panes:

`SOURCE PROJECT` | `TARGET PROJECT`

Each pane has tabs:

- Kits
- Samples
- Sound Pool
- Patterns (read-only dependency context)

### Global rule

Dragging left -> right defaults to Copy.  
Dragging right -> left also defaults to Copy.  
Move is a separate explicit action after successful copy.

---

## 29. Cross-project sample transfer semantics

On one physical Analog Rytm, the +Drive sample bank is global. Therefore, a sample already present in the +Drive usually does **not** need to be duplicated as a file just to use it in another project.

A target project needs a sample-slot mapping to that +Drive sample.

### Preflight algorithm

When copying a sample from Project A to Project B:

1. Identify the source +Drive sample by stable path/hash/size data available to the protocol.
2. Check whether Project B already loads the same sample.
3. If yes, reuse that target project slot.
4. If no, check free project sample slot count and RAM budget.
5. Allocate a free slot; do not overwrite an occupied slot silently.
6. Load/map the +Drive sample into that slot.
7. Verify target slot identity.

### Preview copy

Show:

`Crash.wav`  
`Source: Project 002 · slot 03`  
`Target: Project 017 · will use slot 22`  
`+Drive file already exists — no file duplicate`

This is the type of bookkeeping Kitsmith should remove from the user's brain.

---

## 30. Cross-project kit transfer semantics

Copying a kit is a dependency-aware operation.

### A kit copy must include

- the kit object,
- all 12 Track Sounds embedded in the kit,
- Scene data,
- Performance data,
- FX/settings contained by the kit,
- a rewritten sample-slot mapping for every track sample reference.

### Preflight algorithm

1. Read the source kit fresh.
2. Collect all sample-slot references used by its 12 Track Sounds.
3. Resolve each source project slot to a +Drive sample identity.
4. For each sample, check whether the target project already loads it.
5. Reuse target slot where possible.
6. Allocate new target sample slots where required.
7. Check 127-slot limit.
8. Check target project 64 MB sample RAM budget using known sample sizes.
9. Choose target kit slot.
10. If target kit slot is occupied, require the user to select another slot or explicitly replace it.
11. Rewrite source sample-slot numbers inside the copied kit to target slot numbers.
12. Create one transaction Plan.
13. Backup target project before first mutation.
14. Execute in dependency order.
15. Read back target kit and sample mappings.
16. Refresh target project snapshot.

### Kit Transfer Plan modal

The modal should contain a clear table:

| Item | Source | Target | Action |
| --- | --- | --- | --- |
| Kit | 02 DEUIL B | 11 DEUIL B | Copy |
| Crash.wav | slot 03 | slot 22 | Add mapping |
| Tonal.wav | slot 06 | slot 06 | Reuse existing |
| DR-Bass5.wav | slot 23 | slot 31 | Add mapping |

Then show:

- `3 sample slots will be added`
- `0 sample files will be duplicated`
- `Target RAM after copy: 41.2 / 64 MB`
- `A backup will be created before writing`

Primary button:

`Copy kit to Project 017`

Secondary:

`Cancel`

Do not use a generic `Execute` label when the user-facing action can be named.

---

## 31. Conflict resolution

The transfer engine should detect and expose conflicts before the device is mutated.

Types:

- target kit slot occupied,
- target project write-protected,
- no free sample slots,
- RAM limit exceeded,
- sample identity unresolved,
- target project data incomplete,
- incompatible Sound/track,
- protocol capability unavailable.

### Conflict UI

Use a dedicated `Needs attention` section in the Plan modal.

Examples:

`Kit slot 11 is occupied by ACID KIT.`  
Options: `Choose another slot` / `Replace`.

`Project has only 2 free sample slots; this kit needs 3.`  
Options: `Review safe cleanup` / `Cancel`.

Never hide conflict resolution inside a tooltip.

---

# PART X — NAMING, LABELS AND LOCAL METADATA

## 32. Native names vs local aliases

Kitsmith should support two naming layers:

### Native device name

A name the device actually stores and can read/write.

Examples:

- project name,
- kit name via save/rename semantics,
- Track Sound name,
- sample file name,
- Sound library name.

### Kitsmith local alias

A label that improves organization but is not necessarily representable on the device.

Useful for:

- Scene 1 -> `Breakdown`
- Perf 3 -> `Kick Pressure`
- local project notes,
- favorites,
- tags across devices.

The UI must visibly distinguish an alias from a native rename when editing.

Example field label:

`Kitsmith label (local only)`

### Never lie about writes

If a device concept cannot be renamed natively, do not use a plain `Rename` action. Use `Add local label`.

---

## 33. Name resolution rules

Display name precedence:

1. local alias, when the user explicitly chose it as display label;
2. native device name;
3. deterministic semantic fallback.

A local alias should still expose the native name in inspector/details.

### Fallbacks

Project: `Project 042`  
Kit: `Kit 014`  
Track Sound: `BD sound`  
Sample: `Sample slot 058`  
Sound Pool: `Pool Sound 031`

### Unresolved metadata

If a name should exist but a protocol phase failed, add a small metadata state:

`Name unavailable`

Do not fabricate a probable name.

### Data-quality panel

Diagnostics may show:

- Project names: `46/128 resolved`
- Sample names: `27/31 used slots resolved`
- Object replies: `380/384`

This belongs in Diagnostics/Activity, not beside every main control.

---

## 34. Local metadata storage

Add a versioned local metadata store separate from device snapshots.

Suggested metadata:

- aliases,
- notes,
- favorite flags,
- local tags,
- optional transfer provenance.

Key metadata by a stable combination such as:

- device identity,
- project slot,
- entity kind,
- entity slot,
- project/content fingerprint where possible.

If Project slot 17 is overwritten with a different project, old aliases must not silently attach to unrelated new content. Use a project fingerprint/change detector and mark old metadata as detached when identity is uncertain.

---

# PART XI — INTERACTION SPECIFICATION

## 35. Drag-and-drop vocabulary

Every draggable object has:

- a drag handle,
- a drag ghost,
- compatible drop targets,
- a non-drag menu alternative.

### Sample drag ghost

Show:

- sample icon,
- sample name,
- source slot/path context.

Example:

`Crash.wav · Project slot 03`

### Kit drag ghost

`Kit 02 · DEUIL B`

### Track Sound drag ghost

`BD · HARD KICK`

### Drop target feedback

On drag hover, a destination should state the resulting action:

- `Assign to BD`
- `Copy kit to Project 017`
- `Load into sample slot 22`

Do not rely only on a border color.

---

## 36. Context menus as accessibility fallback

Every drag operation must have an equivalent explicit action:

Sample:

- Assign to track...
- Copy to project...
- Replace slot...

Kit:

- Copy to project...
- Copy to kit slot...
- Compare...

Track Sound:

- Copy to track...

This also makes the product usable for users who dislike drag-and-drop.

---

## 37. Confirmation philosophy

Not every action needs a modal. Every **device mutation with dependency consequences** does.

### No modal needed

- navigation,
- selecting a track,
- tracing usage,
- local alias edit,
- filtering/searching,
- preparing an import queue.

### Plan preview required

- assigning/replacing a project sample slot,
- replacing a Track Sound,
- copying kit/project data,
- unload/delete,
- rename on device,
- write-protection changes,
- project load if it can overwrite active working state.

### Modal wording

Use user intent as title:

`Assign Crash.wav to BD?`

not:

`Confirm change`

Then show protocol-level steps only in a collapsible `Technical steps` section.

Default visible summary should be human-readable.

---

## 38. Toasts and completion feedback

Successful one-entity writes should end with a brief toast:

`Crash.wav assigned to BD`

with `View details` for backup/verification logs.

Large batch operations stay visible in Activity until complete.

Failures should include:

- what failed,
- what completed,
- whether anything after failure ran,
- where the backup is,
- suggested next action.

Example:

`Kit copy stopped before writing the kit. 2 sample mappings were added successfully. The kit was not changed. Backup is available.`

This is more useful than a raw error string.

---

# PART XII — VISUAL DESIGN SYSTEM

## 39. Theme

Kitsmith should use an explicit app-owned theme.

### Default: Dark

Suggested tokens:

- App background: `#0D0F12`
- Sidebar/background-2: `#12151A`
- Surface: `#181C22`
- Elevated surface: `#1E232B`
- Hover: `#252B35`
- Border: `#303743`
- Primary text: `#F3F5F7`
- Secondary text: `#A6AFBA`
- Muted text: `#747E8A`
- Sample purple: keep current semantic family around `#9B59B6`, optionally increase luminance for dark contrast
- Scene blue: around `#4DA3FF`
- Performance green: around `#38C977`
- Success: `#2ECC71`
- Warning: `#F2B84B`
- Danger: `#E16464`

Track colors can retain the current warm-to-cool identity system but should be tested for dark-background contrast.

### Light theme

Optional setting. Do not derive it by inverting the dark theme. Define separate tokens if implemented.

---

## 40. Typography

Use the macOS/system proportional sans where practical.

Minimum normal text sizes:

- Page title: 22–24
- Section title: 15–16 semibold
- Primary card text: 15–16
- Body/table: 13–14
- Metadata: 12–13

**Do not use main-screen text below approximately 12 px equivalent.**

The current pilot's tiny gray metadata should be removed or moved to tooltip/inspector.

---

## 41. Component sizing

Desktop minimums:

- Default window: ~1280 x 800.
- Minimum supported: ~1024 x 640.
- App bar: 52 px.
- Sidebar: 240–280 px.
- Inspector: 300–340 px.
- Main buttons: 34–38 px height.
- Icon-only hit target: at least 30–32 px.
- List row: 40–48 px.
- Kit list row: 52–60 px.
- Table row: ~40–44 px.
- Pad card: ~170–220 x 108–128 px at normal widths.
- Content gaps: 8 / 12 / 16 / 24 token system.

Use responsive constraints rather than hard-coded one-window coordinates.

---

## 42. Icons

Use icons consistently, but pair important icons with text.

Required concepts:

- drag handle: six dots
- sample: waveform/file-wave icon
- kit: 3x4 pad/grid icon
- project: stacked document/folder icon
- sync: circular arrows
- import: arrow into tray
- usage/reference: link/branch icon
- Scene: blue mode icon
- Performance: green pressure/gauge icon
- lock: padlock
- warning: triangle
- diagnostics: wrench/info

Avoid adding a large icon dependency solely for decoration. If a small egui-compatible icon set is introduced, document it and keep semantic usage centralized in `gui/icons.rs` or equivalent.

---

## 43. Color is not enough

Every state that has color also needs a textual/icon cue.

Examples:

- Scene = blue + `Scene` label.
- Performance = green + `Perf` label.
- write-protected = lock icon + tooltip.
- partial sync = amber + `Partial` text.
- active project = green accent + `ACTIVE` label.

---

# PART XIII — SYNC, OFFLINE AND ACTIVITY

## 44. Cache-first becomes an explicit product feature

The current cache-first architecture is valuable. Make it visible and trustworthy.

When disconnected, users should still be able to browse cached projects and plan work that does not require execution.

Project header state examples:

- `LIVE · synced 2 min ago`
- `CACHED · device disconnected`
- `PARTIAL · 4 items unavailable`

Do not make cached data look live.

---

## 45. Activity center

All long operations should appear in one consistent Activity center:

- device sync,
- project indexing,
- WAV conversion,
- sample upload,
- file transfer,
- transaction Plan execution,
- backup,
- verification.

Each task row:

- clear user-facing title,
- current phase,
- progress,
- cancel if safe,
- result.

Example:

`Copy Kit 02 to Project 017`  
`Verifying target kit… 4/5`

Raw protocol logs are expandable.

---

## 46. Serialize device I/O

The current architecture allows separate workers to open their own MIDI links. For the next phase, introduce a **DeviceSession / I/O coordinator** that serializes request/reply traffic across sync, file operations and writes.

Reasons:

- object protocol and file protocol share USB-MIDI SysEx transport,
- concurrent request/reply streams can make reply ownership ambiguous,
- cross-project transfer will involve many ordered steps,
- the UI needs one coherent task state.

### Required behavior

- one device command queue,
- one active transactional writer at a time,
- sync/import/write tasks cannot blindly interleave SysEx,
- cancellation is cooperative between safe boundaries,
- watcher/detection remains independent.

This refactor should happen before complex multi-protocol transfer plans.

---

# PART XIV — DOMAIN AND ARCHITECTURE EVOLUTION

## 47. Separate device index from active project model

The current `ProjectModel` is suitable for the active project but cross-project management needs a device-wide index.

Suggested conceptual models:

### `DeviceIndex`

- device identity,
- OS version,
- project slots 1..128,
- +Drive sample tree metadata,
- global Sound library metadata when implemented,
- sync timestamps/completeness.

### `ProjectSnapshot`

- project slot/id,
- native name,
- write-protection state if available,
- kits,
- patterns,
- Sound Pool,
- sample slots,
- Scenes/Performance data when parsed,
- dependency graph,
- completeness flags,
- content fingerprint.

### `SampleFileRef`

Represents +Drive file identity separately from project slot assignment:

- path,
- name,
- hash/size or stable protocol identity,
- file size.

### `ProjectSampleSlot`

- slot 1..127,
- optional `SampleFileRef`,
- load/usage state.

This distinction is essential for correct cross-project transfers.

---

## 48. Build a first-class dependency graph

Do not continue adding ad-hoc `slot_usage()` helpers for every feature.

Create a reusable graph/index with typed edges.

Potential node kinds:

- Project
- Pattern
- Kit
- Track
- Track Sound
- Sound Pool Sound
- Project Sample Slot
- +Drive Sample File
- Scene
- Performance macro

Important edge kinds:

- Pattern `uses kit`
- Pattern `sound-locks pool Sound`
- Kit `contains track Sound`
- Track Sound `references sample slot`
- Pool Sound `references sample slot`
- Project sample slot `maps to +Drive sample`
- Scene `modulates track/FX parameter`
- Performance `modulates track/FX parameter`

### Graph APIs

Examples:

- `usages_of_sample_file()`
- `usages_of_project_sample_slot()`
- `patterns_using_kit()`
- `sound_locks_using_pool_sound()`
- `can_safely_unload_sample_slot()`
- `dependencies_for_kit_transfer()`

Each API should return both references and **completeness**, e.g. `Complete` / `Incomplete(reason)`.

A cleanup UI must not treat incomplete dependency data as empty dependency data.

---

## 49. Capability-driven device architecture

Long-term support for multiple Elektron devices should not force the Analog Rytm model into every product.

Introduce a device adapter/capability layer gradually.

Conceptual interface:

```text
ElektronDeviceAdapter
  identity()
  capabilities()
  sync_device_index()
  sync_active_state()
  build_plan(user_intent)
  execute_step(step)
```

Capabilities might include:

- projects
- project-file read
- project-file write
- sample +Drive browser
- project sample slots
- kit-like presets
- pad-grid renderer
- Sound Pool
- Scenes
- Performance macros
- direct project load

The shared UI shell asks for capabilities. Device-specific workspace code can still exist, e.g. `gui/rytm/`.

Do **not** prematurely create a fake universal `Kit` abstraction if another Elektron device does not have the same semantics.

---

## 50. Generalize the Plan system, do not bypass it

All new writes should compile from user intent into a Plan.

New high-level intents may include:

- `AssignSampleToTrack`
- `LoadSampleIntoProject`
- `ReplaceProjectSampleSlot`
- `CopyTrackSound`
- `CopyKitAcrossProjects`
- `RenameProject`
- `RenameSampleFile`
- `UnloadProjectSample`
- `ToggleProjectProtection`

Plans should contain explicit low-level steps and preconditions.

### Plan lifecycle

1. user intent,
2. preflight read,
3. dependency resolution,
4. conflict detection,
5. human preview,
6. backup,
7. mutation,
8. read-back verification,
9. cache/model refresh,
10. completion report.

### Plan guarantees

- no mutation before preflight passes,
- no silent overwrite,
- backup before first mutation,
- stop at first failed step,
- report exactly which steps succeeded,
- refresh any modified model after completion.

---

## 51. Backups should gain a manifest

The existing raw `.syx` backup is valuable. Add a small JSON manifest beside each backup containing:

- timestamp,
- device identity/OS,
- active project name/slot if known,
- user intent that caused the backup,
- objects included,
- app version.

Activity can then say:

`Backup created · before “Copy Kit 02 to Project 017”`

instead of exposing a cryptic filename only.

---

# PART XV — IMPLEMENTATION WORK PACKAGES

The following work packages are intentionally sized so Claude Code can implement them as separate branches/PRs. Do not combine all of this PRD into one rewrite.

## WP0 — Hardware verification gate

**Goal:** establish which existing read/write assumptions are real on AR MKII OS 1.72 before building more write UI.

### Scope

- Verify object sync for kits/patterns/Sounds.
- Verify sample-off values.
- Verify file-protocol project/sample-name reads.
- Verify RAM slot table/path lookup or implement fallback.
- Verify one scratch-kit sample-slot write with backup/readback.
- Verify actual sample upload path used by current code.

### Deliverable

A short `docs/hardware-verification.md` matrix:

`feature | status | observed bytes/behavior | fallback`

### Acceptance

No new risky write feature starts until its dependency is marked verified.

---

## WP1 — UI foundation and design system

**Goal:** make the app readable and structured without changing device behavior.

### Scope

- enforce dark default,
- add explicit design tokens in `theme.rs`,
- default window ~1280x800, minimum ~1024x640,
- implement top app bar,
- implement left navigation shell,
- move debug ports into Device & Diagnostics,
- implement consistent buttons/list rows/cards,
- implement drag-handle component,
- remove `synth` label,
- raise font sizes,
- improve partial-sync banner.

### Acceptance

- No visible `synth` chip anywhere.
- No primary screen text smaller than ~12 px equivalent.
- App launches dark regardless of macOS appearance unless user explicitly selected another Kitsmith theme.
- Device status and Sync are understandable without hover.
- Raw MIDI port names are not primary navigation.

---

## WP2 — Naming and completeness model

**Goal:** stop conflating missing, unresolved and local names.

### Scope

- model native names as optional/quality-aware values where needed,
- deterministic fallbacks,
- name-resolution completeness flags,
- data-quality summary for Diagnostics,
- versioned local metadata store,
- local aliases/notes/favorites,
- project fingerprint safeguard for local metadata.

### Acceptance

- unresolved sample names show `Slot N · name unavailable`, not invented names,
- local-only labels are clearly marked local when edited,
- replacing a project slot does not silently reuse stale aliases if fingerprint changes.

---

## WP3 — Project browser and cache architecture

**Goal:** make projects the primary navigation object.

### Scope

- new `DeviceIndex`,
- project sidebar list 1..128,
- search/filter,
- active/locked/cached/indexed states,
- selected project vs active-on-device distinction,
- per-project cache directory structure,
- Overview page.

### Acceptance

- User can tell which project is merely selected and which is active on hardware.
- Project list remains usable with 128 entries.
- Clicking a project never silently loads it on the device.
- Offline cached browsing is visibly marked `CACHED`.

---

## WP4 — Kits workspace redesign

**Goal:** make the 12-pad view the polished core workspace.

### Scope

- kit list pane with In use / All / Unused filters,
- kit search,
- kit header with linked pattern summary,
- responsive 12-pad grid,
- new card anatomy,
- clear sample pills,
- track inspector,
- sample trace integration,
- contextual menu alternatives to drag.

### Acceptance

- At 1280x800 the kit list, full 3x4 pad grid and inspector can be used without tiny text.
- No sample slot has an unexplained purple number-only rectangle.
- A user can point to every visible item and tell whether it is a kit, track, Sound, machine, sample slot or sample file.
- `All 128` reveals unused kit slots.

---

## WP5 — Project Samples workspace

**Goal:** replace the small sample rail with a real slot manager.

### Scope

- Project Slots table 1..127,
- usage inspector,
- empty/in-use states,
- multi-select framework,
- cross-navigation to kit pads,
- safe-vs-incomplete cleanup wording.

### Acceptance

- User can see all 127 slots and free slots.
- Usage count opens concrete references.
- Destructive `unused` operation is disabled when dependency completeness is insufficient.

---

## WP6 — Device I/O coordinator + Activity center

**Goal:** make complex operations safe and observable.

### Scope

- centralized DeviceSession command queue,
- serialize SysEx request/reply operations,
- Activity model/UI,
- migrate sync, upload and Plan execution onto the coordinator,
- cancellation at safe boundaries,
- backup manifest.

### Acceptance

- A sync and a write cannot interleave requests on the same device.
- All long operations show consistent progress in Activity.
- Completion reports survive navigation away from the initiating screen.

---

## WP7 — +Drive browser read UX

**Goal:** browse global samples from Kitsmith.

### Scope

- folder tree,
- file list,
- search,
- upload-here marker,
- loaded-in-current-project indicator,
- drag affordances without writes yet if file write protocol is not ready.

### Acceptance

- Sample file and project sample slot are visually distinct concepts.
- User can trace a project slot back to its +Drive file.

---

## WP8 — Import workspace and batch upload

**Goal:** eliminate the prototype bottom tray and support real batch use.

### Scope

- global Import queue,
- drag-anywhere ingestion,
- batch selection,
- target folder,
- conversion status,
- upload task integration with Activity,
- optional `Upload + add to active project` when slot-loading op exists.

### Acceptance

- 20+ dropped files remain manageable.
- User can set one destination for the batch.
- Leaving the Import page does not hide/cancel running work.

---

## WP9 — File-protocol write slice

**Goal:** implement the missing operations that make Kitsmith replace Transfer for sample management.

### Scope — only after protocol verification

- create directory,
- rename/move/delete sample file,
- load +Drive sample into project slot,
- replace/unload project sample slot,
- verification reads,
- all writes exposed as Plans.

### Acceptance

- `+Drive sample -> pad` can compile into a verified multi-step Plan.
- No file delete is allowed if dependency state is unknown without a strong explicit override path.

---

## WP10 — Full dependency graph including Sound Locks

**Goal:** make “what uses what” correct enough for cleanup and transfer.

### Scope

- parse Pattern Sound Lock references,
- connect Sound Pool Sounds to sample slots,
- typed dependency graph,
- completeness reporting,
- update all usage views,
- safe cleanup eligibility.

### Acceptance

A sample referenced only by a Sound Pool Sound used through a Pattern Sound Lock must be reported as used.

---

## WP11 — Non-active project indexer

**Goal:** inspect project contents without loading each project on hardware.

### Scope

- `.arprj` download/read,
- parser for sample-slot table first,
- then minimum kit/related data required for transfer,
- per-project snapshots,
- background indexing with Activity progress,
- cache completeness.

### Acceptance

- User can select Project A and Project B and see their sample mappings while neither selection operation changes the hardware's active project.

---

## WP12 — Project Compare + cross-project sample copy

**Goal:** first true cross-project workflow.

### Scope

- Compare UI,
- drag sample between projects,
- reuse target slots by sample identity,
- allocation/preflight,
- capacity checks,
- transaction preview,
- verification.

### Acceptance

- Copying a sample that already exists in target does not create a redundant project slot unless user explicitly asks.
- Copying a sample never duplicates the +Drive file merely because the target project differs.

---

## WP13 — Cross-project kit copy

**Goal:** headline feature.

### Scope

- kit dependency collector,
- target sample mapping planner,
- target kit slot selection,
- reference rewriting,
- Plan preview table,
- backup/execute/verify,
- refresh target project snapshot.

### Acceptance

- A copied kit sounds with the intended samples because source project slot numbers were translated to valid target project slot numbers.
- Occupied target kit slots are never overwritten silently.
- RAM/slot capacity is checked before write.

---

## WP14 — Compare Kits + Track Sound copy

**Goal:** make kit-to-kit work fast and visual.

### Scope

- dual 12-pad grids,
- pad-to-pad drag,
- track compatibility check,
- sample dependency handling,
- Plan preview.

### Acceptance

- User can copy one track Sound from one kit to another without manually managing the sample slot.

---

## WP15 — Scenes and Performance read visualization

**Goal:** surface important kit content hidden on hardware.

### Scope

- parse/read Scene locks,
- parse/read Performance locks,
- Tracks/Scenes/Performance mode switch,
- 12-card blue Scene grid,
- 48-lock Scene budget,
- 12-card green Performance grid,
- local aliases,
- inspectors.

### Acceptance

- Scenes and Performances are visibly part of the kit.
- Scene lock count matches hardware data on verified test kits.
- No write controls are shown until write mapping is proven.

---

## WP16 — Scene/Performance management writes, gated

**Goal:** desktop editing/copying only after verified model round-trip.

### Possible scope

- copy/paste/clear Scene,
- copy/paste/clear Performance macro,
- edit individual lock values/depths,
- read-back verify complete kit.

### Gate

Do not start until byte-level round-trip tests and scratch-kit hardware verification exist.

---

## WP17 — Sound Pool manager

**Goal:** manage the layer required by Sound Locks and reusable Sounds.

### Scope

- 128-slot Sound Pool inventory,
- usage counts,
- Sound Lock trace,
- rename/copy/delete/tag/protect after verified writes,
- drag Sound to compatible pad as a future Plan.

### Acceptance

- Deleting a pool Sound with Pattern Sound Lock usage is blocked or clearly warns with exact affected patterns.

---

# PART XVI — ACCEPTANCE TESTS AND UX QA

## 52. Core usability scenarios

### Scenario A — Understand the current kit

Given a synced active project, the user should be able to answer in under 10 seconds:

- Which kit is selected?
- Which patterns use it?
- What is on BD?
- Which machine is BD using?
- Is a sample assigned?
- Which sample file and slot is it?

No tooltip should be required for these six questions.

### Scenario B — Find every use of a sample

1. Open Project Samples.
2. Search `Crash`.
3. Click `6 uses`.
4. See exact kits/tracks and any Sound Lock-based pattern uses.
5. Jump to a usage.

If Sound Lock indexing is unavailable, the UI must explicitly say the result is incomplete.

### Scenario C — Assign a +Drive sample to a track

1. Open +Drive.
2. Drag `Kick Dusty.wav` to BD in a visible kit destination or use `Assign to track...`.
3. Kitsmith detects whether the sample is already loaded into the project.
4. Preview explains slot reuse/allocation.
5. Backup/execute/verify.
6. BD card updates immediately.

The user never manually types a sample slot number unless they choose an advanced override.

### Scenario D — Copy a kit to another project

1. Drag Kit 02 `DEUIL B` onto Project 017.
2. Target project highlights as compatible.
3. Transfer Plan lists kit destination and sample dependencies.
4. Any conflicts are resolved before execution.
5. Kitsmith copies mappings and kit, verifies, and shows success.

No manual “remember that Crash is slot 3 in source but slot 22 in target” step.

### Scenario E — Batch import

1. Drop 30 WAV files.
2. Files appear in Import queue.
3. Select all.
4. Set target folder once.
5. Start upload.
6. Continue browsing projects while upload runs.
7. Activity shows status.

### Scenario F — Offline planning

1. Launch with device disconnected.
2. Browse cached Project 002.
3. View kit/sample relationships.
4. UI clearly states `CACHED`.
5. Device-mutating buttons are disabled with explanation, not silently absent.

---

## 53. Visual QA checklist

Every UI PR should be reviewed at:

- 1024x640,
- 1280x800,
- 1680x900 or larger,
- disconnected state,
- empty cache state,
- partial sync state,
- long project/kit/sample names.

Checklist:

- no clipped text,
- no important 9–10 px labels,
- no unexplained icon-only primary actions,
- no state communicated by color alone,
- no giant unused blank region while lists are cramped,
- no horizontal scrolling in normal Kits workspace at 1280x800,
- no `synth` badge,
- no ambiguous `projects (N)` menu as the primary project manager,
- every draggable entity has a visible drag affordance,
- every drop target states the action during hover.

---

## 54. Write-safety QA checklist

For every new mutating Plan:

- preflight uses fresh target data,
- backup occurs before first write,
- occupied slots are detected,
- write-protected targets are blocked,
- capacity limits are checked,
- target identities are resolved,
- write is read back,
- mismatch fails the Plan,
- no later step executes after failure,
- UI reports exactly what changed,
- modified snapshots are refreshed,
- integration tests use MockBackend or protocol fixtures where hardware is not required.

---

# PART XVII — PRODUCT SCOPE BOUNDARIES

## 55. Explicitly out of scope for this redesign

- DAW-like sequencer editing.
- Full synth parameter editor for every Track Sound.
- Overbridge protocol / multitrack audio / mixing. (Amended 2026-09-13: a plain
  class-compliant *stereo* monitor — hear the device's main outs through the
  computer — is IN scope as part of WP18; it is read-only and involves no
  Overbridge protocol work.)
- Live multitrack audio mixer.
- Automatic destructive cleanup based on incomplete dependency data.
- Remote project switching until the load/swap operation is understood and verified.
- Pretending non-active project contents are known before `.arprj`/project indexing exists.

---

## 56. Later opportunities

After the management foundation is solid:

- Sound Library browser across banks A–P.
- Local tags/favorites across samples and projects.
- `Kit Bundle` export/import: kit + dependency manifest + samples.
- WAV export/download.
- Duplicate sample detection by identity/hash.
- Cleanup assistant across all indexed projects.
- Songs overview.
- Global-slot comparison.
- Cross-device transfer when more Elektron adapters exist.
- Sample Prep waveform editor.
- Verified direct project load from Kitsmith.

---

# PART XVIII — COPY AND MICROCOPY

## 57. Required user-facing terminology

Use these exact concepts consistently:

- **Project** — saved/working top-level device state.
- **Kit** — 12 Track Sounds + kit settings/Scene/Performance/FX.
- **Track Sound** — the Sound embedded in a kit track.
- **Sound Pool** — 128 per-project Sound slots used for Sound Locks.
- **+Drive Sound Library** — device-wide Sound library.
- **Project sample slot** — one of 127 loaded sample references in a project.
- **+Drive sample** — sample file in global +Drive storage.
- **Scene** — fixed-value kit macro, blue.
- **Performance** — pressure-relative kit macro, green.

Avoid generic `object`, `entity`, `slot item` in user-facing UI.

---

## 58. Preferred microcopy examples

Instead of `Confirm change`:

`Assign Crash.wav to BD?`

Instead of `Execute on device`:

`Assign sample`

Instead of `sync complete — 4 object(s) did not answer`:

`Partial sync · 4 items unavailable`

Instead of `(name unknown—sync)`:

`Name unavailable`

Instead of `synth`:

`No sample` or nothing.

Instead of `projects (46)`:

Persistent `PROJECTS` sidebar with rows.

Instead of `slot 3` as the main sample label:

`Crash.wav` with secondary `Slot 03`.

Instead of `x6` without explanation:

`6 uses`.

---

# PART XIX — CLAUDE CODE IMPLEMENTATION RULES

## 59. Do not rewrite working protocol code just to fit the new UI

The first redesign PRs should be able to render from current `ProjectModel` plus adapters/view-models. Keep protocol work isolated.

Use view models where the UI needs richer states than the wire model.

---

## 60. Preserve transaction safety as a hard invariant

No GUI widget may call a write transport directly.

All writes:

`UI intent -> Plan builder -> Preview -> DeviceSession -> Backend -> Verify -> Report`

If a feature cannot fit this route yet, it is not ready to ship.

---

## 61. Capability-gate UI instead of leaving dead controls

There are three valid UI states:

1. capability available and verified -> enabled;
2. capability conceptually exists but unavailable -> disabled with explanatory tooltip only if discoverability matters;
3. capability not part of this device -> do not show it.

Do not show dozens of permanently disabled future buttons.

---

## 62. Keep protocol uncertainty visible in code

Every assumption from community reverse engineering should stay documented and tested. New protocol work should include:

- fixture tests,
- round-trip tests where possible,
- hardware verification notes,
- fallback behavior.

Do not infer undocumented write opcodes from neighboring read opcodes.

---

## 63. Prefer incremental PRs

For each work package:

1. branch from the correct current tip,
2. keep one user-visible concept per PR when possible,
3. include before/after screenshots in PR description,
4. run fmt/clippy/tests,
5. update PRD/status docs only for what is actually implemented.

The UI redesign should not claim cross-project functionality before the indexer and Plan engine support it.

---

# PART XX — SOURCE NOTES

This document is a future-state product specification built on the current Kitsmith milestone PRD and Analog Rytm MKII OS 1.72 documentation.

Primary sources used for device facts and feature opportunities:

- Existing Kitsmith PRD / system description, version 2026-09-05.
- Elektron Analog Rytm MKII User Manual, OS 1.72 (official PDF):
  https://elektron.se/wp-content/uploads/2025/01/Analog-Rytm-MKII-User-Manual_ENG_OS1.72_250130.pdf
- Elektron support: “The difference between Sounds and Samples”.
- Elektron support: sound-pack/sample-transfer documentation.

Important manual-derived facts used in this PRD include:

- 128 projects on +Drive.
- per-project 128 patterns, 128 kits, 16 songs, 4 globals, 128 Sound Pool slots, 127 sample slots and 64 MB sample RAM.
- kit contains Scene and Performance settings.
- 12 Scenes per kit; Scene locks use fixed values; 48 lock total across the kit's Scenes.
- 12 Performance macros per kit; modulation amount responds to pad pressure.
- Sound Pool Sounds can be selected by Pattern Sound Locks.
- sample manager concepts include LOAD TO PROJ, VIEW RAM, REPLACE, UNLOAD and removing unused project samples.
- project manager concepts include rename, load/save, protection, initialize and delete/clear.

---

# FINAL PRODUCT DEFINITION

Kitsmith should feel like a **visual dependency-aware librarian for Elektron hardware**.

The 12-pad grid remains the musical center of the Analog Rytm experience, but it sits inside a stronger management system:

- projects are always easy to find,
- samples are named and traceable,
- slots are visible but not the user's burden,
- Scenes and Performances are understandable,
- batch import is first-class,
- cross-project copy is drag-and-drop with automatic dependency mapping,
- every write is previewed, backed up and verified,
- partial or uncertain data is never disguised as certainty.

When the product reaches the cross-project kit-transfer milestone, the user should be able to perform in seconds what currently requires a careful manual sequence of project switching, sample-slot bookkeeping and transfer utilities. That is the defining value of Kitsmith.

---

# ADDENDUM (2026-09-12) — FULL-MANAGEMENT DIRECTION

The product goal expands from librarian to **full Rytm management**: everything
the device allows should be creatable, editable, playable and analyzable from
Kitsmith. The per-operation feasibility/status ledger is
`docs/management-matrix.md`; the sound-design ground truth is the
`ar-sound-machines` skill. WP0–WP17 stand unchanged; the following work packages
extend them.

## WP18 — Play surface (MIDI, zero write risk)

Play pads from the app (notes 0–11 on channels 1–12, velocity from click/drag),
chromatic preview of the active track, kit audition sequences that respect
voice-pair chokes (BT/LT, MT/HT, CH/OH, CY/CB), scene switching via CC 92 and
performance macros via CC 35–47 (sliders emulate pad pressure), track
mute/solo/level via CC 93–95. Local WAV preview of samples works with the device
unplugged. Live CC edits are working-state only and the UI never presents them
as saved. Spec source: `docs/research/midi-control-map.md`.

Also in this package (added 2026-09-13): **live audio monitoring** — the Rytm's
class-compliant USB audio input (USB CONFIG → USB AUDIO/MIDI, OS 1.50+) passed
through to the computer's default output (`audio::monitor`), so pressing Play
on the device — or a pad from the app — is heard on the computer. Stereo main
outs only; read-only; no Overbridge protocol. Toggle in the top bar, status and
glitch counters under Device & Diagnostics; when the Rytm's MIDI is visible but
its audio isn't, the UI explains the USB CONFIG setting instead of failing
mutely.

## WP19 — Kit creation and editing (the headline write)

Create a kit from scratch in the app and save it to a chosen kit slot: machine
selection per track constrained by the machine/track validity table, SRC/SMPL/
FLTR/AMP/LFO parameter editing with musically sane defaults, FX track settings,
sample-slot assignment from the project pool. Gate: kit object byte-level
round-trip on OS 1.72 (already proven offline via libanalogrytm re-encode; needs
scratch-slot hardware verification). Renaming kits and Sounds ships with this
package. Saving to a kit slot that patterns reference is a REPLACE and must be
labeled with the affected pattern list.

## WP20 — Bulk sample operations

Multi-file upload queue (extends WP8) and **bulk delete** with a cross-project
reference scan: the plan lists every file and every project/kit that references
its hash before anything is removed. Bulk rename of samples and directories.
Every item is an individual ops/ Plan step — serialized, abortable, individually
verified; one failure never misaligns the rest of the queue.

## WP21 — Rename & reorder everything

One consistent rename surface for projects (native, verified — WP9), kits,
Sounds, patterns, samples and directories. Reorder for whatever is reorderable:
project slots, kit slots, sample slots, pattern slots — always as
reference-rewriting transactions (a slot move rewrites every referrer in the
same plan), never raw swaps. Entities whose order the device fixes are shown as
fixed, not silently unreorderable.

## WP22 — Analysis area

A dedicated read-only workspace built on the dependency graph (WP10) and the
cross-project index (WP11): duplicate/shared samples across projects (by content
hash), unused samples per project and +Drive-wide, storage breakdown against the
64 MB / 127-slot / 1 GB budgets, and project-health checks (broken references,
name gaps, empty-but-referenced slots). Zero device writes; every finding links
to the management action that fixes it.

## Sequencing note

WP18 and WP22 carry no write risk and may proceed ahead of hardware write
verification. WP19–WP21 remain behind the WP0 write gate and the
backup-first/plan/verify pipeline without exception.
