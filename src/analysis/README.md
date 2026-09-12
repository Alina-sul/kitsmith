# analysis/

Read-only analysis over the model and local project backups — the seed of the
WP22 analysis area. Everything is pure over injected data (no device I/O, no
cache reads inside the module), so callers pass whatever view of the world
they hold and tests feed synthetic payloads.

- `refscan.rs` — cross-project reference scan for one +Drive sample by
  content identity (hash + size): active-project slots, backed-up projects'
  slot tables, and the honest blind spot (device projects with no scannable
  local backup). Gates `Plan::delete_drive_file`.
