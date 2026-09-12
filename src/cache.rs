//! On-disk cache of the last synced project model.
//!
//! Sysex pulls are slow (hundreds of round-trips); the dashboard renders from
//! this cache instantly on launch and refreshes it after every sync. GUI-side,
//! plain JSON — the model is small (unlike raw rytm-rs structs).

use std::io;
use std::path::{Path, PathBuf};

use crate::model::ProjectModel;

/// File name of the active-project cache inside the cache directory.
const ACTIVE_PROJECT_FILE: &str = "active_project.json";

/// Platform cache directory (`~/Library/Application Support/rytmdash` on
/// macOS, `~/.rytmdash` elsewhere). Not created until the first save.
pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/rytmdash")
    } else {
        home.join(".rytmdash")
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
