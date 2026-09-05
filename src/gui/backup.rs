//! Full-device project backup: downloads every valid +Drive project slot's
//! raw payload into the local cache (read-only on the device). The stored
//! payloads are the WP11 indexing substrate and a real disaster-recovery
//! backup at the same time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::cache;
use crate::device::detect::is_rytm_port;
use crate::device::files::{crc32, FileClient, PROJECTS_PATH};
use crate::midi::link::MidiLink;
use crate::midi::ports;

/// What the backup worker last reported.
#[derive(Debug, Clone, Default)]
pub enum BackupStatus {
    #[default]
    Idle,
    Running {
        current: String,
        done: usize,
        total: usize,
    },
    Finished {
        ok: usize,
        failed: usize,
    },
    Failed(String),
}

/// Handle to the (at most one) running backup worker.
#[derive(Default)]
pub struct BackupController {
    status: Arc<Mutex<BackupStatus>>,
    cancel: Arc<AtomicBool>,
}

impl BackupController {
    pub fn status(&self) -> BackupStatus {
        self.status
            .lock()
            .expect("backup status lock poisoned")
            .clone()
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status(), BackupStatus::Running { .. })
    }

    /// Asks a running backup to stop after the current project.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// If a backup finished, returns `(ok, failed)` counts (resetting to idle).
    pub fn take_finished(&self) -> Option<(usize, usize)> {
        let mut status = self.status.lock().expect("backup status lock poisoned");
        if let BackupStatus::Finished { ok, failed } = &*status {
            let result = (*ok, *failed);
            *status = BackupStatus::Idle;
            return Some(result);
        }
        None
    }

    /// Starts backing up `projects` (slot id + native name) via `in_port`.
    pub fn start(
        &self,
        in_port: String,
        projects: Vec<(u32, String)>,
        on_change: impl Fn() + Send + 'static,
    ) {
        if self.is_running() || projects.is_empty() {
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        *self.status.lock().expect("backup status lock poisoned") = BackupStatus::Running {
            current: String::new(),
            done: 0,
            total: projects.len(),
        };

        let status = Arc::clone(&self.status);
        let cancel = Arc::clone(&self.cancel);
        thread::Builder::new()
            .name("project-backup".into())
            .spawn(move || {
                let result = run_backup(&in_port, &projects, &cancel, &status, &on_change);
                *status.lock().expect("backup status lock poisoned") = result;
                on_change();
            })
            .expect("failed to spawn backup thread");
    }
}

fn run_backup(
    in_port: &str,
    projects: &[(u32, String)],
    cancel: &AtomicBool,
    status: &Arc<Mutex<BackupStatus>>,
    on_change: &impl Fn(),
) -> BackupStatus {
    let Some(out_port) = ports::snapshot()
        .outputs
        .into_iter()
        .find(|n| is_rytm_port(n))
    else {
        return BackupStatus::Failed("no Rytm MIDI output port found".into());
    };
    let mut link = match MidiLink::open(in_port, &out_port) {
        Ok(link) => link,
        Err(e) => return BackupStatus::Failed(e.to_string()),
    };
    let mut client = FileClient::new();
    if !client.ping(&mut link) {
        return BackupStatus::Failed("device did not answer the file-protocol ping".into());
    }

    let cache_dir = cache::default_dir();
    let (mut ok, mut failed) = (0, 0);
    for (done, (slot, name)) in projects.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        *status.lock().expect("backup status lock poisoned") = BackupStatus::Running {
            current: name.clone(),
            done,
            total: projects.len(),
        };
        on_change();

        let path = format!("{PROJECTS_PATH}/{slot}");
        match client.download_data(&mut link, &path, |_, _| {}) {
            Some(download) => {
                let fingerprint = crc32(0, &download.bytes);
                match cache::save_project_backup(
                    &cache_dir,
                    *slot,
                    name,
                    &download.bytes,
                    fingerprint,
                ) {
                    Ok(()) => ok += 1,
                    Err(_) => failed += 1,
                }
            }
            None => failed += 1,
        }
    }
    BackupStatus::Finished { ok, failed }
}
