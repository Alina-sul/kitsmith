//! On-demand +Drive directory browser: fetches one directory listing at a
//! time on a worker thread and caches results for the session.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::device::detect::is_rytm_port;
use crate::device::files::{DriveEntry, FileClient};
use crate::midi::link::MidiLink;
use crate::midi::ports;

#[derive(Default)]
pub struct DriveBrowser {
    cache: Arc<Mutex<BTreeMap<String, Vec<DriveEntry>>>>,
    loading: Arc<Mutex<Option<String>>>,
}

impl DriveBrowser {
    /// Cached listing for `path`, if fetched this session.
    pub fn get(&self, path: &str) -> Option<Vec<DriveEntry>> {
        self.cache
            .lock()
            .expect("drive cache lock poisoned")
            .get(path)
            .cloned()
    }

    /// Drops one cached listing so the next render re-fetches it (call after
    /// any write that changed the directory).
    pub fn invalidate(&self, path: &str) {
        self.cache
            .lock()
            .expect("drive cache lock poisoned")
            .remove(path);
    }

    /// The directories fetched this session (for move-target pickers).
    pub fn cached_dirs(&self) -> Vec<String> {
        self.cache
            .lock()
            .expect("drive cache lock poisoned")
            .keys()
            .cloned()
            .collect()
    }

    /// Fetches `path` unless cached or a fetch is already running.
    pub fn request(&self, in_port: String, path: String, on_change: impl Fn() + Send + 'static) {
        {
            let cache = self.cache.lock().expect("drive cache lock poisoned");
            if cache.contains_key(&path) {
                return;
            }
        }
        {
            let mut loading = self.loading.lock().expect("drive loading lock poisoned");
            if loading.is_some() {
                return;
            }
            *loading = Some(path.clone());
        }
        let cache = Arc::clone(&self.cache);
        let loading = Arc::clone(&self.loading);
        thread::Builder::new()
            .name("drive-browse".into())
            .spawn(move || {
                let entries = fetch(&in_port, &path).unwrap_or_default();
                cache
                    .lock()
                    .expect("drive cache lock poisoned")
                    .insert(path, entries);
                *loading.lock().expect("drive loading lock poisoned") = None;
                on_change();
            })
            .expect("failed to spawn drive-browse thread");
    }
}

fn fetch(in_port: &str, path: &str) -> Option<Vec<DriveEntry>> {
    let out_port = ports::snapshot()
        .outputs
        .into_iter()
        .find(|n| is_rytm_port(n))?;
    let mut link = MidiLink::open(in_port, &out_port).ok()?;
    let mut client = FileClient::new();
    if !client.ping(&mut link) {
        return None;
    }
    client.list_sample_dir(&mut link, path)
}
