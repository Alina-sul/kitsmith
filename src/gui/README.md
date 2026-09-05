# gui — the desktop shell

The egui UI restructured per `docs/PRD-v2.md` Part II: a persistent app shell
(top bar + left navigation) around routed workspace views. No GUI code talks
to a write transport directly — user gestures compile into `ops::Plan`s and go
through the preview modal.

## Pieces

| Module | Job |
| --- | --- |
| `app.rs` | App state, navigation routing, drag-anywhere ingestion, toasts, activity log |
| `theme.rs` | All design tokens (dark palette, typography scale, layout sizes) + `apply()` |
| `top_bar.rs` | 52 px app bar: device pill, breadcrumb, sync states, Import shortcut |
| `sidebar.rs` | Top-level navigation + the project browser (search, slot rows) |
| `views/project.rs` | Project workspace header, LIVE/CACHED/PARTIAL state, tab row |
| `views/kits.rs` | Kit list (filters/search), 12-pad grid, track inspector, sample rail |
| `views/samples.rs` | All 127 project sample slots as a table + usage inspector |
| `views/patterns.rs` | 8×16 pattern map with kit links + dependency inspector |
| `views/pool.rs` | Sound Pool inventory with Sound-Lock usage |
| `views/overview.rs` | Project stat cards + completeness notes |
| `views/drive.rs` | +Drive files known from loaded slots (full browser is a later milestone) |
| `views/import_page.rs` | Batch import queue table |
| `views/activity.rs` | Running work + session history |
| `views/diagnostics.rs` | Device state, MIDI ports, data-quality panel |
| `widgets.rs` | Shared pills/badges/rows/inspector groups |
| `execute.rs` | Plan preview modal (intent-named) + execution worker |
| `import.rs` | Import queue model + SDS upload workers |
| `backup.rs` | Full-device project backup worker (data-read protocol) |
| `transfer_ctl.rs` | Pattern-capture worker for cross-project transfer |
| `sync.rs` | Sync worker controller |

## Conventions

- Every color/size comes from `theme.rs`; no inline magic values.
- Name rendering goes through `model::names` (never fabricate a name).
- Colored states always carry a textual cue too (PRD §43).
- Capability-gated controls are disabled with an explanatory tooltip, or not
  shown at all — never silently dead.
