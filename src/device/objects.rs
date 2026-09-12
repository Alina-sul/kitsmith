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

/// One object the sync could not read, and how it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissedObject {
    /// e.g. `kit 07`, `pattern B03`, `pool sound 041`.
    pub what: String,
    /// True when sysex arrived but rytm-rs did not accept it (parse/version
    /// problem); false when the device stayed silent (timeout).
    pub got_reply: bool,
}

impl MissedObject {
    pub fn describe(&self) -> String {
        if self.got_reply {
            format!("{} — reply received but not understood", self.what)
        } else {
            format!("{} — no reply within the timeout", self.what)
        }
    }
}

/// Outcome of a sync run.
#[derive(Debug)]
pub enum SyncOutcome {
    /// Model built; `missed` lists unread objects (empty = complete pull).
    Complete {
        model: ProjectModel,
        missed: Vec<MissedObject>,
    },
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
    let mut missed: Vec<MissedObject> = Vec::new();
    let mut consecutive_misses = 0usize;

    let kit_queries: Vec<(&'static str, String, Vec<u8>)> = (0..128)
        .map(|i| {
            Ok((
                "kits",
                format!("kit {:02}", i + 1),
                KitQuery::new(i)?.as_sysex()?,
            ))
        })
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;
    let pattern_queries: Vec<(&'static str, String, Vec<u8>)> = (0..128)
        .map(|i| {
            Ok((
                "patterns",
                format!("pattern {}", crate::model::pattern_display_name(i)),
                PatternQuery::new(i)?.as_sysex()?,
            ))
        })
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;
    let sound_queries: Vec<(&'static str, String, Vec<u8>)> = (0..128)
        .map(|i| {
            Ok((
                "pool sounds",
                format!("pool sound {:03}", i + 1),
                SoundQuery::new(i)?.as_sysex()?,
            ))
        })
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;

    let queries: Vec<(&'static str, String, Vec<u8>)> = kit_queries
        .into_iter()
        .chain(pattern_queries)
        .chain(sound_queries)
        .collect();
    let total = queries.len();

    for (done, (stage, what, query)) in queries.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(SyncOutcome::Cancelled);
        }
        if link.send(&query).is_err() {
            return Ok(SyncOutcome::DeviceSilent);
        }
        match await_reply(link, &mut rytm, &query) {
            ReplyResult::Accepted => consecutive_misses = 0,
            outcome => {
                missed.push(MissedObject {
                    what,
                    got_reply: outcome == ReplyResult::ReceivedButRejected,
                });
                consecutive_misses += 1;
                if consecutive_misses >= SYNC_MISS_LIMIT {
                    return Ok(SyncOutcome::DeviceSilent);
                }
            }
        }
        progress(SyncProgress {
            stage,
            done: done + 1,
            total,
            missed: missed.len(),
        });
    }

    Ok(SyncOutcome::Complete {
        model: ProjectModel::from_rytm(&rytm),
        missed,
    })
}

/// How waiting for one object's reply ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplyResult {
    /// The reply to this query was accepted into the project.
    Accepted,
    /// Sysex arrived within the timeout but none of it answered the query —
    /// the device spoke, rytm-rs could not parse it.
    ReceivedButRejected,
    /// Nothing arrived at all.
    Silent,
}

/// Waits for the reply to `query`, feeding every received sysex to the
/// project, and reports how the wait ended.
///
/// An AR object that parses but belongs to a *different* query is a late
/// reply to an earlier one: it is kept (the data is real) but does not end
/// the wait, and does not count as this query having been answered.
fn await_reply(link: &MidiLink, rytm: &mut RytmProject, query: &[u8]) -> ReplyResult {
    let deadline = std::time::Instant::now() + OBJECT_REPLY_TIMEOUT;
    let mut rejected_any = false;
    loop {
        let ran_out = |rejected_any: bool| {
            if rejected_any {
                ReplyResult::ReceivedButRejected
            } else {
                ReplyResult::Silent
            }
        };
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return ran_out(rejected_any);
        };
        let Some(msg) = link.recv_timeout(remaining) else {
            return ran_out(rejected_any);
        };
        if !apply_reply(rytm, &msg) {
            rejected_any = true;
            continue;
        }
        if is_reply_to(&msg, query) {
            return ReplyResult::Accepted;
        }
    }
}

/// Feeds one message to the project, reporting whether it was understood.
///
/// rytm-rs can panic on objects it does not understand — treat that as a
/// rejected reply, not an app crash.
fn apply_reply(rytm: &mut RytmProject, msg: &[u8]) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rytm.update_from_sysex_response(msg).is_ok()
    }))
    .unwrap_or(false)
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
        if apply_reply(rytm, &msg) && is_reply_to(&msg, query) {
            return Some(msg);
        }
    }
}
