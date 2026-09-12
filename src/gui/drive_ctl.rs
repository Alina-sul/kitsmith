//! On-demand +Drive directory browser: fetches one directory listing at a
//! time on a worker thread and caches results for the session.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::device::detect::is_rytm_port;
use crate::device::files::{DriveEntry, FileClient};
use crate::midi::link::MidiLink;
use crate::midi::ports;

/// What the browser knows about one directory: either its entries, or that
/// the device did not answer. A failed read is remembered so the UI can
/// offer a retry instead of silently showing an empty folder.
pub type DriveListing = Option<Vec<DriveEntry>>;

#[derive(Default)]
pub struct DriveBrowser {
    cache: Arc<Mutex<BTreeMap<String, DriveListing>>>,
    loading: Arc<Mutex<Option<String>>>,
}

impl DriveBrowser {
    /// Cached listing for `path`: `None` when not fetched yet,
    /// `Some(None)` when the read failed, `Some(Some(entries))` on success.
    pub fn get(&self, path: &str) -> Option<DriveListing> {
        self.cache
            .lock()
            .expect("drive cache lock poisoned")
            .get(path)
            .cloned()
    }

    /// Forgets `path` so the next [`Self::request`] reads it again.
    pub fn forget(&self, path: &str) {
        self.cache
            .lock()
            .expect("drive cache lock poisoned")
            .remove(path);
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
                let listing = fetch(&in_port, &path);
                cache
                    .lock()
                    .expect("drive cache lock poisoned")
                    .insert(path, listing);
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
