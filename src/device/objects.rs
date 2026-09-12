//! Object sync engine: pulls the active project's kits/patterns/sounds over
//! the AR object-sysex protocol (rytm-rs) through a [`MidiLink`].
//!
//! Query → wait for the reply → feed it to `RytmProject`. The device answers
//! on its own schedule (sysex runs on a low-priority thread in its RTOS), so
//! every reply is awaited with a timeout and unrelated sysex is skipped.
//!
//! Replies are *correlated* with the query that asked for them. A reply that
//! arrives after its query timed out is still real device data — it is fed to
//! the project — but it must not be handed back as the answer to whatever
//! query happens to be in flight, or `missed` under-reports and a read-back
//! verification can pass against a stale object.

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
        if await_reply(link, &mut rytm, &query) {
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

/// Waits for the reply to `query`, feeding every received sysex to the
/// project. Returns false when the timeout elapses with no matching message.
fn await_reply(link: &MidiLink, rytm: &mut RytmProject, query: &[u8]) -> bool {
    accept_reply(link, rytm, query).is_some()
}

/// Byte offsets shared by every AR object message — query, response, and
/// write alike: `F0 00 20 3C 07 <dev_id> <obj_type> <container:2> <obj_nr> …`.
const OBJ_TYPE_OFFSET: usize = 6;
const OBJ_NR_OFFSET: usize = 9;

/// A query's command byte is its response's object type plus this: kit
/// `0x62`→`0x52`, sound `0x63`→`0x53`, pattern `0x64`→`0x54`, settings
/// `0x66`→`0x56`, global `0x67`→`0x57`.
const QUERY_CMD_BIAS: u8 = 0x10;

/// True when `reply` answers `query`: same object type and object number.
pub fn is_reply_to(reply: &[u8], query: &[u8]) -> bool {
    let (Some(&cmd), Some(&want_nr)) = (query.get(OBJ_TYPE_OFFSET), query.get(OBJ_NR_OFFSET))
    else {
        return false;
    };
    let (Some(&kind), Some(&got_nr)) = (reply.get(OBJ_TYPE_OFFSET), reply.get(OBJ_NR_OFFSET))
    else {
        return false;
    };
    cmd.checked_sub(QUERY_CMD_BIAS) == Some(kind) && want_nr == got_nr
}

/// Waits for the reply to `query` and returns its raw bytes (`None` on
/// timeout). Non-AR sysex is skipped; an AR object that is *not* the answer
/// is still applied to the project (it is a late reply to an earlier query)
/// but does not end the wait.
pub fn accept_reply(link: &MidiLink, rytm: &mut RytmProject, query: &[u8]) -> Option<Vec<u8>> {
    let deadline = std::time::Instant::now() + OBJECT_REPLY_TIMEOUT;
    loop {
        let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
        let msg = link.recv_timeout(remaining)?;
        if rytm.update_from_sysex_response(&msg).is_ok() && is_reply_to(&msg, query) {
            return Some(msg);
        }
    }
}
