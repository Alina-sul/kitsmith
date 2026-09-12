//! On-disk cache of the last synced project model, plus the per-project
//! backup store (raw downloaded project payloads + manifest).
//!
//! Sysex pulls are slow (hundreds of round-trips); the dashboard renders from
//! this cache instantly on launch and refreshes it after every sync. GUI-side,
//! plain JSON — the model is small (unlike raw rytm-rs structs).

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::ProjectModel;

/// File name of the active-project cache inside the cache directory.
const ACTIVE_PROJECT_FILE: &str = "active_project.json";

/// Subdirectory holding one raw payload file per backed-up project slot.
const PROJECTS_SUBDIR: &str = "projects";

/// Manifest file describing the backed-up project payloads.
const PROJECTS_MANIFEST_FILE: &str = "projects/manifest.json";

/// What we know about one backed-up project slot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectBackup {
    /// Native project name at backup time.
    pub name: String,
    /// Compressed payload size in bytes.
    pub bytes: u64,
    /// CRC-32 of the payload — the content fingerprint.
    pub crc: u32,
    /// Unix seconds when the backup was taken.
    pub taken_at: u64,
    /// True when the payload is the raw (uncompressed) format the project
    /// parser understands; false for older compressed backups.
    #[serde(default)]
    pub raw: bool,
}

/// Manifest: slot id → backup info.
pub type ProjectManifest = BTreeMap<u32, ProjectBackup>;

/// Saves one project's payload and updates the manifest.
pub fn save_project_backup(
    dir: &Path,
    slot: u32,
    name: &str,
    payload: &[u8],
    crc: u32,
    raw: bool,
) -> io::Result<()> {
    let projects_dir = dir.join(PROJECTS_SUBDIR);
    std::fs::create_dir_all(&projects_dir)?;
    std::fs::write(projects_dir.join(format!("{slot:03}.bin")), payload)?;
    let mut manifest = load_project_manifest(dir);
    manifest.insert(
        slot,
        ProjectBackup {
            name: name.to_owned(),
            bytes: payload.len() as u64,
            crc,
            taken_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            raw,
        },
    );
    let json = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    std::fs::write(dir.join(PROJECTS_MANIFEST_FILE), json)
}

/// Reads one backed-up project payload, if present.
pub fn load_project_backup(dir: &Path, slot: u32) -> Option<Vec<u8>> {
    std::fs::read(dir.join(PROJECTS_SUBDIR).join(format!("{slot:03}.bin"))).ok()
}

/// Loads the backup manifest from `dir` (empty when absent/unreadable).
pub fn load_project_manifest(dir: &Path) -> ProjectManifest {
    std::fs::read(dir.join(PROJECTS_MANIFEST_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Platform cache directory (`~/Library/Application Support/kitsmith` on
/// macOS, `~/.kitsmith` elsewhere). Not created until the first save.
pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/kitsmith")
    } else {
        home.join(".kitsmith")
    }
}

/// Saves the model as JSON under `dir`, creating it if needed.
/// Returns the file path written.
pub fn save_in(dir: &Path, model: &ProjectModel) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(ACTIVE_PROJECT_FILE);
    let json = serde_json::to_vec_pretty(model).map_err(io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Loads the cached model from `dir`, if present and readable.
pub fn load_from(dir: &Path) -> Option<ProjectModel> {
    let bytes = std::fs::read(dir.join(ACTIVE_PROJECT_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// [`save_in`] into the default directory.
pub fn save(model: &ProjectModel) -> io::Result<PathBuf> {
    save_in(&default_dir(), model)
}

/// [`load_from`] the default directory.
pub fn load() -> Option<ProjectModel> {
    load_from(&default_dir())
}
