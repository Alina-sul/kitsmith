//! Read-only analysis over the model and the local project backups.
//!
//! Seeds the WP22 analysis area. Everything here is pure over injected data —
//! no device I/O, no cache reads — so ops/ and the GUI can call it with
//! whatever view of the world they hold, and tests feed it synthetic data.

pub mod refscan;

pub use refscan::{DriveRefScan, ProjectHit};
