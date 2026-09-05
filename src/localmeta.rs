//! Versioned local metadata store (PRD v2 §32–§34): labels that live in
//! Rytmdash, not on the device.
//!
//! No spec'd protocol operation renames a +Drive project natively — the name
//! is metadata inside the project data itself (elektroid's data filesystem
//! only moves/copies/swaps slots). Until a verified native path exists, the
//! product offers **local labels**: clearly marked local, persisted beside
//! the cache, and guarded by a fingerprint so a label never silently attaches
//! to different content after a slot is overwritten.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name of the local metadata store inside the cache directory.
const LOCAL_META_FILE: &str = "local_meta.json";

/// Store format version, bumped on incompatible layout changes.
const STORE_VERSION: u32 = 1;

/// Local metadata for one +Drive project slot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMeta {
    /// The user's local label for this slot.
    pub alias: String,
    /// The slot's native name when the alias was set — the fingerprint. If
    /// the native name later differs, the alias is detached, not reused.
    pub native_name: String,
}

/// Everything Rytmdash remembers locally about a device's contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalMeta {
    pub version: u32,
    /// Project-slot aliases keyed by slot id.
    #[serde(default)]
    pub projects: BTreeMap<u32, ProjectMeta>,
}

impl Default for LocalMeta {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            projects: BTreeMap::new(),
        }
    }
}

impl LocalMeta {
    /// The alias for a project slot, if one is set **and** still attached —
    /// i.e. the slot's current native name matches the one recorded when the
    /// alias was created. A detached alias is never displayed (PRD §34).
    pub fn project_alias(&self, slot: u32, current_native_name: &str) -> Option<&str> {
        self.projects
            .get(&slot)
            .filter(|meta| meta.native_name == current_native_name)
            .map(|meta| meta.alias.as_str())
    }

    /// Sets (or replaces) the alias for a project slot, fingerprinting the
    /// current native name. An empty alias clears instead.
    pub fn set_project_alias(&mut self, slot: u32, alias: &str, current_native_name: &str) {
        let alias = alias.trim();
        if alias.is_empty() {
            self.projects.remove(&slot);
        } else {
            self.projects.insert(
                slot,
                ProjectMeta {
                    alias: alias.to_owned(),
                    native_name: current_native_name.to_owned(),
                },
            );
        }
    }

    /// Removes the alias for a project slot.
    pub fn clear_project_alias(&mut self, slot: u32) {
        self.projects.remove(&slot);
    }
}

/// Saves the store as JSON under `dir`, creating it if needed.
pub fn save_in(dir: &Path, meta: &LocalMeta) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(LOCAL_META_FILE);
    let json = serde_json::to_vec_pretty(meta).map_err(io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Loads the store from `dir`; a missing/unreadable/newer-versioned file
/// yields the default (empty) store rather than an error.
pub fn load_from(dir: &Path) -> LocalMeta {
    let Some(meta) = std::fs::read(dir.join(LOCAL_META_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<LocalMeta>(&bytes).ok())
    else {
        return LocalMeta::default();
    };
    if meta.version > STORE_VERSION {
        // Written by a newer Rytmdash — don't guess at its semantics.
        return LocalMeta::default();
    }
    meta
}

/// [`save_in`] into the default cache directory.
pub fn save(meta: &LocalMeta) -> io::Result<PathBuf> {
    save_in(&crate::cache::default_dir(), meta)
}

/// [`load_from`] the default cache directory.
pub fn load() -> LocalMeta {
    load_from(&crate::cache::default_dir())
}
