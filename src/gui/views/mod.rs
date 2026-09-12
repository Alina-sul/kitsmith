//! Workspace views routed by the left sidebar navigation.

/// Payload carried by a sample-slot drag; pads accept it as a drop.
pub(crate) struct SlotDrag {
    pub slot: u8,
}

pub mod activity;
pub mod diagnostics;
pub mod drive;
pub mod import_page;
pub mod kits;
pub mod overview;
pub mod patterns;
pub mod pool;
pub mod project;
pub mod samples;
