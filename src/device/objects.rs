//! Object sync engine: pulls the active project's kits/patterns/sounds over
//! the AR object-sysex protocol (rytm-rs) through a [`MidiLink`].
//!
//! Query → wait for the reply → feed it to `RytmProject`. The device answers
//! on its own schedule (sysex runs on a low-priority thread in its RTOS), so
//! every reply is awaited with a timeout and unrelated sysex is skipped.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rytm_rs::prelude::*;
use rytm_rs::RytmProject;

use crate::config::{OBJECT_REPLY_TIMEOUT, SYNC_MISS_LIMIT};
use crate::midi::link::MidiLink;
use crate::model::ProjectModel;

/// Progress of a running sync, published after every object.
#[derive(Debug, Clone, Default)]
pub struct SyncProgress {
    pub stage: &'static str,
    pub done: usize,
    pub total: usize,
    /// Objects that did not answer within the timeout.
    pub missed: usize,
}

/// Outcome of a sync run.
#[derive(Debug)]
pub enum SyncOutcome {
    /// Model built; `missed` objects went unanswered (0 = complete pull).
    Complete { model: ProjectModel, missed: usize },
    /// Cancelled from the UI; nothing published.
    Cancelled,
    /// Too many consecutive silent objects — device gone or not listening.
    DeviceSilent,
}

/// Pulls kits, patterns, and pool sounds of the active project.
///
/// `cancel` is checked between objects; `progress` fires after each one.
pub fn sync_active_project(
    link: &mut MidiLink,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(SyncProgress),
) -> Result<SyncOutcome, rytm_rs::error::RytmError> {
    let mut rytm = RytmProject::try_default()?;
    let mut missed = 0usize;
    let mut consecutive_misses = 0usize;

    let kit_queries: Vec<(&'static str, Vec<u8>)> = (0..128)
        .map(|i| Ok(("kits", KitQuery::new(i)?.as_sysex()?)))
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;
    let pattern_queries: Vec<(&'static str, Vec<u8>)> = (0..128)
        .map(|i| Ok(("patterns", PatternQuery::new(i)?.as_sysex()?)))
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;
    let sound_queries: Vec<(&'static str, Vec<u8>)> = (0..128)
        .map(|i| Ok(("pool sounds", SoundQuery::new(i)?.as_sysex()?)))
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;

    let queries: Vec<(&'static str, Vec<u8>)> = kit_queries
        .into_iter()
        .chain(pattern_queries)
        .chain(sound_queries)
        .collect();
    let total = queries.len();

    for (done, (stage, query)) in queries.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(SyncOutcome::Cancelled);
        }
        if link.send(&query).is_err() {
            return Ok(SyncOutcome::DeviceSilent);
        }
        if await_reply(link, &mut rytm) {
            consecutive_misses = 0;
        } else {
            missed += 1;
            consecutive_misses += 1;
            if consecutive_misses >= SYNC_MISS_LIMIT {
                return Ok(SyncOutcome::DeviceSilent);
            }
        }
        progress(SyncProgress {
            stage,
            done: done + 1,
            total,
            missed,
        });
    }

    Ok(SyncOutcome::Complete {
        model: ProjectModel::from_rytm(&rytm),
        missed,
    })
}

/// Waits for the reply to the last query, feeding every received sysex to the
/// project. Returns false when the timeout elapses with no accepted message.
fn await_reply(link: &MidiLink, rytm: &mut RytmProject) -> bool {
    let deadline = std::time::Instant::now() + OBJECT_REPLY_TIMEOUT;
    loop {
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return false;
        };
        let Some(msg) = link.recv_timeout(remaining) else {
            return false;
        };
        // Non-AR sysex (or a reply we cannot parse) is skipped, not fatal.
        if rytm.update_from_sysex_response(&msg).is_ok() {
            return true;
        }
    }
}
